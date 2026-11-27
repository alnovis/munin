mod schema;
mod search;

pub use search::{
    Brief, BriefEntry, Card, KindCounts, ListQuery, Listed, Scope, ScopeKind, SearchQuery, TagUse,
};

use std::path::Path;
use std::time::Duration;

use rusqlite::types::{FromSql, FromSqlError, FromSqlResult, ValueRef};
use rusqlite::{Connection, OptionalExtension, params};

use crate::error::{Error, Result};
use crate::model::{
    Entry, Event, EventKind, Group, Kind, NewEntry, Owner, Status, normalize_name,
    normalize_reason, normalize_summary, normalize_tags,
};

const BUSY_TIMEOUT: Duration = Duration::from_millis(5000);

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Options {
    /// Age at which a decaying entry's score is halved.
    pub half_life_days: f64,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            half_life_days: 30.0,
        }
    }
}

/// The memory database. Every write runs in its own transaction.
pub struct Store {
    conn: Connection,
}

impl Store {
    /// Opens the database at `path`, creating the file and its directory if
    /// needed, and migrates it to the current schema.
    pub fn open(path: &Path) -> Result<Self> {
        Self::open_with(path, Options::default())
    }

    pub fn open_with(path: &Path, options: Options) -> Result<Self> {
        if let Some(dir) = path.parent()
            && !dir.as_os_str().is_empty()
        {
            std::fs::create_dir_all(dir)?;
        }
        Self::init(Connection::open(path)?, options)
    }

    pub fn open_in_memory() -> Result<Self> {
        Self::open_in_memory_with(Options::default())
    }

    pub fn open_in_memory_with(options: Options) -> Result<Self> {
        Self::init(Connection::open_in_memory()?, options)
    }

    fn init(mut conn: Connection, options: Options) -> Result<Self> {
        if !(options.half_life_days.is_finite() && options.half_life_days > 0.0) {
            return Err(Error::invalid(
                "half_life_days",
                format!("got {}, expected a positive number", options.half_life_days),
            ));
        }
        conn.pragma_update_and_check(None, "journal_mode", "WAL", |row| row.get::<_, String>(0))?;
        conn.pragma_update(None, "foreign_keys", true)?;
        conn.busy_timeout(BUSY_TIMEOUT)?;
        migrate(&mut conn)?;
        search::register_functions(&conn, options)?;
        Ok(Self { conn })
    }

    pub fn schema_version(&self) -> Result<i64> {
        Ok(self
            .conn
            .pragma_query_value(None, "user_version", |row| row.get(0))?)
    }

    /// Whether any entry was written or retracted at or after `since_ms`.
    pub fn activity_since(&self, since_ms: i64) -> Result<bool> {
        Ok(self.conn.query_row(
            "SELECT EXISTS (SELECT 1 FROM entries WHERE created_at >= ?1)
                 OR EXISTS (SELECT 1 FROM events WHERE at >= ?1)",
            [since_ms],
            |row| row.get(0),
        )?)
    }

    /// Validates and records a new entry, and marks the entry it supersedes,
    /// if any. Returns the new entry's id.
    pub fn append(&mut self, new: &NewEntry, now_ms: i64) -> Result<i64> {
        let project = normalize_name("project", &new.project)?;
        let group = new
            .group
            .as_deref()
            .map(|g| normalize_name("group", g))
            .transpose()?;
        let summary = normalize_summary(&new.summary)?;
        let body = new.body.trim().to_owned();
        let tags = normalize_tags(&new.tags)?;

        let tx = self.conn.transaction()?;

        let owner = match group {
            Some(group) => {
                if !group_exists(&tx, &group)? {
                    return Err(Error::GroupNotFound(group));
                }
                if !is_member(&tx, &group, &project)? {
                    return Err(Error::invalid(
                        "group",
                        format!("project {project} is not a member of group {group}"),
                    ));
                }
                Owner::Group(group)
            }
            None => Owner::Project(project),
        };

        if let Some(old_id) = new.supersedes {
            let (old_owner, old_status) = owner_and_status(&tx, old_id)?;
            if old_owner != owner {
                return Err(Error::Conflict(format!(
                    "entry {old_id} belongs to {old_owner}, the new entry to {owner}; \
                     an entry can only supersede one with the same owner"
                )));
            }
            if old_status != Status::Active {
                return Err(Error::Conflict(format!(
                    "entry {old_id} is {old_status} and cannot be superseded"
                )));
            }
        }

        let (project_col, group_col) = owner_columns(&owner);
        tx.execute(
            "INSERT INTO entries (project, group_name, kind, summary, body, created_at, supersedes)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                project_col,
                group_col,
                new.kind.as_str(),
                summary,
                body,
                now_ms,
                new.supersedes
            ],
        )?;
        let id = tx.last_insert_rowid();

