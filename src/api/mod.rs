use axum::{
    Router, middleware,
    routing::{get, post},
};

use crate::{AppState, auth};

mod capture;
mod graph;
mod health;
mod memories;
mod search;
mod sources;
mod timeline;

pub fn router(state: AppState) -> Router {
    let protected = Router::new()
        .route("/v1/captures", post(capture::create_capture))
        .route("/v1/sources", post(sources::create_source))
        .route("/v1/sources/{id}", get(sources::get_source))
        .route("/v1/search", post(search::search))
        .route("/v1/memories", get(memories::list_memories))
        .route("/v1/memories/candidates", post(memories::create_candidate))
        .route("/v1/memories/{id}", get(memories::get_memory))
        .route("/v1/memories/{id}/approve", post(memories::approve_memory))
        .route("/v1/memories/{id}/reject", post(memories::reject_memory))
        .route(
            "/v1/entities",
            get(graph::list_entities).post(graph::create_entity),
        )
        .route("/v1/entities/{id}", get(graph::get_entity))
        .route("/v1/entities/{id}/graph", get(graph::get_entity_graph))
        .route("/v1/relations", post(graph::create_relation))
        .route("/v1/relations/{id}", get(graph::get_relation))
        .route("/v1/relations/{id}/close", post(graph::close_relation))
        .route("/v1/timeline", get(timeline::list_timeline))
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
