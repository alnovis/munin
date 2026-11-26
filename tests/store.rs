use munin::{Error, EventKind, Kind, NewEntry, Owner, Status, Store};
use rusqlite::Connection;

const T0: i64 = 1_700_000_000_000;

fn entry(project: &str, summary: &str) -> NewEntry {
    NewEntry {
        project: project.to_owned(),
        group: None,
        kind: Kind::Decision,
        summary: summary.to_owned(),
        body: String::new(),
        tags: Vec::new(),
        supersedes: None,
    }
}

fn names(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| (*s).to_owned()).collect()
}

// Check that a file database and its directory are created and migrated,
// and that reopening it keeps the data.
#[test]
fn open_creates_and_reopens() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("nested").join("munin.db");

    let id = {
        let mut store = Store::open(&path).unwrap();
        assert_eq!(store.schema_version().unwrap(), 1);
        store.append(&entry("scry", "first"), T0).unwrap()
    };

    let store = Store::open(&path).unwrap();
    assert_eq!(store.schema_version().unwrap(), 1);
    assert_eq!(store.get(id).unwrap().summary, "first");
}

// Check that a database written by a newer munin is refused, not downgraded.
#[test]
fn open_refuses_newer_schema() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("munin.db");
    Store::open(&path).unwrap();
    Connection::open(&path)
        .unwrap()
        .pragma_update(None, "user_version", 99)
        .unwrap();

    let err = Store::open(&path).err().unwrap();
    assert!(
        matches!(
            err,
            Error::NewerSchema {
                found: 99,
                known: 1
            }
        ),
        "{err}"
    );
}

// Check that an appended entry reads back normalized.
#[test]
fn append_and_get_round_trip() {
    let mut store = Store::open_in_memory().unwrap();
    let mut new = entry(" Scry ", "  Use WAL mode  ");
    new.kind = Kind::Bugfix;
    new.body = "\n details \n".to_owned();
    new.tags = names(&["SQLite", "wal", "sqlite"]);

    let id = store.append(&new, T0).unwrap();
    let got = store.get(id).unwrap();

    assert_eq!(got.owner, Owner::Project("scry".to_owned()));
    assert_eq!(got.kind, Kind::Bugfix);
    assert_eq!(got.summary, "Use WAL mode");
    assert_eq!(got.body, "details");
    assert_eq!(got.tags, names(&["sqlite", "wal"]));
    assert_eq!(got.created_at, T0);
    assert_eq!(got.status, Status::Active);
    assert_eq!(got.supersedes, None);
    assert_eq!(got.superseded_by, None);
    assert!(got.events.is_empty());
}

#[test]
fn get_missing_entry() {
    let store = Store::open_in_memory().unwrap();
    assert!(matches!(store.get(42), Err(Error::NotFound(42))));
}

// Check that each kind of bad input is rejected with the field it concerns.
#[test]
fn append_rejects_invalid_input() {
    let mut store = Store::open_in_memory().unwrap();
    let cases: Vec<(&str, NewEntry)> = vec![
        ("summary", entry("scry", "   ")),
        ("summary", entry("scry", "two\nlines")),
        ("summary", entry("scry", &"x".repeat(201))),
        ("project", entry("", "ok")),
        ("project", entry("my project", "ok")),
        (
            "tags",
            NewEntry {
                tags: names(&["ok", "not ok"]),
                ..entry("scry", "ok")
            },
        ),
        (
            "tags",
            NewEntry {
                tags: names(&["a", "b", "c", "d", "e", "f", "g"]),
                ..entry("scry", "ok")
            },
        ),
    ];

    for (field, new) in cases {
        match store.append(&new, T0) {
            Err(Error::Invalid { field: got, .. }) => assert_eq!(got, field, "{new:?}"),
            other => panic!("expected invalid {field} for {new:?}, got {other:?}"),
        }
    }
}

#[test]
fn summary_limit_counts_characters_not_bytes() {
    let mut store = Store::open_in_memory().unwrap();
    store
        .append(&entry("scry", &"\u{44f}".repeat(200)), T0)
        .unwrap();
}

#[test]
fn kind_parsing() {
    assert_eq!("Decision".parse::<Kind>().unwrap(), Kind::Decision);
    let err = "adr".parse::<Kind>().unwrap_err();
    assert!(matches!(err, Error::Invalid { field: "kind", .. }));
    assert!(
        err.to_string()
            .contains("decision, architecture, bugfix, note")
    );
}

