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
    pub memory_model: String,
    pub memory_extraction_enabled: bool,
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
            openai_base_url: optional("PKOS_OPENAI_BASE_URL"),
            openai_api_key: optional("PKOS_OPENAI_API_KEY"),
            embedding_model: env::var("PKOS_EMBEDDING_MODEL").unwrap_or_else(|_| "text-embedding-3-small".into()),
            embedding_dimensions,
            memory_model: env::var("PKOS_MEMORY_MODEL").unwrap_or_else(|_| "gpt-5-mini".into()),
            memory_extraction_enabled: parse_bool("PKOS_MEMORY_EXTRACTION_ENABLED", false)?,
        })
    }
}

fn required(name: &'static str) -> Result<String, ConfigError> {
    env::var(name).ok().filter(|v| !v.trim().is_empty()).ok_or(ConfigError::Missing(name))
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
        .map_err(|err: T::Err| ConfigError::Invalid { name, message: err.to_string() })
}

fn parse_bool(name: &'static str, default: bool) -> Result<bool, ConfigError> {
    match env::var(name) {
        Ok(value) => match value.to_ascii_lowercase().as_str() {
            "1" | "true" | "yes" | "on" => Ok(true),
            "0" | "false" | "no" | "off" => Ok(false),
            _ => Err(ConfigError::Invalid { name, message: "expected boolean".into() }),
        },
        Err(_) => Ok(default),
    }
}
