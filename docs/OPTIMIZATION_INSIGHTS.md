# Shai optimization insights

This document lists optimizations that are actually visible in the current Shai codebase, plus the practical limits they still leave in place.

## Implemented optimizations

### Passive Sniffing (PTY Bridge)

- **Byte-Safe Proxy:** Shai wraps agents in a pseudo-terminal (PTY) that preserves native ANSI colors and progress bars while sniffing bytes asynchronously.
- **Balanced JSON Scanning:** The sniffer uses a brace-depth algorithm to identify valid tool-call JSON blocks in the middle of standard conversational text without corrupting the output stream.
- **Background Serialization:** File hashing, semantic parsing, and database commits happen on a dedicated background worker thread to ensure zero UI lag for the user.

### Storage

- **Content-Addressable Snapshots:** BLAKE3-based content identity avoids duplicate writes for unchanged content across different files or sessions.
- **Native Zstd Compression:** Every file snapshot is compressed with Zstd (level 3) before being stored in the `redb` blob store.
- **Atomic Transactions:** Every session start and file change is wrapped in an atomic SQLite transaction to protect integrity during abrupt process exits (e.g., Ctrl+C).

### Semantic Parsing

- **AST-level Summarization:** `record_change` routes file writes to per-language tree-sitter parsers (Rust, Python, TS, JS, Go, Java, C++, Ruby, Swift, Kotlin, Markdown).
- **Symbol Extraction:** Instead of raw diffs, Shai records changes like "Added struct Storage" or "Modified method get_status", which are significantly more useful for AI context.

### Search and Context

- **Budgeted Retrieval:** Search results and summary reports are strictly clipped to a character budget (e.g., 2,500 chars) to prevent overwhelming the AI's token limit.
- **Native Agent Injection:** Shai writes or passes context using the agent's supported file, flag, or environment-variable path when available.

## Real limits

- **Local-first:** Database access is single-machine; concurrent access is handled by SQLite WAL mode but not distributed across a network.
- **Disk Latency:** While sniffs are async, very high-frequency file writes (e.g., automated refactoring of thousands of files) may saturate the background worker's queue.
- **CLI-only Runtime Wrapper:** GUI-based agents are outside `shai run`'s PTY model. Shai's current runtime integration is for terminal-launched agents.

## Operational Maintenance

Use `shai status` and `shai analytics` to monitor the health of these optimizations:
- **Compression Ratio:** `shai status` shows the real-world multiplier you are getting from Zstd.
- **Storage Hotspots:** `shai analytics` identifies which files are generating the most snapshots (good candidates for `shai gc`).
