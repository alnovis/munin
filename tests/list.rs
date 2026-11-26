use std::process::Command;

use munin::{Error, Kind, ListQuery, NewEntry, Scope, Status, Store};

const DAY: i64 = 86_400_000;
const NOW: i64 = 1_800_000_000_000;

fn entry(project: &str, kind: Kind, summary: &str) -> NewEntry {
    NewEntry {
        project: project.to_owned(),
        group: None,
        kind,
        summary: summary.to_owned(),
        body: String::new(),
        tags: Vec::new(),
        supersedes: None,
    }
}

fn query(project: &str) -> ListQuery {
    ListQuery {
        project: project.to_owned(),
        scope: None,
        kind: None,
        include_inactive: false,
        limit: None,
    }
}

fn ids(store: &Store, query: &ListQuery) -> Vec<i64> {
    store
        .list(query, NOW)
        .unwrap()
        .iter()
        .map(|e| e.id)
        .collect()
}

struct Data {
    store: Store,
    old: i64,
    arch: i64,
    new: i64,
    bug: i64,
    wrong: i64,
    other: i64,
}

// scry: a superseded decision, its replacement, an architecture entry, a
// bugfix and a retracted note; one decision in another project.
fn data() -> Data {
    let mut store = Store::open_in_memory().unwrap();
    let old = store
        .append(
            &entry("scry", Kind::Decision, "retry three times"),
            NOW - 5 * DAY,
        )
        .unwrap();
    let arch = store
        .append(
            &entry("scry", Kind::Architecture, "two crates"),
            NOW - 4 * DAY,
        )
        .unwrap();
    let new = store
        .append(
            &NewEntry {
                supersedes: Some(old),
                ..entry("scry", Kind::Decision, "retry five times")
            },
            NOW - 3 * DAY,
        )
        .unwrap();
    let bug = store
        .append(&entry("scry", Kind::Bugfix, "flaky test"), NOW - 2 * DAY)
        .unwrap();
    let wrong = store
        .append(&entry("scry", Kind::Note, "myth"), NOW - DAY)
        .unwrap();
    store.retract(wrong, "not true", NOW).unwrap();
    let other = store
        .append(&entry("munin", Kind::Decision, "elsewhere"), NOW)
        .unwrap();
    Data {
        store,
        old,
        arch,
        new,
        bug,
        wrong,
        other,
    }
}

#[test]
fn active_entries_of_the_project_newest_first() {
    let d = data();
    assert_eq!(ids(&d.store, &query("scry")), vec![d.bug, d.new, d.arch]);
}

#[test]
fn inactive_kind_and_scope() {
    let d = data();
    let all_scry = ListQuery {
        include_inactive: true,
        ..query("scry")
    };
    assert_eq!(
        ids(&d.store, &all_scry),
        vec![d.wrong, d.bug, d.new, d.arch, d.old]
    );

    let decisions = ListQuery {
        kind: Some(Kind::Decision),
        include_inactive: true,
        ..query("scry")
    };
    assert_eq!(ids(&d.store, &decisions), vec![d.new, d.old]);

    let everything = ListQuery {
        scope: Some(Scope::All),
        ..query("scry")
    };
    assert_eq!(
        ids(&d.store, &everything),
        vec![d.other, d.bug, d.new, d.arch]
    );
}

#[test]
fn listed_fields_and_limit() {
    let d = data();
    let listed = d
        .store
        .list(
            &ListQuery {
                include_inactive: true,
                limit: Some(2),
                ..query("scry")
            },
            NOW,
        )
        .unwrap();
    assert_eq!(listed.len(), 2);
    assert_eq!(listed[0].status, Status::Retracted);
    assert_eq!(listed[1].age_days, 2);

    let old = d
        .store
        .list(
            &ListQuery {
                include_inactive: true,
                ..query("scry")
            },
            NOW,
        )
        .unwrap()
        .into_iter()
        .find(|e| e.id == d.old)
        .unwrap();
    assert_eq!(old.superseded_by, Some(d.new));

    assert!(matches!(
        d.store.list(
            &ListQuery {
                limit: Some(0),
                ..query("scry")
            },
            NOW
        ),
        Err(Error::Invalid { field: "limit", .. })
    ));
}

#[test]
fn cli_lists_everything() {
    let tmp = tempfile::tempdir().unwrap();
    let run = |args: &[&str]| {
        let out = Command::new(env!("CARGO_BIN_EXE_munin"))
            .args(args)
            .current_dir(tmp.path())
            .env("HOME", tmp.path())
            .env("MUNIN_DB", tmp.path().join("munin.db"))
            .env("MUNIN_PROJECT", "scry")
            .env_remove("MUNIN_HALF_LIFE_DAYS")
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8(out.stdout).unwrap()
    };

    assert_eq!(run(&["list"]), "no entries\n");
    run(&["add", "--kind", "note", "--summary", "first"]);
    run(&[
        "add",
        "--kind",
        "decision",
        "--summary",
        "second",
        "--project",
        "munin",
    ]);

    let here = run(&["list"]);
    assert!(here.contains("first") && !here.contains("second"), "{here}");
    let all = run(&["list", "--scope", "all"]);
    assert!(all.contains("first") && all.contains("second"), "{all}");
    let decisions = run(&["list", "--scope", "all", "--kind", "decision"]);
    assert!(
        !decisions.contains("first") && decisions.contains("second"),
        "{decisions}"
    );
}
