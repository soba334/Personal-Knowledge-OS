use axum::{
    Router, middleware,
    routing::{get, post},
};

use crate::{AppState, auth};

mod health;
mod memories;
mod search;
mod sources;

pub fn router(state: AppState) -> Router {
    let protected = Router::new()
        .route("/v1/sources", post(sources::create_source))
        .route("/v1/sources/{id}", get(sources::get_source))
        .route("/v1/search", post(search::search))
        .route("/v1/memories", get(memories::list_memories))
        .route("/v1/memories/candidates", post(memories::create_candidate))
        .route("/v1/memories/{id}", get(memories::get_memory))
        .route("/v1/memories/{id}/approve", post(memories::approve_memory))
        .route("/v1/memories/{id}/reject", post(memories::reject_memory))
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            auth::require_auth,
        ));

    Router::new()
        .route("/healthz", get(health::health))
        .route("/readyz", get(health::ready))
        .merge(protected)
        .with_state(state)
}
