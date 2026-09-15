use chrono::{DateTime, Utc};
use pgvector::Vector;
use serde_json::Value;
use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::{
    error::AppError,
    models::{JobRecord, MemoryRecord, RankedChunk, RankedMemory, SourceRecord},
    services::chunking::Chunk,
};

#[derive(Clone)]
pub struct Storage {
    pool: PgPool,
}

impl Storage {
    pub fn new(pool: PgPool) -> Self { Self { pool } }
    pub fn pool(&self) -> &PgPool { &self.pool }

    #[allow(clippy::too_many_arguments)]
    pub async fn ingest_source(
        &self,
        owner_id: Uuid,
        kind: &str,
        title: Option<&str>,
        content: &str,
        external_id: Option<&str>,
        source_uri: Option<&str>,
        sha256: &str,
        occurred_at: Option<DateTime<Utc>>,
        metadata: &Value,
        chunks: &[Chunk],
        schedule_memory_extraction: bool,
    ) -> Result<SourceRecord, AppError> {
        let mut tx = self.pool.begin().await?;
        let source_id = Uuid::now_v7();
        let row = sqlx::query(
            r#"INSERT INTO sources
               (id, owner_id, kind, title, content, external_id, source_uri, sha256, occurred_at, metadata)
               VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)
               RETURNING ingested_at"#,
        )
        .bind(source_id).bind(owner_id).bind(kind).bind(title).bind(content)
        .bind(external_id).bind(source_uri).bind(sha256).bind(occurred_at).bind(metadata)
        .fetch_one(&mut *tx).await.map_err(map_unique_conflict)?;
        let ingested_at: DateTime<Utc> = row.try_get("ingested_at")?;

        for chunk in chunks {
            let chunk_id = Uuid::now_v7();
            sqlx::query(
                r#"INSERT INTO chunks
                   (id, owner_id, source_id, position, char_start, char_end, content)
                   VALUES ($1,$2,$3,$4,$5,$6,$7)"#,
            )
            .bind(chunk_id).bind(owner_id).bind(source_id).bind(chunk.position)
            .bind(chunk.char_start).bind(chunk.char_end).bind(&chunk.content)
            .execute(&mut *tx).await?;

            sqlx::query("INSERT INTO jobs (id, owner_id, kind, payload) VALUES ($1,$2,'embed_chunk',$3)")
                .bind(Uuid::now_v7()).bind(owner_id)
                .bind(serde_json::json!({"chunk_id": chunk_id}))
                .execute(&mut *tx).await?;
        }

        if schedule_memory_extraction {
            sqlx::query("INSERT INTO jobs (id, owner_id, kind, payload) VALUES ($1,$2,'extract_memories',$3)")
                .bind(Uuid::now_v7()).bind(owner_id)
                .bind(serde_json::json!({"source_id": source_id}))
                .execute(&mut *tx).await?;
        }
        self.audit_tx(&mut tx, owner_id, "source.ingested", "source", source_id, serde_json::json!({"kind": kind})).await?;
        tx.commit().await?;

