use rusqlite::functions::FunctionFlags;
use rusqlite::{Connection, OptionalExtension, named_params};
use serde::Serialize;

use super::{Options, Store, group_exists, owner_from_columns};
use crate::error::{Error, Result};
use crate::model::{Kind, Owner, Status, normalize_name};

pub const DEFAULT_LIMIT: usize = 5;
pub const MAX_LIMIT: usize = 15;
pub const DEFAULT_TAG_LIMIT: usize = 40;
pub const MAX_TAG_LIMIT: usize = 200;
/// Tags used within this window rank above tags that were only used before it.
pub const RECENT_TAG_DAYS: i64 = 60;

const MS_PER_DAY: i64 = 86_400_000;

/// Which entries a search or a tag listing sees, relative to the current project.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Scope {
    /// Entries owned by the current project.
    Project,
    /// `None`: entries of the current project, of every project that shares a
    /// group with it, and of those groups. `Some(name)`: entries of that group
    /// and of its member projects.
    Group(Option<String>),
    All,
}

/// The scope a caller names, before a group name is attached to it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScopeKind {
    Project,
    Group,
    All,
}

impl Scope {
    /// Combines a scope kind and a group name as given by a caller. A group
    /// name alone means the group scope; with any other kind it is an error.
    pub fn from_parts(kind: Option<ScopeKind>, group: Option<String>) -> Result<Option<Scope>> {
        Ok(match (kind, group) {
            (None, None) => None,
            (None | Some(ScopeKind::Group), Some(group)) => Some(Scope::Group(Some(group))),
            (Some(ScopeKind::Group), None) => Some(Scope::Group(None)),
            (Some(ScopeKind::Project), None) => Some(Scope::Project),
            (Some(ScopeKind::All), None) => Some(Scope::All),
            (Some(ScopeKind::Project | ScopeKind::All), Some(_)) => {
                return Err(Error::invalid(
                    "group",
                    "a group can only be given with scope group",
                ));
            }
        })
    }
}

#[derive(Debug, Clone)]
pub struct SearchQuery {
    pub text: String,
    /// The current project, already resolved by the caller.
    pub project: String,
    /// `None` picks the default: `Group(None)` when the project belongs to a
    /// group, `Project` otherwise.
    pub scope: Option<Scope>,
    pub include_inactive: bool,
    /// Disables decay, so old entries rank on text relevance alone.
    pub deep_history: bool,
    pub limit: Option<usize>,
}

/// A compact search hit; the full entry is fetched with `Store::get`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Card {
    pub id: i64,
    pub owner: Owner,
    pub kind: Kind,
    pub summary: String,
    pub tags: Vec<String>,
    pub age_days: i64,
    pub score: f64,
    pub status: Status,
    pub superseded_by: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TagUse {
    pub tag: String,
    /// Active entries using the tag within the last `RECENT_TAG_DAYS`.
    pub recent: i64,
    pub total: i64,
}

/// What memory holds for a project, in its default scope: enough for an
/// agent to know that searching is worth it and with which words.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Brief {
    pub project: String,
    /// Groups the project belongs to; their entries are in scope.
    pub groups: Vec<String>,
    pub active: KindCounts,
    pub tags: Vec<String>,
    /// Newest active `decision` and `architecture` entries.
    pub durable: Vec<BriefEntry>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct KindCounts {
    pub decision: i64,
    pub architecture: i64,
    pub bugfix: i64,
    pub note: i64,
}

