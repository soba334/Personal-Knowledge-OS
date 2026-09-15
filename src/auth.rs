use axum::{extract::{Request, State}, middleware::Next, response::Response};
use subtle::ConstantTimeEq;

use crate::{AppState, error::AppError};

pub async fn require_auth(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Result<Response, AppError> {
    let provided = request
        .headers()
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .ok_or(AppError::Unauthorized)?;

    let expected = state.config.api_key.as_bytes();
    let actual = provided.as_bytes();
    let matches = expected.len() == actual.len() && bool::from(expected.ct_eq(actual));
    if !matches {
        return Err(AppError::Unauthorized);
    }

    Ok(next.run(request).await)
}
