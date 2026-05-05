# Shai Ghost Engine

> **⚠️ Disclaimer:** This project is an experimental implementation of an idea and is currently still being tested. Expect rough edges.

Shai is a **Pure Ghost Wrapper** for terminal-based AI agents. It provides a local, high-fidelity project memory layer without requiring any configuration files in your repository or persistent system hooks.

## Key Features

- **Zero Configuration:** No `.claude.json` or `.clinerules` cluttering your repository.
- **PTY Bridge:** Wraps agents in a pseudo-terminal to record every tool call and file change while preserving a native interactive terminal experience.
- **Memory Awakening:** Project history and skills guide are automatically injected into the agent's stream on the first prompt.
- **Semantic Snapshots:** Translates raw file writes into human-readable AST summaries (e.g., "Added struct Storage").
- **Local-First & Secure:** All data stays in your `.shai/` directory.

## Quick Start

**Linux / macOS:**
```bash
curl -fsSL https://raw.githubusercontent.com/awssat/shai/main/install.sh | sh
```

**Windows (PowerShell):**
```powershell
Invoke-Expression (Invoke-WebRequest -Uri "https://raw.githubusercontent.com/awssat/shai/main/install.ps1" -UseBasicParsing).Content
```

**Run your favorite agent through Shai:**
```bash
shai run claude
```

## Core Skills

| Command | What it does |
|---|---|
| `shai run <agent>` | The main entry point. Wraps any CLI agent with memory. |
| `shai history` | See the chronological timeline of sessions and changes. |
| `shai log <file>` | Trace the evolution of a specific file with content hashes. |
| `shai search "<q>"` | Blazing fast SQL-backed search across all sessions. |
| `shai summary` | Get a compact digest of recent project activity. |
| `shai why <path>` | Understand the reasoning behind a file's recent changes. |
| `shai diff <file>` | Preview what a rollback would change before applying it. |
| `shai rollback <file>` | Instantly restore a file to any previous version. |
| `shai status` | Monitor project health and storage compression. |
| `shai analytics` | Show normalized file/tool activity hotspots. |
| `shai gc` | Archive or delete old blobs to reclaim space. |
| `shai export` / `import` | Backup or share memory archives for team collaboration. |

## Documentation

- `docs/INDEX.md` — Complete documentation map.
- `docs/QUICK_REFERENCE.md` — Command usage and examples.
- `docs/SCHEMA.md` — Details on the hybrid SQLite/redb storage model.
- `docs/TROUBLESHOOTING.md` — Solutions for common terminal and sniffer issues.

## Supported Agents

Shai officially supports any agent that runs as a CLI process, including:
- Claude Code (`claude`)
- Gemini CLI (`gemini`)
- Copilot CLI (`ghcs`)
- OpenCode
- Any custom script or shell process.

---

*Shai: The invisible brain for your AI agents.*
