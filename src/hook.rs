use std::fs;

use serde_json::Value;

use crate::time::parse_utc_ms;

/// Tools that change files.
const EDIT_TOOLS: [&str; 4] = ["Edit", "MultiEdit", "Write", "NotebookEdit"];
/// Tools that count as having recorded something.
const RECORD_TOOLS: [&str; 2] = ["mcp__munin__append_memory", "mcp__munin__retract_memory"];

/// A turn is worth a reminder when it edited at least this many times, or
/// edited at all and ran at least `MIN_TOOLS_WITH_EDIT` tools (a long
/// investigation that ends in a small fix).
pub const MIN_EDITS: usize = 3;
pub const MIN_TOOLS_WITH_EDIT: usize = 8;

pub const STOP_REASON: &str = "Files were edited in this turn and nothing was written to Munin. \
If the work settled a decision, found the cause of a bug or measured something, record it with \
append_memory before finishing; otherwise finish without recording.";

/// Printed by the PreCompact hook; Claude Code uses it as instructions for the
/// compaction summary.
pub const PRE_COMPACT: &str = "In the summary, keep a short list headed \"Not yet in Munin\": \
decisions made, causes of bugs found and measurements taken in this session that were not \
recorded with append_memory, each with the exact names, paths and numbers involved. Leave the \
list out if there are none.";

/// Tool use of the current turn: everything after the last prompt the user typed.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Turn {
    pub tools: usize,
    pub edits: usize,
    pub recorded: bool,
    /// When the prompt was sent, in Unix milliseconds, if the transcript says.
    pub started_ms: Option<i64>,
}

impl Turn {
    pub fn wants_reminder(&self) -> bool {
        !self.recorded
            && (self.edits >= MIN_EDITS || (self.edits > 0 && self.tools >= MIN_TOOLS_WITH_EDIT))
    }
}

/// Reads a Claude Code transcript (JSON lines) backwards to the last prompt
/// typed by the user. Lines that do not parse are skipped.
pub fn last_turn(transcript: &str) -> Turn {
    let mut turn = Turn::default();
    for line in transcript.lines().rev() {
        let Ok(entry) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        let content = &entry["message"]["content"];
        match entry["type"].as_str() {
            Some("user") if entry["isMeta"] != true && is_prompt(content) => {
                turn.started_ms = entry["timestamp"].as_str().and_then(parse_utc_ms);
                break;
            }
            Some("assistant") => {
                for item in content.as_array().into_iter().flatten() {
                    if item["type"] != "tool_use" {
                        continue;
                    }
                    let name = item["name"].as_str().unwrap_or_default();
                    turn.tools += 1;
                    if EDIT_TOOLS.contains(&name) {
                        turn.edits += 1;
                    }
                    if RECORD_TOOLS.contains(&name) {
                        turn.recorded = true;
                    }
                }
            }
            _ => {}
        }
    }
    turn
}

// A user line is a typed prompt unless it only carries tool results.
fn is_prompt(content: &Value) -> bool {
    match content {
        Value::String(_) => true,
        Value::Array(items) => items.iter().any(|i| i["type"] == "text"),
        _ => false,
    }
}

