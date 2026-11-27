use std::process::Command;

use munin::{Kind, KindCounts, NewEntry, Owner, Store};

const NOW: i64 = 1_800_000_000_000;

fn entry(project: &str, kind: Kind, summary: &str, tags: &[&str]) -> NewEntry {
    NewEntry {
        project: project.to_owned(),
        group: None,
        kind,
        summary: summary.to_owned(),
        body: String::new(),
        tags: tags.iter().map(|t| (*t).to_owned()).collect(),
        supersedes: None,
    }
}

#[test]
fn empty_project() {
    let store = Store::open_in_memory().unwrap();
    let brief = store.brief("scry", NOW).unwrap();
    assert_eq!(brief.project, "scry");
    assert_eq!(brief.active, KindCounts::default());
    assert!(brief.tags.is_empty() && brief.durable.is_empty() && brief.groups.is_empty());
}

// Check that counts cover active entries only and durable entries come
// newest first.
#[test]
fn counts_tags_and_durable_entries() {
    let mut store = Store::open_in_memory().unwrap();
    let old = store
        .append(
            &entry("scry", Kind::Decision, "retry three times", &["retry"]),
            NOW - 3,
        )
        .unwrap();
    let arch = store
        .append(
            &entry("scry", Kind::Architecture, "two crates", &["layout"]),
            NOW - 2,
        )
        .unwrap();
    let new = store
        .append(
            &NewEntry {
                supersedes: Some(old),
                ..entry("scry", Kind::Decision, "retry five times", &["retry"])
            },
            NOW - 1,
        )
        .unwrap();
    store
        .append(&entry("scry", Kind::Bugfix, "flaky test", &[]), NOW)
        .unwrap();
    let wrong = store
        .append(&entry("scry", Kind::Note, "myth", &[]), NOW)
        .unwrap();
    store.retract(wrong, "not true", NOW).unwrap();
    store
        .append(&entry("other", Kind::Decision, "elsewhere", &["x"]), NOW)
        .unwrap();

    let brief = store.brief("scry", NOW).unwrap();
    assert_eq!(
        brief.active,
        KindCounts {
            decision: 1,
            architecture: 1,
            bugfix: 1,
            note: 0
        }
    );
    let ids: Vec<i64> = brief.durable.iter().map(|e| e.id).collect();
    assert_eq!(ids, vec![new, arch]);
    assert_eq!(brief.tags, vec!["layout", "retry"]);
}

#[test]
fn group_entries_are_in_scope() {
    let mut store = Store::open_in_memory().unwrap();
    store
        .add_members("platform", &["billing".to_owned(), "auth".to_owned()])
        .unwrap();
    store
        .append(
            &NewEntry {
                group: Some("platform".to_owned()),
                ..entry("billing", Kind::Decision, "events are JSON", &[])
            },
            NOW,
        )
        .unwrap();

    let brief = store.brief("auth", NOW).unwrap();
    assert_eq!(brief.groups, vec!["platform"]);
    assert_eq!(brief.active.decision, 1);
    assert_eq!(brief.durable[0].owner, Owner::Group("platform".to_owned()));
}

// Check the text a SessionStart hook passes to the agent.
#[test]
fn cli_text() {
    let tmp = tempfile::tempdir().unwrap();
    let run = |args: &[&str]| {
        let out = Command::new(env!("CARGO_BIN_EXE_munin"))
            .args(args)
            .current_dir(tmp.path())
            .env("HOME", tmp.path())
            .env("MUNIN_DB", tmp.path().join("munin.db"))
            .env("MUNIN_PROJECT", "scry")
            .env_remove("MUNIN_HALF_LIFE_DAYS")
            .env_remove("MUNIN_CONFIG")
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8(out.stdout).unwrap()
    };

    let empty = run(&["brief"]);
    assert!(
        empty.starts_with("Munin has no entries for project scry yet.\n"),
        "{empty}"
    );
    assert!(empty.contains("append_memory"), "{empty}");

    run(&[
        "add",
        "--kind",
        "decision",
        "--summary",
        "Use WAL mode",
        "--tag",
        "sqlite",
    ]);
    let text = run(&["brief"]);
    assert!(
        text.starts_with("Munin holds 1 active entries for project scry: 1 decision,"),
        "{text}"
    );
    assert!(text.contains("Tags in use: sqlite."), "{text}");
    assert!(
        text.contains("#1 decision (project scry): Use WAL mode"),
        "{text}"
    );
}
