# Shai docs index

This folder contains the detailed docs for the Shai Ghost Engine.

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
| `SHAI_ARCHITECTURE.txt` | Developers | ASCII architecture sketch of the Ghost Engine |
| `GC_GUIDE.md` | Developers | Details on blob archiving and space reclamation |
| `OPTIMIZATION_INSIGHTS.md` | Operations | Performance limits, compression, and PTY sniffer logic |

## The Ghost Philosophy

Shai operates as a **Pure Ghost Wrapper**:
- **Zero Configuration:** No `.claude.json` or `.github/` files added to your project.
- **PTY-based Sniffing:** Passively observes terminal traffic to record changes without explicit hooks.
- **In-flight Injection:** Project memory is whispered to the agent via `stdin` during runtime.
- **CLI-as-a-Skill:** Capabilities are provided as standard terminal commands in the agent's `$PATH`.
