# Shai Storage Schema

Shai uses a hybrid storage model: SQLite for relational session metadata and redb for compressed content blobs.

## SQLite: `timeline.sqlite`

### Table: `sessions`

Records logical agent runs.

| Column | Type | Description |
|---|---|---|
| `id` | INTEGER | Primary key |
| `session_key` | TEXT | Session identifier from the PTY wrapper |
| `project_id` | TEXT | Stable project identifier |
| `llm` | TEXT | Agent command/model family |
| `agent_family` | TEXT | Normalized family (`anthropic`, `google`, etc.) |
| `agent_name` | TEXT | Specific tool name (`claude-code`, `gemini-cli`) |
| `started_at` | DATETIME | Session start timestamp |
| `closed_at` | DATETIME | Session end timestamp |

### Table: `timeline_events`

Canonical ordered event stream for the project.

| Column | Type | Description |
|---|---|---|
| `id` | INTEGER | Primary key |
| `project_id` | TEXT | Stable project identifier |
| `session_id` | INTEGER | FK to `sessions.id` |
| `seq_in_session` | INTEGER | Per-session ordering key |
| `event_kind` | TEXT | Event type (`session_started`, `prompt_submitted`, `tool_called`, `file_snapshot`, `checkpoint_created`, `guard_blocked`, `guard_allowed`, `session_closed`) |
| `timestamp` | DATETIME | Event timestamp |
| `actor_family` | TEXT | Normalized actor family |
| `actor_name` | TEXT | Actor name/adapter |
| `file_path` | TEXT | Optional path for file-backed events |
| `blob_hash` | TEXT | Optional BLAKE3 content hash for snapshot events |
| `tool_name` | TEXT | Optional observed tool name |
| `summary` | TEXT | Human-readable event summary |
| `payload_json` | TEXT | Optional raw JSON payload |
| `storage_kind` | TEXT | Snapshot storage mode |
| `base_event_id` | INTEGER | Optional parent snapshot reference |
| `raw_bytes` | INTEGER | Uncompressed payload size |
| `stored_bytes` | INTEGER | Stored payload size |

### Memory tables

- `memory_facts` — verified project facts and conventions
- `memory_decisions` — recorded architecture/implementation decisions
- `memory_refs` — branch and other scope references linked to memory entities

These memory entities are exported and imported alongside timeline events.

`memory_refs` columns:

| Column | Type | Description |
|---|---|---|
| `id` | INTEGER | Primary key |
| `project_id` | TEXT | Stable project identifier |
| `ref_kind` | TEXT | Scope kind (for example `branch`) |
| `ref_value` | TEXT | Scope value (for example `main`) |
| `target_kind` | TEXT | Target table kind (`fact` or `decision`) |
| `target_id` | INTEGER | Referenced `memory_facts.id` or `memory_decisions.id` |

### Table: `internal_state`

Key-Value store for system configuration.

| Key | Value | Purpose |
|---|---|---|
| `schema_version` | `11` | Migration tracking |

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
