# Install

How to get the `munin` binary and connect it to an agent.

## Table of Contents

- [From a release](#from-a-release)
- [From source](#from-source)
- [Connecting an agent](#connecting-an-agent)
- [Which project an entry lands in](#which-project-an-entry-lands-in)
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

The agent talks to `munin serve` over stdio. Each client below starts it on
its own; nothing has to run in the background. On connect, the server sends
the agent its instructions (the text `munin protocol` prints), so no snippet
has to be copied into `AGENTS.md` or `CLAUDE.md` for clients that pass them
on.

### Claude Code

```bash
claude mcp add --scope user munin -- munin serve
```

`--scope user` makes Munin available in every project. Use the full path to
the binary if `munin` is not on the `PATH` Claude Code sees.

Left alone, an agent rarely uses Munin: Claude Code defers MCP tools, so the
agent sees only their names until it loads them, and nothing prompts it to
record. Load Munin's tools at session start instead (`alwaysLoad`):

```bash
claude mcp add-json munin --scope user \
  '{"type":"stdio","command":"/full/path/to/munin","args":["serve"],"alwaysLoad":true}'
```

Then, in `~/.claude/settings.json`, allow the tools without a prompt each
time, tell the agent at session start what memory holds for the project, ask
it once to record after a turn that edited files and recorded nothing, and
keep unrecorded findings in the summary when the context is compacted:

```json
{
  "permissions": { "allow": ["mcp__munin"] },
  "hooks": {
    "SessionStart": [
      { "hooks": [
        { "type": "command", "command": "/full/path/to/munin brief 2>/dev/null || true", "timeout": 5 }
      ] }
    ],
    "Stop": [
      { "hooks": [
        { "type": "command", "command": "/full/path/to/munin hook stop 2>/dev/null || true", "timeout": 5 }
      ] }
    ],
    "PreCompact": [
      { "hooks": [
        { "type": "command", "command": "/full/path/to/munin hook pre-compact 2>/dev/null || true", "timeout": 5 }
      ] }
    ],
    "UserPromptSubmit": [
      { "hooks": [
        { "type": "command", "command": "/full/path/to/munin hook prompt 2>/dev/null || true", "timeout": 5 }
      ] }
    ]
  }
}
```

`munin hook stop` asks only after a turn with at least 3 edits, or with an
edit among 8 or more tool calls, and only if nothing was recorded since the
prompt (it asks the database, since the transcript lags behind); it asks
once, never in a loop. `munin hook pre-compact` prints instructions for the
compaction summary: list decisions, causes of bugs and measurements not yet
in Munin, so the agent still has them after compaction. `munin hook prompt`
finds task numbers in the prompt (see [Tasks](USAGE.md#tasks)) and adds
their entries, history included, to the context; it adds nothing when the
prompt has no task number or Munin has nothing on it.

If Claude Code's built-in memory is on, say in `~/.claude/CLAUDE.md` which
memory holds what; otherwise the agent keeps writing where it is used to.
For example: project knowledge (decisions, architecture, bugs and fixes,
steps of routine jobs) goes to Munin; facts about the user and how to work with them stay in built-in
memory.

### Codex

`~/.codex/config.toml`:

```toml
[mcp_servers.munin]
command = "munin"
args = ["serve"]
```

### Cursor

`~/.cursor/mcp.json`, or `.cursor/mcp.json` in a project:

```json
{
  "mcpServers": {
    "munin": { "command": "munin", "args": ["serve"] }
  }
}
```

### OpenCode

`~/.config/opencode/opencode.jsonc`:

```jsonc
{
  "mcp": {
    "munin": { "type": "local", "command": ["munin", "serve"], "enabled": true }
  }
}
```

### Agents without MCP (pi)

Agents that call command-line tools instead of MCP servers use Munin through
its CLI. [`skills/munin/SKILL.md`](../skills/munin/SKILL.md) is the protocol
written as `munin` commands; put it where the agent loads skills from, or
link it there so it follows upgrades.

### Clients that ignore server instructions

Run `munin protocol` and put its output into the client's own global
instructions file. Keep it out of a shared repository's `AGENTS.md`: other
contributors' agents would be told to call tools they do not have. Re-run it
after upgrading Munin, since the text follows the tools.

## Which project an entry lands in

Every call resolves the current project, first match wins:

1. the `project` argument of the call (`--project` on the command line);
2. `MUNIN_PROJECT` in the server's environment;
3. the directory the client started the server in: the name of its git root,
   or of the directory itself outside git.

`all` is not a project name: entries of every project are reached with
`--scope all` (`scope: "all"`), and `--project all` is refused.

If a client starts servers somewhere other than the project directory, every
entry lands in one project. Set `MUNIN_PROJECT` in that client's server
configuration, or ask the agent to pass `project`.

## Upgrading

Replace the binary. A release that changes the database schema migrates it
on first open, after which older releases refuse the file; such releases
say so under `Migration` in the changelog. Before installing one, back up the
database as described in [USAGE.md](USAGE.md#backups) if you may want to go
back.
