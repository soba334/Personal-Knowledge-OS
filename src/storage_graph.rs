use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::{Postgres, Row, Transaction};
use uuid::Uuid;

use crate::{
    error::AppError,
    models::{EntityGraphResponse, EntityRecord, RelationRecord, TimelineItem},
    storage::Storage,
};

pub struct RelationInput<'a> {
    pub subject_entity_id: Uuid,
    pub predicate: &'a str,
    pub object_entity_id: Uuid,
    pub valid_from: Option<DateTime<Utc>>,
    pub valid_until: Option<DateTime<Utc>>,
    pub confidence: f32,
    pub source_id: Option<Uuid>,
}

impl Storage {
    pub async fn create_entity(
        &self,
        owner_id: Uuid,
        entity_type: &str,
        canonical_name: &str,
        attributes: &Value,
        aliases: &[String],
    ) -> Result<EntityRecord, AppError> {
        let mut tx = self.pool().begin().await?;
        let id = Uuid::now_v7();

        let inserted = sqlx::query(
            r#"INSERT INTO entities (id,owner_id,entity_type,canonical_name,attributes)
               VALUES ($1,$2,$3,$4,$5)
               ON CONFLICT DO NOTHING
               RETURNING id"#,
        )
        .bind(id)
        .bind(owner_id)
        .bind(entity_type)
        .bind(canonical_name)
        .bind(attributes)
        .fetch_optional(&mut *tx)
        .await?;

        if inserted.is_none() {
            return Err(AppError::Conflict(
                "entity with the same type and canonical_name already exists".into(),
            ));
        }

        for alias in aliases {
            sqlx::query(
                "INSERT INTO entity_aliases (entity_id,alias) VALUES ($1,$2) ON CONFLICT DO NOTHING",
            )
            .bind(id)
            .bind(alias)
            .execute(&mut *tx)
            .await?;
        }

        audit_tx(
            &mut tx,
            owner_id,
            "entity.created",
            "entity",
            id,
            serde_json::json!({"entity_type": entity_type}),
        )
        .await?;
        tx.commit().await?;

        self.get_entity(owner_id, id).await
    }

    pub async fn get_entity(&self, owner_id: Uuid, id: Uuid) -> Result<EntityRecord, AppError> {
        let row = sqlx::query(
            r#"SELECT e.id,e.entity_type,e.canonical_name,e.attributes,e.created_at,e.updated_at,
               COALESCE(array_agg(ea.alias ORDER BY ea.alias) FILTER (WHERE ea.alias IS NOT NULL), ARRAY[]::text[]) AS aliases
               FROM entities e
               LEFT JOIN entity_aliases ea ON ea.entity_id=e.id
               WHERE e.owner_id=$1 AND e.id=$2
               GROUP BY e.id"#,
        )
        .bind(owner_id)
        .bind(id)
        .fetch_optional(self.pool())
        .await?
        .ok_or_else(|| AppError::NotFound("entity".into()))?;

        entity_from_row(&row)
    }

    pub async fn list_entities(
        &self,
        owner_id: Uuid,
        entity_type: Option<&str>,
        query: Option<&str>,
        limit: i64,
    ) -> Result<Vec<EntityRecord>, AppError> {
        let rows = sqlx::query(
            r#"SELECT e.id,e.entity_type,e.canonical_name,e.attributes,e.created_at,e.updated_at,
               COALESCE(array_agg(ea.alias ORDER BY ea.alias) FILTER (WHERE ea.alias IS NOT NULL), ARRAY[]::text[]) AS aliases
               FROM entities e
               LEFT JOIN entity_aliases ea ON ea.entity_id=e.id
               WHERE e.owner_id=$1
                 AND ($2::text IS NULL OR e.entity_type=$2)
                 AND ($3::text IS NULL OR e.canonical_name ILIKE '%' || $3 || '%'
                      OR EXISTS (SELECT 1 FROM entity_aliases x WHERE x.entity_id=e.id AND x.alias ILIKE '%' || $3 || '%'))
               GROUP BY e.id
               ORDER BY lower(e.canonical_name), e.id
               LIMIT $4"#,
        )
        .bind(owner_id)
        .bind(entity_type)
        .bind(query)
        .bind(limit)
        .fetch_all(self.pool())
        .await?;

        rows.iter().map(entity_from_row).collect()
    }

