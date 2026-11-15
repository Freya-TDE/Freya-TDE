//! Checks before a key leaves the machine.

use super::{AiIssue, codes};

/// The host of an `http://` or `https://` address (without port; IPv6
/// without brackets).
fn host(url: &str) -> Option<(&str, &str)> {
    let (scheme, rest) = url.trim().split_once("://")?;
    let authority = rest.split(['/', '?', '#']).next()?;
    let authority = authority.rsplit('@').next()?;
    let host = if let Some(v6) = authority.strip_prefix('[') {
        v6.split(']').next()?
    } else {
        authority.split(':').next()?
    };
    Some((scheme, host))
}

/// Is `host` this machine (`localhost`, `127.0.0.0/8`, `::1`)?
pub fn is_loopback(host: &str) -> bool {
    let host = host.trim_matches(['[', ']']);
    host.eq_ignore_ascii_case("localhost")
        || host.to_ascii_lowercase().ends_with(".localhost")
        || host
            .parse::<std::net::IpAddr>()
            .is_ok_and(|ip| ip.is_loopback())
}

/// A warning if an API key would go unencrypted (`http://`) to another
/// machine, e.g. a server in the local network. Local servers without a
/// key are fine.
///
/// ```
/// use freya_tde::ai::key_transport_risk;
/// assert!(key_transport_risk("http://192.168.1.20:11434", true).is_some());
/// assert!(key_transport_risk("http://localhost:11434", true).is_none());
/// assert!(key_transport_risk("https://api.anthropic.com", true).is_none());
/// assert!(key_transport_risk("http://192.168.1.20:11434", false).is_none());
/// ```
pub fn key_transport_risk(endpoint: &str, sends_key: bool) -> Option<AiIssue> {
    if !sends_key {
        return None;
    }
    let (scheme, host) = host(endpoint)?;
    (scheme.eq_ignore_ascii_case("http") && !is_loopback(host)).then(|| AiIssue {
        code: codes::INSECURE_KEY_TRANSPORT,
        detail: format!(
            "Der API-Key ginge unverschlüsselt (http://) an {host}; besser https:// verwenden"
        ),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hosts() {
        assert_eq!(host("http://[::1]:8080/v1"), Some(("http", "::1")));
        assert_eq!(
            host("http://user:pw@10.0.0.2:80"),
            Some(("http", "10.0.0.2"))
        );
        assert_eq!(host("https://api.x.ai/v1"), Some(("https", "api.x.ai")));
        assert!(is_loopback("127.0.0.5") && is_loopback("::1") && is_loopback("LOCALHOST"));
        assert!(!is_loopback("192.168.1.2") && !is_loopback("example.com"));
        assert!(key_transport_risk("http://[::1]:1234/v1", true).is_none());
        assert_eq!(
            key_transport_risk("http://gpu-box:8000/v1", true)
                .unwrap()
                .code,
            codes::INSECURE_KEY_TRANSPORT
        );
    }
}
