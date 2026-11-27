# Connecting an Agent

How to connect Munin to a coding agent so that it is not only reachable but
used: the agent sees the tools, gets Munin's instructions, is told at session
start what memory holds, and records without being asked.

## Table of Contents

- [How agents reach Munin](#how-agents-reach-munin)
- [At a glance](#at-a-glance)
- [Claude Code](#claude-code)
- [Codex](#codex)
- [Cursor](#cursor)
- [OpenCode](#opencode)
- [pi](#pi)
- [Other clients](#other-clients)
- [Which project an entry lands in](#which-project-an-entry-lands-in)
- [Checking that it works](#checking-that-it-works)

---

## How agents reach Munin

An agent with MCP support talks to `munin serve` over stdio. The client
starts it on its own; nothing has to run in the background. On connect, the
server sends the agent its instructions (the text `munin protocol` prints),
so for clients that pass them on, no snippet has to be copied into
`AGENTS.md` or `CLAUDE.md`.

An agent that calls command-line tools instead uses the `munin` CLI, with
[`skills/munin/SKILL.md`](../skills/munin/SKILL.md): the same protocol
written as `munin` commands.

Connecting is rarely enough. Agents load MCP tools on demand, so they see
only names until they decide to look, and nothing prompts them to record.
Each section below therefore has the same parts: connect, make the agent use
Munin, and, where the client has hooks, run Munin's hook commands:

| Command | Prints | Input |
|---------|--------|-------|
| `munin brief` | what memory holds for the current project | none |
| `munin hook prompt` | entries for task numbers in the prompt (see [Tasks](USAGE.md#tasks)) | JSON on stdin with `prompt` and `cwd` |
| `munin hook stop` | a request to record, in Claude Code's format | a Claude Code Stop hook input and transcript |
| `munin hook pre-compact` | instructions for the compaction summary | none |

## At a glance

| | Claude Code | Codex | Cursor | OpenCode | pi |
|---|---|---|---|---|---|
| Connects through | MCP | MCP | MCP | MCP | MCP or CLI |
| Munin's instructions reach the model | yes | yes | not documented | yes | no, via `AGENTS.md` |
| Server starts in the project directory | yes | yes | not documented | yes | yes |
| Brief at session start | hook | hook | hook | no | no |
| Entries for task numbers in the prompt | hook | hook | no | no | no |
| Reminder to record | hook | no | no | no | no |
| Unrecorded findings kept on compaction | hook | no | no | no | no |

"No" means Munin ships nothing for it yet. Cursor, OpenCode and pi have hook
or plugin APIs that could carry the rest; see their sections.

## Claude Code

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
steps of routine jobs) goes to Munin; facts about the user and how to work
with them stay in built-in memory.

## Codex

```bash
codex mcp add munin -- munin serve
```

This writes `[mcp_servers.munin]` to `~/.codex/config.toml`. Add two keys
there by hand:

```toml
[mcp_servers.munin]
command = "munin"
args = ["serve"]
omit_tools_from = ["deferred"]
default_tools_approval_mode = "approve"
```

- `omit_tools_from = ["deferred"]`: on current models Codex defers MCP tools
  behind its tool search; this lists Munin's tools directly instead.
- `default_tools_approval_mode = "approve"`: Munin's tools run without a
  prompt. Under the default (`auto`), recording asks each time.

Codex sends Munin's instructions to the model as the description of the
`munin` tool namespace, and starts the server in the session's directory.

Hooks go in `~/.codex/hooks.json`. Codex adds the plain output of
`SessionStart` and `UserPromptSubmit` hooks to the model's context, and its
`UserPromptSubmit` input carries `prompt` and `cwd`, so `munin brief` and
`munin hook prompt` work as they are:

```json
{
  "hooks": {
    "SessionStart": [
      { "matcher": "startup|resume", "hooks": [
        { "type": "command", "command": "/full/path/to/munin brief 2>/dev/null || true", "timeout": 5 }
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

Codex runs a hook only after it has been reviewed and trusted: open `/hooks`
in Codex once after adding them, and again after changing them.
`munin hook stop` reads Claude Code transcripts and does nothing useful here;
`PreCompact` hooks in Codex ignore plain output, so `munin hook pre-compact`
has no effect either.

The global instructions file is `~/.codex/AGENTS.md`.

## Cursor

`~/.cursor/mcp.json`, or `.cursor/mcp.json` in a project:

```json
{
  "mcpServers": {
    "munin": { "type": "stdio", "command": "munin", "args": ["serve"] }
  }
}
```

Cursor's documentation does not say which directory it starts servers in.
If entries land in one project named after your home directory (see
[Checking that it works](#checking-that-it-works)), configure Munin per
project instead, in `.cursor/mcp.json`, and name the project:

```json
{
  "mcpServers": {
    "munin": {
      "type": "stdio",
      "command": "munin",
      "args": ["serve"],
      "env": { "MUNIN_PROJECT": "${workspaceFolderBasename}" }
    }
  }
}
```

`${workspaceFolderBasename}` is the name of the folder that holds
`.cursor/mcp.json`. Keep that file out of a shared repository, or check that
contributors without Munin do not mind it.

Cursor asks before every MCP tool call by default. To allow Munin's tools,
set Run Mode to Allowlist (Settings > Agents > Approvals & Execution) and
add them in `~/.cursor/permissions.json`:

```json
{ "mcpAllowlist": ["munin:*"] }
```

The Cursor CLI reads `~/.cursor/cli-config.json` instead:

```json
{ "permissions": { "allow": ["Mcp(munin:*)"] } }
```

Cursor loads MCP tools on demand, and its documentation does not say whether
server instructions reach the model. Paste the output of `munin protocol`
into User Rules (Customize > Rules), so the agent knows when to search and
record.

The brief at session start goes through a `sessionStart` hook, which expects
JSON with `additional_context`. `~/.cursor/hooks.json`:

```json
{
  "version": 1,
  "hooks": {
    "sessionStart": [ { "command": "./hooks/munin-brief.sh" } ]
  }
}
```

`~/.cursor/hooks/munin-brief.sh` (user hooks run from `~/.cursor/`; the
script needs `jq` and must be executable):

```sh
#!/bin/sh
munin brief 2>/dev/null | jq -Rs '{additional_context: .}'
```

Cursor's `beforeSubmitPrompt` cannot add context and its `preCompact` hook
only observes, so `munin hook prompt` and `munin hook pre-compact` have no
place here. Its `stop` hook can send a follow-up message, but
`munin hook stop` reads Claude Code transcripts and does not work with
Cursor's.

By default Cursor also runs the Claude Code hooks from
`~/.claude/settings.json` ("Include Third-Party Plugins, Skills, and Other
Configs"). With Munin set up for both, check that the brief does not arrive
twice.

## OpenCode

`~/.config/opencode/opencode.json`, or `opencode.json` in a project:

```jsonc
{
  "$schema": "https://opencode.ai/config.json",
  "mcp": {
    "servers": {
      "munin": { "type": "local", "command": ["munin", "serve"], "codemode": false }
    }
  }
}
```

`opencode mcp add --global munin -- munin serve` writes the same entry
without `codemode`. This is the OpenCode 2 format; OpenCode 1 put servers
directly under `mcp` with `"enabled": true`, which OpenCode 2 still reads.

- `"codemode": false` lists Munin's tools as ordinary tools. By default
  OpenCode reaches MCP tools through its `execute` tool instead, as
  `tools.munin.search_memory(...)`, which also works.
- OpenCode adds Munin's instructions to the system prompt, starts the server
  in the project directory and allows MCP tools without a prompt by default.

Hooks need an OpenCode plugin (`~/.config/opencode/plugins/`); Munin does not
ship one. Without a global `~/.config/opencode/AGENTS.md`, OpenCode reads
`~/.claude/CLAUDE.md`, so a note there on which memory holds what applies to
both.

## pi

pi 0.99.0 and later connect to MCP servers on their own:

```bash
pi mcp add munin --exposure direct -- munin serve
```

This writes to `~/.pi/agent/mcp.json`:

```json
{
  "mcpServers": {
    "munin": { "command": "munin", "args": ["serve"], "exposure": "direct" }
  }
}
```

By default (`codemode`) pi does not declare MCP tools to the model at all;
`direct` lists Munin's tools like built-in ones. pi starts the server in the
session's directory.

pi does not pass server instructions on to a server whose tools are direct
(in the other modes, only their first line). Put the output of
`munin protocol` into `~/.pi/agent/AGENTS.md`, and re-run it after upgrading
Munin.

Through the CLI instead, link the skill into pi's skills directory, so it
follows the checkout:

```bash
mkdir -p ~/.pi/agent/skills
ln -s /path/to/munin/skills/munin ~/.pi/agent/skills/munin
```

pi lists each skill's name and description in the system prompt and reads
the skill when a task matches. It runs shell commands without asking, so
`munin` needs no allowlist. The release archives do not include the skill;
take `skills/munin/SKILL.md` from the repository at the same tag.

Use one of the two, not both: with MCP and the skill together the agent gets
the same instructions twice, once for tools and once for commands.

pi extensions (`~/.pi/agent/extensions/`, TypeScript) can run a command at
session start and ask the agent to continue after it stops; Munin does not
ship one.

## Other clients

A client that starts stdio MCP servers can run `munin serve`. If it does not
pass server instructions on, run `munin protocol` and put its output into
the client's own global instructions file. Keep it out of a shared
repository's `AGENTS.md`: other contributors' agents would be told to call
tools they do not have. Re-run it after upgrading Munin, since the text
follows the tools.

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

## Checking that it works

In a project, ask the agent what Munin holds for it; it should answer from
`search_memory` or the brief, not from the files. After a session that
settled something, look at what was recorded and where:

```bash
munin list --scope all --limit 5
```

Each entry shows its project. Entries under your home directory's name, or
under one name for every project, mean the client starts the server
elsewhere: see [Which project an entry lands in](#which-project-an-entry-lands-in).