        Ok(SourceRecord { id: source_id, kind: kind.to_string(), title: title.map(str::to_string), content: content.to_string(), external_id: external_id.map(str::to_string), source_uri: source_uri.map(str::to_string), sha256: sha256.to_string(), occurred_at, ingested_at, metadata: metadata.clone() })
    }

    pub async fn get_source(&self, owner_id: Uuid, id: Uuid) -> Result<SourceRecord, AppError> {
        let row = sqlx::query("SELECT id,kind,title,content,external_id,source_uri,sha256,occurred_at,ingested_at,metadata FROM sources WHERE owner_id=$1 AND id=$2")
            .bind(owner_id).bind(id).fetch_optional(&self.pool).await?
            .ok_or_else(|| AppError::NotFound("source".into()))?;
        source_from_row(&row)
    }

    pub async fn search_chunks_lexical(&self, owner_id: Uuid, query: &str, limit: i64, use_pgroonga: bool) -> Result<Vec<RankedChunk>, AppError> {
        let sql = if use_pgroonga {
            r#"SELECT c.id,c.source_id,s.title,c.content,s.occurred_at,pgroonga_score(c.tableoid,c.ctid)::float8 AS score
               FROM chunks c JOIN sources s ON s.id=c.source_id
               WHERE c.owner_id=$1 AND c.content &@~ $2
               ORDER BY score DESC LIMIT $3"#
        } else {
            r#"SELECT c.id,c.source_id,s.title,c.content,s.occurred_at,
               CASE WHEN c.content ILIKE '%' || $2 || '%' THEN 1.0 ELSE 0.0 END::float8 AS score
               FROM chunks c JOIN sources s ON s.id=c.source_id
               WHERE c.owner_id=$1 AND c.content ILIKE '%' || $2 || '%'
               ORDER BY score DESC LIMIT $3"#
        };
        let rows = sqlx::query(sql).bind(owner_id).bind(query).bind(limit).fetch_all(&self.pool).await?;
        rows.iter().map(chunk_from_row).collect()
    }

    pub async fn search_chunks_dense(&self, owner_id: Uuid, embedding: Vec<f32>, limit: i64) -> Result<Vec<RankedChunk>, AppError> {
        let vector = Vector::from(embedding);
        let rows = sqlx::query(
            r#"SELECT c.id,c.source_id,s.title,c.content,s.occurred_at,(1.0-(c.embedding <=> $2))::float8 AS score
               FROM chunks c JOIN sources s ON s.id=c.source_id
               WHERE c.owner_id=$1 AND c.embedding IS NOT NULL
               ORDER BY c.embedding <=> $2 LIMIT $3"#,
        ).bind(owner_id).bind(vector).bind(limit).fetch_all(&self.pool).await?;
        rows.iter().map(chunk_from_row).collect()
    }

    pub async fn search_memories_lexical(&self, owner_id: Uuid, query: &str, limit: i64, use_pgroonga: bool) -> Result<Vec<RankedMemory>, AppError> {
        let sql = if use_pgroonga {
            r#"SELECT m.id,m.statement,m.valid_from,pgroonga_score(m.tableoid,m.ctid)::float8 AS score,
               COALESCE(array_agg(me.source_id) FILTER (WHERE me.source_id IS NOT NULL), ARRAY[]::uuid[]) AS evidence
               FROM memories m LEFT JOIN memory_evidence me ON me.memory_id=m.id
               WHERE m.owner_id=$1 AND m.status='active' AND m.statement &@~ $2
               GROUP BY m.id,m.tableoid,m.ctid ORDER BY score DESC LIMIT $3"#
        } else {
            r#"SELECT m.id,m.statement,m.valid_from,1.0::float8 AS score,
               COALESCE(array_agg(me.source_id) FILTER (WHERE me.source_id IS NOT NULL), ARRAY[]::uuid[]) AS evidence
               FROM memories m LEFT JOIN memory_evidence me ON me.memory_id=m.id
               WHERE m.owner_id=$1 AND m.status='active' AND m.statement ILIKE '%' || $2 || '%'
               GROUP BY m.id ORDER BY score DESC LIMIT $3"#
        };
        let rows = sqlx::query(sql).bind(owner_id).bind(query).bind(limit).fetch_all(&self.pool).await?;
        rows.iter().map(memory_rank_from_row).collect()
    }

    pub async fn search_memories_dense(&self, owner_id: Uuid, embedding: Vec<f32>, limit: i64) -> Result<Vec<RankedMemory>, AppError> {
        let rows = sqlx::query(
            r#"SELECT m.id,m.statement,m.valid_from,(1.0-(m.embedding <=> $2))::float8 AS score,
               COALESCE(array_agg(me.source_id) FILTER (WHERE me.source_id IS NOT NULL), ARRAY[]::uuid[]) AS evidence
               FROM memories m LEFT JOIN memory_evidence me ON me.memory_id=m.id
               WHERE m.owner_id=$1 AND m.status='active' AND m.embedding IS NOT NULL
               GROUP BY m.id ORDER BY m.embedding <=> $2 LIMIT $3"#,
        ).bind(owner_id).bind(Vector::from(embedding)).bind(limit).fetch_all(&self.pool).await?;
        rows.iter().map(memory_rank_from_row).collect()
    }

    pub async fn create_memory_candidate(&self, owner_id: Uuid, memory_type: &str, statement: &str, confidence: f32, importance: f32, valid_from: Option<DateTime<Utc>>, evidence: &[Uuid]) -> Result<MemoryRecord, AppError> {
        if evidence.is_empty() { return Err(AppError::BadRequest("memory requires at least one evidence source".into())); }
        let mut tx = self.pool.begin().await?;
        let count: i64 = sqlx::query_scalar("SELECT count(*) FROM sources WHERE owner_id=$1 AND id=ANY($2)")
            .bind(owner_id).bind(evidence).fetch_one(&mut *tx).await?;
        if count != evidence.len() as i64 { return Err(AppError::BadRequest("one or more evidence sources do not exist".into())); }
        let id = Uuid::now_v7();
        sqlx::query("INSERT INTO memories (id,owner_id,memory_type,statement,status,confidence,importance,valid_from) VALUES ($1,$2,$3,$4,'candidate',$5,$6,$7)")
            .bind(id).bind(owner_id).bind(memory_type).bind(statement).bind(confidence).bind(importance).bind(valid_from)
            .execute(&mut *tx).await?;
        for source_id in evidence {
            sqlx::query("INSERT INTO memory_evidence (memory_id,source_id) VALUES ($1,$2) ON CONFLICT DO NOTHING")
                .bind(id).bind(source_id).execute(&mut *tx).await?;
        }
        self.audit_tx(&mut tx, owner_id, "memory.candidate_created", "memory", id, Value::Null).await?;
        tx.commit().await?;
        self.get_memory(owner_id, id).await
    }

    pub async fn approve_memory(&self, owner_id: Uuid, id: Uuid, supersedes_id: Option<Uuid>) -> Result<MemoryRecord, AppError> {
        let mut tx = self.pool.begin().await?;
        let current: Option<String> = sqlx::query_scalar("SELECT status FROM memories WHERE owner_id=$1 AND id=$2 FOR UPDATE")
            .bind(owner_id).bind(id).fetch_optional(&mut *tx).await?;
        if current.as_deref() != Some("candidate") { return Err(AppError::Conflict("memory must be in candidate status".into())); }
        if let Some(previous) = supersedes_id {
            let result = sqlx::query("UPDATE memories SET status='superseded',valid_until=COALESCE(valid_until,now()),updated_at=now() WHERE owner_id=$1 AND id=$2 AND status='active'")
                .bind(owner_id).bind(previous).execute(&mut *tx).await?;
            if result.rows_affected() != 1 { return Err(AppError::Conflict("superseded memory must exist and be active".into())); }
        }
        sqlx::query("UPDATE memories SET status='active',supersedes_id=$3,valid_from=COALESCE(valid_from,now()),updated_at=now() WHERE owner_id=$1 AND id=$2")
            .bind(owner_id).bind(id).bind(supersedes_id).execute(&mut *tx).await?;
        sqlx::query("INSERT INTO jobs (id,owner_id,kind,payload) VALUES ($1,$2,'embed_memory',$3)")
            .bind(Uuid::now_v7()).bind(owner_id).bind(serde_json::json!({"memory_id":id})).execute(&mut *tx).await?;
        self.audit_tx(&mut tx, owner_id, "memory.approved", "memory", id, serde_json::json!({"supersedes_id": supersedes_id})).await?;
        tx.commit().await?;
        self.get_memory(owner_id, id).await
    }

    pub async fn reject_memory(&self, owner_id: Uuid, id: Uuid) -> Result<MemoryRecord, AppError> {
        let result = sqlx::query("UPDATE memories SET status='rejected',updated_at=now() WHERE owner_id=$1 AND id=$2 AND status='candidate'")
            .bind(owner_id).bind(id).execute(&self.pool).await?;
        if result.rows_affected()!=1 { return Err(AppError::Conflict("memory must be in candidate status".into())); }
        self.audit(owner_id,"memory.rejected","memory",id,Value::Null).await?;
        self.get_memory(owner_id,id).await
    }

    pub async fn get_memory(&self, owner_id: Uuid, id: Uuid) -> Result<MemoryRecord, AppError> {
        let row = sqlx::query(
            r#"SELECT m.id,m.memory_type,m.statement,m.status,m.confidence,m.importance,m.recurrence_count,m.valid_from,m.valid_until,m.supersedes_id,m.created_at,m.updated_at,
               COALESCE(array_agg(me.source_id) FILTER (WHERE me.source_id IS NOT NULL), ARRAY[]::uuid[]) AS evidence
               FROM memories m LEFT JOIN memory_evidence me ON me.memory_id=m.id
               WHERE m.owner_id=$1 AND m.id=$2 GROUP BY m.id"#,
        ).bind(owner_id).bind(id).fetch_optional(&self.pool).await?.ok_or_else(|| AppError::NotFound("memory".into()))?;
        memory_from_row(&row)
    }

    pub async fn list_memories(&self, owner_id: Uuid, status: Option<&str>, limit: i64) -> Result<Vec<MemoryRecord>, AppError> {
        let rows = sqlx::query(
            r#"SELECT m.id,m.memory_type,m.statement,m.status,m.confidence,m.importance,m.recurrence_count,m.valid_from,m.valid_until,m.supersedes_id,m.created_at,m.updated_at,
               COALESCE(array_agg(me.source_id) FILTER (WHERE me.source_id IS NOT NULL), ARRAY[]::uuid[]) AS evidence
               FROM memories m LEFT JOIN memory_evidence me ON me.memory_id=m.id
               WHERE m.owner_id=$1 AND ($2::text IS NULL OR m.status=$2)
               GROUP BY m.id ORDER BY m.updated_at DESC LIMIT $3"#,
        ).bind(owner_id).bind(status).bind(limit).fetch_all(&self.pool).await?;
        rows.iter().map(memory_from_row).collect()
    }

    pub async fn lease_jobs(&self, owner_id: Uuid, limit: i64) -> Result<Vec<JobRecord>, AppError> {
        let rows = sqlx::query(
            r#"WITH picked AS (
                 SELECT id FROM jobs WHERE owner_id=$1 AND status IN ('pending','running')
                 AND available_at<=now() AND (status='pending' OR lease_until<now())
                 ORDER BY created_at FOR UPDATE SKIP LOCKED LIMIT $2
               )
               UPDATE jobs j SET status='running',lease_until=now()+interval '60 seconds',attempts=attempts+1,updated_at=now()
               FROM picked WHERE j.id=picked.id RETURNING j.id,j.kind,j.payload,j.attempts"#,
        ).bind(owner_id).bind(limit).fetch_all(&self.pool).await?;
        rows.iter().map(|row| Ok(JobRecord { id: row.try_get("id")?, kind: row.try_get("kind")?, payload: row.try_get("payload")?, attempts: row.try_get("attempts")? })).collect()
    }

    pub async fn complete_job(&self, id: Uuid) -> Result<(), AppError> {
        sqlx::query("UPDATE jobs SET status='done',lease_until=NULL,updated_at=now() WHERE id=$1").bind(id).execute(&self.pool).await?; Ok(())
    }

    pub async fn fail_job(&self, id: Uuid, attempts: i32, error: &str) -> Result<(), AppError> {
        let terminal = attempts >= 8;
        sqlx::query("UPDATE jobs SET status=CASE WHEN $2 THEN 'failed' ELSE 'pending' END,available_at=CASE WHEN $2 THEN available_at ELSE now()+make_interval(secs => LEAST(300, (2 ^ LEAST($3,8))::int)) END,lease_until=NULL,last_error=$4,updated_at=now() WHERE id=$1")
            .bind(id).bind(terminal).bind(attempts).bind(error).execute(&self.pool).await?; Ok(())
    }

    pub async fn load_chunk_content(&self, owner_id: Uuid, id: Uuid) -> Result<String, AppError> {
        sqlx::query_scalar("SELECT content FROM chunks WHERE owner_id=$1 AND id=$2").bind(owner_id).bind(id).fetch_optional(&self.pool).await?.ok_or_else(|| AppError::NotFound("chunk".into()))
    }
    pub async fn set_chunk_embedding(&self, owner_id: Uuid, id: Uuid, vector: Vec<f32>, model: &str) -> Result<(), AppError> {
        sqlx::query("UPDATE chunks SET embedding=$3,embedding_model=$4,embedded_at=now() WHERE owner_id=$1 AND id=$2")
            .bind(owner_id).bind(id).bind(Vector::from(vector)).bind(model).execute(&self.pool).await?; Ok(())
    }
    pub async fn load_memory_statement(&self, owner_id: Uuid, id: Uuid) -> Result<String, AppError> {
        sqlx::query_scalar("SELECT statement FROM memories WHERE owner_id=$1 AND id=$2 AND status='active'").bind(owner_id).bind(id).fetch_optional(&self.pool).await?.ok_or_else(|| AppError::NotFound("active memory".into()))
    }
    pub async fn set_memory_embedding(&self, owner_id: Uuid, id: Uuid, vector: Vec<f32>, model: &str) -> Result<(), AppError> {
        sqlx::query("UPDATE memories SET embedding=$3,embedding_model=$4,embedded_at=now(),updated_at=now() WHERE owner_id=$1 AND id=$2")
            .bind(owner_id).bind(id).bind(Vector::from(vector)).bind(model).execute(&self.pool).await?; Ok(())
    }
    pub async fn record_retrieval(&self, owner_id: Uuid, query: &str, methods: &[String], degraded: bool, latency_ms: i64, top_k: i32) -> Result<(), AppError> {
        sqlx::query("INSERT INTO retrieval_runs (id,owner_id,query,methods,degraded,latency_ms,top_k) VALUES ($1,$2,$3,$4,$5,$6,$7)")
            .bind(Uuid::now_v7()).bind(owner_id).bind(query).bind(methods).bind(degraded).bind(latency_ms).bind(top_k).execute(&self.pool).await?; Ok(())
    }
    pub async fn ping(&self) -> Result<(), AppError> { sqlx::query("SELECT 1").execute(&self.pool).await?; Ok(()) }

    async fn audit(&self, owner_id: Uuid, event_type: &str, subject_type: &str, subject_id: Uuid, details: Value) -> Result<(), AppError> {
        sqlx::query("INSERT INTO audit_log (id,owner_id,event_type,subject_type,subject_id,actor,details) VALUES ($1,$2,$3,$4,$5,'api',$6)")
            .bind(Uuid::now_v7()).bind(owner_id).bind(event_type).bind(subject_type).bind(subject_id).bind(details).execute(&self.pool).await?; Ok(())
    }
    async fn audit_tx(&self, tx: &mut sqlx::Transaction<'_, sqlx::Postgres>, owner_id: Uuid, event_type: &str, subject_type: &str, subject_id: Uuid, details: Value) -> Result<(), AppError> {
        sqlx::query("INSERT INTO audit_log (id,owner_id,event_type,subject_type,subject_id,actor,details) VALUES ($1,$2,$3,$4,$5,'api',$6)")
            .bind(Uuid::now_v7()).bind(owner_id).bind(event_type).bind(subject_type).bind(subject_id).bind(details).execute(&mut **tx).await?; Ok(())
    }
}

