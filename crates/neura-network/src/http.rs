use reqwest::Client;
use serde::de::DeserializeOwned;
use thiserror::Error;
use tracing::debug;
use neura_app_framework::consts::USER_AGENT;

#[derive(Error, Debug)]
pub enum HttpError {
    #[error("Request failed: {0}")]
    Request(#[from] reqwest::Error),
    #[error("Deserialization failed: {0}")]
    Deserialize(String),
    #[error("HTTP {status}: {body}")]
    Status { status: u16, body: String },
}

pub type HttpResult<T> = Result<T, HttpError>;

/// HTTP client wrapper with default configuration.
pub struct HttpClient {
    client: Client,
}

impl HttpClient {
    pub fn new() -> Self {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .user_agent(USER_AGENT)
            .build()
            .expect("Failed to build HTTP client");
        Self { client }
    }

    pub async fn get_text(&self, url: &str) -> HttpResult<String> {
        debug!("GET {}", url);
        let resp = self.client.get(url).send().await?;
        let status = resp.status().as_u16();
        if !resp.status().is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(HttpError::Status { status, body });
        }
        Ok(resp.text().await?)
    }

    pub async fn get_json<T: DeserializeOwned>(&self, url: &str) -> HttpResult<T> {
        debug!("GET JSON {}", url);
        let resp = self.client.get(url).send().await?;
        let status = resp.status().as_u16();
        if !resp.status().is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(HttpError::Status { status, body });
        }
        Ok(resp.json().await?)
    }

    pub async fn post_json<T: DeserializeOwned>(&self, url: &str, body: &serde_json::Value) -> HttpResult<T> {
        debug!("POST JSON {}", url);
        let resp = self.client.post(url).json(body).send().await?;
        let status = resp.status().as_u16();
        if !resp.status().is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(HttpError::Status { status, body });
        }
        Ok(resp.json().await?)
    }

    /// Get the inner reqwest client for advanced usage.
    pub fn inner(&self) -> &Client {
        &self.client
    }
}

impl Default for HttpClient {
    fn default() -> Self {
        Self::new()
    }
}
