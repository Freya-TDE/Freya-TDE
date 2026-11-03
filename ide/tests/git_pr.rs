//! Commit & Create Pull Request against a local bare remote, with a stand-in
//! for the GitHub CLI (`FREYA_TDE_GH`): it records its arguments and prints a
//! pull request address like `gh pr create` does.

#![cfg(unix)]

use std::path::Path;
use std::process::Command;

use freya_tde::git::{After, Git, GitOp};

fn write(dir: &Path, path: &str, text: &str) {
    std::fs::write(dir.join(path), text).unwrap();
}

fn sh(dir: &Path, args: &[&str]) {
    let ok = Command::new("git")
        .current_dir(dir)
        .args(args)
        .status()
        .unwrap()
        .success();
    assert!(ok, "git {args:?}");
}

fn codes(git: &Git, op: &GitOp) -> Vec<&'static str> {
    git.preview(op)
        .unwrap()
        .issues
        .iter()
        .map(|i| i.code)
        .collect()
}

/// A fake `gh`: `--version` works, `pr create` records its arguments and
/// prints an address – or fails like gh when `fail` is given.
fn fake_gh(dir: &Path, fail: Option<&str>) -> std::path::PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let path = dir.join(if fail.is_some() { "gh-fail" } else { "gh" });
    let args = dir.join("gh-args");
    let body = match fail {
        Some(message) => format!("echo '{message}' >&2\nexit 1"),
        None => "echo https://github.com/demo/repo/pull/7".to_owned(),
    };
    std::fs::write(
        &path,
        format!(
            "#!/bin/sh\nif [ \"$1\" = --version ]; then echo 'gh version 2.0'; exit 0; fi\necho \"$@\" > '{}'\n{body}\n",
            args.display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path
}

fn pull_request(message: &str) -> GitOp {
    GitOp::Commit {
        message: message.into(),
        amend: false,
        then: After::PullRequest,
    }
}

#[test]
fn commit_and_create_pull_request() {
    if Command::new("git").arg("--version").output().is_err() {
        eprintln!("git nicht installiert – Test übersprungen");
        return;
    }
    let base = std::env::temp_dir().join(format!("freya-git-pr-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(&base).unwrap();
    let config = base.join("gitconfig");
    std::fs::write(
        &config,
        "[user]\n\tname = Test\n\temail = test@example.com\n[init]\n\tdefaultBranch = main\n",
    )
    .unwrap();
    let gh = fake_gh(&base, None);
    // SAFETY: einziger Test in diesem Prozess, vor allen git-Aufrufen.
    unsafe {
        std::env::set_var("GIT_CONFIG_GLOBAL", &config);
        std::env::set_var("GIT_CONFIG_NOSYSTEM", "1");
        std::env::set_var("FREYA_TDE_GH", &gh);
    }

    // Remote mit main, origin/HEAD zeigt darauf.
    let remote = base.join("remote.git");
    sh(&base, &["init", "--bare", "-q", remote.to_str().unwrap()]);
    let dir = base.join("work");
    let git = Git::init(&dir).unwrap();
    write(&dir, "README.md", "# Demo\n");
    git.run(&GitOp::Stage { paths: vec![] }).unwrap();
    git.run(&GitOp::Commit {
        message: "Start".into(),
        amend: false,
        then: After::Nothing,
    })
    .unwrap();
    sh(&dir, &["remote", "add", "origin", remote.to_str().unwrap()]);
    sh(&dir, &["push", "-q", "-u", "origin", "main"]);
    sh(&dir, &["remote", "set-head", "origin", "main"]);
    assert_eq!(git.default_branch().as_deref(), Some("main"));

    // Auf dem Hauptzweig gibt es keinen Pull Request.
    write(&dir, "README.md", "# Demo\nmehr\n");
    git.run(&GitOp::Stage { paths: vec![] }).unwrap();
    assert!(codes(&git, &pull_request("Mehr")).contains(&"pull_request_from_base"));

    // Eigener Zweig: Commit, Zweig veröffentlichen, gh pr create.
    git.run(&GitOp::CreateBranch {
        name: "feature".into(),
        checkout: true,
    })
    .unwrap();
    let preview = git.preview(&pull_request("Mehr")).unwrap();
    assert!(!preview.has_errors(), "{:#?}", preview.issues);
    assert_eq!(
        preview.commands.last().unwrap(),
        &[
            "gh", "pr", "create", "--fill", "--head", "feature", "--base", "main"
        ]
    );
    let log = git.run(&pull_request("Mehr")).unwrap();
    assert!(log.contains("https://github.com/demo/repo/pull/7"), "{log}");
    assert_eq!(
        std::fs::read_to_string(base.join("gh-args"))
            .unwrap()
            .trim(),
        "pr create --fill --head feature --base main"
    );
    let status = git.status().unwrap();
    assert_eq!(status.upstream.as_deref(), Some("origin/feature"));

    // gh meldet: gibt es schon.
    let failing = fake_gh(
        &base,
        Some("a pull request for branch \"feature\" already exists"),
    );
    // SAFETY: wie oben, kein anderer Thread.
    unsafe { std::env::set_var("FREYA_TDE_GH", &failing) };
    write(&dir, "README.md", "# Demo\nnoch mehr\n");
    git.run(&GitOp::Stage { paths: vec![] }).unwrap();
    let error = git.run(&pull_request("Noch mehr")).unwrap_err();
    assert_eq!(error.code, "pull_request_exists", "{error:?}");

    // Ohne GitHub-CLI meldet schon die Vorschau den Grund.
    // SAFETY: wie oben.
    unsafe { std::env::set_var("FREYA_TDE_GH", base.join("kein-gh")) };
    write(&dir, "README.md", "# Demo\nzuletzt\n");
    git.run(&GitOp::Stage { paths: vec![] }).unwrap();
    assert!(codes(&git, &pull_request("Zuletzt")).contains(&"gh_missing"));
    assert_eq!(
        git.run(&pull_request("Zuletzt")).unwrap_err().code,
        "refused"
    );

    let _ = std::fs::remove_dir_all(&base);
}
