//! The Git API against real repositories in a temporary folder: a local
//! bare repository plays the remote (push, pull, ahead/behind, rejected).

use std::path::Path;

use freya_td::git::{After, Change, Git, GitOp};

fn write(dir: &Path, path: &str, text: &str) {
    let file = dir.join(path);
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(file, text).unwrap();
}

fn codes(git: &Git, op: &GitOp) -> Vec<&'static str> {
    git.preview(op)
        .unwrap()
        .issues
        .iter()
        .map(|i| i.code)
        .collect()
}

fn commit(git: &Git, message: &str) {
    git.run(&GitOp::Stage { paths: vec![] }).unwrap();
    git.run(&GitOp::Commit {
        message: message.into(),
        amend: false,
        then: After::Nothing,
    })
    .unwrap();
}

#[test]
fn git_round_trip() {
    if std::process::Command::new("git")
        .arg("--version")
        .output()
        .is_err()
    {
        eprintln!("git nicht installiert – Test übersprungen");
        return;
    }
    let base = std::env::temp_dir().join(format!("freya-git-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(&base).unwrap();
    // Unabhängig von der Konfiguration des Nutzers (Signieren, Hooks …).
    let config = base.join("gitconfig");
    std::fs::write(
        &config,
        "[user]\n\tname = Test\n\temail = test@example.com\n[init]\n\tdefaultBranch = main\n[pull]\n\trebase = false\n",
    )
    .unwrap();
    // SAFETY: einziger Test in diesem Prozess, vor allen git-Aufrufen.
    unsafe {
        std::env::set_var("GIT_CONFIG_GLOBAL", &config);
        std::env::set_var("GIT_CONFIG_NOSYSTEM", "1");
    }

    // Neues Repository: noch kein Commit.
    let dir = base.join("work");
    let git = Git::init(&dir).unwrap();
    assert_eq!(git.root(), dir.canonicalize().unwrap());
    let status = git.status().unwrap();
    assert_eq!(status.head, None);
    assert_eq!(status.branch.as_deref(), Some("main"));
    assert!(git.log(10).unwrap().is_empty());
    assert!(Git::open(&base).is_err_and(|e| e.code == "not_a_repository"));

    // Ungetrackt → gestaged → Commit.
    write(&dir, "src/main.rs", "fn main() {}\n");
    write(&dir, "README.md", "# Demo\n");
    let status = git.status().unwrap();
    assert_eq!(status.change_of("src/main.rs"), Some(Change::Untracked));
    assert_eq!(
        codes(
            &git,
            &GitOp::Commit {
                message: "x".into(),
                amend: false,
                then: After::Nothing
            }
        ),
        ["nothing_staged"]
    );
    git.run(&GitOp::Stage {
        paths: vec!["src/main.rs".into()],
    })
    .unwrap();
    let status = git.status().unwrap();
    assert_eq!(status.change_of("src/main.rs"), Some(Change::Added));
    assert_eq!(status.branch_label(), "main+");
    git.run(&GitOp::Unstage { paths: vec![] }).unwrap();
    assert!(!git.status().unwrap().has_staged());
    commit(&git, "First commit\n\nWith a body.");
    let log = git.log(10).unwrap();
    assert_eq!(log.len(), 1);
    assert_eq!(log[0].subject, "First commit");
    assert!(log[0].head);
    assert_eq!(log[0].refs, ["main"]);
    assert_eq!(log[0].author, "Test");
    assert_eq!(git.head_text("README.md").as_deref(), Some("# Demo\n"));

    // Änderungen: Diff, verwerfen.
    write(&dir, "README.md", "# Demo\n\nMehr.\n");
    write(&dir, "notes.txt", "neu\n");
    let diff = git.diff("README.md", false).unwrap();
    assert_eq!(diff.hunks.len(), 1);
    assert!(diff.text.contains("+Mehr."));
    let untracked = git.diff("notes.txt", false).unwrap();
    assert_eq!(untracked.hunks[0].lines[0].text, "neu");
    let preview = git
        .preview(&GitOp::Discard {
            paths: vec!["README.md".into()],
        })
        .unwrap();
    assert_eq!(
        preview.commands,
        [["restore", "--worktree", "--", "README.md"]]
    );
    git.run(&GitOp::Discard {
        paths: vec!["README.md".into()],
    })
    .unwrap();
    assert_eq!(
        std::fs::read_to_string(dir.join("README.md")).unwrap(),
        "# Demo\n"
    );
    std::fs::remove_file(dir.join("notes.txt")).unwrap();
    assert_eq!(
        codes(&git, &GitOp::Discard { paths: vec![] }),
        ["nothing_to_discard"]
    );

    // Remote (bare) einrichten: Push ohne Upstream veröffentlicht den Zweig.
    let remote = base.join("remote.git");
    std::process::Command::new("git")
        .args(["init", "-q", "--bare"])
        .arg(&remote)
        .status()
        .unwrap();
    assert_eq!(codes(&git, &GitOp::Push), ["no_remote"]);
    std::process::Command::new("git")
        .arg("-C")
        .arg(&dir)
        .args(["remote", "add", "origin"])
        .arg(&remote)
        .status()
        .unwrap();
    let preview = git.preview(&GitOp::Push).unwrap();
    assert_eq!(preview.commands, [["push", "-u", "origin", "main"]]);
    assert_eq!(preview.issues[0].code, "no_upstream");
    git.run(&GitOp::Push).unwrap();
    let status = git.status().unwrap();
    assert_eq!(status.upstream.as_deref(), Some("origin/main"));
    assert_eq!((status.ahead, status.behind), (0, 0));

    // Commit & Push; Amend eines gepushten Commits warnt.
    write(
        &dir,
        "src/main.rs",
        "fn main() {\n    println!(\"hi\");\n}\n",
    );
    git.run(&GitOp::Stage { paths: vec![] }).unwrap();
    git.run(&GitOp::Commit {
        message: "Say hi".into(),
        amend: false,
        then: After::Push,
    })
    .unwrap();
    assert_eq!(git.status().unwrap().ahead, 0);
    assert_eq!(
        codes(
            &git,
            &GitOp::Commit {
                message: "Say hi!".into(),
                amend: true,
                then: After::Nothing
            }
        ),
        ["amend_pushed"]
    );

    // Ein zweiter Klon pusht; hier: 1↓, eigener Commit 1↑, Push abgelehnt, Sync.
    let other = base.join("other");
    std::process::Command::new("git")
        .args(["clone", "-q"])
        .arg(&remote)
        .arg(&other)
        .status()
        .unwrap();
    let other = Git::open(&other).unwrap();
    write(other.root(), "LICENSE", "MIT\n");
    commit(&other, "Add license");
    other.run(&GitOp::Push).unwrap();
    write(&dir, "README.md", "# Demo\n\nLokal.\n");
    commit(&git, "Local change");
    git.run(&GitOp::Fetch).unwrap();
    let status = git.status().unwrap();
    assert_eq!((status.ahead, status.behind), (1, 1));
    let log = git.log(10).unwrap();
    assert!(
        log.iter()
            .any(|c| c.subject == "Local change" && c.outgoing)
    );
    assert!(log.iter().any(|c| c.subject == "Add license" && c.incoming));
    assert!(
        log.iter()
            .any(|c| c.refs.contains(&"origin/main".to_owned()))
    );
    let rejected = git.run(&GitOp::Push).unwrap_err();
    assert_eq!(rejected.code, "rejected", "{rejected}");
    git.run(&GitOp::Sync).unwrap();
    let status = git.status().unwrap();
    assert_eq!((status.ahead, status.behind), (0, 0));
    assert!(dir.join("LICENSE").is_file());

    // Zweige.
    assert_eq!(
        codes(
            &git,
            &GitOp::CreateBranch {
                name: "bad name..".into(),
                checkout: true
            }
        ),
        ["invalid_branch_name"]
    );
    git.run(&GitOp::CreateBranch {
        name: "feature".into(),
        checkout: true,
    })
    .unwrap();
    assert_eq!(git.status().unwrap().branch.as_deref(), Some("feature"));
    assert_eq!(
        codes(
            &git,
            &GitOp::CreateBranch {
                name: "feature".into(),
                checkout: false
            }
        ),
        ["branch_exists"]
    );
    git.run(&GitOp::Checkout {
        branch: "main".into(),
    })
    .unwrap();
    let branches = git.branches().unwrap();
    assert!(
        branches
            .iter()
            .any(|b| b.name == "main" && b.current && b.upstream.as_deref() == Some("origin/main"))
    );
    assert!(branches.iter().any(|b| b.name == "origin/main" && b.remote));
    assert_eq!(
        codes(
            &git,
            &GitOp::Checkout {
                branch: "nope".into()
            }
        ),
        ["unknown_branch"]
    );

    // Pfade relativ zum Repository.
    assert_eq!(
        git.relative(&dir.join("src/main.rs")).as_deref(),
        Some("src/main.rs")
    );
    let json = serde_json::to_string(&git.status().unwrap()).unwrap();
    assert!(json.contains(r#""upstream":"origin/main""#), "{json}");

    let _ = std::fs::remove_dir_all(&base);
}
