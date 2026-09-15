use axum::{Json, http::StatusCode, response::{IntoResponse, Response}};
use serde::Serialize;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("unauthorized")]
    Unauthorized,
    #[error("not found: {0}")]
    NotFound(String),
    #[error("invalid request: {0}")]
    BadRequest(String),
    #[error("conflict: {0}")]
    Conflict(String),
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("migration error: {0}")]
    Migration(#[from] sqlx::migrate::MigrateError),
    #[error("upstream error: {0}")]
    Upstream(String),
    #[error("configuration error: {0}")]
    Config(String),
}

#[derive(Serialize)]
struct ErrorBody {
    error: ErrorPayload,
}

#[derive(Serialize)]
struct ErrorPayload {
    code: &'static str,
    message: String,
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, code) = match &self {
            AppError::Unauthorized => (StatusCode::UNAUTHORIZED, "unauthorized"),
            AppError::NotFound(_) => (StatusCode::NOT_FOUND, "not_found"),
            AppError::BadRequest(_) => (StatusCode::BAD_REQUEST, "bad_request"),
            AppError::Conflict(_) => (StatusCode::CONFLICT, "conflict"),
            AppError::Database(_) | AppError::Migration(_) | AppError::Upstream(_) | AppError::Config(_) => {
                (StatusCode::INTERNAL_SERVER_ERROR, "internal_error")
            }
        };

        let message = if status.is_server_error() {
            tracing::error!(error = %self, "request failed");
            "internal server error".to_string()
        } else {
            self.to_string()
        };

        (status, Json(ErrorBody { error: ErrorPayload { code, message } })).into_response()
    }
}
