use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

#[derive(Debug, Deserialize)]
pub struct CreateSourceRequest {
    pub kind: String,
    pub title: Option<String>,
    pub content: String,
    pub external_id: Option<String>,
    pub source_uri: Option<String>,
    pub occurred_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub metadata: Value,
}

#[derive(Debug, Serialize, Clone)]
pub struct SourceRecord {
    pub id: Uuid,
    pub kind: String,
    pub title: Option<String>,
    pub content: String,
    pub external_id: Option<String>,
    pub source_uri: Option<String>,
    pub sha256: String,
    pub occurred_at: Option<DateTime<Utc>>,
    pub ingested_at: DateTime<Utc>,
    pub metadata: Value,
}

#[derive(Debug, Deserialize)]
pub struct SearchRequest {
    pub query: String,
    pub limit: Option<usize>,
    #[serde(default = "default_true")]
    pub include_memories: bool,
}

fn default_true() -> bool { true }

#[derive(Debug, Serialize, Clone)]
pub struct SearchHit {
    pub kind: String,
    pub id: Uuid,
    pub source_id: Option<Uuid>,
    pub title: Option<String>,
    pub content: String,
    pub score: f64,
    pub lexical_score: Option<f64>,
    pub dense_score: Option<f64>,
    pub occurred_at: Option<DateTime<Utc>>,
    pub evidence_source_ids: Vec<Uuid>,
}

#[derive(Debug, Serialize)]
pub struct SearchResponse {
    pub query: String,
    pub hits: Vec<SearchHit>,
    pub degraded: bool,
    pub methods: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct CreateMemoryCandidateRequest {
    pub memory_type: String,
    pub statement: String,
    pub confidence: Option<f32>,
    pub importance: Option<f32>,
    pub valid_from: Option<DateTime<Utc>>,
    pub evidence_source_ids: Vec<Uuid>,
}

#[derive(Debug, Deserialize)]
pub struct ApproveMemoryRequest {
    pub supersedes_id: Option<Uuid>,
}

#[derive(Debug, Serialize, Clone)]
pub struct MemoryRecord {
    pub id: Uuid,
    pub memory_type: String,
    pub statement: String,
    pub status: String,
    pub confidence: f32,
    pub importance: f32,
    pub recurrence_count: i32,
    pub valid_from: Option<DateTime<Utc>>,
    pub valid_until: Option<DateTime<Utc>>,
    pub supersedes_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub evidence_source_ids: Vec<Uuid>,
}

#[derive(Debug, Deserialize)]
pub struct ListMemoriesQuery {
    pub status: Option<String>,
    pub limit: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub status: &'static str,
    pub database: &'static str,
}

#[derive(Debug, Clone)]
pub struct RankedChunk {
    pub chunk_id: Uuid,
    pub source_id: Uuid,
    pub title: Option<String>,
    pub content: String,
    pub occurred_at: Option<DateTime<Utc>>,
    pub score: f64,
}

#[derive(Debug, Clone)]
pub struct RankedMemory {
    pub memory_id: Uuid,
    pub statement: String,
    pub score: f64,
    pub evidence_source_ids: Vec<Uuid>,
    pub valid_from: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone)]
pub struct JobRecord {
    pub id: Uuid,
    pub kind: String,
    pub payload: Value,
    pub attempts: i32,
}
