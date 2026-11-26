# Development

How to check a change to Munin and get it into `main`. Releases are in
[RELEASING.md](RELEASING.md).

## Prerequisites

- Rust, stable toolchain (edition 2024).
- A C compiler: `rusqlite` builds SQLite from source.

## Checks

CI runs the same commands on every push and pull request:

```bash
cargo fmt --all --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
```

Tests never touch `~/.munin`: tests that run the binary give it its own
`HOME`, `MUNIN_DB` and working directory, and remove `MUNIN_*` from its
environment. A new test that runs the binary must do the same.

## Branches

- `main` is always releasable: every commit on it has passed CI.
- Work happens on short-lived branches named by intent (`feat/...`, `fix/...`,
  `docs/...`, `chore/...`) and reaches `main` through a pull request with green
  CI.
- There is no `develop` branch and no release branch. A release is a tag on
  `main`; a fix to a release is a new patch release from `main`.
- Each pull request that changes behaviour adds a line under `## [Unreleased]`
  in [CHANGELOG.md](../CHANGELOG.md).

## CI Workflows

| Workflow | Trigger | What it does |
|----------|---------|--------------|
| `build.yml` | push to `main`, pull request | Formatting and clippy on Linux; tests on Linux and macOS (Apple Silicon) |
| `release.yml` | tag `v*.*.*` | Checks the tag against `Cargo.toml` and `CHANGELOG.md`, runs the checks, builds and smoke-tests the archives, creates the GitHub release |

## Running Locally

```bash
cargo run -- --help
MUNIN_DB=/tmp/munin-dev.db cargo run -- add --kind note --summary "hello"
MUNIN_DB=/tmp/munin-dev.db cargo run -- search hello
```

Point `MUNIN_DB` at a scratch file while developing, so experiments stay out
of the memory you use for real work.

## README Cover

```bash
node scripts/cover.mjs
```

Draws `docs/images/cover.svg` from the wordmark and `docs/images/raven.svg`,
and renders `docs/images/cover.png` at 2560x1280 with headless Chrome
(`CHROME` overrides the path). Node and Chrome are needed for this only, not
for building Munin. Colours: parchment `#ece3cf`, ink `#1d1a17`.
