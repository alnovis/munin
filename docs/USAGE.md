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
```

Exit codes: 0 on success, 1 when Munin rejects the request (invalid input,
not found, conflict), 2 on a malformed command line.

## Configuration

| Variable | Default | Meaning |
|----------|---------|---------|
| `MUNIN_DB` | `~/.munin/munin.db` | Database file; created with its directory on first use |
| `MUNIN_PROJECT` | git root or directory name | Current project |
| `MUNIN_HALF_LIFE_DAYS` | `30` | Age at which `bugfix` and `note` lose half their weight |

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
