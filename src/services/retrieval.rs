use std::{collections::HashMap, time::Instant};
use uuid::Uuid;

use crate::{AppState, error::AppError, models::{SearchHit, SearchRequest, SearchResponse}};

const RRF_K: f64 = 60.0;

#[derive(Default)]
struct Acc {
    kind: String,
    id: Uuid,
    source_id: Option<Uuid>,
    title: Option<String>,
    content: String,
    occurred_at: Option<chrono::DateTime<chrono::Utc>>,
    evidence: Vec<Uuid>,
    score: f64,
    lexical: Option<f64>,
    dense: Option<f64>,
}

pub async fn search(state: &AppState, request: SearchRequest) -> Result<SearchResponse, AppError> {
    let query = request.query.trim();
    if query.is_empty() { return Err(AppError::BadRequest("query must not be empty".into())); }
    if query.chars().count() > 2000 { return Err(AppError::BadRequest("query is too long".into())); }

    let started = Instant::now();
    let limit = request.limit.unwrap_or(state.config.search_result_limit).clamp(1, 50);
    let candidates = state.config.search_candidate_limit.clamp(limit, 200) as i64;
    let use_pgroonga = state.config.lexical_backend == "pgroonga";
    let mut degraded = false;
    let mut methods = vec!["lexical".to_string()];
    let mut merged: HashMap<(String, Uuid), Acc> = HashMap::new();

    let lexical_chunks = match state.storage.search_chunks_lexical(state.config.owner_id, query, candidates, use_pgroonga).await {
        Ok(v) => v,
        Err(err) if use_pgroonga => {
            tracing::warn!(error=%err,"PGroonga search failed; falling back to ILIKE");
            degraded=true;
            state.storage.search_chunks_lexical(state.config.owner_id, query, candidates, false).await?
        }
        Err(err) => return Err(err),
    };
    for (rank, hit) in lexical_chunks.into_iter().enumerate() {
        let entry = merged.entry(("source_chunk".into(), hit.chunk_id)).or_insert_with(|| Acc {
            kind:"source_chunk".into(), id:hit.chunk_id, source_id:Some(hit.source_id), title:hit.title.clone(), content:hit.content.clone(), occurred_at:hit.occurred_at, evidence:vec![hit.source_id], ..Default::default()
        });
        entry.score += 1.0/(RRF_K+rank as f64+1.0);
        entry.lexical=Some(hit.score);
    }

    if request.include_memories {
        let lexical_memories = match state.storage.search_memories_lexical(state.config.owner_id, query, candidates, use_pgroonga).await {
            Ok(v)=>v,
            Err(err) if use_pgroonga => {
                tracing::warn!(error=%err,"PGroonga memory search failed; falling back to ILIKE");
                degraded=true;
                state.storage.search_memories_lexical(state.config.owner_id, query, candidates, false).await?
            },
            Err(err)=>return Err(err),
        };
        for (rank, hit) in lexical_memories.into_iter().enumerate() {
            let entry = merged.entry(("memory".into(), hit.memory_id)).or_insert_with(|| Acc {
                kind:"memory".into(), id:hit.memory_id, source_id:None, title:None, content:hit.statement.clone(), occurred_at:hit.valid_from, evidence:hit.evidence_source_ids.clone(), ..Default::default()
            });
            entry.score += 1.0/(RRF_K+rank as f64+1.0);
            entry.lexical=Some(hit.score);
        }
    }

    match state.embeddings.embed(query).await {
        Ok(Some(vector)) => {
            methods.push("dense".into());
            for (rank, hit) in state.storage.search_chunks_dense(state.config.owner_id, vector.clone(), candidates).await?.into_iter().enumerate() {
                let entry = merged.entry(("source_chunk".into(), hit.chunk_id)).or_insert_with(|| Acc {
                    kind:"source_chunk".into(), id:hit.chunk_id, source_id:Some(hit.source_id), title:hit.title.clone(), content:hit.content.clone(), occurred_at:hit.occurred_at, evidence:vec![hit.source_id], ..Default::default()
                });
                entry.score += 1.0/(RRF_K+rank as f64+1.0);
                entry.dense=Some(hit.score);
            }
            if request.include_memories {
                for (rank, hit) in state.storage.search_memories_dense(state.config.owner_id, vector, candidates).await?.into_iter().enumerate() {
                    let entry = merged.entry(("memory".into(), hit.memory_id)).or_insert_with(|| Acc {
                        kind:"memory".into(), id:hit.memory_id, source_id:None, title:None, content:hit.statement.clone(), occurred_at:hit.valid_from, evidence:hit.evidence_source_ids.clone(), ..Default::default()
                    });
                    entry.score += 1.0/(RRF_K+rank as f64+1.0);
                    entry.dense=Some(hit.score);
                }
            }
        }
        Ok(None) => { degraded=true; methods.push("dense_unavailable".into()); }
        Err(err) => { degraded=true; methods.push("dense_failed".into()); tracing::warn!(error=%err,"embedding search unavailable; continuing lexical-only"); }
    }

    let mut hits: Vec<SearchHit> = merged.into_values().map(|a| SearchHit {
        kind:a.kind,id:a.id,source_id:a.source_id,title:a.title,content:a.content,score:a.score,lexical_score:a.lexical,dense_score:a.dense,occurred_at:a.occurred_at,evidence_source_ids:a.evidence
    }).collect();
    hits.sort_by(|a,b| b.score.total_cmp(&a.score));
    hits.truncate(limit);

    let latency_ms = started.elapsed().as_millis().min(i64::MAX as u128) as i64;
    if let Err(err)=state.storage.record_retrieval(state.config.owner_id,query,&methods,degraded,latency_ms,limit as i32).await {
        tracing::warn!(error=%err,"failed to record retrieval telemetry");
    }
    Ok(SearchResponse { query:query.to_string(),hits,degraded,methods })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rrf_rewards_better_rank() {
        assert!(1.0/(RRF_K+1.0) > 1.0/(RRF_K+10.0));
    }
}
