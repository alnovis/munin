# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html),
with the rules for what counts as breaking in [docs/RELEASING.md](docs/RELEASING.md).

## [Unreleased]

### Added
- docs/CONNECTING.md: setting up Claude Code, Codex, Cursor, OpenCode and pi
  -- connection, permissions, where Munin's instructions go, which hooks
  work -- moved out of docs/INSTALL.md and extended to the agents that had
  only a config snippet; pi now through its built-in MCP or the CLI skill

## [0.2.3] - 2026-11-27

### Fixed
- A running MCP server whose database a newer munin has migrated since it
  started refuses every tool call with a message to reconnect the server in
  the agent (for example `/mcp` in Claude Code) or restart the session; it
  used to keep serving and fail on entries of a kind it did not know

## [0.2.2] - 2026-11-27

### Fixed
- `--project all` (and `project: "all"` in MCP tools) is refused with a
  pointer to `--scope all`; it used to search a project named `all` and
  return nothing, which read as an empty database

## [0.2.1] - 2026-11-27

### Changed
- The agent protocol and the CLI skill say when to use a `howto`: search for
  one before a routine job done in steps (release, deploy, a migration), and
  record steps the user had to explain or that took several tries

## [0.2.0] - 2026-11-27

### Added
- Entry kind `howto`: steps to do something again -- commands in order,
  `<placeholders>` for what changes each time. Like `decision` and
  `architecture`, it never fades. Accepted by `munin add --kind`,
  `munin list --kind` and `append_memory`

### Migration
- Schema 2 rebuilds the `entries` table to allow the new kind; ids, links,
  tags and events are kept. Once 0.2 opens the database, 0.1 releases refuse
  it: back up `~/.munin/munin.db` before upgrading if a downgrade may be
  needed (see docs/USAGE.md#backups)

## [0.1.2] - 2026-11-27

### Added
- `munin hook stop` and `munin hook pre-compact` for Claude Code hooks: after
  a turn that edited files and recorded nothing, the agent is asked once to
  record what it found; before compaction, the summary keeps a list of
  findings not yet in Munin
- docs/INSTALL.md: load Munin's tools at session start with `alwaysLoad`,
  since deferred MCP tools were rarely used
- Tasks: work with a task number (`PROJ-123`, `gh-6`) is recorded under it --
  number in the summary and as a tag, body as Summary / Root cause /
  Implementation -- in the protocol, the CLI skill and docs/USAGE.md
- `munin hook prompt` for a UserPromptSubmit hook: a prompt that mentions a
  task number gets that task's entries, history included, as context;
  tracker keys are looked up in all projects, GitHub numbers in the current one
- Config file `~/.munin/config.toml` (or `$MUNIN_CONFIG`), optional: task
  number formats as `[[task]]` rules (regex `pattern`, `tag` template, `scope`
  all or project), replacing the built-in tracker-key and GitHub formats;
  `munin config` shows the formats in effect, and `munin brief` lists
  configured ones for the agent

### Changed
- `munin brief` also says that findings are recorded with `append_memory`,
  including when the project has no entries yet
- The agent protocol grew a TASKS section; it is still under Claude Code's
  4096-character cap for server instructions, which a test now checks

### Fixed
- `list`, `search` and `tags` with `--scope all` (and the MCP tools
  `search_memory` and `get_active_tags` with `scope: all`) no longer fail in
  a directory that names no project, such as `/`: that scope does not depend
  on the current project, so it is no longer resolved

## [0.1.1] - 2026-11-27

### Added
- `munin list`: every entry in scope, newest first, without a search query;
  filters `--scope`, `--group`, `--kind`, `--inactive`, `--limit`

## [0.1.0] - 2026-11-27

### Added
- Memory store in one SQLite file (`~/.munin/munin.db`, or `$MUNIN_DB`):
  entries of kind `decision`, `architecture`, `bugfix` and `note`, owned by a
  project or by a group of projects
- Nothing is overwritten: an entry is replaced by a new one that supersedes it,
  or retracted with a reason; triggers in the database refuse edits and deletes
- Full-text search ranked by BM25 times age decay (30-day half-life for
  `bugfix` and `note`, none for `decision` and `architecture`), applied to every
  match before the limit
- Search scopes `project`, `group` and `all`; a project in a group searches the
  whole group by default
- Tag listing by recent use, so agents reuse tags instead of inventing synonyms
- CLI: `search`, `show`, `add`, `retract`, `tags`, `group`, `protocol`, with
  text or `--json` output
- MCP server over stdio (`munin serve`) with `search_memory`, `get_entry`,
  `append_memory`, `retract_memory`, `get_active_tags` and `list_groups`; the
  agent protocol is sent as the server's `instructions`
- Release archives for `aarch64-apple-darwin` and `x86_64-unknown-linux-musl`
- `munin brief`: what memory holds for the current project (counts by kind,
  tags in use, newest decisions), worded as context for a SessionStart hook
- `skills/munin/SKILL.md` for agents without MCP, such as pi: the same
  protocol, written as munin CLI commands
