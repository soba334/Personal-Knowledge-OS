pub mod api;
pub mod auth;
pub mod config;
pub mod error;
pub mod models;
pub mod services;
pub mod storage;

use std::{sync::Arc, time::Duration};

use axum::{Router, http::HeaderName};
use sqlx::postgres::PgPoolOptions;
use tower_http::{
    request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer},
    timeout::TimeoutLayer,
    trace::TraceLayer,
};

use crate::{
    config::Config, error::AppError, services::embedding::EmbeddingClient, storage::Storage,
};

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub storage: Storage,
    pub embeddings: EmbeddingClient,
}

impl AppState {
    pub async fn from_config(config: Config) -> Result<Self, AppError> {
        let pool = PgPoolOptions::new()
            .max_connections(config.database_max_connections)
            .acquire_timeout(Duration::from_secs(10))
            .connect(&config.database_url)
            .await?;

        sqlx::migrate!("./migrations").run(&pool).await?;

        let embeddings = EmbeddingClient::new(&config)?;
        Ok(Self {
            config: Arc::new(config),
            storage: Storage::new(pool),
            embeddings,
        })
    }
}

pub fn build_app(state: AppState) -> Router {
    let request_id_header = HeaderName::from_static("x-request-id");

    api::router(state)
        .layer(PropagateRequestIdLayer::new(request_id_header.clone()))
        .layer(SetRequestIdLayer::new(request_id_header, MakeRequestUuid))
        .layer(TimeoutLayer::new(Duration::from_secs(30)))
        .layer(TraceLayer::new_for_http())
}
