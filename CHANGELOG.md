# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html),
with the rules for what counts as breaking in [docs/RELEASING.md](docs/RELEASING.md).

## [Unreleased]

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
