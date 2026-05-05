# Shai Storage Schema

Shai uses a hybrid storage model: SQLite for relational session metadata and redb for compressed content blobs.

## SQLite: `timeline.sqlite`

### Table: `sessions`

Records logical agent conversations.

| Column | Type | Description |
|---|---|---|
| `id` | INTEGER | Primary key |
| `session_key` | TEXT | Unique key from the agent process |
| `project_id` | TEXT | Stable project identifier |
| `llm` | TEXT | The model being used (e.g. `claude-3-5-sonnet`) |
| `agent_family` | TEXT | Normalized family (`anthropic`, `google`, etc) |
| `agent_name` | TEXT | Specific tool name (`claude-code`, `gemini-cli`) |
| `prompt` | TEXT | The user's input message |
| `started_at` | DATETIME | Session start timestamp |
| `closed_at` | DATETIME | Session end timestamp (NULL if still active) |

### Table: `changes`

Records every file modification captured by the sniffer.

| Column | Type | Description |
|---|---|---|
| `id` | INTEGER | Primary key |
| `session_id` | INTEGER | FK to `sessions.id` |
| `file_path` | TEXT | Path relative to project root |
| `blob_hash` | TEXT | BLAKE3 hash of the new content |
| `ast_summary` | TEXT | Semantic digest (e.g. "Added function handle_error") |
| `tool_name_norm` | TEXT | Normalized tool used (`Write`, `Delete`, `Search`) |
| `raw_bytes` | INTEGER | Uncompressed file size |
| `stored_bytes` | INTEGER | Zstd-compressed size in redb |
| `timestamp` | DATETIME | Capture time |

### Table: `internal_state`

Key-Value store for system configuration.

| Key | Value | Purpose |
|---|---|---|
| `schema_version` | `10` | Migration tracking |

## redb: `blobs.redb`

Stores the actual file content snapshots.

- **Table:** `code_blobs`
- **Key:** `String` (BLAKE3 hex hash)
- **Value:** `Vec<u8>` (Zstd-compressed content)

## redb: `blobs_archive.redb`

Temporary home for snapshots marked for deletion by `shai gc` (if run without `--delete`).

- **Table:** `code_blobs`
- **Key:** `String` (BLAKE3 hex hash)
- **Value:** `Vec<u8>` (Zstd-compressed content)
