# Install

How to get the `munin` binary. Connecting it to an agent is in
[CONNECTING.md](CONNECTING.md).

## Table of Contents

- [From a release](#from-a-release)
- [From source](#from-source)
- [Connecting an agent](#connecting-an-agent)
- [Upgrading](#upgrading)

---

## From a release

Each release on GitHub has an archive per platform and a `.sha256` next to it:

| Archive | Runs on |
|---------|---------|
| `munin-vX.Y.Z-aarch64-apple-darwin.tar.gz` | Macs with Apple Silicon |
| `munin-vX.Y.Z-x86_64-unknown-linux-musl.tar.gz` | any x86_64 Linux, statically linked |

```bash
VERSION=vX.Y.Z
TARGET=aarch64-apple-darwin        # or x86_64-unknown-linux-musl
BASE=https://github.com/alnovis/munin/releases/download/$VERSION
curl -LO "$BASE/munin-$VERSION-$TARGET.tar.gz"
curl -LO "$BASE/munin-$VERSION-$TARGET.tar.gz.sha256"
shasum -a 256 -c "munin-$VERSION-$TARGET.tar.gz.sha256"   # sha256sum -c on Linux
tar -xzf "munin-$VERSION-$TARGET.tar.gz"
install -m 755 "munin-$VERSION-$TARGET/munin" ~/.local/bin/munin
```

Any directory on `PATH` works instead of `~/.local/bin`. The macOS binary is
not notarized: downloaded with `curl` it runs as is, while a browser download
is quarantined by Gatekeeper and needs `xattr -d com.apple.quarantine munin`
once.

## From source

Needs a stable Rust toolchain and a C compiler (SQLite is built from source).

```bash
git clone https://github.com/alnovis/munin.git
cd munin
cargo install --locked --path .      # installs to ~/.cargo/bin/munin
```

Check it:

```bash
munin --version
munin protocol
```

## Connecting an agent

Claude Code, Codex, Cursor, OpenCode and pi each need their own setup to use
Munin and not only reach it: see [CONNECTING.md](CONNECTING.md).

## Upgrading

Replace the binary. A release that changes the database schema migrates it
on first open, after which older releases refuse the file; such releases
say so under `Migration` in the changelog. Before installing one, back up the
database as described in [USAGE.md](USAGE.md#backups) if you may want to go
back.

MCP servers that are already running keep the old binary until their client
restarts them. In Claude Code, reconnect munin with `/mcp`, or restart the
session. A server that finds the database migrated by a newer release
refuses every tool call and says so, instead of failing on entries it
cannot read.

If you put the output of `munin protocol` into an instructions file
([Other clients](CONNECTING.md#other-clients)), re-run it after upgrading.