    pub async fn create_relation(
        &self,
        owner_id: Uuid,
        input: RelationInput<'_>,
    ) -> Result<RelationRecord, AppError> {
        if input.subject_entity_id == input.object_entity_id {
            return Err(AppError::BadRequest(
                "subject_entity_id and object_entity_id must differ".into(),
            ));
        }
        if let (Some(from), Some(until)) = (input.valid_from, input.valid_until)
            && until < from
        {
            return Err(AppError::BadRequest(
                "valid_until must be greater than or equal to valid_from".into(),
            ));
        }

        let mut tx = self.pool().begin().await?;
        let entity_ids = [input.subject_entity_id, input.object_entity_id];
        let owned_entities: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM entities WHERE owner_id=$1 AND id=ANY($2)",
        )
        .bind(owner_id)
        .bind(&entity_ids[..])
        .fetch_one(&mut *tx)
        .await?;
        if owned_entities != 2 {
            return Err(AppError::BadRequest(
                "both relation entities must exist for the current owner".into(),
            ));
        }

        if let Some(source_id) = input.source_id {
            let source_exists: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM sources WHERE owner_id=$1 AND id=$2)",
            )
            .bind(owner_id)
            .bind(source_id)
            .fetch_one(&mut *tx)
            .await?;
            if !source_exists {
                return Err(AppError::BadRequest(
                    "relation source_id must exist for the current owner".into(),
                ));
            }
        }

        let id = Uuid::now_v7();
        let row = sqlx::query(
            r#"INSERT INTO relations
               (id,owner_id,subject_entity_id,predicate,object_entity_id,valid_from,valid_until,confidence,source_id)
               VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)
               RETURNING id,subject_entity_id,predicate,object_entity_id,valid_from,valid_until,confidence,source_id,created_at"#,
        )
        .bind(id)
        .bind(owner_id)
        .bind(input.subject_entity_id)
        .bind(input.predicate)
        .bind(input.object_entity_id)
        .bind(input.valid_from)
        .bind(input.valid_until)
        .bind(input.confidence)
        .bind(input.source_id)
        .fetch_one(&mut *tx)
        .await?;

        audit_tx(
            &mut tx,
            owner_id,
            "relation.created",
            "relation",
            id,
            serde_json::json!({
                "subject_entity_id": input.subject_entity_id,
                "predicate": input.predicate,
                "object_entity_id": input.object_entity_id,
                "source_id": input.source_id,
            }),
        )
        .await?;
        tx.commit().await?;

        relation_from_row(&row)
    }

    pub async fn get_relation(
        &self,
        owner_id: Uuid,
        id: Uuid,
    ) -> Result<RelationRecord, AppError> {
        let row = sqlx::query(
            r#"SELECT id,subject_entity_id,predicate,object_entity_id,valid_from,valid_until,confidence,source_id,created_at
               FROM relations WHERE owner_id=$1 AND id=$2"#,
        )
        .bind(owner_id)
        .bind(id)
        .fetch_optional(self.pool())
        .await?
        .ok_or_else(|| AppError::NotFound("relation".into()))?;

        relation_from_row(&row)
    }

    pub async fn close_relation(
        &self,
        owner_id: Uuid,
        id: Uuid,
        valid_until: DateTime<Utc>,
    ) -> Result<RelationRecord, AppError> {
        let mut tx = self.pool().begin().await?;
        let row = sqlx::query(
            r#"UPDATE relations
               SET valid_until=$3
               WHERE owner_id=$1 AND id=$2
                 AND (valid_from IS NULL OR valid_from <= $3)
                 AND (valid_until IS NULL OR valid_until > $3)
               RETURNING id,subject_entity_id,predicate,object_entity_id,valid_from,valid_until,confidence,source_id,created_at"#,
        )
        .bind(owner_id)
        .bind(id)
        .bind(valid_until)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| {
            AppError::Conflict(
                "relation must exist, be active at valid_until, and not end before valid_from".into(),
            )
        })?;

        audit_tx(
            &mut tx,
            owner_id,
            "relation.closed",
            "relation",
            id,
            serde_json::json!({"valid_until": valid_until}),
        )
        .await?;
        tx.commit().await?;

        relation_from_row(&row)
    }

    pub async fn get_entity_graph(
        &self,
        owner_id: Uuid,
        id: Uuid,
        as_of: DateTime<Utc>,
        include_historical: bool,
    ) -> Result<EntityGraphResponse, AppError> {
        let entity = self.get_entity(owner_id, id).await?;
        let relation_sql = r#"SELECT id,subject_entity_id,predicate,object_entity_id,valid_from,valid_until,confidence,source_id,created_at
                              FROM relations
                              WHERE owner_id=$1 AND {direction}=$2
                                AND ($3 OR ((valid_from IS NULL OR valid_from <= $4)
                                         AND (valid_until IS NULL OR valid_until > $4)))
                              ORDER BY predicate, valid_from DESC NULLS LAST, created_at DESC"#;

        let outgoing_sql = relation_sql.replace("{direction}", "subject_entity_id");
        let incoming_sql = relation_sql.replace("{direction}", "object_entity_id");

        let outgoing_rows = sqlx::query(&outgoing_sql)
            .bind(owner_id)
            .bind(id)
            .bind(include_historical)
            .bind(as_of)
            .fetch_all(self.pool())
            .await?;
        let incoming_rows = sqlx::query(&incoming_sql)
            .bind(owner_id)
            .bind(id)
            .bind(include_historical)
            .bind(as_of)
            .fetch_all(self.pool())
            .await?;

        Ok(EntityGraphResponse {
            entity,
            outgoing: outgoing_rows
                .iter()
                .map(relation_from_row)
                .collect::<Result<_, _>>()?,
            incoming: incoming_rows
                .iter()
                .map(relation_from_row)
                .collect::<Result<_, _>>()?,
        })
    }

    pub async fn list_timeline(
        &self,
        owner_id: Uuid,
        before: Option<DateTime<Utc>>,
        limit: i64,
    ) -> Result<Vec<TimelineItem>, AppError> {
        let rows = sqlx::query(
            r#"SELECT id AS source_id,kind,title,occurred_at,ingested_at,left(content,280) AS preview
               FROM sources
               WHERE owner_id=$1
                 AND ($2::timestamptz IS NULL OR COALESCE(occurred_at,ingested_at) < $2)
               ORDER BY COALESCE(occurred_at,ingested_at) DESC, ingested_at DESC, id DESC
               LIMIT $3"#,
        )
        .bind(owner_id)
        .bind(before)
        .bind(limit)
        .fetch_all(self.pool())
        .await?;

        rows.iter().map(timeline_from_row).collect()
    }
}