// Check that superseding links both ends and records an event on the old entry.
#[test]
fn supersede_links_both_ends() {
    let mut store = Store::open_in_memory().unwrap();
    let old = store.append(&entry("scry", "use tokio"), T0).unwrap();
    let new = store
        .append(
            &NewEntry {
                supersedes: Some(old),
                ..entry("scry", "use smol")
            },
            T0 + 1,
        )
        .unwrap();

    let old_entry = store.get(old).unwrap();
    assert_eq!(old_entry.status, Status::Superseded);
    assert_eq!(old_entry.superseded_by, Some(new));
    assert_eq!(old_entry.events.len(), 1);
    assert_eq!(old_entry.events[0].kind, EventKind::Superseded);
    assert_eq!(old_entry.events[0].related, Some(new));
    assert_eq!(old_entry.events[0].at, T0 + 1);

    let new_entry = store.get(new).unwrap();
    assert_eq!(new_entry.status, Status::Active);
    assert_eq!(new_entry.supersedes, Some(old));
}

#[test]
fn supersede_rejects_inactive_foreign_and_missing() {
    let mut store = Store::open_in_memory().unwrap();
    let a = store.append(&entry("scry", "a"), T0).unwrap();
    store
        .append(
            &NewEntry {
                supersedes: Some(a),
                ..entry("scry", "b")
            },
            T0,
        )
        .unwrap();

    let again = store.append(
        &NewEntry {
            supersedes: Some(a),
            ..entry("scry", "c")
        },
        T0,
    );
    assert!(matches!(again, Err(Error::Conflict(_))), "{again:?}");

    let foreign = store.append(&entry("munin", "x"), T0).unwrap();
    let cross = store.append(
        &NewEntry {
            supersedes: Some(foreign),
            ..entry("scry", "d")
        },
        T0,
    );
    assert!(matches!(cross, Err(Error::Conflict(_))), "{cross:?}");

    let missing = store.append(
        &NewEntry {
            supersedes: Some(999),
            ..entry("scry", "e")
        },
        T0,
    );
    assert!(matches!(missing, Err(Error::NotFound(999))), "{missing:?}");
}

// Check that a failed supersede leaves nothing behind.
#[test]
fn failed_append_writes_nothing() {
    let mut store = Store::open_in_memory().unwrap();
    let foreign = store.append(&entry("munin", "x"), T0).unwrap();
    store
        .append(
            &NewEntry {
                supersedes: Some(foreign),
                ..entry("scry", "d")
            },
            T0,
        )
        .unwrap_err();

    assert!(matches!(store.get(foreign + 1), Err(Error::NotFound(_))));
    assert_eq!(store.get(foreign).unwrap().status, Status::Active);
}

#[test]
fn retract_records_reason() {
    let mut store = Store::open_in_memory().unwrap();
    let id = store.append(&entry("scry", "wrong"), T0).unwrap();
    store.retract(id, "  measured, not true  ", T0 + 5).unwrap();

    let got = store.get(id).unwrap();
    assert_eq!(got.status, Status::Retracted);
    assert_eq!(got.events.len(), 1);
    assert_eq!(got.events[0].kind, EventKind::Retracted);
    assert_eq!(got.events[0].reason.as_deref(), Some("measured, not true"));
    assert_eq!(got.events[0].at, T0 + 5);
}

#[test]
fn retract_rejects_empty_reason_twice_and_missing() {
    let mut store = Store::open_in_memory().unwrap();
    let id = store.append(&entry("scry", "wrong"), T0).unwrap();

    assert!(matches!(
        store.retract(id, " ", T0),
        Err(Error::Invalid {
            field: "reason",
            ..
        })
    ));
    store.retract(id, "wrong", T0).unwrap();
    assert!(matches!(
        store.retract(id, "again", T0),
        Err(Error::Conflict(_))
    ));
    assert!(matches!(
        store.retract(999, "x", T0),
        Err(Error::NotFound(999))
    ));
}

// Check that a retracted entry cannot be superseded.
#[test]
fn retracted_entry_cannot_be_superseded() {
    let mut store = Store::open_in_memory().unwrap();
    let id = store.append(&entry("scry", "wrong"), T0).unwrap();
    store.retract(id, "wrong", T0).unwrap();

    let res = store.append(
        &NewEntry {
            supersedes: Some(id),
            ..entry("scry", "fix")
        },
        T0,
    );
    assert!(matches!(res, Err(Error::Conflict(_))), "{res:?}");
}

#[test]
fn group_owned_entry_requires_membership() {
    let mut store = Store::open_in_memory().unwrap();
    let in_group = |project: &str| NewEntry {
        group: Some("platform".to_owned()),
        ..entry(project, "events are JSON")
    };

    assert!(matches!(
        store.append(&in_group("billing"), T0),
        Err(Error::GroupNotFound(_))
    ));

    store
        .add_members("platform", &names(&["billing", "auth"]))
        .unwrap();
    assert!(matches!(
        store.append(&in_group("scry"), T0),
        Err(Error::Invalid { field: "group", .. })
    ));

    let id = store.append(&in_group("billing"), T0).unwrap();
    assert_eq!(
        store.get(id).unwrap().owner,
        Owner::Group("platform".to_owned())
    );
}

