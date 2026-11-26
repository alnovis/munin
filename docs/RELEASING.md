# Releasing

What a version number promises, and how a release is cut.

## Versions

Munin follows [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
Before 1.0 the minor number plays the role of the major one:

| Change | Before 1.0 | From 1.0 |
|--------|-----------|----------|
| Breaking (see below) | minor: `0.3.4 -> 0.4.0` | major |
| New feature, backward compatible | patch | minor |
| Fix | patch | patch |

A change is breaking when something that worked with the previous release
stops working without the user changing anything:

- **Database schema.** A new migration is always breaking: once a newer Munin
  opens the database, older releases refuse it. The changelog entry says so
  under `### Migration`, so the user knows a downgrade needs a backup made
  before the upgrade.
- **MCP tools.** A tool is renamed or removed; a parameter is renamed, removed,
  made required or given a different meaning; a field of the result is renamed
  or removed. Agents call these by name, often from instructions written
  months earlier.
- **CLI.** A command or flag is renamed or removed; a field of `--json` output
  is renamed or removed; an exit code changes meaning. Text output is for
  humans and may change in any release.
- **Configuration.** An environment variable or the default database path
  changes.
- **Behaviour agents rely on.** Default search scope, default limits, or decay
  rules change in a way that returns different entries for the same query.

Adding a tool, an optional parameter, a JSON field or a command is not
breaking. Changing the protocol text in `src/protocol.rs` is not breaking by
itself, but it ships with the tools it describes, never ahead of them.

Pre-releases use a suffix: `v0.2.0-rc.1`. A tag with `-` is published as a
GitHub pre-release.

## Changelog

[CHANGELOG.md](../CHANGELOG.md) follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/): entries go under
`## [Unreleased]` as changes land, grouped as `Added`, `Changed`, `Fixed`,
`Removed`, plus `Migration` when the schema changes. Each entry says what
changed for the user, not which file was edited. The GitHub release notes are
the version's section, copied verbatim.

## Cutting a release

1. Set the version in `Cargo.toml` and run `cargo check` to update
   `Cargo.lock`.
2. In `CHANGELOG.md`, add `## [X.Y.Z] - YYYY-MM-DD` right under
   `## [Unreleased]`, so the unreleased entries move into the new version.
3. Run the checks from [DEVELOPMENT.md](DEVELOPMENT.md#checks).
4. Commit as `chore: release vX.Y.Z`, push `main`, wait for CI, then tag and
   push the tag:

```bash
git tag vX.Y.Z
git push origin vX.Y.Z
```

`release.yml` then:

- fails unless the tag equals `v` + the `Cargo.toml` version and
  `CHANGELOG.md` has a section for it;
- runs formatting, clippy and tests;
- builds `aarch64-apple-darwin` on macOS and `x86_64-unknown-linux-musl` on
  Linux, runs each binary once against a scratch database, and packs it with
  `LICENSE`, `README.md` and `CHANGELOG.md` into
  `munin-vX.Y.Z-<target>.tar.gz` with a `.sha256` next to it;
- creates the GitHub release with the archives and the changelog section.

A failed release is fixed with a new patch version. A tag that has been pushed
is never moved or reused.

## Archives

| Target | Runs on |
|--------|---------|
| `aarch64-apple-darwin` | Macs with Apple Silicon |
| `x86_64-unknown-linux-musl` | any x86_64 Linux; statically linked, no glibc version requirement |

The macOS binary is not notarized. Downloaded with `curl` it runs as is; a
browser download is quarantined by Gatekeeper and needs
`xattr -d com.apple.quarantine munin` once.