async fn audit_tx(
    tx: &mut Transaction<'_, Postgres>,
    owner_id: Uuid,
    event_type: &str,
    subject_type: &str,
    subject_id: Uuid,
    details: Value,
) -> Result<(), AppError> {
    sqlx::query(
        "INSERT INTO audit_log (id,owner_id,event_type,subject_type,subject_id,actor,details) VALUES ($1,$2,$3,$4,$5,'api',$6)",
    )
    .bind(Uuid::now_v7())
    .bind(owner_id)
    .bind(event_type)
    .bind(subject_type)
    .bind(subject_id)
    .bind(details)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

fn entity_from_row(row: &sqlx::postgres::PgRow) -> Result<EntityRecord, AppError> {
    Ok(EntityRecord {
        id: row.try_get("id")?,
        entity_type: row.try_get("entity_type")?,
        canonical_name: row.try_get("canonical_name")?,
        attributes: row.try_get("attributes")?,
        aliases: row.try_get("aliases")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

fn relation_from_row(row: &sqlx::postgres::PgRow) -> Result<RelationRecord, AppError> {
    Ok(RelationRecord {
        id: row.try_get("id")?,
        subject_entity_id: row.try_get("subject_entity_id")?,
        predicate: row.try_get("predicate")?,
        object_entity_id: row.try_get("object_entity_id")?,
        valid_from: row.try_get("valid_from")?,
        valid_until: row.try_get("valid_until")?,
        confidence: row.try_get("confidence")?,
        source_id: row.try_get("source_id")?,
        created_at: row.try_get("created_at")?,
    })
}

fn timeline_from_row(row: &sqlx::postgres::PgRow) -> Result<TimelineItem, AppError> {
    Ok(TimelineItem {
        source_id: row.try_get("source_id")?,
        kind: row.try_get("kind")?,
        title: row.try_get("title")?,
        occurred_at: row.try_get("occurred_at")?,
        ingested_at: row.try_get("ingested_at")?,
        preview: row.try_get("preview")?,
    })
}
