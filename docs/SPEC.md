# Munin -- specification v0.1 (draft)

Local, single-binary memory for AI coding agents. An agent searches it before
unfamiliar work and records what it learned after. Exposed as an MCP server
over stdio, plus a CLI for humans.

Named after Muninn, the raven of memory who flies out every day and brings
back what Odin needs.

## 1. Starting point

Munin is a Rust reimplementation of the ideas in
[larchanka/memo-mcp](https://github.com/larchanka/memo-mcp) (TypeScript, MIT,
v0.1.2). From it, Munin keeps:

- one SQLite file, WAL mode, FTS5 with BM25 ranking;
- two-stage retrieval: compact cards first, the full body by id on request;
- typed entries with time decay: `bugfix` and `note` fade, `decision` and
  `architecture` do not;
- a shared tag vocabulary that agents read before writing.

What Munin changes, and why:

| memo-mcp | Munin | Reason |
|---|---|---|
| Entries can never change | Entries can be superseded or retracted | A wrong `decision` never decays and keeps winning searches |
| No links between entries | `supersedes` field, chain visible on both ends | Knowledge evolves; the old entry must point to the new one |
| Project only by convention in the summary | `project` column, search scoped to it by default | Entries from other projects are noise |
| Decay applied after a top-30 cut by raw BM25 | Decay applied over all matches | An old immune `decision` can fall outside the cut |
| Invalid input silently coerced to `note` | Invalid input rejected with a clear error | A silent `note` decays away a `decision` the agent meant to keep |
| `npx -y` fetches latest on every start | One pinned binary | No unreviewed code runs on start |
| MCP only | MCP + CLI | Humans inspect and fix memory without `sqlite3` |

## 2. Goals and non-goals

Goals:

- Any MCP client (Claude Code, Codex, Cursor, OpenCode, ...) can read and
  write the same memory on one machine.
- Every change to memory is preserved: nothing is overwritten or deleted.
- A search returns few, relevant, current entries.

Non-goals for v1:

- Embeddings or vector search.
- Sync between machines or users; network access of any kind.
- Telemetry.
- A web UI.
- Import of other memory formats (memo-mcp, Claude Code memory files).

## 3. Data model

Entry:

| Field | Type | Notes |
|---|---|---|
| `id` | integer | Assigned on insert, never reused |
| `project` | text, nullable | Normalized like a tag |
| `group` | text, nullable | Name of an existing group |
| `kind` | enum | `decision`, `architecture`, `bugfix`, `note` |
| `summary` | text | One line, at most 200 characters |
| `body` | text | Markdown, may be empty |
| `tags` | set of text | 0-6 items, `[a-z0-9-_/@]`, lower case |
| `created_at` | integer | Unix milliseconds, UTC |
| `supersedes` | integer, nullable | Id of the entry this one replaces |
| `status` | enum | `active`, `superseded`, `retracted` |

Rules:

- Exactly one of `project` and `group` is set: the entry's owner. An entry
  owned by a group records a decision that spans its projects.
- `summary`, `body`, `kind`, `tags`, `project`, `group` and `supersedes` are
  immutable after insert.
- `status` is the only mutable field and only moves forward:
  `active -> superseded` (when a newer entry supersedes it) or
  `active -> retracted` (explicit retract with a reason).
- An entry may be superseded once. Superseding an entry that is not `active`
  is an error.
- `supersedes` must point to an entry with the same owner.

Group: a named set of projects that are worked on together, for example the
services of one microservice system. A project may belong to several groups.
Groups are defined by humans through the CLI (section 6); agents only read
them. Stored in a `groups` table (name) and a `group_members` table (group,
project). Membership in any group makes `group` the default search scope for
that project (section 4). A group that still owns entries cannot be deleted.
- Every status change is written to an `events` table: entry id, event,
  reason, timestamp. History is reconstructed from it.

## 4. Search and ranking

- Query: free text. Unquoted words are OR-ed; quoted phrases match as
  phrases. Characters `-_/@` are part of a token, so `@scope/pkg` and
  `supersedes-51` match as one token. `.` is not, so a word at the end of a
  sentence still matches; `client.rs` matches as the phrase `client rs`.
- Scope, set by `scope`:
  - `project`: entries owned by the current project only;
  - `group`: entries owned by the current project, by every project that
    shares a group with it, and by those groups themselves; with
    `group: <name>`, that one group and its projects only;
  - `all`: every entry in the database.

  Default: `group` when the current project belongs to at least one group,
  `project` otherwise.

  Cards always carry `project`, so hits from different services stay
  distinguishable. Ranking does not favour the current project in wider
  scopes.
- Status: only `active` entries by default; `include_inactive: true` returns
  superseded and retracted entries too, marked as such.
- Score: `bm25_relevance x decay`, where decay is 1 for `decision` and
  `architecture`, and `0.5 ^ (age_days / half_life)` for `bugfix` and `note`
  (half-life 30 days, configurable). `deep_history: true` sets decay to 1 for
  all kinds.
- Ranking is computed over all matches before the limit is applied.
- Limit: default 5, maximum 15.