// Check that a group entry can be superseded from any member project.
#[test]
fn group_entry_superseded_from_another_member() {
    let mut store = Store::open_in_memory().unwrap();
    store
        .add_members("platform", &names(&["billing", "auth"]))
        .unwrap();
    let old = store
        .append(
            &NewEntry {
                group: Some("platform".to_owned()),
                ..entry("billing", "v1")
            },
            T0,
        )
        .unwrap();
    store
        .append(
            &NewEntry {
                group: Some("platform".to_owned()),
                supersedes: Some(old),
                ..entry("auth", "v2")
            },
            T0,
        )
        .unwrap();

    assert_eq!(store.get(old).unwrap().status, Status::Superseded);
}

#[test]
fn group_membership() {
    let mut store = Store::open_in_memory().unwrap();
    store
        .add_members("platform", &names(&["billing", "auth"]))
        .unwrap();
    store.add_members("platform", &names(&["auth"])).unwrap();
    store
        .add_members("rust", &names(&["auth", "munin"]))
        .unwrap();

    let all = store.groups(None).unwrap();
    assert_eq!(all.len(), 2);
    assert_eq!(all[0].name, "platform");
    assert_eq!(all[0].members, names(&["auth", "billing"]));

    let of_auth: Vec<_> = store
        .groups(Some("auth"))
        .unwrap()
        .into_iter()
        .map(|g| g.name)
        .collect();
    assert_eq!(of_auth, names(&["platform", "rust"]));
    assert!(store.groups(Some("scry")).unwrap().is_empty());

    store.remove_members("platform", &names(&["auth"])).unwrap();
    assert_eq!(store.groups(Some("auth")).unwrap().len(), 1);
    assert!(matches!(
        store.remove_members("platform", &names(&["auth"])),
        Err(Error::Invalid {
            field: "projects",
            ..
        })
    ));
    assert!(matches!(
        store.remove_members("nope", &names(&["auth"])),
        Err(Error::GroupNotFound(_))
    ));
}

#[test]
fn group_with_entries_cannot_be_deleted() {
    let mut store = Store::open_in_memory().unwrap();
    store.add_members("platform", &names(&["billing"])).unwrap();
    store.add_members("empty", &names(&["billing"])).unwrap();
    store
        .append(
            &NewEntry {
                group: Some("platform".to_owned()),
                ..entry("billing", "x")
            },
            T0,
        )
        .unwrap();

    assert!(matches!(
        store.delete_group("platform"),
        Err(Error::Conflict(_))
    ));
    store.delete_group("empty").unwrap();
    assert!(
        store
            .groups(Some("billing"))
            .unwrap()
            .iter()
            .all(|g| g.name != "empty")
    );
    assert!(matches!(
        store.delete_group("empty"),
        Err(Error::GroupNotFound(_))
    ));
}

// Check that the database itself refuses to rewrite or delete history,
// whatever client writes to it.
#[test]
fn database_enforces_immutability() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("munin.db");
    let id = {
        let mut store = Store::open(&path).unwrap();
        let id = store
            .append(
                &NewEntry {
                    tags: names(&["t"]),
                    ..entry("scry", "keep")
                },
                T0,
            )
            .unwrap();
        store.retract(id, "wrong", T0).unwrap();
        id
    };

    let conn = Connection::open(&path).unwrap();
    let refused = [
        "UPDATE entries SET summary = 'changed'",
        "UPDATE entries SET status = 'active'",
        "DELETE FROM entries",
        "DELETE FROM entry_tags",
        "UPDATE events SET reason = 'changed'",
        "DELETE FROM events",
    ];
    for sql in refused {
        assert!(conn.execute(sql, []).is_err(), "{sql} should fail");
    }

    let store = Store::open(&path).unwrap();
    let got = store.get(id).unwrap();
    assert_eq!(got.summary, "keep");
    assert_eq!(got.status, Status::Retracted);
    assert_eq!(got.tags, names(&["t"]));
}

// Check that FTS5 is compiled in and keeps code-like tokens whole.
#[test]
fn fts_index_is_populated() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("munin.db");
    let id = {
        let mut store = Store::open(&path).unwrap();
        store
            .append(
                &NewEntry {
                    body: "see supersedes-51 and @scope/pkg".to_owned(),
                    tags: names(&["sqlite"]),
                    ..entry("scry", "Configured WAL")
                },
                T0,
            )
            .unwrap()
    };

    let conn = Connection::open(&path).unwrap();
    let hit = |query: &str| -> Option<i64> {
        conn.query_row(
            "SELECT rowid FROM entries_fts WHERE entries_fts MATCH ?1",
            [query],
            |row| row.get(0),
        )
        .ok()
    };
    assert_eq!(hit("\"supersedes-51\""), Some(id));
    assert_eq!(hit("\"@scope/pkg\""), Some(id));
    assert_eq!(hit("wal"), Some(id));
    assert_eq!(hit("tags:sqlite"), Some(id));
    assert_eq!(hit("\"supersedes\""), None);
}
