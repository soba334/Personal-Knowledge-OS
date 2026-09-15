use std::time::Duration;
use uuid::Uuid;

use crate::{AppState, error::AppError};

pub async fn run(state: AppState) -> Result<(), AppError> {
    loop {
        let jobs = state.storage.lease_jobs(state.config.owner_id, state.config.worker_batch_size).await?;
        if jobs.is_empty() {
            tokio::time::sleep(Duration::from_millis(state.config.worker_poll_ms)).await;
            continue;
        }

        for job in jobs {
            let result = process(&state, &job.kind, &job.payload).await;
            match result {
                Ok(()) => state.storage.complete_job(job.id).await?,
                Err(err) => {
                    tracing::warn!(job_id=%job.id,kind=%job.kind,error=%err,"job failed");
                    state.storage.fail_job(job.id, job.attempts, &err.to_string()).await?;
                }
            }
        }
    }
}

async fn process(state: &AppState, kind: &str, payload: &serde_json::Value) -> Result<(), AppError> {
    match kind {
        "embed_chunk" => {
            let id = parse_uuid(payload, "chunk_id")?;
            let content = state.storage.load_chunk_content(state.config.owner_id, id).await?;
            if let Some(vector) = state.embeddings.embed(&content).await? {
                state.storage.set_chunk_embedding(state.config.owner_id, id, vector, &state.config.embedding_model).await?;
            }
            Ok(())
        }
        "embed_memory" => {
            let id = parse_uuid(payload, "memory_id")?;
            let statement = state.storage.load_memory_statement(state.config.owner_id, id).await?;
            if let Some(vector) = state.embeddings.embed(&statement).await? {
                state.storage.set_memory_embedding(state.config.owner_id, id, vector, &state.config.embedding_model).await?;
            }
            Ok(())
        }
        "extract_memories" => {
            // Safety invariant: unverified model output never becomes active memory.
            // The extraction hook is retained until verifier-backed extraction is enabled.
            tracing::info!(payload=%payload,"memory extraction job retained without activation");
            Ok(())
        }
        _ => Err(AppError::BadRequest(format!("unknown job kind: {kind}"))),
    }
}

fn parse_uuid(payload: &serde_json::Value, key: &str) -> Result<Uuid, AppError> {
    payload
        .get(key)
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::BadRequest(format!("job payload missing {key}")))?
        .parse()
        .map_err(|_| AppError::BadRequest(format!("invalid {key}")))
}
