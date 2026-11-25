//! Texts of Freya TDE in several languages: `locales/<language>.toml`,
//! embedded and registered with `ruf::i18n` (English is the fallback).
//! Keys are dotted paths (`[menu.file] new = "…"` → `menu.file.new`),
//! placeholders in curly braces.
//!
//! The language comes from the settings (`System` = the system's language,
//! German if it is German, else English); `FREYA_TDE_LANG` overrides it
//! (tests, scripts). A change applies from the next start.
//!
//! *Texte von Freya TDE je Sprache; die Sprache gilt ab dem Start.*

use std::sync::Once;

use crate::settings::Language;

const EMBEDDED: &[(&str, &str)] = &[
    ("de", include_str!("../locales/de.toml")),
    ("en", include_str!("../locales/en.toml")),
];

/// Registers the embedded texts once (also used without [`init`], e.g. by
/// tests or the AI, which then get English until a language is set).
pub fn ensure() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| ruf::i18n::register(EMBEDDED));
}

/// Sets the language from the settings (or `FREYA_TDE_LANG`); returns the
/// one in use (`de`, `en`).
pub fn init(language: Language) -> String {
    ensure();
    let wanted = std::env::var("FREYA_TDE_LANG")
        .ok()
        .filter(|l| !l.is_empty())
        .or_else(|| language.code().map(str::to_owned))
        .unwrap_or_else(ruf::i18n::system_language);
    ruf::i18n::set_language(&wanted)
}

/// [`init`] with the language from the settings file (before any window,
/// e.g. for the start screen).
pub fn init_from_settings() -> String {
    init(
        crate::settings::Settings::file()
            .map(|f| crate::settings::Settings::load_from(&f))
            .unwrap_or_default()
            .language,
    )
}

/// Translates `key` (registers the texts first if needed).
pub fn translate(key: &str) -> String {
    ensure();
    ruf::i18n::translate(key)
}

/// Like [`translate`], replacing `{name}` placeholders.
pub fn translate_with(key: &str, args: &[(&str, String)]) -> String {
    ensure();
    ruf::i18n::translate_with(key, args)
}

/// Translates a key of Freya TDE: `tr!("menu.file.new")` or with
/// placeholders `tr!("status.saved", path = p.display())`.
#[macro_export]
macro_rules! tr {
    ($key:expr) => {
        $crate::i18n::translate($key)
    };
    ($key:expr, $($name:ident = $value:expr),+ $(,)?) => {
        $crate::i18n::translate_with($key, &[$((stringify!($name), $value.to_string())),+])
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Both languages have the same keys.
    #[test]
    fn languages_are_complete() {
        ensure();
        let reference = ruf::i18n::keys("en");
        assert!(!reference.is_empty());
        assert_eq!(
            ruf::i18n::keys("de"),
            reference,
            "de.toml und en.toml unterscheiden sich"
        );
    }

    /// Every `tr!("…")` in the sources has a text.
    #[test]
    fn every_used_key_exists() {
        ensure();
        let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut missing = Vec::new();
        let mut stack = vec![src];
        while let Some(dir) = stack.pop() {
            for entry in std::fs::read_dir(&dir).unwrap().flatten() {
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                } else if path.extension().is_some_and(|e| e == "rs") {
                    let text = std::fs::read_to_string(&path).unwrap();
                    for part in text.split("tr!(\"").skip(1) {
                        let key = part.split('"').next().unwrap_or_default();
                        if !key.is_empty() && !key.contains('{') && !ruf::i18n::has(key) {
                            missing.push(format!("{}: {key}", path.display()));
                        }
                    }
                }
            }
        }
        // Die Beispiele in dieser Datei sind keine echten Schlüssel.
        missing.retain(|m| !m.contains("i18n.rs"));
        assert!(
            missing.is_empty(),
            "Schlüssel fehlen:\n{}",
            missing.join("\n")
        );
    }
}
