use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::{config::Config, error::AppError};

#[derive(Clone)]
pub struct EmbeddingClient {
    http: Client,
    base_url: Option<String>,
    api_key: Option<String>,
    model: String,
    dimensions: usize,
}

#[derive(Serialize)]
struct EmbeddingRequest<'a> {
    model: &'a str,
    input: &'a str,
    dimensions: usize,
}

#[derive(Deserialize)]
struct EmbeddingResponse {
    data: Vec<EmbeddingDatum>,
}

#[derive(Deserialize)]
struct EmbeddingDatum {
    embedding: Vec<f32>,
}

impl EmbeddingClient {
    pub fn new(config: &Config) -> Result<Self, AppError> {
        let http = Client::builder()
            .timeout(std::time::Duration::from_secs(20))
            .build()
            .map_err(|err| AppError::Config(err.to_string()))?;
        Ok(Self {
            http,
            base_url: config.openai_base_url.clone(),
            api_key: config.openai_api_key.clone(),
            model: config.embedding_model.clone(),
            dimensions: config.embedding_dimensions,
        })
    }

    pub fn enabled(&self) -> bool {
        self.base_url.is_some() && self.api_key.is_some()
    }

    pub async fn embed(&self, input: &str) -> Result<Option<Vec<f32>>, AppError> {
        let (Some(base_url), Some(api_key)) = (&self.base_url, &self.api_key) else {
            return Ok(None);
        };

        let response = self.http
            .post(format!("{}/embeddings", base_url.trim_end_matches('/')))
            .bearer_auth(api_key)
            .json(&EmbeddingRequest { model: &self.model, input, dimensions: self.dimensions })
            .send()
            .await
            .map_err(|err| AppError::Upstream(err.to_string()))?;

        if !response.status().is_success() {
            return Err(AppError::Upstream(format!("embedding provider returned {}", response.status())));
        }

        let body: EmbeddingResponse = response.json().await.map_err(|err| AppError::Upstream(err.to_string()))?;
        let vector = body.data.into_iter().next().ok_or_else(|| AppError::Upstream("embedding response contained no vectors".into()))?.embedding;
        if vector.len() != self.dimensions {
            return Err(AppError::Upstream(format!("embedding dimension mismatch: expected {}, got {}", self.dimensions, vector.len())));
        }
        Ok(Some(vector))
    }
}
