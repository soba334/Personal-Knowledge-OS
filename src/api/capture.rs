use axum::{Json, extract::State};
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};

use crate::{
    AppState,
    error::AppError,
    models::{CaptureResponse, CreateCaptureRequest},
    services::chunking::chunk_text,
};

const MAX_CAPTURE_MESSAGES: usize = 200;
const MAX_CAPTURE_BYTES: usize = 10 * 1024 * 1024;

pub async fn create_capture(
    State(state): State<AppState>,
    Json(request): Json<CreateCaptureRequest>,
) -> Result<Json<CaptureResponse>, AppError> {
    let kind = request.kind.trim();
    if kind.is_empty() || kind.len() > 64 {
        return Err(AppError::BadRequest("kind must be 1..=64 bytes".into()));
    }
    if request.messages.is_empty() || request.messages.len() > MAX_CAPTURE_MESSAGES {
        return Err(AppError::BadRequest(format!(
            "messages must contain 1..={MAX_CAPTURE_MESSAGES} items"
        )));
    }
    if request
        .external_id
        .as_deref()
        .is_some_and(|value| value.len() > 512)
    {
        return Err(AppError::BadRequest("external_id is too long".into()));
    }
    if request
        .session_id
        .as_deref()
        .is_some_and(|value| value.len() > 512)
    {
        return Err(AppError::BadRequest("session_id is too long".into()));
    }
    if request
        .provider
        .as_deref()
        .is_some_and(|value| value.len() > 128)
    {
        return Err(AppError::BadRequest("provider is too long".into()));
    }

    let mut content = String::new();
    for message in &request.messages {
        let role = message.role.trim().to_ascii_lowercase();
        if !matches!(
            role.as_str(),
            "user" | "assistant" | "system" | "tool" | "agent"
        ) {
            return Err(AppError::BadRequest(format!(
                "unsupported capture role: {}",
                message.role
            )));
        }
        let body = message.content.trim();
        if body.is_empty() {
            continue;
        }
        if let Some(name) = message.name.as_deref() {
            if name.len() > 128 {
                return Err(AppError::BadRequest("message name is too long".into()));
            }
            content.push_str(&format!("## {role} ({})\n", name.trim()));
        } else {
            content.push_str(&format!("## {role}\n"));
        }
        content.push_str(body);
        content.push_str("\n\n");
        if content.len() > MAX_CAPTURE_BYTES {
            return Err(AppError::BadRequest(
                "captured conversation exceeds 10 MiB API limit".into(),
            ));
        }
    }
    let content = content.trim();
    if content.is_empty() {
        return Err(AppError::BadRequest(
            "capture contains no non-empty messages".into(),
        ));
    }

    let mut metadata = match request.metadata {
        Value::Null => Map::new(),
        Value::Object(map) => map,
        _ => {
            return Err(AppError::BadRequest(
                "metadata must be a JSON object".into(),
            ));
        }
    };
    metadata.insert(
        "pkos_capture".into(),
        json!({
            "provider": request.provider,
            "session_id": request.session_id,
            "message_count": request.messages.len(),
            "automatic": true
        }),
    );

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
            &Value::Object(metadata),
            &chunks,
            state.config.memory_extraction_enabled,
        )
        .await?;

    Ok(Json(CaptureResponse {
        source,
        memory_extraction_scheduled: state.config.memory_extraction_enabled,
    }))
}

#[cfg(test)]
mod tests {
    use crate::models::{CaptureMessage, CreateCaptureRequest};

    #[test]
    fn capture_types_deserialize_with_default_kind() {
        let request: CreateCaptureRequest = serde_json::from_value(serde_json::json!({
            "messages": [{"role": "user", "content": "remember this", "name": null}]
        }))
        .expect("capture request should deserialize");
        assert_eq!(request.kind, "conversation");
        assert_eq!(request.messages.len(), 1);
        let CaptureMessage { role, .. } = &request.messages[0];
        assert_eq!(role, "user");
    }
}