impl KindCounts {
    pub fn total(&self) -> i64 {
        self.decision + self.architecture + self.bugfix + self.note
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BriefEntry {
    pub id: i64,
    pub kind: Kind,
    pub owner: Owner,
    pub summary: String,
}

pub const BRIEF_TAGS: usize = 12;
pub const BRIEF_DURABLE: i64 = 5;

// Codes bound to :scope in SCOPE_FILTER.
const SCOPE_PROJECT: i64 = 0;
const SCOPE_OWN_GROUPS: i64 = 1;
const SCOPE_ONE_GROUP: i64 = 2;
const SCOPE_ALL: i64 = 3;

const SCOPE_FILTER: &str = "
    CASE :scope
        WHEN 0 THEN e.project = :project
        WHEN 1 THEN e.project = :project
            OR e.project IN (
                SELECT peer.project FROM group_members own
                JOIN group_members peer ON peer.group_name = own.group_name
                WHERE own.project = :project)
            OR e.group_name IN (SELECT group_name FROM group_members WHERE project = :project)
        WHEN 2 THEN e.project IN (SELECT project FROM group_members WHERE group_name = :group)
            OR e.group_name = :group
        ELSE 1
    END";

impl Store {
    /// Full-text search ranked by relevance times decay. Ranking covers every
    /// match before the limit is applied.
    pub fn search(&self, query: &SearchQuery, now_ms: i64) -> Result<Vec<Card>> {
        let fts = fts_query(&query.text)?;
        let project = normalize_name("project", &query.project)?;
        let (scope, group) = self.resolve_scope(&project, query.scope.as_ref())?;
        let limit = check_limit(query.limit, DEFAULT_LIMIT, MAX_LIMIT)?;

        let sql = format!(
            "SELECT e.id, e.project, e.group_name, e.kind, e.summary, e.created_at, e.status,
                    (SELECT s.id FROM entries s WHERE s.supersedes = e.id),
                    -bm25(entries_fts) * munin_decay(e.kind, :now - e.created_at, :deep) AS score
             FROM entries_fts
             JOIN entries e ON e.id = entries_fts.rowid
             WHERE entries_fts MATCH :query
               AND (:inactive OR e.status = 'active')
               AND ({SCOPE_FILTER})
             ORDER BY score DESC, e.id DESC
             LIMIT :limit"
        );
        let mut cards = self
            .conn
            .prepare(&sql)?
            .query_map(
                named_params! {
                    ":query": fts,
                    ":now": now_ms,
                    ":deep": query.deep_history,
                    ":inactive": query.include_inactive,
                    ":scope": scope,
                    ":project": project,
                    ":group": group,
                    ":limit": limit as i64,
                },
                |row| {
                    let created_at: i64 = row.get(5)?;
                    Ok(Card {
                        id: row.get(0)?,
                        owner: owner_from_columns(row.get(1)?, row.get(2)?),
                        kind: row.get(3)?,
                        summary: row.get(4)?,
                        tags: Vec::new(),
                        age_days: (now_ms - created_at).max(0) / MS_PER_DAY,
                        score: row.get(8)?,
                        status: row.get(6)?,
                        superseded_by: row.get(7)?,
                    })
                },
            )?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        let mut tags = self
            .conn
            .prepare("SELECT tag FROM entry_tags WHERE entry_id = ?1 ORDER BY rowid")?;
        for card in &mut cards {
            card.tags = tags
                .query_map([card.id], |row| row.get(0))?
                .collect::<rusqlite::Result<_>>()?;
        }
        Ok(cards)
    }

    /// Tags of active entries in scope, most recently used first, so agents
    /// reuse existing tags instead of inventing synonyms.
    pub fn tags(
        &self,
        project: &str,
        scope: Option<&Scope>,
        limit: Option<usize>,
        now_ms: i64,
    ) -> Result<Vec<TagUse>> {
        let project = normalize_name("project", project)?;
        let (scope, group) = self.resolve_scope(&project, scope)?;
        let limit = check_limit(limit, DEFAULT_TAG_LIMIT, MAX_TAG_LIMIT)?;

        let sql = format!(
            "SELECT t.tag, SUM(e.created_at >= :since) AS recent, COUNT(*) AS total
             FROM entry_tags t
             JOIN entries e ON e.id = t.entry_id
             WHERE e.status = 'active'
               AND ({SCOPE_FILTER})
             GROUP BY t.tag
             ORDER BY recent DESC, total DESC, t.tag
             LIMIT :limit"
        );
        let tags = self
            .conn
            .prepare(&sql)?
            .query_map(
                named_params! {
                    ":since": now_ms - RECENT_TAG_DAYS * MS_PER_DAY,
                    ":scope": scope,
                    ":project": project,
                    ":group": group,
                    ":limit": limit as i64,
                },
                |row| {
                    Ok(TagUse {
                        tag: row.get(0)?,
                        recent: row.get(1)?,
                        total: row.get(2)?,
                    })
                },
            )?
            .collect::<rusqlite::Result<_>>()?;
        Ok(tags)
    }

    /// Summary of the active entries in the project's default scope.
    pub fn brief(&self, project: &str, now_ms: i64) -> Result<Brief> {
        let project = normalize_name("project", project)?;
        let (scope, group) = self.resolve_scope(&project, None)?;

        let mut active = KindCounts::default();
        let sql = format!(
            "SELECT e.kind, COUNT(*) FROM entries e
             WHERE e.status = 'active' AND ({SCOPE_FILTER})
             GROUP BY e.kind"
        );
        let counts = self
            .conn
            .prepare(&sql)?
            .query_map(
                named_params! { ":scope": scope, ":project": project, ":group": group },
                |row| Ok((row.get::<_, Kind>(0)?, row.get::<_, i64>(1)?)),
            )?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        for (kind, n) in counts {
            match kind {
                Kind::Decision => active.decision = n,
                Kind::Architecture => active.architecture = n,
                Kind::Bugfix => active.bugfix = n,
                Kind::Note => active.note = n,
            }
        }

        let sql = format!(
            "SELECT e.id, e.project, e.group_name, e.kind, e.summary FROM entries e
             WHERE e.status = 'active' AND e.kind IN ('decision', 'architecture')
               AND ({SCOPE_FILTER})
             ORDER BY e.created_at DESC, e.id DESC
             LIMIT :limit"
        );
        let durable = self
            .conn
            .prepare(&sql)?
            .query_map(
                named_params! {
                    ":scope": scope,
                    ":project": project,
                    ":group": group,
                    ":limit": BRIEF_DURABLE,
                },
                |row| {
                    Ok(BriefEntry {
                        id: row.get(0)?,
                        owner: owner_from_columns(row.get(1)?, row.get(2)?),
                        kind: row.get(3)?,
                        summary: row.get(4)?,
                    })
                },
            )?
            .collect::<rusqlite::Result<_>>()?;

        let tags = self
            .tags(&project, None, Some(BRIEF_TAGS), now_ms)?
            .into_iter()
            .map(|t| t.tag)
            .collect();
        let groups = self
            .groups(Some(&project))?
            .into_iter()
            .map(|g| g.name)
            .collect();

        Ok(Brief {
            project,
            groups,
            active,
            tags,
            durable,
        })
    }

    fn resolve_scope(&self, project: &str, scope: Option<&Scope>) -> Result<(i64, Option<String>)> {
        Ok(match scope {
            None => {
                if in_any_group(&self.conn, project)? {
                    (SCOPE_OWN_GROUPS, None)
                } else {
                    (SCOPE_PROJECT, None)
                }
            }
            Some(Scope::Project) => (SCOPE_PROJECT, None),
            Some(Scope::Group(None)) => (SCOPE_OWN_GROUPS, None),
            Some(Scope::Group(Some(name))) => {
                let name = normalize_name("group", name)?;
                if !group_exists(&self.conn, &name)? {
                    return Err(Error::GroupNotFound(name));
                }
                (SCOPE_ONE_GROUP, Some(name))
            }
            Some(Scope::All) => (SCOPE_ALL, None),
        })
    }
}

pub(super) fn register_functions(conn: &Connection, options: Options) -> Result<()> {
    let half_life_days = options.half_life_days;
    conn.create_scalar_function(
        "munin_decay",
        3,
        FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC,
        move |ctx| {
            let kind: Kind = ctx.get(0)?;
            let age_ms: i64 = ctx.get(1)?;
            let deep_history: bool = ctx.get(2)?;
            Ok(decay(kind, age_ms, half_life_days, deep_history))
        },
    )?;
    Ok(())
}

/// Score multiplier: 1 for kinds that do not decay, halved every
/// `half_life_days` for those that do.
pub(crate) fn decay(kind: Kind, age_ms: i64, half_life_days: f64, deep_history: bool) -> f64 {
    if deep_history || !kind.decays() {
        return 1.0;
    }
    let age_days = age_ms.max(0) as f64 / MS_PER_DAY as f64;
    0.5_f64.powf(age_days / half_life_days)
}

/// Turns free text into an FTS5 query: each word or `"quoted phrase"` becomes
/// a phrase, and phrases are OR-ed. FTS5 operators in the text lose their
/// meaning, so any input is a valid query.
pub(crate) fn fts_query(text: &str) -> Result<String> {
    let mut phrases = Vec::new();
    let mut rest = text;
    loop {
        rest = rest.trim_start();
        if rest.is_empty() {
            break;
        }
        let term;
        if let Some(after) = rest.strip_prefix('"') {
            match after.find('"') {
                Some(end) => {
                    term = &after[..end];
                    rest = &after[end + 1..];
                }
                None => {
                    term = after;
                    rest = "";
                }
            }
        } else {
            let end = rest
                .find(|c: char| c.is_whitespace() || c == '"')
                .unwrap_or(rest.len());
            term = &rest[..end];
            rest = &rest[end..];
        }
        if term.chars().any(char::is_alphanumeric) {
            phrases.push(format!("\"{}\"", term.trim()));
        }
    }
    if phrases.is_empty() {
        return Err(Error::invalid("query", "contains no words to search for"));
    }
    Ok(phrases.join(" OR "))
}

fn check_limit(limit: Option<usize>, default: usize, max: usize) -> Result<usize> {
    match limit {
        None => Ok(default),
        Some(n) if (1..=max).contains(&n) => Ok(n),
        Some(n) => Err(Error::invalid(
            "limit",
            format!("got {n}, expected 1 to {max}"),
        )),
    }
}

fn in_any_group(conn: &Connection, project: &str) -> Result<bool> {
    Ok(conn
        .query_row(
            "SELECT 1 FROM group_members WHERE project = ?1 LIMIT 1",
            [project],
            |_| Ok(()),
        )
        .optional()?
        .is_some())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fts_query_quotes_words_and_keeps_phrases() {
        assert_eq!(fts_query("wal mode").unwrap(), r#""wal" OR "mode""#);
        assert_eq!(
            fts_query(r#"tokio "busy timeout" x"#).unwrap(),
            r#""tokio" OR "busy timeout" OR "x""#
        );
    }

    #[test]
    fn fts_query_neutralizes_operators() {
        assert_eq!(
            fts_query("a AND NOT b*").unwrap(),
            r#""a" OR "AND" OR "NOT" OR "b*""#
        );
        assert_eq!(
            fts_query(r#"open "unterminated"#).unwrap(),
            r#""open" OR "unterminated""#
        );
        assert_eq!(fts_query(r#"x"y"#).unwrap(), r#""x" OR "y""#);
    }

    #[test]
    fn fts_query_drops_terms_without_words() {
        assert_eq!(fts_query("-- wal ...").unwrap(), r#""wal""#);
        assert!(matches!(
            fts_query("  -- \"\" ... "),
            Err(Error::Invalid { field: "query", .. })
        ));
    }

    #[test]
    fn decay_halves_per_half_life() {
        let day = MS_PER_DAY;
        assert_eq!(decay(Kind::Note, 0, 30.0, false), 1.0);
        assert!((decay(Kind::Note, 30 * day, 30.0, false) - 0.5).abs() < 1e-12);
        assert!((decay(Kind::Bugfix, 60 * day, 30.0, false) - 0.25).abs() < 1e-12);
        assert_eq!(decay(Kind::Note, -day, 30.0, false), 1.0);
    }

    #[test]
    fn decay_spares_durable_kinds_and_deep_history() {
        let old = 3650 * MS_PER_DAY;
        assert_eq!(decay(Kind::Decision, old, 30.0, false), 1.0);
        assert_eq!(decay(Kind::Architecture, old, 30.0, false), 1.0);
        assert_eq!(decay(Kind::Note, old, 30.0, true), 1.0);
    }
}
