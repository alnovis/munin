# AGENTS.md

Guidance for AI coding agents working in this repository. Humans are welcome to
read it too.

## Project

munin is local, single-binary memory for AI coding agents: an MCP server over
stdio plus a CLI, backed by one SQLite file. The design is in
[docs/SPEC.md](docs/SPEC.md); read it before changing behaviour, and update it
in the same change when behaviour changes.

## Principles

- **Nothing is overwritten.** Entry content is immutable; changes are new
  entries that supersede old ones, or a retract with a reason.
- **Local only.** No network access, no telemetry.
- **Errors the agent can act on.** Reject bad input with a message naming the
  field and what is allowed; never coerce it silently.

## Before calling a change done

Run at the repository root; all three must pass cleanly:

```sh
cargo fmt --all
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
```

A change that users or agents can notice also gets a line under
`## [Unreleased]` in [CHANGELOG.md](CHANGELOG.md). Whether it is breaking is
decided by [docs/RELEASING.md](docs/RELEASING.md); branches and CI are in
[docs/DEVELOPMENT.md](docs/DEVELOPMENT.md).

## Conventions

- **Search with `rg`, not `grep -r`.** ripgrep honours `.gitignore` and skips
  `target/`, so build output does not flood the context.
- **ASCII only** in code, comments, config and docs, this file and the README
  included. Use `->`, `--`, `...`, `x`, `>=` and `<=` instead of their unicode
  forms.
- **Comment like the file already does:** match its form, length and density.
  Comments say what the code does and what would surprise the next reader; the
  reason behind a decision goes in the commit message or the PR description.
- **Reference only what is tracked in git.** Docs and comments must not point to
  local files that other contributors do not have.
- **Schema changes are forward-only migrations.** Never edit a migration that
  has been released.

## Git

- Do not commit, push, tag, merge or rebase unless the human you work with
  explicitly asks. Leave changes in the working tree and report what changed.