        for tag in &tags {
            tx.execute(
                "INSERT INTO entry_tags (entry_id, tag) VALUES (?1, ?2)",
                params![id, tag],
            )?;
        }
        tx.execute(
            "INSERT INTO entries_fts (rowid, summary, body, tags) VALUES (?1, ?2, ?3, ?4)",
            params![id, summary, body, tags.join(" ")],
        )?;

        if let Some(old_id) = new.supersedes {
            tx.execute(
                "UPDATE entries SET status = 'superseded' WHERE id = ?1",
                [old_id],
            )?;
            tx.execute(
                "INSERT INTO events (entry_id, kind, related_id, at) VALUES (?1, ?2, ?3, ?4)",
                params![old_id, EventKind::Superseded.as_str(), id, now_ms],
            )?;
        }

        tx.commit()?;
        Ok(id)
    }

    /// Marks an active entry as wrong. The entry stays readable.
    pub fn retract(&mut self, id: i64, reason: &str, now_ms: i64) -> Result<()> {
        let reason = normalize_reason(reason)?;
        let tx = self.conn.transaction()?;

        let (_, status) = owner_and_status(&tx, id)?;
        if status != Status::Active {
            return Err(Error::Conflict(format!(
                "entry {id} is {status} and cannot be retracted"
            )));
        }

        tx.execute(
            "UPDATE entries SET status = 'retracted' WHERE id = ?1",
            [id],
        )?;
        tx.execute(
            "INSERT INTO events (entry_id, kind, reason, at) VALUES (?1, ?2, ?3, ?4)",
            params![id, EventKind::Retracted.as_str(), reason, now_ms],
        )?;

        tx.commit()?;
        Ok(())
    }

    pub fn get(&self, id: i64) -> Result<Entry> {
        let mut entry = self
            .conn
            .query_row(
                "SELECT id, project, group_name, kind, summary, body, created_at, status, supersedes
                 FROM entries WHERE id = ?1",
                [id],
                |row| {
                    Ok(Entry {
                        id: row.get(0)?,
                        owner: owner_from_columns(row.get(1)?, row.get(2)?),
                        kind: row.get(3)?,
                        summary: row.get(4)?,
                        body: row.get(5)?,
                        created_at: row.get(6)?,
                        status: row.get(7)?,
                        supersedes: row.get(8)?,
                        superseded_by: None,
                        tags: Vec::new(),
                        events: Vec::new(),
                    })
                },
            )
            .optional()?
            .ok_or(Error::NotFound(id))?;

        entry.tags = self
            .conn
            .prepare("SELECT tag FROM entry_tags WHERE entry_id = ?1 ORDER BY rowid")?
            .query_map([id], |row| row.get(0))?
            .collect::<rusqlite::Result<_>>()?;

        entry.superseded_by = self
            .conn
            .query_row(
                "SELECT id FROM entries WHERE supersedes = ?1",
                [id],
                |row| row.get(0),
            )
            .optional()?;

        entry.events = self
            .conn
            .prepare(
                "SELECT kind, reason, related_id, at FROM events WHERE entry_id = ?1 ORDER BY id",
            )?
            .query_map([id], |row| {
                Ok(Event {
                    kind: row.get(0)?,
                    reason: row.get(1)?,
                    related: row.get(2)?,
                    at: row.get(3)?,
                })
            })?
            .collect::<rusqlite::Result<_>>()?;

        Ok(entry)
    }

    /// Adds projects to a group, creating the group if it does not exist.
    /// Adding a project that is already a member is not an error.
    pub fn add_members(&mut self, group: &str, projects: &[String]) -> Result<()> {
        let group = normalize_name("group", group)?;
        if projects.is_empty() {
            return Err(Error::invalid(
                "projects",
                "at least one project is required",
            ));
        }
        let projects = projects
            .iter()
            .map(|p| normalize_name("project", p))
            .collect::<Result<Vec<_>>>()?;

        let tx = self.conn.transaction()?;
        tx.execute(
            "INSERT INTO groups (name) VALUES (?1) ON CONFLICT (name) DO NOTHING",
            [&group],
        )?;
        for project in &projects {
            tx.execute(
                "INSERT INTO group_members (group_name, project) VALUES (?1, ?2)
                 ON CONFLICT (group_name, project) DO NOTHING",
                params![group, project],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn remove_members(&mut self, group: &str, projects: &[String]) -> Result<()> {
        let group = normalize_name("group", group)?;
        if projects.is_empty() {
            return Err(Error::invalid(
                "projects",
                "at least one project is required",
            ));
        }
        let projects = projects
            .iter()
            .map(|p| normalize_name("project", p))
            .collect::<Result<Vec<_>>>()?;

        let tx = self.conn.transaction()?;
        if !group_exists(&tx, &group)? {
            return Err(Error::GroupNotFound(group));
        }
        for project in &projects {
            let removed = tx.execute(
                "DELETE FROM group_members WHERE group_name = ?1 AND project = ?2",
                params![group, project],
            )?;
            if removed == 0 {
                return Err(Error::invalid(
                    "projects",
                    format!("project {project} is not a member of group {group}"),
                ));
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// Deletes a group and its membership. Refused while the group owns entries.
    pub fn delete_group(&mut self, group: &str) -> Result<()> {
        let group = normalize_name("group", group)?;
        let tx = self.conn.transaction()?;
        if !group_exists(&tx, &group)? {
            return Err(Error::GroupNotFound(group));
        }
        let owned: i64 = tx.query_row(
            "SELECT COUNT(*) FROM entries WHERE group_name = ?1",
            [&group],
            |row| row.get(0),
        )?;
        if owned > 0 {
            return Err(Error::Conflict(format!(
                "group {group} owns {owned} entries and cannot be deleted"
            )));
        }
        tx.execute("DELETE FROM groups WHERE name = ?1", [&group])?;
        tx.commit()?;
        Ok(())
    }

    /// All groups, or only those `project` belongs to; sorted by name.
    pub fn groups(&self, project: Option<&str>) -> Result<Vec<Group>> {
        let project = project.map(|p| normalize_name("project", p)).transpose()?;
        let names: Vec<String> = match &project {
            Some(project) => self
                .conn
                .prepare(
                    "SELECT group_name FROM group_members WHERE project = ?1 ORDER BY group_name",
                )?
                .query_map([project], |row| row.get(0))?
                .collect::<rusqlite::Result<_>>()?,
            None => self
                .conn
                .prepare("SELECT name FROM groups ORDER BY name")?
                .query_map([], |row| row.get(0))?
                .collect::<rusqlite::Result<_>>()?,
        };

        let mut members = self
            .conn
            .prepare("SELECT project FROM group_members WHERE group_name = ?1 ORDER BY project")?;
        names
            .into_iter()
            .map(|name| {
                let projects = members
                    .query_map([&name], |row| row.get(0))?
                    .collect::<rusqlite::Result<_>>()?;
                Ok(Group {
                    name,
                    members: projects,
                })
            })
            .collect()
    }
}

fn migrate(conn: &mut Connection) -> Result<()> {
    let known = schema::MIGRATIONS.len() as i64;
    let current: i64 = conn.pragma_query_value(None, "user_version", |row| row.get(0))?;
    if current > known {
        return Err(Error::NewerSchema {
            found: current,
            known,
        });
    }
    for (index, sql) in schema::MIGRATIONS.iter().enumerate().skip(current as usize) {
        let tx = conn.transaction()?;
        tx.execute_batch(sql)?;
        tx.pragma_update(None, "user_version", index as i64 + 1)?;
        tx.commit()?;
    }
    Ok(())
}

fn group_exists(conn: &Connection, group: &str) -> Result<bool> {
    Ok(conn
        .query_row("SELECT 1 FROM groups WHERE name = ?1", [group], |_| Ok(()))
        .optional()?
        .is_some())
}

fn is_member(conn: &Connection, group: &str, project: &str) -> Result<bool> {
    Ok(conn
        .query_row(
            "SELECT 1 FROM group_members WHERE group_name = ?1 AND project = ?2",
            [group, project],
            |_| Ok(()),
        )
        .optional()?
        .is_some())
}

fn owner_and_status(conn: &Connection, id: i64) -> Result<(Owner, Status)> {
    conn.query_row(
        "SELECT project, group_name, status FROM entries WHERE id = ?1",
        [id],
        |row| Ok((owner_from_columns(row.get(0)?, row.get(1)?), row.get(2)?)),
    )
    .optional()?
    .ok_or(Error::NotFound(id))
}

fn owner_columns(owner: &Owner) -> (Option<&str>, Option<&str>) {
    match owner {
        Owner::Project(name) => (Some(name), None),
        Owner::Group(name) => (None, Some(name)),
    }
}

// The schema guarantees exactly one of the two columns is set.
fn owner_from_columns(project: Option<String>, group: Option<String>) -> Owner {
    match (project, group) {
        (Some(project), None) => Owner::Project(project),
        (None, Some(group)) => Owner::Group(group),
        (project, group) => unreachable!("entry owner columns: {project:?}, {group:?}"),
    }
}

impl FromSql for Kind {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        value
            .as_str()?
            .parse()
            .map_err(|_| FromSqlError::InvalidType)
    }
}

impl FromSql for Status {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        Status::from_db(value.as_str()?).ok_or(FromSqlError::InvalidType)
    }
}

impl FromSql for EventKind {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        EventKind::from_db(value.as_str()?).ok_or(FromSqlError::InvalidType)
    }
}
