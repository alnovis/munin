/// Forward-only migrations; migration `i` brings the schema to version `i + 1`.
/// Never edit a migration that has been released: append a new one.
/// They run with foreign keys off, so a migration may rebuild a table.
pub(crate) const MIGRATIONS: &[&str] = &[V1, V2];

const V1: &str = r#"
CREATE TABLE groups (
    name TEXT PRIMARY KEY
) STRICT;

CREATE TABLE group_members (
    group_name TEXT NOT NULL REFERENCES groups(name) ON DELETE CASCADE,
    project TEXT NOT NULL,
    PRIMARY KEY (group_name, project)
) STRICT;

CREATE INDEX group_members_project ON group_members(project);

CREATE TABLE entries (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    project TEXT,
    group_name TEXT REFERENCES groups(name),
    kind TEXT NOT NULL CHECK (kind IN ('decision', 'architecture', 'bugfix', 'note')),
    summary TEXT NOT NULL,
    body TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    supersedes INTEGER UNIQUE REFERENCES entries(id),
    status TEXT NOT NULL DEFAULT 'active'
        CHECK (status IN ('active', 'superseded', 'retracted')),
    CHECK ((project IS NULL) != (group_name IS NULL))
) STRICT;

CREATE INDEX entries_project ON entries(project);
CREATE INDEX entries_group ON entries(group_name);

CREATE TABLE entry_tags (
    entry_id INTEGER NOT NULL REFERENCES entries(id),
    tag TEXT NOT NULL,
    PRIMARY KEY (entry_id, tag)
) STRICT;

CREATE INDEX entry_tags_tag ON entry_tags(tag);

CREATE TABLE events (
    id INTEGER PRIMARY KEY,
    entry_id INTEGER NOT NULL REFERENCES entries(id),
    kind TEXT NOT NULL CHECK (kind IN ('superseded', 'retracted')),
    reason TEXT,
    related_id INTEGER REFERENCES entries(id),
    at INTEGER NOT NULL
) STRICT;

CREATE INDEX events_entry ON events(entry_id);

CREATE VIRTUAL TABLE entries_fts USING fts5(
    summary,
    body,
    tags,
    tokenize = "unicode61 tokenchars '-_/@'"
);

CREATE TRIGGER entries_no_delete BEFORE DELETE ON entries
BEGIN
    SELECT RAISE(ABORT, 'entries are never deleted');
END;

CREATE TRIGGER entries_immutable
BEFORE UPDATE OF id, project, group_name, kind, summary, body, created_at, supersedes
ON entries
BEGIN
    SELECT RAISE(ABORT, 'entry content is immutable');
END;

CREATE TRIGGER entries_status_forward
BEFORE UPDATE OF status ON entries
WHEN OLD.status != 'active' OR NEW.status = 'active'
BEGIN
    SELECT RAISE(ABORT, 'entry status only moves away from active');
END;

CREATE TRIGGER entry_tags_no_update BEFORE UPDATE ON entry_tags
BEGIN
    SELECT RAISE(ABORT, 'entry tags are immutable');
END;

CREATE TRIGGER entry_tags_no_delete BEFORE DELETE ON entry_tags
BEGIN
    SELECT RAISE(ABORT, 'entry tags are immutable');
END;

CREATE TRIGGER events_no_update BEFORE UPDATE ON events
BEGIN
    SELECT RAISE(ABORT, 'events are append-only');
END;

CREATE TRIGGER events_no_delete BEFORE DELETE ON events
BEGIN
    SELECT RAISE(ABORT, 'events are append-only');
END;
"#;

// Kind `howto`. SQLite cannot change a CHECK constraint in place, so
// `entries` is rebuilt; ids, the AUTOINCREMENT counter, indexes and triggers
// carry over.
const V2: &str = r#"
CREATE TABLE entries_v2 (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    project TEXT,
    group_name TEXT REFERENCES groups(name),
    kind TEXT NOT NULL
        CHECK (kind IN ('decision', 'architecture', 'bugfix', 'note', 'howto')),
    summary TEXT NOT NULL,
    body TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    supersedes INTEGER UNIQUE REFERENCES entries(id),
    status TEXT NOT NULL DEFAULT 'active'
        CHECK (status IN ('active', 'superseded', 'retracted')),
    CHECK ((project IS NULL) != (group_name IS NULL))
) STRICT;

INSERT INTO entries_v2 (id, project, group_name, kind, summary, body, created_at, supersedes, status)
SELECT id, project, group_name, kind, summary, body, created_at, supersedes, status
FROM entries ORDER BY id;

UPDATE sqlite_sequence
SET seq = (SELECT seq FROM sqlite_sequence WHERE name = 'entries')
WHERE name = 'entries_v2';

