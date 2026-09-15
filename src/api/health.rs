use axum::{Json, extract::State};
use crate::{AppState, error::AppError, models::HealthResponse};

pub async fn health() -> Json<HealthResponse> {
    Json(HealthResponse { status: "ok", database: "unchecked" })
}

pub async fn ready(State(state): State<AppState>) -> Result<Json<HealthResponse>, AppError> {
    state.storage.ping().await?;
    Ok(Json(HealthResponse { status: "ok", database: "ok" }))
}
