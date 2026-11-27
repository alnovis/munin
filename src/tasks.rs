use std::fmt::Write;
use std::fs;
use std::path::Path;

use regex::Regex;
use serde::Deserialize;

use crate::error::{Error, Result};
use crate::model::normalize_name;

/// At most this many task numbers are looked up per prompt.
pub const MAX_TASK_IDS: usize = 5;

/// Where a task number is looked up.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TaskScope {
    /// Every project: the number is unique across them (a tracker key).
    #[default]
    All,
    /// Only the current project: every repository has its own numbers.
    Project,
}

/// A task number found in text, written as the tag it is recorded under.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskId {
    pub tag: String,
    pub scope: TaskScope,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileConfig {
    #[serde(default)]
    task: Vec<RuleConfig>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RuleConfig {
    pattern: String,
    tag: Option<String>,
    #[serde(default)]
    scope: TaskScope,
}

#[derive(Debug, Clone)]
struct Rule {
    regex: Regex,
    /// Expansion template such as `gh-$1`; `None` means the match lower-cased.
    tag: Option<String>,
    scope: TaskScope,
}

/// The formats task numbers take, from the config file or built in.
#[derive(Debug, Clone)]
pub struct TaskPatterns {
    rules: Vec<Rule>,
    /// True when the rules come from a config file.
    pub configured: bool,
}

// Tracker keys (PROJ-123) across all projects; GitHub issues (#6, gh-6) in
// the current project only.
const BUILTIN: [(&str, Option<&str>, TaskScope); 3] = [
    (r"\b[A-Z][A-Z0-9]{1,9}-[0-9]{1,9}\b", None, TaskScope::All),
    (
        r"(?:^|[^\w#])#([0-9]{1,9})\b",
        Some("gh-$1"),
        TaskScope::Project,
    ),
    (r"\bgh-([0-9]{1,9})\b", Some("gh-$1"), TaskScope::Project),
];

impl TaskPatterns {
    pub fn builtin() -> Self {
        let rules = BUILTIN
            .iter()
            .map(|(pattern, tag, scope)| Rule {
                regex: Regex::new(pattern).expect("built-in task pattern"),
                tag: tag.map(str::to_owned),
                scope: *scope,
            })
            .collect();
        TaskPatterns {
            rules,
            configured: false,
        }
    }

    /// Rules from a config file's text. A file without `[[task]]` sections
    /// keeps the built-in rules.
    pub fn from_toml(text: &str) -> Result<Self> {
        let config: FileConfig = toml::from_str(text)
            .map_err(|e| Error::invalid("config", e.to_string().trim().to_owned()))?;
        if config.task.is_empty() {
            return Ok(Self::builtin());
        }
        let rules = config
            .task
            .into_iter()
            .map(|rule| {
                let regex = Regex::new(&rule.pattern).map_err(|e| {
                    Error::invalid("config", format!("task pattern {:?}: {e}", rule.pattern))
                })?;
                Ok(Rule {
                    regex,
                    tag: rule.tag,
                    scope: rule.scope,
                })
            })
            .collect::<Result<_>>()?;
        Ok(TaskPatterns {
            rules,
            configured: true,
        })
    }

    /// Rules from the config file at `path`, or the built-in ones when it
    /// does not exist. A file that exists but is invalid is an error.
    pub fn load(path: &Path) -> Result<Self> {
        match fs::read_to_string(path) {
            Ok(text) => Self::from_toml(&text),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::builtin()),
            Err(e) => Err(e.into()),
        }
    }

    /// Task numbers in `text`, in order of appearance, without repeats, at
    /// most `MAX_TASK_IDS`. A match whose tag is not a valid Munin tag is
    /// skipped.
    pub fn find(&self, text: &str) -> Vec<TaskId> {
        let mut found: Vec<(usize, TaskId)> = Vec::new();
        for rule in &self.rules {
            for caps in rule.regex.captures_iter(text) {
                let whole = caps.get(0).expect("group 0");
                let raw = match &rule.tag {
                    Some(template) => {
                        let mut tag = String::new();
                        caps.expand(template, &mut tag);
                        tag
                    }
                    None => whole.as_str().to_owned(),
                };
                let Ok(tag) = normalize_name("tags", &raw) else {
                    continue;
                };
                if found.iter().all(|(_, id)| id.tag != tag) {
                    found.push((
                        whole.start(),
                        TaskId {
                            tag,
                            scope: rule.scope,
                        },
                    ));
                }
            }
        }
        found.sort_by_key(|(start, _)| *start);
        found
            .into_iter()
            .map(|(_, id)| id)
            .take(MAX_TASK_IDS)
            .collect()
    }

    /// One line per rule, for the agent's context and `munin config`.
    pub fn describe(&self) -> String {
        let mut out = String::new();
        for rule in &self.rules {
            let tag = rule.tag.as_deref().unwrap_or("the match, lower-cased");
            let scope = match rule.scope {
                TaskScope::All => "looked up in all projects",
                TaskScope::Project => "looked up in the current project",
            };
            let _ = writeln!(out, "  {}  -> tag {tag}; {scope}", rule.regex.as_str());
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tags(patterns: &TaskPatterns, text: &str) -> Vec<String> {
        patterns.find(text).into_iter().map(|id| id.tag).collect()
    }

    #[test]
    fn builtin_rules() {
        let p = TaskPatterns::builtin();
        assert_eq!(
            tags(&p, "look at PROJ-123, then gh-6 and #12 (again PROJ-123)"),
            vec!["proj-123", "gh-6", "gh-12"]
        );
        // Non-ASCII text around the number, as in a prompt written in Russian.
        assert_eq!(
            tags(
                &p,
                "\u{437}\u{430}\u{434}\u{430}\u{447}\u{430} PROJ2-7 \u{433}\u{43e}\u{442}\u{43e}\u{432}\u{430}"
            ),
            vec!["proj2-7"]
        );
        assert!(tags(&p, "utf-8, x86-64, a-1, -5, # 3, ABC-,proj-123, a#5").is_empty());
        let scopes: Vec<TaskScope> = p.find("ABC-1 #2").into_iter().map(|id| id.scope).collect();
        assert_eq!(scopes, vec![TaskScope::All, TaskScope::Project]);
        let many = (1..10)
            .map(|n| format!("AB-{n}"))
            .collect::<Vec<_>>()
            .join(" ");
        assert_eq!(p.find(&many).len(), MAX_TASK_IDS);
    }

    #[test]
    fn configured_rules_replace_builtin() {
        let p = TaskPatterns::from_toml(
            r##"
            [[task]]
            pattern = '\bTICKET-([0-9]+)\b'
            tag = "t-$1"
            scope = "project"

            [[task]]
            pattern = '\bINC[0-9]{6}\b'
            "##,
        )
        .unwrap();
        assert!(p.configured);
        assert_eq!(
            tags(&p, "TICKET-42 and INC000123, not ABC-1"),
            vec!["t-42", "inc000123"]
        );
        assert_eq!(p.find("TICKET-42")[0].scope, TaskScope::Project);
        assert_eq!(p.find("INC000123")[0].scope, TaskScope::All);
    }

    #[test]
    fn empty_or_missing_config_keeps_builtin() {
        assert!(!TaskPatterns::from_toml("").unwrap().configured);
        let dir = tempfile::tempdir().unwrap();
        let p = TaskPatterns::load(&dir.path().join("absent.toml")).unwrap();
        assert!(!p.configured);
        assert_eq!(tags(&p, "ABC-1"), vec!["abc-1"]);
    }

    #[test]
    fn invalid_config_is_reported() {
        for bad in [
            "[[task]]\npattern = '('",
            "[[task]]\ntag = 'x'",
            "[[task]]\npattern = 'x'\nscope = 'everywhere'",
            "[[task]]\npattern = 'x'\nunknown = 1",
            "not toml at all = = =",
        ] {
            assert!(
                matches!(
                    TaskPatterns::from_toml(bad),
                    Err(Error::Invalid {
                        field: "config",
                        ..
                    })
                ),
                "{bad}"
            );
        }
    }

    #[test]
    fn tags_that_are_not_valid_are_skipped() {
        let p = TaskPatterns::from_toml("[[task]]\npattern = 'X\\+[0-9]+'").unwrap();
        assert!(p.find("X+12").is_empty(), "'+' is not allowed in a tag");
    }
}
