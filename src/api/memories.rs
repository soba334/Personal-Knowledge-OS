use axum::{
    Json,
    extract::{Path, Query, State},
};
use uuid::Uuid;

use crate::{
    AppState,
    error::AppError,
    models::{ApproveMemoryRequest, CreateMemoryCandidateRequest, ListMemoriesQuery, MemoryRecord},
};

pub async fn create_candidate(
    State(state): State<AppState>,
    Json(request): Json<CreateMemoryCandidateRequest>,
) -> Result<Json<MemoryRecord>, AppError> {
    let memory_type = request.memory_type.trim();
    let statement = request.statement.trim();
    if memory_type.is_empty() || memory_type.len() > 64 {
        return Err(AppError::BadRequest(
            "memory_type must be 1..=64 bytes".into(),
        ));
    }
    if statement.is_empty() || statement.chars().count() > 4000 {
        return Err(AppError::BadRequest(
            "statement must be 1..=4000 characters".into(),
        ));
    }

    let confidence = request.confidence.unwrap_or(0.8).clamp(0.0, 1.0);
    let importance = request.importance.unwrap_or(0.5).clamp(0.0, 1.0);
    Ok(Json(
        state
            .storage
            .create_memory_candidate(
                state.config.owner_id,
                memory_type,
                statement,
                confidence,
                importance,
                request.valid_from,
                &request.evidence_source_ids,
            )
            .await?,
    ))
}

pub async fn approve_memory(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(request): Json<ApproveMemoryRequest>,
) -> Result<Json<MemoryRecord>, AppError> {
    Ok(Json(
        state
            .storage
            .approve_memory(state.config.owner_id, id, request.supersedes_id)
            .await?,
    ))
}

pub async fn reject_memory(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<MemoryRecord>, AppError> {
    Ok(Json(
        state
            .storage
            .reject_memory(state.config.owner_id, id)
            .await?,
    ))
}

pub async fn get_memory(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<MemoryRecord>, AppError> {
    Ok(Json(
        state.storage.get_memory(state.config.owner_id, id).await?,
    ))
}

pub async fn list_memories(
    State(state): State<AppState>,
    Query(query): Query<ListMemoriesQuery>,
) -> Result<Json<Vec<MemoryRecord>>, AppError> {
    let limit = query.limit.unwrap_or(50).clamp(1, 200);
    Ok(Json(
        state
            .storage
            .list_memories(state.config.owner_id, query.status.as_deref(), limit)
            .await?,
    ))
}
