use std::collections::HashSet;

use axum::{
    Json,
    extract::{Path, Query, State},
};
use chrono::Utc;
use serde_json::json;
use uuid::Uuid;

use crate::{
    AppState,
    error::AppError,
    models::{
        CloseRelationRequest, CreateEntityRequest, CreateRelationRequest, EntityGraphQuery,
        EntityGraphResponse, EntityRecord, ListEntitiesQuery, RelationRecord,
    },
    storage_graph::RelationInput,
};

pub async fn create_entity(
    State(state): State<AppState>,
    Json(request): Json<CreateEntityRequest>,
) -> Result<Json<EntityRecord>, AppError> {
    let entity_type = request.entity_type.trim();
    let canonical_name = request.canonical_name.trim();
    validate_nonempty("entity_type", entity_type, 64)?;
    validate_nonempty("canonical_name", canonical_name, 256)?;

    let attributes = if request.attributes.is_null() {
        json!({})
    } else if request.attributes.is_object() {
        request.attributes
    } else {
        return Err(AppError::BadRequest(
            "attributes must be a JSON object".into(),
        ));
    };

    let aliases = normalize_aliases(request.aliases, canonical_name)?;
    Ok(Json(
        state
            .storage
            .create_entity(
                state.config.owner_id,
                entity_type,
                canonical_name,
                &attributes,
                &aliases,
            )
            .await?,
    ))
}

pub async fn get_entity(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<EntityRecord>, AppError> {
    Ok(Json(
        state.storage.get_entity(state.config.owner_id, id).await?,
    ))
}

pub async fn list_entities(
    State(state): State<AppState>,
    Query(query): Query<ListEntitiesQuery>,
) -> Result<Json<Vec<EntityRecord>>, AppError> {
    let limit = query.limit.unwrap_or(50).clamp(1, 200);
    let entity_type = query
        .entity_type
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let search = query
        .query
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());

    if let Some(entity_type) = entity_type {
        validate_nonempty("entity_type", entity_type, 64)?;
    }
    if let Some(search) = search
        && search.chars().count() > 256
    {
        return Err(AppError::BadRequest(
            "query must be at most 256 characters".into(),
        ));
    }

    Ok(Json(
        state
            .storage
            .list_entities(state.config.owner_id, entity_type, search, limit)
            .await?,
    ))
}

pub async fn create_relation(
    State(state): State<AppState>,
    Json(request): Json<CreateRelationRequest>,
) -> Result<Json<RelationRecord>, AppError> {
    let predicate = request.predicate.trim();
    validate_nonempty("predicate", predicate, 128)?;
    if request.subject_entity_id == request.object_entity_id {
        return Err(AppError::BadRequest(
            "subject_entity_id and object_entity_id must differ".into(),
        ));
    }

    let confidence = request.confidence.unwrap_or(0.8);
    if !confidence.is_finite() || !(0.0..=1.0).contains(&confidence) {
        return Err(AppError::BadRequest(
            "confidence must be a finite number between 0 and 1".into(),
        ));
    }
    if let (Some(from), Some(until)) = (request.valid_from, request.valid_until)
        && until < from
    {
        return Err(AppError::BadRequest(
            "valid_until must be greater than or equal to valid_from".into(),
        ));
    }

    let input = RelationInput {
        subject_entity_id: request.subject_entity_id,
        predicate,
        object_entity_id: request.object_entity_id,
        valid_from: request.valid_from,
        valid_until: request.valid_until,
        confidence,
        source_id: request.source_id,
    };

    Ok(Json(
        state
            .storage
            .create_relation(state.config.owner_id, input)
            .await?,
    ))
}

pub async fn get_relation(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<RelationRecord>, AppError> {
    Ok(Json(
        state
            .storage
            .get_relation(state.config.owner_id, id)
            .await?,
    ))
}

pub async fn close_relation(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(request): Json<CloseRelationRequest>,
) -> Result<Json<RelationRecord>, AppError> {
    let valid_until = request.valid_until.unwrap_or_else(Utc::now);
    Ok(Json(
        state
            .storage
            .close_relation(state.config.owner_id, id, valid_until)
            .await?,
    ))
}

pub async fn get_entity_graph(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Query(query): Query<EntityGraphQuery>,
) -> Result<Json<EntityGraphResponse>, AppError> {
    let as_of = query.as_of.unwrap_or_else(Utc::now);
    Ok(Json(
        state
            .storage
            .get_entity_graph(state.config.owner_id, id, as_of, query.include_historical)
            .await?,
    ))
}

fn validate_nonempty(field: &str, value: &str, max_chars: usize) -> Result<(), AppError> {
    let len = value.chars().count();
    if len == 0 || len > max_chars {
        return Err(AppError::BadRequest(format!(
            "{field} must be 1..={max_chars} characters"
        )));
    }
    Ok(())
}

fn normalize_aliases(aliases: Vec<String>, canonical_name: &str) -> Result<Vec<String>, AppError> {
    if aliases.len() > 100 {
        return Err(AppError::BadRequest(
            "aliases must contain at most 100 values".into(),
        ));
    }

    let canonical_key = canonical_name.to_lowercase();
    let mut seen = HashSet::new();
    let mut normalized = Vec::with_capacity(aliases.len());
    for alias in aliases {
        let alias = alias.trim();
        if alias.is_empty() {
            continue;
        }
        if alias.chars().count() > 256 {
            return Err(AppError::BadRequest(
                "each alias must be at most 256 characters".into(),
            ));
        }
        let key = alias.to_lowercase();
        if key == canonical_key || !seen.insert(key) {
            continue;
        }
        normalized.push(alias.to_string());
    }
    normalized.sort_unstable_by_key(|value| value.to_lowercase());
    Ok(normalized)
}

#[cfg(test)]
mod tests {
    use super::normalize_aliases;

    #[test]
    fn aliases_are_trimmed_and_case_insensitively_deduplicated() {
        let aliases = vec![
            "  PKOS  ".to_string(),
            "pkos".to_string(),
            "Personal KB".to_string(),
            "".to_string(),
        ];
        let normalized = normalize_aliases(aliases, "Personal Knowledge OS").unwrap();
        assert_eq!(normalized, vec!["Personal KB", "PKOS"]);
    }

    #[test]
    fn canonical_name_is_not_duplicated_as_alias() {
        let aliases = vec!["personal knowledge os".to_string()];
        let normalized = normalize_aliases(aliases, "Personal Knowledge OS").unwrap();
        assert!(normalized.is_empty());
    }
}
