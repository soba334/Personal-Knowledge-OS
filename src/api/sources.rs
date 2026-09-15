use axum::{
    Json,
    extract::{Path, State},
};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::{
    AppState,
    error::AppError,
    models::{CreateSourceRequest, SourceRecord},
    services::chunking::chunk_text,
};

pub async fn create_source(
    State(state): State<AppState>,
    Json(request): Json<CreateSourceRequest>,
) -> Result<Json<SourceRecord>, AppError> {
    let kind = request.kind.trim();
    if kind.is_empty() || kind.len() > 64 {
        return Err(AppError::BadRequest("kind must be 1..=64 bytes".into()));
    }
    let content = request.content.trim();
    if content.is_empty() {
        return Err(AppError::BadRequest("content must not be empty".into()));
    }
    if content.len() > 10 * 1024 * 1024 {
        return Err(AppError::BadRequest(
            "content exceeds 10 MiB API limit".into(),
        ));
    }
    if request
        .external_id
        .as_deref()
        .is_some_and(|v| v.len() > 512)
    {
        return Err(AppError::BadRequest("external_id is too long".into()));
    }

    let sha256 = hex::encode(Sha256::digest(content.as_bytes()));
    let chunks = chunk_text(content, 1400, 180);
    let source = state
        .storage
        .ingest_source(
            state.config.owner_id,
            kind,
            request.title.as_deref(),
            content,
            request.external_id.as_deref(),
            request.source_uri.as_deref(),
            &sha256,
            request.occurred_at,
            &request.metadata,
            &chunks,
            state.config.memory_extraction_enabled,
        )
        .await?;
    Ok(Json(source))
}

pub async fn get_source(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<SourceRecord>, AppError> {
    Ok(Json(
        state.storage.get_source(state.config.owner_id, id).await?,
    ))
}
