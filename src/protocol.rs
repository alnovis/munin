/// How an agent should use Munin. Sent as the MCP server's `instructions`
/// and printed by `munin protocol`; keep it in step with the tools.
pub const TEXT: &str = "\
Munin is your memory across sessions. It holds what earlier sessions learned
about this project: decisions, how things are built, bugs and their fixes,
steps of routine jobs.

BEFORE unfamiliar code, a non-trivial change, an error you do not recognise,
or a routine job done in steps (release, deploy, pushing an image, a
migration):
- Call search_memory with key words, symbols or file paths. For a routine
  job, look for a howto instead of working the steps out again.
- Read the full entry with get_entry only when a card's summary looks relevant.
- Searches cover the current project and, when it belongs to a group, the
  whole group; list_groups shows which group that is. If nothing comes
  back, retry with deep_history: true.
- Asked what memory holds, or to load a project before broad work: call
  list_memory, then search for specifics.

AFTER a milestone, a fix, or a decision, call append_memory:
- kind: decision      a choice or convention that holds until replaced
        architecture  how the system is built
        bugfix        a problem, its cause and the fix
        note          anything else worth knowing next time
        howto         steps to do something again: commands, in order,
                      with <placeholders> for what changes each time
  decision, architecture and howto never fade; bugfix and note fade over
  about a month.
  Steps the user had to explain, or that took several tries to get right,
  go in as a howto.
- summary: one line a future reader can judge relevance by.
- body: context, what was tried, why this choice. Exact names, paths and
  commands beat paraphrase.
- tags: at most 6. Call get_active_tags first and reuse existing tags.
- group: set it instead of the project when a decision spans several
  services of one group.

TASKS: when the work has a task number (by default a tracker key such as
PROJ-123, or a GitHub issue written gh-6; other formats, when configured,
are listed at session start):
- Search for the number first: search_memory with it, deep_history: true,
  include_inactive: true, finds everything earlier sessions recorded.
- Start the summary with the number in brackets: [PROJ-123] ...
- Add the number as a tag (proj-123), and the component as another tag
  (billing, auth/session).
- Write the body in three parts:
    Summary: what was asked
    Root cause: why it happened (for a bug)
    Implementation: what changed and where (files, functions, commits)

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
            "list_memory",
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

    // Claude Code cuts server instructions at 4096 characters by default.
    #[test]
    fn fits_the_instructions_cap() {
        assert!(TEXT.len() <= 4096, "{} characters", TEXT.len());
    }
}
