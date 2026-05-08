# Shai troubleshooting guide

Common issues, diagnostics, and solutions for Shai.

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
**Solution:** Verify you can run the agent directly (e.g., `claude --version`). Shai relies on your normal `PATH` resolution to find the executable.

### The agent starts but does not see project context
**Diagnosis:**
1. Run `shai adapters list` and confirm the agent is one of the built-in integrations.
2. If it is not built in, check that `.shai/skills/shai-context.md` was created.

**Solution:**
- For built-in integrations, relaunch with `shai run <agent> ...` so Shai can attach context through the agent's native integration path.
- For other CLI agents, pass `.shai/skills/shai-context.md` to the agent manually using its own instructions or system-prompt mechanism.

## Recording and Sniffing

### No changes recorded in `shai history`
**Diagnosis:**
1. Check if `.shai/` was created in your project root.
2. Ensure the agent is actually outputting JSON-like tool calls. Shai's sniffer looks for balanced `{}` blocks in PTY output.
3. Check `shai status` to see if `changes` count is increasing.

**Solution:**
- Run `shai run <agent>` again.
- Verify the agent has permissions to write files in the current directory.
- If Shai prints a generic-sniffing warning, the agent is emitting a tool-call shape Shai does not fully understand yet. Guard wrappers still cover observable shell commands, but file and tool capture may be partial unless you add a native integration plugin.

### "Ctrl+C" doesn't exit the session
**Problem:** You press Ctrl+C but the agent ignores it or only cancels the current generation without exiting.

**How Shai handles it:**
- Each Ctrl+C is forwarded to the child agent process as normal.
- Some agents (e.g. Copilot) need **two** Ctrl+C presses: the first cancels the running generation, the second exits.
- If the agent is still stuck, press **Ctrl+C three times rapidly** (within 2 seconds). Shai will force-kill the agent process with SIGKILL and print:
  ```
  [shai] Force-killing agent (PID XXXXX) after 3× Ctrl+C
  ```

**If force-kill is unavailable** (PID unknown), Shai will print a message directing you to `kill <pid>` from another terminal.

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
2. Check `SHAI_LOG=debug shai run <agent>` for internal trace logs.
