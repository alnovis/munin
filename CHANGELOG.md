# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html),
with the rules for what counts as breaking in [docs/RELEASING.md](docs/RELEASING.md).

## [Unreleased]

### Added
- `munin hook stop` and `munin hook pre-compact` for Claude Code hooks: after
  a turn that edited files and recorded nothing, the agent is asked once to
  record what it found; before compaction, the summary keeps a list of
  findings not yet in Munin
- docs/INSTALL.md: load Munin's tools at session start with `alwaysLoad`,
  since deferred MCP tools were rarely used

### Changed
- `munin brief` also says that findings are recorded with `append_memory`,
  including when the project has no entries yet

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
