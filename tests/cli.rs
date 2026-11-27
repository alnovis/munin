use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};

use serde_json::Value;

// Every run gets its own HOME, database and working directory, and no
// MUNIN_* variables from the caller, so a test can never touch the real
// ~/.munin.
struct Sandbox {
    _tmp: tempfile::TempDir,
    home: PathBuf,
    db: PathBuf,
    repo: PathBuf,
}

impl Sandbox {
    fn new() -> Self {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        let repo = tmp.path().join("work").join("Scry.Repo");
        std::fs::create_dir_all(&home).unwrap();
        std::fs::create_dir_all(repo.join(".git")).unwrap();
        std::fs::create_dir_all(repo.join("src")).unwrap();
        let db = tmp.path().join("db").join("munin.db");
        Sandbox {
            _tmp: tmp,
            home,
            db,
            repo,
        }
    }

    fn command(&self, args: &[&str]) -> Command {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_munin"));
        cmd.args(args)
            .current_dir(self.repo.join("src"))
            .env("HOME", &self.home)
            .env("MUNIN_DB", &self.db)
            .env_remove("MUNIN_PROJECT")
            .env_remove("MUNIN_HALF_LIFE_DAYS")
            .env_remove("MUNIN_CONFIG");
        cmd
    }

    fn run(&self, args: &[&str]) -> Output {
        self.command(args).output().unwrap()
    }

    fn ok(&self, args: &[&str]) -> String {
        let out = self.run(args);
        assert!(
            out.status.success(),
            "munin {args:?} failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8(out.stdout).unwrap()
    }

    fn json(&self, args: &[&str]) -> Value {
        let mut all = vec!["--json"];
        all.extend_from_slice(args);
        serde_json::from_str(&self.ok(&all)).unwrap()
    }

    fn fail(&self, args: &[&str]) -> (i32, String) {
        let out = self.run(args);
        assert!(!out.status.success(), "munin {args:?} should fail");
        (
            out.status.code().unwrap(),
            String::from_utf8(out.stderr).unwrap(),
        )
    }

    fn add(&self, args: &[&str]) -> i64 {
        let mut all = vec!["add"];
        all.extend_from_slice(args);
        self.json(&all)["id"].as_i64().unwrap()
    }
}

// Check that the project is named after the git root and the entry reads back.
#[test]
fn add_then_show() {
    let sb = Sandbox::new();
    let id = sb.add(&[
        "--kind",
        "decision",
        "--summary",
        "Use WAL mode",
        "--body",
        "Readers do not block the writer.",
        "--tag",
        "SQLite",
        "--tag",
        "wal",
    ]);

    let entry = sb.json(&["show", &id.to_string()]);
    assert_eq!(entry["owner"]["project"], "scry-repo");
    assert_eq!(entry["kind"], "decision");
    assert_eq!(entry["status"], "active");
    assert_eq!(entry["tags"], serde_json::json!(["sqlite", "wal"]));
    assert_eq!(entry["body"], "Readers do not block the writer.");

    let text = sb.ok(&["show", &id.to_string()]);
    assert!(text.contains("owner:   project scry-repo"), "{text}");
    assert!(text.contains("Use WAL mode"), "{text}");
}

#[test]
fn search_finds_and_reports_no_matches() {
    let sb = Sandbox::new();
    let id = sb.add(&["--kind", "bugfix", "--summary", "flaky socket test"]);

    let cards = sb.json(&["search", "socket", "timeout"]);
    assert_eq!(cards.as_array().unwrap().len(), 1);
    assert_eq!(cards[0]["id"], id);
    assert_eq!(cards[0]["kind"], "bugfix");

    assert_eq!(sb.ok(&["search", "nothing"]), "no matches\n");
}

#[test]
fn supersede_and_retract_show_in_history() {
    let sb = Sandbox::new();
    let old = sb.add(&["--kind", "decision", "--summary", "retry three times"]);
    let new = sb.add(&[
        "--kind",
        "decision",
        "--summary",
        "retry five times",
        "--supersedes",
        &old.to_string(),
    ]);
    let wrong = sb.add(&["--kind", "note", "--summary", "retry is free"]);
    sb.ok(&[
        "retract",
        &wrong.to_string(),
        "--reason",
        "it costs a request",
    ]);

    let shown = sb.ok(&["show", &old.to_string()]);
    assert!(shown.contains(&format!("superseded by #{new}")), "{shown}");
    let shown = sb.ok(&["show", &wrong.to_string()]);
    assert!(shown.contains("retracted: it costs a request"), "{shown}");

    let active = sb.json(&["search", "retry"]);
    assert_eq!(active.as_array().unwrap().len(), 1);
    assert_eq!(active[0]["id"], new);
    let all = sb.json(&["search", "retry", "--inactive"]);
    assert_eq!(all.as_array().unwrap().len(), 3);
}

#[test]
fn groups_widen_the_default_search() {
    let sb = Sandbox::new();
    sb.ok(&["group", "add", "platform", "billing", "auth"]);
    let billing = sb.add(&[
        "--project",
        "billing",
        "--kind",
        "note",
        "--summary",
        "event bus",
    ]);
    let shared = sb.add(&[
        "--project",
        "auth",
        "--group",
        "platform",
        "--kind",
        "decision",
        "--summary",
        "event format is JSON",
    ]);
    sb.add(&["--kind", "note", "--summary", "event loop"]);

    let mut ids: Vec<i64> = sb
        .json(&["search", "event", "--project", "auth"])
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["id"].as_i64().unwrap())
        .collect();
    ids.sort();
    assert_eq!(ids, vec![billing, shared]);

