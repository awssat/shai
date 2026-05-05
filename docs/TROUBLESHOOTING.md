# Shai troubleshooting guide

Common issues, diagnostics, and solutions for the Shai Ghost Engine.

## Installation and execution

### `command not found: shai`
**Solution:** Ensure the binary is installed and in your `$PATH`. You can install it quickly using the official scripts:

**Linux / macOS:**
```bash
curl -fsSL https://raw.githubusercontent.com/awssat/shai/main/install.sh | sh
```

**Windows (PowerShell):**
```powershell
Invoke-Expression (Invoke-WebRequest -Uri "https://raw.githubusercontent.com/awssat/shai/main/install.ps1" -UseBasicParsing).Content
```

### `shai run claude` fails to spawn
**Problem:** The agent command is not installed or not in your path.
**Solution:** Verify you can run the agent directly (e.g., `claude --version`). Shai uses your environment's shell to locate the agent.

## Recording and Sniffing

### No changes recorded in `shai history`
**Diagnosis:**
1. Check if `.shai/` was created in your project root.
2. Ensure the agent is actually outputting JSON tool calls. Shai's sniffer looks for balanced `{}` blocks.
3. Check `shai status` to see if `changes` count is increasing.

**Solution:**
- Run `shai run <agent>` again.
- Verify the agent has permissions to write files in the current directory.

### "Ctrl+C" kills the whole session
**Problem:** You want to stop a long AI response but stay in Shai.
**Solution:** Shai is designed to pass `SIGINT` (Ctrl+C) directly to the child agent. If the agent process exits on Ctrl+C, the Shai session will also end. Modern agents like `claude-code` usually catch Ctrl+C to stop generation without exiting.

## Search and Retrieval

### `shai search` returns nothing
**Diagnosis:**
1. Run `shai history` to ensure sessions have been recorded.
2. Remember that search uses **SQL LIKE** substring matching. It does **not** support regex.
3. Use `%` as a wildcard if needed, e.g., `shai search "feat%storage"`.

### Search is slow
**Solution:**
- Run `shai gc --days 30` to remove old content blobs and shrink the database indexes.
- Use `--limit` to reduce the number of sessions scanned.

## Storage and Database

### "database disk image is malformed"
**Solution:** SQLite database corruption is rare but can happen during hard power loss.
```bash
# 1. Backup
cp -r .shai .shai.bak
# 2. Re-initialize (WARNING: This loses history)
rm -rf .shai/
shai run <agent>
```

### `.shai/blobs.redb` is too large
**Solution:** 
- Shai uses Zstd compression, but many snapshots of large files can add up.
- Run `shai gc --days 14 --delete` to permanently reclaim space.

## Performance baselines

| Operation | Healthy | Warning |
|-----------|----------|----------|
| `shai history` | < 0.2s | > 2s |
| `shai search` | < 0.5s | > 3s |
| `shai run` startup | < 0.1s | > 1s |
| `.shai` size | < 200MB | > 1GB |

## Getting Help

If you encounter an unrecoverable error:
1. Run `shai status` and `shai analytics` to collect environment data.
2. Check `RUST_LOG=debug shai run <agent>` for internal trace logs.
