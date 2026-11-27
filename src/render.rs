use std::fmt::Write;

use munin::time::format_utc;
use munin::{Brief, Card, Entry, EventKind, Group, Listed, Status, TagUse};

pub fn listed(listed: &[Listed]) -> String {
    if listed.is_empty() {
        return "no entries\n".to_owned();
    }
    let mut out = String::new();
    for (i, item) in listed.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        let _ = write!(
            out,
            "#{}  {}  {}  {}d",
            item.id, item.kind, item.owner, item.age_days
        );
        if item.status != Status::Active {
            let _ = write!(out, "  [{}]", item.status);
        }
        let _ = writeln!(out, "\n    {}", item.summary);
        if !item.tags.is_empty() {
            let _ = writeln!(out, "    tags: {}", item.tags.join(", "));
        }
        if let Some(next) = item.superseded_by {
            let _ = writeln!(out, "    superseded by #{next}");
        }
    }
    out
}

// Plain statements, not instructions: a SessionStart hook passes this text to
// the agent as context.
pub fn brief(brief: &Brief) -> String {
    let mut out = String::new();
    let scope = if brief.groups.is_empty() {
        format!("project {}", brief.project)
    } else {
        format!(
            "project {} and its group {}",
            brief.project,
            brief.groups.join(", ")
        )
    };
    let total = brief.active.total();
    if total == 0 {
        let _ = writeln!(out, "Munin has no entries for {scope} yet.");
        out.push_str(
            "Memory for this project starts with this session: decisions, causes of bugs and \
             measurements recorded with append_memory are what later sessions will find.\n",
        );
        return out;
    }
    let a = &brief.active;
    let _ = writeln!(
        out,
        "Munin holds {total} active entries for {scope}: {} decision, {} architecture, {} bugfix, {} note.",
        a.decision, a.architecture, a.bugfix, a.note
    );
    if !brief.tags.is_empty() {
        let _ = writeln!(out, "Tags in use: {}.", brief.tags.join(", "));
    }
    if !brief.durable.is_empty() {
        out.push_str("Newest decisions and architecture:\n");
        for e in &brief.durable {
            let _ = writeln!(out, "  #{} {} ({}): {}", e.id, e.kind, e.owner, e.summary);
        }
    }
    out.push_str(
        "search_memory finds entries by words, symbols or file paths; get_entry reads one in full.\n",
    );
    out.push_str(
        "New decisions, causes of bugs and measurements go in with append_memory; an outdated \
         entry is replaced with supersedes.\n",
    );
    out
}

pub fn cards(cards: &[Card]) -> String {
    if cards.is_empty() {
        return "no matches\n".to_owned();
    }
    let mut out = String::new();
    for (i, card) in cards.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        let _ = write!(
            out,
            "#{}  {}  {}  {}d",
            card.id, card.kind, card.owner, card.age_days
        );
        if card.status != Status::Active {
            let _ = write!(out, "  [{}]", card.status);
        }
        let _ = writeln!(out, "\n    {}", card.summary);
        if !card.tags.is_empty() {
            let _ = writeln!(out, "    tags: {}", card.tags.join(", "));
        }
        if let Some(next) = card.superseded_by {
            let _ = writeln!(out, "    superseded by #{next}");
        }
    }
    out
}

pub fn entry(entry: &Entry) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "#{}  {}  {}", entry.id, entry.kind, entry.status);
    let _ = writeln!(out, "owner:   {}", entry.owner);
    let _ = writeln!(out, "created: {}", format_utc(entry.created_at));
    if !entry.tags.is_empty() {
        let _ = writeln!(out, "tags:    {}", entry.tags.join(", "));
    }
    if let Some(old) = entry.supersedes {
        let _ = writeln!(out, "supersedes #{old}");
    }
    if let Some(next) = entry.superseded_by {
        let _ = writeln!(out, "superseded by #{next}");
    }
    let _ = writeln!(out, "\n{}", entry.summary);
    if !entry.body.is_empty() {
        let _ = writeln!(out, "\n{}", entry.body);
    }
    if !entry.events.is_empty() {
        out.push_str("\nevents:\n");
        for event in &entry.events {
            let _ = write!(out, "  {}  ", format_utc(event.at));
            match (event.kind, event.related, &event.reason) {
                (EventKind::Superseded, Some(next), _) => {
                    let _ = writeln!(out, "superseded by #{next}");
                }
                (EventKind::Retracted, _, Some(reason)) => {
                    let _ = writeln!(out, "retracted: {reason}");
                }
                (kind, _, _) => {
                    let _ = writeln!(out, "{}", kind.as_str());
                }
            }
        }
    }
    out
}

pub fn tags(tags: &[TagUse]) -> String {
    if tags.is_empty() {
        return "no tags\n".to_owned();
    }
    let width = tags.iter().map(|t| t.tag.len()).max().unwrap_or(0);
    let mut out = String::new();
    for tag in tags {
        let _ = writeln!(
            out,
            "{:width$}  {} recent, {} total",
            tag.tag, tag.recent, tag.total
        );
    }
    out
}

pub fn groups(groups: &[Group]) -> String {
    if groups.is_empty() {
        return "no groups\n".to_owned();
    }
    let mut out = String::new();
    for group in groups {
        let _ = writeln!(out, "{}: {}", group.name, group.members.join(", "));
    }
    out
}