/// Decides the Stop hook from its JSON input. Returns the JSON to print, or
/// `None` to let the agent stop. Any problem reading the input lets it stop:
/// a reminder is never worth a broken session.
///
/// The transcript is written behind the session, so the last tool calls may
/// be missing from it when the hook runs. Whether anything was recorded is
/// therefore asked of the database: `recorded_since(ms)` answers whether an
/// entry or event appeared since the prompt, `None` meaning it cannot tell.
pub fn stop(input: &str, recorded_since: impl FnOnce(i64) -> Option<bool>) -> Option<String> {
    let input: Value = serde_json::from_str(input).ok()?;
    if input["stop_hook_active"].as_bool() == Some(true) {
        return None;
    }
    let transcript = fs::read_to_string(input["transcript_path"].as_str()?).ok()?;
    let turn = last_turn(&transcript);
    if !turn.wants_reminder() {
        return None;
    }
    if let Some(started) = turn.started_ms
        && recorded_since(started) != Some(false)
    {
        return None;
    }
    Some(serde_json::json!({ "decision": "block", "reason": STOP_REASON }).to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    const PROMPT_TIME: &str = "2026-11-27T07:47:00.000Z";

    fn prompt(text: &str) -> String {
        serde_json::json!({"type": "user", "timestamp": PROMPT_TIME,
            "message": {"role": "user", "content": text}})
        .to_string()
    }

    fn meta(text: &str) -> String {
        serde_json::json!({"type": "user", "isMeta": true,
            "message": {"role": "user", "content": text}})
        .to_string()
    }

    fn tool(name: &str) -> String {
        serde_json::json!({"type": "assistant", "message": {"role": "assistant", "content": [
            {"type": "text", "text": "working"},
            {"type": "tool_use", "id": "x", "name": name, "input": {}}
        ]}})
        .to_string()
    }

    fn result() -> String {
        serde_json::json!({"type": "user", "message": {"role": "user", "content": [
            {"type": "tool_result", "tool_use_id": "x", "content": "ok"}
        ]}})
        .to_string()
    }

    fn transcript(lines: &[String]) -> String {
        lines.join("\n")
    }

    #[test]
    fn counts_only_the_last_turn() {
        let t = transcript(&[
            prompt("first"),
            tool("Edit"),
            result(),
            tool("Edit"),
            result(),
            prompt("second"),
            tool("Read"),
            result(),
            tool("Write"),
            result(),
        ]);
        assert_eq!(
            last_turn(&t),
            Turn {
                tools: 2,
                edits: 1,
                recorded: false,
                started_ms: parse_utc_ms(PROMPT_TIME),
            }
        );
    }

    // Check that hook feedback and other meta lines do not end the turn.
    #[test]
    fn meta_lines_are_not_prompts() {
        let t = transcript(&[
            prompt("go"),
            tool("Edit"),
            meta("Stop hook feedback: ..."),
            tool("Edit"),
            tool("Edit"),
        ]);
        assert_eq!(last_turn(&t).edits, 3);
    }

    #[test]
    fn reminder_thresholds() {
        let edits = |n: usize, extra: usize| {
            let mut lines = vec![prompt("go")];
            lines.extend((0..n).map(|_| tool("Edit")));
            lines.extend((0..extra).map(|_| tool("Bash")));
            last_turn(&transcript(&lines))
        };
        assert!(!edits(0, 20).wants_reminder(), "no edits");
        assert!(!edits(1, 2).wants_reminder(), "a quick one-line change");
        assert!(
            edits(1, 7).wants_reminder(),
            "long investigation, small fix"
        );
        assert!(edits(3, 0).wants_reminder(), "several edits");
    }

    #[test]
    fn recording_in_the_turn_silences_it() {
        let t = transcript(&[
            prompt("go"),
            tool("Edit"),
            tool("Edit"),
            tool("Edit"),
            tool("mcp__munin__append_memory"),
        ]);
        assert!(!last_turn(&t).wants_reminder());
    }

    #[test]
    fn garbage_lines_are_skipped() {
        let t = format!(
            "{}\nnot json\n{}\n{}\n{}",
            prompt("go"),
            tool("Edit"),
            tool("Edit"),
            tool("Write")
        );
        assert_eq!(last_turn(&t).edits, 3);
    }

    #[test]
    fn stop_blocks_once_and_never_breaks() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("t.jsonl");
        fs::write(
            &path,
            transcript(&[prompt("go"), tool("Edit"), tool("Edit"), tool("Edit")]),
        )
        .unwrap();
        let input = |active: bool| {
            serde_json::json!({"transcript_path": path, "stop_hook_active": active}).to_string()
        };

        let nothing = |_| Some(false);
        let out: Value = serde_json::from_str(&stop(&input(false), nothing).unwrap()).unwrap();
        assert_eq!(out["decision"], "block");
        assert_eq!(out["reason"], STOP_REASON);

        assert_eq!(stop(&input(true), nothing), None, "already continuing");
        assert_eq!(stop("not json", nothing), None);
        assert_eq!(
            stop(r#"{"transcript_path": "/nonexistent"}"#, nothing),
            None
        );
    }

    // Check that the database decides "recorded" when the transcript lags
    // behind, and that an unanswerable database means silence.
    #[test]
    fn database_overrides_a_lagging_transcript() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("t.jsonl");
        fs::write(
            &path,
            transcript(&[prompt("go"), tool("Edit"), tool("Edit"), tool("Edit")]),
        )
        .unwrap();
        let input = serde_json::json!({"transcript_path": path}).to_string();

        let mut asked = None;
        assert_eq!(
            stop(&input, |since| {
                asked = Some(since);
                Some(true)
            }),
            None
        );
        assert_eq!(
            asked,
            parse_utc_ms(PROMPT_TIME),
            "asked from the prompt time"
        );
        assert_eq!(stop(&input, |_| None), None, "database unavailable");
        assert!(stop(&input, |_| Some(false)).is_some());
    }

    #[test]
    fn texts_are_ascii() {
        assert!(STOP_REASON.is_ascii() && PRE_COMPACT.is_ascii());
    }
}