fn map_unique_conflict(err: sqlx::Error) -> AppError {
    if let sqlx::Error::Database(db) = &err { if db.code().as_deref()==Some("23505") { return AppError::Conflict("source external_id already exists".into()); } }
    AppError::Database(err)
}
fn source_from_row(row: &sqlx::postgres::PgRow) -> Result<SourceRecord, AppError> { Ok(SourceRecord { id: row.try_get("id")?, kind: row.try_get("kind")?, title: row.try_get("title")?, content: row.try_get("content")?, external_id: row.try_get("external_id")?, source_uri: row.try_get("source_uri")?, sha256: row.try_get("sha256")?, occurred_at: row.try_get("occurred_at")?, ingested_at: row.try_get("ingested_at")?, metadata: row.try_get("metadata")? }) }
fn chunk_from_row(row: &sqlx::postgres::PgRow) -> Result<RankedChunk, AppError> { Ok(RankedChunk { chunk_id: row.try_get("id")?, source_id: row.try_get("source_id")?, title: row.try_get("title")?, content: row.try_get("content")?, occurred_at: row.try_get("occurred_at")?, score: row.try_get("score")? }) }
fn memory_rank_from_row(row: &sqlx::postgres::PgRow) -> Result<RankedMemory, AppError> { Ok(RankedMemory { memory_id: row.try_get("id")?, statement: row.try_get("statement")?, score: row.try_get("score")?, evidence_source_ids: row.try_get("evidence")?, valid_from: row.try_get("valid_from")? }) }
fn memory_from_row(row: &sqlx::postgres::PgRow) -> Result<MemoryRecord, AppError> { Ok(MemoryRecord { id: row.try_get("id")?, memory_type: row.try_get("memory_type")?, statement: row.try_get("statement")?, status: row.try_get("status")?, confidence: row.try_get("confidence")?, importance: row.try_get("importance")?, recurrence_count: row.try_get("recurrence_count")?, valid_from: row.try_get("valid_from")?, valid_until: row.try_get("valid_until")?, supersedes_id: row.try_get("supersedes_id")?, created_at: row.try_get("created_at")?, updated_at: row.try_get("updated_at")?, evidence_source_ids: row.try_get("evidence")? }) }
