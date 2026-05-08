use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub(super) fn open_wal_connection(shai_dir: &std::path::Path) -> Connection {
    let conn = Connection::open(shai_dir.join("timeline.sqlite")).expect("Failed to open SQLite");
    conn.execute_batch(
        "PRAGMA journal_mode=WAL; PRAGMA busy_timeout=5000; PRAGMA synchronous=NORMAL;",
    )
    .expect("Failed to enable WAL mode");
    conn
}

pub struct Storage {
    pub shai_dir: PathBuf,
    pub(super) project_id_cache: std::sync::Mutex<Option<String>>,
    pub(super) content_store: super::content_store::RedbContentStore,
    pub(super) gitignore_cache: std::sync::OnceLock<ignore::gitignore::Gitignore>,
}

pub struct StorageConn(pub(super) Connection);

impl std::ops::Deref for StorageConn {
    type Target = Connection;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::ops::DerefMut for StorageConn {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

#[derive(Debug)]
pub enum BlobLoadError {
    Missing,
    DeltaCorrupted(String),
}

impl BlobLoadError {
    pub fn message(&self) -> String {
        match self {
            Self::Missing => "blob metadata missing".to_string(),
            Self::DeltaCorrupted(reason) => format!("delta blob corrupted: {}", reason),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SearchMode {
    All,
    Prompt,
    Summary,
    Path,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SessionRecord {
    pub id: i64,
    pub session_key: String,
    pub llm: String,
    pub prompt: String,
    pub started_at: String,
    pub changes: Vec<ChangeRecord>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ChangeRecord {
    pub file_path: String,
    pub blob_hash: String,
    pub ast_summary: String,
    pub tool_name: String,
    pub timestamp: String,
    pub agent_family: String,
    pub agent_name: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TimelineEventRecord {
    pub id: i64,
    pub project_id: String,
    pub session_id: i64,
    pub seq_in_session: i64,
    pub event_kind: String,
    pub timestamp: String,
    pub actor_family: String,
    pub actor_name: String,
    pub file_path: Option<String>,
    pub blob_hash: Option<String>,
    pub tool_name: Option<String>,
    pub summary: String,
    pub payload_json: Option<String>,
    pub raw_bytes: u64,
    pub stored_bytes: u64,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ExportEventRecord {
    pub session_key: String,
    pub llm: String,
    pub started_at: String,
    pub closed_at: Option<String>,
    pub event: TimelineEventRecord,
    pub blob_content_base64: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct MemoryFactRecord {
    pub id: i64,
    pub project_id: String,
    pub fact_key: String,
    pub content: String,
    pub verified: bool,
    pub source: String,
    pub created_at: String,
    pub branch_refs: Vec<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct MemoryDecisionRecord {
    pub id: i64,
    pub project_id: String,
    pub title: String,
    pub rationale: String,
    pub alternatives: String,
    pub status: String,
    pub verified: bool,
    pub created_at: String,
    pub branch_refs: Vec<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "record_type", rename_all = "snake_case")]
pub enum ExportRecord {
    Event(Box<ExportEventRecord>),
    MemoryFact(MemoryFactRecord),
    MemoryDecision(MemoryDecisionRecord),
}

#[derive(Debug, Clone)]
pub struct ProjectTimelineRecord {
    pub session_key: String,
    pub llm: String,
    pub event: TimelineEventRecord,
}

pub struct FileChangeRecord {
    pub timestamp: String,
    pub tool_name: String,
    pub ast_summary: String,
    pub blob_hash: String,
    pub prompt: Option<String>,
    pub llm: Option<String>,
}

pub struct StatusInfo {
    pub total_sessions: usize,
    pub open_sessions: usize,
    pub total_changes: usize,
    pub unique_files: usize,
    pub project_id: String,
    pub raw_bytes: u64,
    pub stored_bytes: u64,
    pub compression_ratio: f64,
    pub last_prompt: Option<String>,
    pub last_checkpoint: Option<String>,
    pub last_checkpoint_at: Option<String>,
    pub last_at: Option<String>,
    pub last_change_at: Option<String>,
    pub first_at: Option<String>,
    pub top_agents: Vec<(String, usize)>,
    pub top_files: Vec<(String, usize)>,
    pub storage_hotspots: Vec<HotspotInfo>,
}

pub struct HotspotInfo {
    pub file_path: String,
    pub revisions: usize,
    pub raw_bytes: u64,
    pub stored_bytes: u64,
}

pub struct ToolUsage {
    pub tool_name_norm: String,
    pub count: usize,
}

pub struct AnalyticsTouch {
    pub file_path: String,
    pub touch_count: usize,
    pub agent_family: String,
    pub prompt_kind: String,
    pub timestamp: String,
    pub llm: String,
    pub tool_name_norm: String,
}

pub struct MissingPromptSession {
    pub session_key: String,
    pub change_count: usize,
    pub started_at: String,
    pub llm: String,
}

pub struct AnalyticsInfo {
    pub recent_touches: Vec<AnalyticsTouch>,
    pub top_tools: Vec<ToolUsage>,
    pub missing_prompt_sessions: Vec<MissingPromptSession>,
}

pub struct GcResult {
    pub blob_count: usize,
    pub bytes_freed: u64,
}

pub struct ExportStats {
    pub sessions: usize,
    pub events: usize,
    pub memory_records: usize,
}

pub struct ImportStats {
    pub sessions_inserted: usize,
    pub events_inserted: usize,
    pub memory_records_inserted: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryVerifyOutcome {
    Verified,
    AlreadyVerified,
    NotFound,
}
