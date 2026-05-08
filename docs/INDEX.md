# Shai docs index

This folder contains the detailed docs for Shai.

## Start here

**For users:**
1. Read `../README.md` for the top-level overview.
2. Read `QUICK_REFERENCE.md` for command usage and common examples.
3. Read `TROUBLESHOOTING.md` if something doesn't work.

**For developers:**
1. Read `SCHEMA.md` for database structure and relationships.
2. Read `SHAI_ARCHITECTURE.txt` for a high-level component map.
3. Read `GC_GUIDE.md` for the garbage collection strategy.

**For performance:**
1. Read `OPTIMIZATION_INSIGHTS.md` for storage and sniffer limits.

## Files

| File | Audience | Purpose |
|---|---|---|
| `QUICK_REFERENCE.md` | Users | Command reference, setup, and examples |
| `TROUBLESHOOTING.md` | Users | Common issues and solutions |
| `SCHEMA.md` | Developers | SQLite schema, table relationships, and storage model |
| `SHAI_ARCHITECTURE.txt` | Developers | ASCII architecture sketch of the runtime wrapper and storage flow |
| `GC_GUIDE.md` | Developers | Details on blob archiving and space reclamation |
| `OPTIMIZATION_INSIGHTS.md` | Operations | Performance limits, compression, and PTY sniffer logic |

## Operating model

Shai operates as a local wrapper around CLI agents:
- **Project-local storage:** Timeline data, durable memory, and file snapshots live in `.shai/`.
- **PTY-based capture:** Shai observes prompts, tool payloads, and agent output from a wrapped terminal session.
- **Native agent integrations:** Context is injected using each supported agent's expected file, flag, or environment variable.
- **Shell guardrails:** Shai prepends PATH wrappers so risky shell commands can be blocked or snapshotted before execution when observable.
- **CLI workflow:** Agents can query and extend memory using normal `shai` commands.
