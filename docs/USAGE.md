# Usage

What Munin stores, how search finds it, and the command line around it.
Agents use the same operations through MCP tools; the tool names are given
next to each command.

## Table of Contents

- [Entries](#entries)
- [Search](#search)
- [Superseding and retracting](#superseding-and-retracting)
- [Groups](#groups)
- [Command line](#command-line)
- [Configuration](#configuration)
- [Tasks](#tasks)
- [Backups](#backups)

---

## Entries

An entry is one thing worth knowing next time: a one-line `summary`, an
optional Markdown `body`, up to 6 tags and a kind.

| Kind | For | Fades with age |
|------|-----|----------------|
| `decision` | a choice or convention that holds until replaced | no |
| `architecture` | how the system is built | no |
| `bugfix` | a problem, its cause and the fix | yes, half its weight every 30 days |
| `note` | anything else | yes, half its weight every 30 days |

Every entry belongs to a project, or to a group of projects (see
[Groups](#groups)). The content of an entry never changes after it is
written.

## Search

`munin search` / `search_memory` returns compact cards: id, kind, owner, age,
summary and tags. Read the full entry with `munin show <id>` / `get_entry`.

- Words are OR-ed and matched case-insensitively against summary, body and
  tags. A phrase in double quotes matches in order.
- `-`, `_`, `/` and `@` are part of a word, so `supersedes-51`, `@scope/pkg`
  and `src/db` match as one token. `.` is not: `client.rs` matches as the
  phrase `client rs`.
- There is no stemming: `upload` does not find `uploads`. Search with the
  form you expect, or rely on tags.
- Ranking is text relevance (BM25) times the age factor of the kind. It is
  applied to every match before the limit, so one old `decision` is not
  pushed out by many stale fixes.
- `--deep` (`deep_history`) ignores age and ranks on text alone.
- `--inactive` (`include_inactive`) also returns superseded and retracted
  entries.
- `--limit` takes 1 to 15; the default is 5.

Scope decides whose entries are searched:

| Scope | Entries of |
|-------|------------|
| `project` | the current project |
| `group` | the current project, every project that shares a group with it, and those groups |
| `group` with `--group G` | group `G` and its projects |
| `all` | everything |

The default is `group` when the current project belongs to a group, and
`project` otherwise.

## Superseding and retracting

Knowledge changes, but Munin never edits an entry:

- When an entry is **outdated**, write the replacement with
  `--supersedes <id>` (`supersedes` in `append_memory`). The old entry becomes
  `superseded` and points to the new one; the new one points back.
- When an entry is **wrong**, retract it with a reason: `munin retract <id>
  --reason "..."` (`retract_memory`).

Only active entries can be superseded or retracted, and an entry can only
supersede one with the same owner. Superseded and retracted entries stay
readable and show their history in `munin show`.

## Groups

A group is a set of projects worked on together, such as the services of one
microservice system. Groups are managed by people through the command line;
agents can only list them (`list_groups`).

```bash
munin group add platform billing auth gateway   # creates the group if needed
munin group remove platform gateway
munin group list                                # all groups
munin group list --project billing              # groups billing belongs to
munin group delete platform                     # refused while it owns entries
```

A decision that spans the services is recorded on the group, so every member
sees it in its default search:

```bash
munin add --group platform --kind decision \
  --summary "Events between services are JSON with a schema version field"
```

The current project has to be a member of the group.

## Command line

The current project is resolved as described in
[INSTALL.md](INSTALL.md#which-project-an-entry-lands-in); `--project`
overrides it for one command. Every command takes `--json` for JSON output
and `--db PATH` for another database file.

```bash
# Record
munin add --kind decision --summary "Use WAL mode" --tag sqlite \
  --body "Readers must not block the writer."
munin add --kind bugfix --summary "Flaky socket test" --body-file notes.md
git log -1 --format=%B | munin add --kind note --summary "Release notes" --body-file -

# Find and read
munin search wal mode
munin search '"busy timeout"' --scope all --limit 10
munin show 12

# Change what is known
munin add --kind decision --summary "Use WAL with 5 s busy timeout" --supersedes 12
munin retract 7 --reason "Measured: the cache never helped"

# Tags in use, most recently used first
munin tags

# What memory holds for this project: counts, tags, newest decisions
munin brief

# Every entry, newest first, without a query
munin list                                  # current project (or its group)
munin list --scope all --kind decision      # all decisions in every project
munin list --inactive --limit 20            # include superseded and retracted

# What agents are told
munin protocol

# Task number formats in effect (from ~/.munin/config.toml or built in)
munin config
```

Exit codes: 0 on success, 1 when Munin rejects the request (invalid input,
not found, conflict), 2 on a malformed command line.

## Configuration

| Variable | Default | Meaning |
|----------|---------|---------|
| `MUNIN_DB` | `~/.munin/munin.db` | Database file; created with its directory on first use |
| `MUNIN_PROJECT` | git root or directory name | Current project |
| `MUNIN_HALF_LIFE_DAYS` | `30` | Age at which `bugfix` and `note` lose half their weight |
| `MUNIN_CONFIG` | `~/.munin/config.toml` | Optional config file; today it holds [task number formats](#task-number-formats) |

## Tasks

Work that has a task number is recorded under it, so the next session finds
its history by the number alone:

- the number in brackets at the start of the summary: `[PROJ-123] ...`;
- the number and the component as tags: `proj-123`, `billing`;
- the body in three parts: `Summary:` what was asked, `Root cause:` why it
  happened (for a bug), `Implementation:` what changed and where.

```bash
munin add --kind bugfix --summary "[PROJ-123] Session lost after the login redirect" \
  --tag proj-123 --tag auth/session --body-file notes.md
munin search PROJ-123 --deep --inactive --scope all     # the whole history
```

With the `munin hook prompt` hook (see
[INSTALL.md](INSTALL.md#claude-code)), a prompt that mentions a task number
brings the entries for that number into the agent's context by itself.

### Task number formats

Built in, without a config file:

- tracker keys as trackers print them -- upper-case letters or digits
  starting with a letter, a dash, digits (`PROJ-123`, `PROJ2-7`); tagged as
  the number lower-cased (`proj-123`) and looked up across all projects;
- GitHub issues, `#6` or `gh-6`; tagged `gh-6` (`#` is not part of a word,
  so `#6` itself is not searchable) and looked up only in the current
  project, since every repository has its own `#6`.

Other formats go in `~/.munin/config.toml` (or the file `$MUNIN_CONFIG`
names). Its `[[task]]` rules **replace** the built-in ones, so a file that
adds a tracker should also list the formats still wanted, such as the GitHub
ones below:

```toml
# Jira keys: unique across projects
[[task]]
pattern = '\b[A-Z][A-Z0-9]{1,9}-[0-9]+\b'
scope = "all"

# ServiceNow incidents
[[task]]
pattern = '\bINC[0-9]{7}\b'

# GitHub issues: every repository has its own numbers
[[task]]
pattern = '(?:^|[^\w#])#([0-9]+)\b'
tag = "gh-$1"
scope = "project"

[[task]]
pattern = '\bgh-([0-9]+)\b'
tag = "gh-$1"
scope = "project"
```

- `pattern` is a regular expression ([Rust regex syntax](https://docs.rs/regex/latest/regex/#syntax));
  start it with `(?i)` to match a number typed in any case
  (`(?i)\bPROJ-[0-9]+\b` also catches `proj-123`);
- `tag` is the tag the number is recorded under; `$1`, `$2` refer to the
  pattern's groups. Without it, the matched text lower-cased. A tag may use
  only `a-z`, `0-9`, `-`, `_`, `/`, `@`; matches that do not fit are skipped;
- `scope` is `all` (default) or `project`.

`munin config` shows the formats in effect and reports a broken file. When
the file defines formats, `munin brief` lists them at session start, so the
agent knows how to tag numbers.

## Backups

The database is a single SQLite file in WAL mode. `sqlite3` copies it safely
even while an agent is using it:

```bash
sqlite3 ~/.munin/munin.db ".backup '$HOME/munin-backup.db'"
```

To look inside without any risk of changing it:

```bash
sqlite3 -readonly ~/.munin/munin.db \
  "SELECT id, kind, status, summary FROM entries ORDER BY id DESC LIMIT 20"
```
