# Shai quick reference

Shai records project memory locally and exposes it through CLI commands and an interactive PTY wrapper.

## Install and run

**Linux / macOS:**
```bash
curl -fsSL https://raw.githubusercontent.com/awssat/shai/main/install.sh | sh
```

**Windows (PowerShell):**
```powershell
Invoke-Expression (Invoke-WebRequest -Uri "https://raw.githubusercontent.com/awssat/shai/main/install.ps1" -UseBasicParsing).Content
```

**Run an agent:**
```bash
shai run <agent>  # e.g., shai run claude
```

`shai run`:
- Wraps any CLI agent in a pseudo-terminal (PTY)
- Automatically sniffs tool calls and records file changes
- Blocks some observable high-risk shell commands and snapshots simple file targets before destructive commands when possible
- Injects project context through agent-native files, flags, or environment variables when supported
- Writes `.shai/skills/shai-context.md` for manual sharing with other CLI agents
- Warns when generic sniffing sees likely tool payloads it cannot classify

## Supported agent integrations

| Agent | How Shai provides context |
|---|---|
| Claude | `--append-system-prompt-file <file>` |
| Gemini | `GEMINI_SYSTEM_MD=<file>` |
| Copilot | `.github/copilot-instructions.md` |
| Goose | `GOOSE_MOIM_MESSAGE_FILE=<file>` |
| Kilo | `AGENTS.md` |
| OpenCode | `--system "<content>"` |
| Junie | `--skill-location <dir>` |
| Other CLI agents | `.shai/skills/shai-context.md` is written for manual sharing |

## Main commands

| Command | What it does |
|---|---|
| `shai run <agent>` | Run a supported CLI agent with project memory and guardrails |
| `shai history` | Print recent sessions and their changes |
| `shai timeline` / `shai replay` | Print the canonical project event stream |
| `shai log <file>` | Print a file timeline across sessions (with hashes) |
| `shai diff <file>` | Preview what rollback would change |
| `shai rollback <file>` | Restore a file to a previous saved state |
| `shai checkpoint "<label>"` | Record an explicit checkpoint event |
| `shai memory add-fact <key> <content>` | Persist a durable project fact |
| `shai memory add-decision <title> <rationale>` | Persist a durable project decision |
| `shai memory verify-fact <id>` / `verify-decision <id>` | Promote previously recorded memory to verified |
| `shai memory list` | Show ranked branch-aware durable memory |
| `shai search <query>` | Search prompts, paths, and summaries (SQL LIKE) |
| `shai summary` | Build a compact project summary (includes git branch) |
| `shai why <path>` | Explain why a file mattered recently |
| `shai adapters list` | List supported built-in adapters |
| `shai status` | Show project stats and storage health |
| `shai analytics` | Show normalized file/tool activity hotspots |
| `shai gc` | Archive or delete old blobs to reclaim space |
| `shai export <file>` | Export project memory to a portable archive |
| `shai import <file>` | Import sessions from an archive |

## Common examples

```bash
shai run claude -- --model sonnet
shai run goose session -n my-project
shai run kilo run "fix the failing test"
shai history --limit 10
shai timeline --limit 30
shai history --file storage.rs
shai log src/main.rs --limit 20
shai diff src/main.rs --steps 2
shai rollback src/main.rs --steps 2
shai checkpoint "finished parser rewrite"
shai memory add-fact build "cargo test --quiet"
shai memory add-decision "Use timeline events" "Keep one canonical replay stream"
shai memory list
shai memory verify-fact 1
shai search "tree-sitter" --mode all
shai search "instruction" --mode prompt
shai status
shai analytics --file src/storage
shai gc --days 30 --dry-run
shai export memory.ndjson
shai import memory.ndjson
```

## Storage files

Stored in the `.shai/` directory at the project root:
- `timeline.sqlite`: Session history and semantic change logs.
- `blobs.redb`: Compressed content snapshots (Zstd).
- `blobs_archive.redb`: Historical snapshots moved to archive.
- `project_id`: Unique stable identifier for the project.

## Memory migration

```bash
# export from source machine
shai export memory.ndjson

# import on target machine (safe to re-run)
shai import memory.ndjson
```

The archive is newline-delimited JSON. Each line is either one canonical timeline event or one durable memory entity, with base64-encoded snapshot content when needed.

## Limitations

- **Local-first:** Shai is a local-only tool; memory is not synced to clouds automatically.
- **PTY-based:** Only agents that run as standard CLI processes are supported by `shai run`; GUI and editor-integrated runtimes are out of scope.
- **Best-effort observability:** file-change recording depends on tool calls or shell actions Shai can actually observe. Unknown agents get common JSON-shape sniffing plus a warning when Shai sees tool-like payloads it cannot classify.
- **Best-effort guardrails:** risky-command interception only works for tool calls and shell payloads Shai can actually observe.
- **Ctrl+C behaviour:** one Ctrl+C is forwarded to the agent (cancels generation). Some agents need two. Press **Ctrl+C three times rapidly** (within 2 s) to force-kill an unresponsive agent.
- **Copilot instructions file:** `.github/copilot-instructions.md` is runtime-generated by `shai run copilot` and is git-ignored. Do not commit it manually.
