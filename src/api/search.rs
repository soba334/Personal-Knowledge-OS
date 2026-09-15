use crate::{
    AppState,
    error::AppError,
    models::{SearchRequest, SearchResponse},
    services::retrieval,
};
use axum::{Json, extract::State};

pub async fn search(
    State(state): State<AppState>,
    Json(request): Json<SearchRequest>,
) -> Result<Json<SearchResponse>, AppError> {
    Ok(Json(retrieval::search(&state, request).await?))
}
