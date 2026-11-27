---
name: munin
description: Project memory across sessions through the munin CLI. Use before unfamiliar code, a non-trivial change or an error not seen before (search), and after a decision, a fix or a measurement (record).
---

# Munin

Munin is memory across sessions: decisions, how the system is built, bugs
with their cause and fix. It lives in one SQLite file and is shared with
other agents on this machine. The current project is the git root of the
working directory.

## Before the work

```bash
munin brief                       # what memory holds for this project
munin search <words> --json       # words are OR-ed; "quoted phrase" matches in order
munin show <id> --json            # one entry in full, with its history
```

Search with key words, symbols or file paths. Read an entry in full only
when its summary looks relevant. If nothing comes back, retry with `--deep`
(ignores age). There is no stemming: `upload` does not find `uploads`.

## After the work

```bash
munin tags                        # reuse existing tags instead of inventing new ones
munin add --kind <kind> --summary "<one line>" --body-file - --tag <t> --tag <t> <<'EOF'
Context, what was tried, why this choice. Exact names, paths, commands.
EOF
```

- `--kind`: `decision` (a choice that holds until replaced),
  `architecture` (how the system is built), `bugfix` (problem, cause, fix),
  `note` (anything else), `howto` (steps to do something again: commands in
  order, `<placeholders>` for what changes each time). `decision`,
  `architecture` and `howto` never fade; `bugfix` and `note` fade over about
  a month.
- `--summary`: one line, at most 200 characters.
- `--tag`: at most 6.
- `--group <name>` instead of the project, when a decision spans the
  services of one group (`munin group list --project <p>` shows groups).

## Tasks

When the work has a task number (PROJ-123; a GitHub issue as gh-6), look it
up first, history included, then record under it:

```bash
munin search PROJ-123 --deep --inactive --scope all
munin add --kind bugfix --summary "[PROJ-123] <one line>" --tag proj-123 --tag <component> --body-file - <<'EOF'
Summary: what was asked
Root cause: why it happened (for a bug)
Implementation: what changed and where (files, functions, commits)
EOF
```

## When an entry is outdated or wrong

```bash
munin add --kind decision --summary "<new>" --supersedes <old-id>
munin retract <id> --reason "<why it is wrong>"
```

Entries are never edited or deleted.

## Do not record

Secrets, credentials, personal data, or anything the code and git history
already say.
