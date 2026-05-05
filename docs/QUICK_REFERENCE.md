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
- Injects project context and skills guide into the agent's stream on the first prompt
- Requires ZERO configuration files in your repository

## Main commands

| Command | What it does |
|---|---|
| `shai run <agent>` | Run an agent (Claude, Gemini, etc.) with automatic memory |
| `shai history` | Print recent sessions and their changes |
| `shai log <file>` | Print a file timeline across sessions (with hashes) |
| `shai diff <file>` | Preview what rollback would change |
| `shai rollback <file>` | Restore a file to a previous saved state |
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
shai history --limit 10
shai history --file storage.rs
shai log src/main.rs --limit 20
shai diff src/main.rs --steps 2
shai rollback src/main.rs --steps 2
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

The archive is newline-delimited JSON. Each line is one session with its changes and base64-encoded blobs.

## Limitations

- **Local-first:** Shai is a local-only tool; memory is not synced to clouds automatically.
- **PTY-based:** Only agents that run as standard CLI processes are supported by `shai run`.
