use std::fmt;
use std::str::FromStr;

use serde::Serialize;

use crate::error::{Error, Result};

pub const MAX_SUMMARY_CHARS: usize = 200;
pub const MAX_TAGS: usize = 6;
pub const MAX_NAME_CHARS: usize = 100;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Decision,
    Architecture,
    Bugfix,
    Note,
    Howto,
}

impl Kind {
    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Decision => "decision",
            Kind::Architecture => "architecture",
            Kind::Bugfix => "bugfix",
            Kind::Note => "note",
            Kind::Howto => "howto",
        }
    }

    /// Whether entries of this kind lose relevance with age.
    pub fn decays(self) -> bool {
        matches!(self, Kind::Bugfix | Kind::Note)
    }
}

impl FromStr for Kind {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "decision" => Ok(Kind::Decision),
            "architecture" => Ok(Kind::Architecture),
            "bugfix" => Ok(Kind::Bugfix),
            "note" => Ok(Kind::Note),
            "howto" => Ok(Kind::Howto),
            _ => Err(Error::invalid(
                "kind",
                format!("got {s:?}, expected one of: decision, architecture, bugfix, note, howto"),
            )),
        }
    }
}

impl fmt::Display for Kind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Active,
    Superseded,
    Retracted,
}

impl Status {
    pub fn as_str(self) -> &'static str {
        match self {
            Status::Active => "active",
            Status::Superseded => "superseded",
            Status::Retracted => "retracted",
        }
    }

    pub(crate) fn from_db(s: &str) -> Option<Self> {
        match s {
            "active" => Some(Status::Active),
            "superseded" => Some(Status::Superseded),
            "retracted" => Some(Status::Retracted),
            _ => None,
        }
    }
}

impl fmt::Display for Status {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Who an entry belongs to: one project, or a group of projects.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Owner {
    Project(String),
    Group(String),
}

impl fmt::Display for Owner {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Owner::Project(name) => write!(f, "project {name}"),
            Owner::Group(name) => write!(f, "group {name}"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum EventKind {
    Superseded,
    Retracted,
}

impl EventKind {
    pub fn as_str(self) -> &'static str {
        match self {
            EventKind::Superseded => "superseded",
            EventKind::Retracted => "retracted",
        }
    }

    pub(crate) fn from_db(s: &str) -> Option<Self> {
        match s {
            "superseded" => Some(EventKind::Superseded),
            "retracted" => Some(EventKind::Retracted),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Event {
    pub kind: EventKind,
    pub reason: Option<String>,
    /// For `Superseded`, the id of the entry that replaced this one.
    pub related: Option<i64>,
    pub at: i64,
}

/// An entry as submitted by an agent or a user, before validation.
#[derive(Debug, Clone)]
pub struct NewEntry {
    /// The current project, already resolved by the caller.
    pub project: String,
    /// When set, the entry is owned by this group instead of the project.
    pub group: Option<String>,
    pub kind: Kind,
    pub summary: String,
    pub body: String,
    pub tags: Vec<String>,
    pub supersedes: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Entry {
    pub id: i64,
    pub owner: Owner,
    pub kind: Kind,
    pub summary: String,
    pub body: String,
    pub tags: Vec<String>,
    pub created_at: i64,
    pub status: Status,
    pub supersedes: Option<i64>,
    pub superseded_by: Option<i64>,
    pub events: Vec<Event>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Group {
    pub name: String,
    pub members: Vec<String>,
}

pub(crate) fn is_name_char(c: char) -> bool {
    c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '-' | '_' | '/' | '@')
}

/// Validates a project, group or tag name and returns it lower-cased.
pub fn normalize_name(field: &'static str, raw: &str) -> Result<String> {
    let name = raw.trim().to_lowercase();
    if name.is_empty() {
        return Err(Error::invalid(field, "must not be empty"));
    }
    if name.chars().count() > MAX_NAME_CHARS {
        return Err(Error::invalid(
            field,
            format!("{name:?} is longer than {MAX_NAME_CHARS} characters"),
        ));
    }
    if let Some(bad) = name.chars().find(|&c| !is_name_char(c)) {
        return Err(Error::invalid(
            field,
            format!("{name:?} contains {bad:?}; allowed: a-z, 0-9, '-', '_', '/', '@'"),
        ));
    }
    if field == "project" && name == "all" {
        return Err(Error::invalid(
            field,
            "\"all\" is reserved; for entries of every project use scope all (--scope all)",
        ));
    }
    Ok(name)
}

pub fn normalize_tags(raw: &[String]) -> Result<Vec<String>> {
    let mut tags: Vec<String> = Vec::with_capacity(raw.len());
    for tag in raw {
        let tag = normalize_name("tags", tag)?;
        if !tags.contains(&tag) {
            tags.push(tag);
        }
    }
    if tags.len() > MAX_TAGS {
        return Err(Error::invalid(
            "tags",
            format!(
                "got {} distinct tags, at most {MAX_TAGS} allowed",
                tags.len()
            ),
        ));
    }
    Ok(tags)
}

pub fn normalize_summary(raw: &str) -> Result<String> {
    let summary = raw.trim();
    if summary.is_empty() {
        return Err(Error::invalid("summary", "must not be empty"));
    }
    if summary.contains(['\n', '\r']) {
        return Err(Error::invalid(
            "summary",
            "must be a single line; put details in body",
        ));
    }
    let chars = summary.chars().count();
    if chars > MAX_SUMMARY_CHARS {
        return Err(Error::invalid(
            "summary",
            format!("{chars} characters, at most {MAX_SUMMARY_CHARS} allowed; put details in body"),
        ));
    }
    Ok(summary.to_owned())
}

pub fn normalize_reason(raw: &str) -> Result<String> {
    let reason = raw.trim();
    if reason.is_empty() {
        return Err(Error::invalid("reason", "must not be empty"));
    }
    Ok(reason.to_owned())
}