Card returned per hit: `id`, `project`, `kind`, `summary`, `tags`,
`age_days`, `score`, and, when not `active`, `status` and `superseded_by`.

## 5. MCP tools

| Tool | Input | Result |
|---|---|---|
| `search_memory` | `query`, `scope?`, `group?`, `project?`, `include_inactive?`, `deep_history?`, `limit?` | Cards |
| `list_groups` | `project?` | Groups the project belongs to, with their members |
| `get_entry` | `id` | Full entry plus `supersedes`, `superseded_by`, `events` |
| `append_memory` | `kind`, `summary`, `body?`, `tags?`, `supersedes?`, `project?` or `group?` | The new entry's id and card |
| `retract_memory` | `id`, `reason` | The updated card |

`project` is optional everywhere; when omitted it is resolved as in section 7.
`append_memory` takes `group` instead of `project` to record a decision that
spans a group; the current project must belong to that group.
`retract_memory` needs only a reason: it marks an entry as wrong without
replacing it. To replace an entry, append a new one with `supersedes`.
| `get_active_tags` | `project?`, `scope?`, `group?`, `limit?` | Tags of active entries in scope: uses in the last 60 days, then total uses (default 40, max 200) |

Errors are returned as MCP tool errors with a message the agent can act on:
which field is wrong and what is allowed.

## 6. CLI

```
munin serve                       # MCP server on stdio
munin search <word>... [--scope project|group|all] [--group G] [--project P] [--inactive] [--deep] [--limit N]
munin show <id>                   # entry, chain, events
munin add --kind K --summary S [--body B | --body-file F] [--tag T]... [--supersedes ID] [--project P] [--group G]
munin retract <id> --reason R
munin tags [--scope ...] [--group G] [--project P] [--limit N]
munin list [--scope ...] [--group G] [--project P] [--kind K] [--inactive] [--limit N]   # newest first, no query
munin brief [--project P]         # counts, tags, newest decisions; for a SessionStart hook
munin protocol                    # print the agent protocol (section 6.1)
munin config                      # task number formats in effect; reports a broken config
munin hook stop|pre-compact|prompt   # Claude Code hooks, input on stdin
munin group add <group> <project>...     # create the group if needed, add members
munin group remove <group> <project>...
munin group delete <group>               # refused while the group owns entries
munin group list [--project P]
```

Global flags: `--db PATH` overrides `$MUNIN_DB`; `--json` prints JSON
instead of text. `--body-file -` reads the body from standard input. In
search and tags, `--group G` alone means `--scope group --group G`.

Exit codes: 0 on success, 1 on an error Munin reports (invalid input, not
found, conflict), 2 on a malformed command line.

### 6.1 Agent protocol

The protocol tells an agent when to search, when to write, and how to pick
`kind`, tags and owner. It is compiled into the binary, so it always matches
the tools of the installed version.

- The MCP server sends it in the `instructions` field on connect; clients
  that support the field put it into the model's context with no setup.
- `munin protocol` prints the same text, for clients that ignore
  `instructions`. It prints only and never edits files.

## 7. Storage and configuration

- Database: `$MUNIN_DB` if set, otherwise `~/.munin/munin.db`. The directory
  and the file are created on first use.
- Project: resolved per call, first match wins:
  1. the `project` argument of the call;
  2. `$MUNIN_PROJECT`;
  3. the server's working directory, which stdio clients set to the
     workspace they start the server for.

  MCP roots are not used: SEP-2577 deprecates them and rmcp will remove
  them.

  A directory resolves to the name of its git root (the nearest ancestor with
  `.git`), or to its own name outside git, normalized like a tag. Two
  repositories with the same directory name share a project; pass `project`
  explicitly to separate them.
- SQLite pragmas: `journal_mode=WAL`, `foreign_keys=ON`,
  `busy_timeout=5000`.
- Schema version kept in `PRAGMA user_version`; migrations are forward-only
  and run on open.
- Configuration from environment variables (database, project, half-life),
  plus one optional file, `$MUNIN_CONFIG` or `~/.munin/config.toml`, for
  task number formats: `[[task]]` rules with a regex `pattern`, an optional
  `tag` template (`gh-$1`; default: the match lower-cased) and a `scope`
  (`all` or `project`). Without the file, or without `[[task]]` rules, the
  built-in formats apply: tracker keys (`PROJ-123`, all projects) and GitHub
  issues (`#6`, `gh-6` -> tag `gh-6`, current project). An invalid file is an
  error for `munin config` and silence for hooks.

## 8. Implementation

- Rust, edition 2024, one package: a library target holding all logic, so
  integration tests in `tests/` can use it, and a thin binary on top.
- `rusqlite` with the `bundled` feature (SQLite built in, FTS5 included).
- `rmcp` for MCP over stdio.
- `clap` for the CLI.
- Decay computed in SQL through a registered scalar function, so ranking and
  limit happen in one query.

## 9. Testing

- Unit: input validation, query formatting, decay.
- Integration on a temporary database: append, search ranking with decay,
  supersede chains in both directions, retract, project scoping, schema
  migration from an empty file.
- MCP: each tool called through the server with valid and invalid input.

## 10. Open questions

None at the moment.
