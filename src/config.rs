use std::{env, net::SocketAddr};

use thiserror::Error;
use uuid::Uuid;

#[derive(Clone, Debug)]
pub struct Config {
    pub bind_addr: SocketAddr,
    pub database_url: String,
    pub owner_id: Uuid,
    pub api_key: String,
    pub database_max_connections: u32,
    pub lexical_backend: String,
    pub search_candidate_limit: usize,
    pub search_result_limit: usize,
    pub worker_poll_ms: u64,
    pub worker_batch_size: i64,
    pub worker_enabled: bool,
    pub openai_base_url: Option<String>,
    pub openai_api_key: Option<String>,
    pub embedding_model: String,
    pub embedding_dimensions: usize,
    pub memory_base_url: Option<String>,
    pub memory_api_key: Option<String>,
    pub memory_model: String,
    pub memory_extraction_enabled: bool,
    pub memory_auto_promote_enabled: bool,
    pub memory_auto_promote_min_confidence: f32,
    pub memory_auto_promote_min_importance: f32,
    pub memory_keep_unpromoted_candidates: bool,
    pub memory_max_candidates_per_source: usize,
    pub memory_context_limit: i64,
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("missing required environment variable {0}")]
    Missing(&'static str),
    #[error("invalid {name}: {message}")]
    Invalid { name: &'static str, message: String },
}

impl Config {
    pub fn from_env() -> Result<Self, ConfigError> {
        let bind_addr = parse("PKOS_BIND_ADDR", "0.0.0.0:8080")?;
        let database_url = required("DATABASE_URL")?;
        let owner_id = parse("PKOS_OWNER_ID", "00000000-0000-0000-0000-000000000001")?;
        let api_key = required("PKOS_API_KEY")?;
        if api_key.len() < 32 {
            return Err(ConfigError::Invalid {
                name: "PKOS_API_KEY",
                message: "must be at least 32 bytes".into(),
            });
        }

        let embedding_dimensions = parse("PKOS_EMBEDDING_DIMENSIONS", "1536")?;
        if embedding_dimensions != 1536 {
            return Err(ConfigError::Invalid {
                name: "PKOS_EMBEDDING_DIMENSIONS",
                message: "v1 schema is fixed to 1536 dimensions".into(),
            });
        }

        let openai_base_url = optional("PKOS_OPENAI_BASE_URL");
        let openai_api_key = optional("PKOS_OPENAI_API_KEY");
        let memory_base_url = optional("PKOS_MEMORY_BASE_URL").or_else(|| openai_base_url.clone());
        let memory_api_key = optional("PKOS_MEMORY_API_KEY").or_else(|| openai_api_key.clone());
        let memory_extraction_enabled = parse_bool("PKOS_MEMORY_EXTRACTION_ENABLED", false)?;
        if memory_extraction_enabled && (memory_base_url.is_none() || memory_api_key.is_none()) {
            return Err(ConfigError::Invalid {
                name: "PKOS_MEMORY_EXTRACTION_ENABLED",
                message: "requires PKOS_MEMORY_BASE_URL/PKOS_MEMORY_API_KEY or the OpenAI-compatible provider variables".into(),
            });
        }

        let memory_auto_promote_min_confidence: f32 =
            parse("PKOS_MEMORY_AUTO_PROMOTE_MIN_CONFIDENCE", "0.92")?;
        validate_unit_interval(
            "PKOS_MEMORY_AUTO_PROMOTE_MIN_CONFIDENCE",
            memory_auto_promote_min_confidence,
        )?;
        let memory_auto_promote_min_importance: f32 =
            parse("PKOS_MEMORY_AUTO_PROMOTE_MIN_IMPORTANCE", "0.65")?;
        validate_unit_interval(
            "PKOS_MEMORY_AUTO_PROMOTE_MIN_IMPORTANCE",
            memory_auto_promote_min_importance,
        )?;

        let memory_max_candidates_per_source: usize =
            parse("PKOS_MEMORY_MAX_CANDIDATES_PER_SOURCE", "8")?;
        if !(1..=32).contains(&memory_max_candidates_per_source) {
            return Err(ConfigError::Invalid {
                name: "PKOS_MEMORY_MAX_CANDIDATES_PER_SOURCE",
                message: "must be between 1 and 32".into(),
            });
        }
        let memory_context_limit: i64 = parse("PKOS_MEMORY_CONTEXT_LIMIT", "50")?;
        if !(1..=200).contains(&memory_context_limit) {
            return Err(ConfigError::Invalid {
                name: "PKOS_MEMORY_CONTEXT_LIMIT",
                message: "must be between 1 and 200".into(),
            });
        }

        Ok(Self {
            bind_addr,
            database_url,
            owner_id,
            api_key,
            database_max_connections: parse("PKOS_DATABASE_MAX_CONNECTIONS", "20")?,
            lexical_backend: env::var("PKOS_LEXICAL_BACKEND").unwrap_or_else(|_| "pgroonga".into()),
            search_candidate_limit: parse("PKOS_SEARCH_CANDIDATE_LIMIT", "40")?,
            search_result_limit: parse("PKOS_SEARCH_RESULT_LIMIT", "12")?,
            worker_poll_ms: parse("PKOS_WORKER_POLL_MS", "1000")?,
            worker_batch_size: parse("PKOS_WORKER_BATCH_SIZE", "8")?,
            worker_enabled: parse_bool("PKOS_WORKER_ENABLED", true)?,
            openai_base_url,
            openai_api_key,
            embedding_model: env::var("PKOS_EMBEDDING_MODEL")
                .unwrap_or_else(|_| "text-embedding-3-small".into()),
            embedding_dimensions,
            memory_base_url,
            memory_api_key,
            memory_model: env::var("PKOS_MEMORY_MODEL").unwrap_or_else(|_| "gpt-5-mini".into()),
            memory_extraction_enabled,
            memory_auto_promote_enabled: parse_bool("PKOS_MEMORY_AUTO_PROMOTE_ENABLED", true)?,
            memory_auto_promote_min_confidence,
            memory_auto_promote_min_importance,
            memory_keep_unpromoted_candidates: parse_bool(
                "PKOS_MEMORY_KEEP_UNPROMOTED_CANDIDATES",
                false,
            )?,
            memory_max_candidates_per_source,
            memory_context_limit,
        })
    }
}

fn required(name: &'static str) -> Result<String, ConfigError> {
    env::var(name)
        .ok()
        .filter(|v| !v.trim().is_empty())
        .ok_or(ConfigError::Missing(name))
}

fn optional(name: &'static str) -> Option<String> {
    env::var(name).ok().filter(|v| !v.trim().is_empty())
}

fn parse<T>(name: &'static str, default: &str) -> Result<T, ConfigError>
where
    T: std::str::FromStr,
    T::Err: std::fmt::Display,
{
    env::var(name)
        .unwrap_or_else(|_| default.to_string())
        .parse()
        .map_err(|err: T::Err| ConfigError::Invalid {
            name,
            message: err.to_string(),
        })
}

fn parse_bool(name: &'static str, default: bool) -> Result<bool, ConfigError> {
    match env::var(name) {
        Ok(value) => match value.to_ascii_lowercase().as_str() {
            "1" | "true" | "yes" | "on" => Ok(true),
            "0" | "false" | "no" | "off" => Ok(false),
            _ => Err(ConfigError::Invalid {
                name,
                message: "expected boolean".into(),
            }),
        },
        Err(_) => Ok(default),
    }
}

fn validate_unit_interval(name: &'static str, value: f32) -> Result<(), ConfigError> {
    if value.is_finite() && (0.0..=1.0).contains(&value) {
        Ok(())
    } else {
        Err(ConfigError::Invalid {
            name,
            message: "must be a finite number between 0 and 1".into(),
        })
    }
}
