use axum::{
    Json,
    extract::{Query, State},
};

use crate::{
    AppState,
    error::AppError,
    models::{TimelineItem, TimelineQuery},
};

pub async fn list_timeline(
    State(state): State<AppState>,
    Query(query): Query<TimelineQuery>,
) -> Result<Json<Vec<TimelineItem>>, AppError> {
    let limit = query.limit.unwrap_or(50).clamp(1, 200);
    Ok(Json(
        state
            .storage
            .list_timeline(state.config.owner_id, query.before, limit)
            .await?,
    ))
}