DROP TABLE entries;
ALTER TABLE entries_v2 RENAME TO entries;

CREATE INDEX entries_project ON entries(project);
CREATE INDEX entries_group ON entries(group_name);

CREATE TRIGGER entries_no_delete BEFORE DELETE ON entries
BEGIN
    SELECT RAISE(ABORT, 'entries are never deleted');
END;

CREATE TRIGGER entries_immutable
BEFORE UPDATE OF id, project, group_name, kind, summary, body, created_at, supersedes
ON entries
BEGIN
    SELECT RAISE(ABORT, 'entry content is immutable');
END;

CREATE TRIGGER entries_status_forward
BEFORE UPDATE OF status ON entries
WHEN OLD.status != 'active' OR NEW.status = 'active'
BEGIN
    SELECT RAISE(ABORT, 'entry status only moves away from active');
END;
"#;

#[cfg(test)]
mod tests {
    use rusqlite::Connection;

    use super::MIGRATIONS;
    use crate::model::{EventKind, Kind, NewEntry, Owner, Status};
    use crate::store::Store;

    const T0: i64 = 1_700_000_000_000;

    // A schema 1 database with a group entry, a superseded entry, a retracted
    // entry, tags and events.
    fn v1_database(path: &std::path::Path) {
        let conn = Connection::open(path).unwrap();
        conn.execute_batch(MIGRATIONS[0]).unwrap();
        conn.pragma_update(None, "user_version", 1).unwrap();
        conn.execute_batch(&format!(
            "INSERT INTO groups (name) VALUES ('platform');
             INSERT INTO group_members VALUES ('platform', 'billing');
             INSERT INTO entries (project, group_name, kind, summary, body, created_at)
                 VALUES ('scry', NULL, 'decision', 'use wal', '', {T0});
             INSERT INTO entries (project, group_name, kind, summary, body, created_at, supersedes)
                 VALUES ('scry', NULL, 'decision', 'use wal with timeout', 'body', {T0} + 1, 1);
             UPDATE entries SET status = 'superseded' WHERE id = 1;
             INSERT INTO events (entry_id, kind, related_id, at) VALUES (1, 'superseded', 2, {T0} + 1);
             INSERT INTO entries (project, group_name, kind, summary, body, created_at)
                 VALUES (NULL, 'platform', 'note', 'myth', '', {T0} + 2);
             UPDATE entries SET status = 'retracted' WHERE id = 3;
             INSERT INTO events (entry_id, kind, reason, at) VALUES (3, 'retracted', 'wrong', {T0} + 3);
             INSERT INTO entry_tags VALUES (2, 'sqlite'), (2, 'wal');"
        ))
        .unwrap();
    }

    // Check that schema 1 migrates with every entry, link, tag and event in
    // place, and that the rebuilt table still refuses edits and deletes.
    #[test]
    fn v2_keeps_data_and_guards() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("munin.db");
        v1_database(&path);

        let mut store = Store::open(&path).unwrap();
        assert_eq!(store.schema_version().unwrap(), 2);

        let old = store.get(1).unwrap();
        assert_eq!(old.status, Status::Superseded);
        assert_eq!(old.superseded_by, Some(2));
        assert_eq!(old.events[0].kind, EventKind::Superseded);
        let new = store.get(2).unwrap();
        assert_eq!(new.supersedes, Some(1));
        assert_eq!(new.tags, vec!["sqlite", "wal"]);
        let retracted = store.get(3).unwrap();
        assert_eq!(retracted.owner, Owner::Group("platform".to_owned()));
        assert_eq!(retracted.status, Status::Retracted);

        for sql in [
            "DELETE FROM entries WHERE id = 1",
            "UPDATE entries SET summary = 'x' WHERE id = 2",
            "UPDATE entries SET status = 'active' WHERE id = 1",
        ] {
            assert!(store.conn.execute(sql, []).is_err(), "{sql}");
        }
        let fk: bool = store
            .conn
            .pragma_query_value(None, "foreign_keys", |row| row.get(0))
            .unwrap();
        assert!(fk, "foreign keys are back on after the migration");

        let id = store
            .append(
                &NewEntry {
                    project: "scry".to_owned(),
                    group: None,
                    kind: Kind::Howto,
                    summary: "cut a release".to_owned(),
                    body: String::new(),
                    tags: Vec::new(),
                    supersedes: None,
                },
                T0 + 4,
            )
            .unwrap();
        assert_eq!(id, 4, "ids continue after the copied rows");
        assert_eq!(store.get(id).unwrap().kind, Kind::Howto);
    }
}
