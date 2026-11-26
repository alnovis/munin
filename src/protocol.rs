/// How an agent should use Munin. Sent as the MCP server's `instructions`
/// and printed by `munin protocol`; keep it in step with the tools.
pub const TEXT: &str = "\
Munin is your memory across sessions. It holds what earlier sessions learned
about this project: decisions, how things are built, bugs and their fixes.

BEFORE unfamiliar code, a non-trivial change, or an error you do not recognise:
- Call search_memory with key words, symbols or file paths.
- Read the full entry with get_entry only when a card's summary looks relevant.
- Searches cover the current project and, when it belongs to a group, the
  whole group; list_groups shows which group that is. If nothing comes
  back, retry with deep_history: true.

AFTER a milestone, a fix, or a decision, call append_memory:
- kind: decision      a choice or convention that holds until replaced
        architecture  how the system is built
        bugfix        a problem, its cause and the fix
        note          anything else worth knowing next time
  decision and architecture never fade; bugfix and note fade over about a
  month.
- summary: one line a future reader can judge relevance by.
- body: context, what was tried, why this choice. Exact names, paths and
  commands beat paraphrase.
- tags: at most 6. Call get_active_tags first and reuse existing tags.
- group: set it instead of the project when a decision spans several
  services of one group.

WHEN an entry turns out outdated, append the replacement with
supersedes: <old id>. When an entry is simply wrong, call retract_memory with
the reason. Entries are never edited or deleted.

DO NOT record secrets, credentials or personal data, or anything the code and
git history already say.
";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_every_tool_and_is_ascii() {
        for tool in [
            "search_memory",
            "get_entry",
            "append_memory",
            "retract_memory",
            "get_active_tags",
            "list_groups",
        ] {
            assert!(TEXT.contains(tool), "{tool}");
        }
        assert!(TEXT.is_ascii());
    }
}
