//! The askpass helper (`freya-tde` as `GIT_ASKPASS`/`SSH_ASKPASS`) and the
//! whole way from `git` to the question service.

/// `freya-tde` as `GIT_ASKPASS`/`SSH_ASKPASS`: hands the question to the
/// service and prints the answer (exit code 1 = cancelled).
#[test]
fn askpass_helper() {
    use freya_tde::git::askpass::Askpass;
    let program = std::path::PathBuf::from(env!("CARGO_BIN_EXE_freya-tde"));
    let askpass = Askpass::start(program.clone(), |question| {
        if question.starts_with("Enter passphrase") || question.starts_with("Password for") {
            Some("s3cret".to_owned())
        } else if question.starts_with("Username for 'https://example.com'") {
            Some("anna".to_owned())
        } else {
            None
        }
    })
    .unwrap();
    let ask = |question: &str| {
        let mut command = std::process::Command::new(&program);
        command.arg(question);
        askpass.apply(&mut command);
        command.output().unwrap()
    };
    let output = ask("Enter passphrase for key '/home/u/.ssh/id_ed25519':");
    assert!(output.status.success());
    assert_eq!(String::from_utf8_lossy(&output.stdout), "s3cret\n");
    let output = ask("Username for 'https://other.example':");
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());

    // Der ganze Weg: git fragt über GIT_ASKPASS nach Name und Passwort.
    if std::process::Command::new("git")
        .arg("--version")
        .output()
        .is_err()
    {
        return;
    }
    let mut command = std::process::Command::new("git");
    command
        .args(["-c", "credential.helper=", "credential", "fill"])
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped());
    askpass.apply(&mut command);
    let mut child = command.spawn().unwrap();
    {
        use std::io::Write;
        let mut stdin = child.stdin.take().unwrap();
        stdin
            .write_all(b"protocol=https\nhost=example.com\n\n")
            .unwrap();
    }
    let output = child.wait_with_output().unwrap();
    let text = String::from_utf8_lossy(&output.stdout);
    assert!(text.contains("username=anna"), "{text}");
    assert!(text.contains("password=s3cret"), "{text}");
}