    let own = sb.json(&["search", "event", "--project", "auth", "--scope", "project"]);
    assert!(own.as_array().unwrap().is_empty());

    let listed = sb.ok(&["group", "list", "--project", "billing"]);
    assert_eq!(listed, "platform: auth, billing\n");

    let (_, err) = sb.fail(&["group", "delete", "platform"]);
    assert!(err.contains("owns 1 entries"), "{err}");
}

#[test]
fn project_from_env_and_scope_flag_conflict() {
    let sb = Sandbox::new();
    let out = sb
        .command(&["--json", "add", "--kind", "note", "--summary", "from env"])
        .env("MUNIN_PROJECT", "Envy")
        .output()
        .unwrap();
    assert!(out.status.success());
    let entry: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(entry["owner"]["project"], "envy");

    let (code, err) = sb.fail(&["search", "x", "--scope", "all", "--group", "g"]);
    assert_eq!(code, 1);
    assert!(
        err.contains("a group can only be given with scope group"),
        "{err}"
    );
}

#[test]
fn body_from_stdin() {
    let sb = Sandbox::new();
    let mut child = sb
        .command(&[
            "--json",
            "add",
            "--kind",
            "note",
            "--summary",
            "piped",
            "--body-file",
            "-",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"line one\nline two\n")
        .unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(out.status.success());
    let entry: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(entry["body"], "line one\nline two");
}

#[test]
fn invalid_input_names_the_problem() {
    let sb = Sandbox::new();
    let (code, err) = sb.fail(&["add", "--kind", "adr", "--summary", "x"]);
    assert_eq!(code, 2, "usage errors come from argument parsing");
    assert!(
        err.contains("decision, architecture, bugfix, note"),
        "{err}"
    );

    let (code, err) = sb.fail(&["add", "--kind", "note", "--summary", "  "]);
    assert_eq!(code, 1);
    assert!(err.contains("invalid summary"), "{err}");

    let (_, err) = sb.fail(&["show", "999"]);
    assert!(err.contains("entry 999 not found"), "{err}");
}

#[test]
fn protocol_does_not_open_the_database() {
    let sb = Sandbox::new();
    let text = sb.ok(&["protocol"]);
    assert!(text.contains("search_memory"));
    assert!(!sb.db.exists());
}

#[test]
fn default_database_lives_under_home() {
    let sb = Sandbox::new();
    let out = sb
        .command(&["add", "--kind", "note", "--summary", "x"])
        .env_remove("MUNIN_DB")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(sb.home.join(".munin").join("munin.db").exists());
    assert!(!sb.db.exists());
}

#[test]
fn tags_list() {
    let sb = Sandbox::new();
    sb.add(&[
        "--kind",
        "note",
        "--summary",
        "a",
        "--tag",
        "sqlite",
        "--tag",
        "wal",
    ]);
    sb.add(&["--kind", "note", "--summary", "b", "--tag", "sqlite"]);
    let tags = sb.json(&["tags"]);
    assert_eq!(tags[0]["tag"], "sqlite");
    assert_eq!(tags[0]["total"], 2);
    assert_eq!(tags[1]["tag"], "wal");
}
