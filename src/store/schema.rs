/// Forward-only migrations; migration `i` brings the schema to version `i + 1`.
/// Never edit a migration that has been released: append a new one.
pub(crate) const MIGRATIONS: &[&str] = &[V1];

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
