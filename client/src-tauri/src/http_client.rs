use reqwest::{Client, RequestBuilder, StatusCode};
use serde::de::DeserializeOwned;

use crate::state::AppState;

/// An HTTP client wrapper that injects the JWT Authorization header and handles common errors.
pub struct HttpClient {
    inner: Client,
}

impl HttpClient {
    pub fn new() -> Self {
        Self {
            inner: Client::builder()
                .timeout(std::time::Duration::from_secs(15))
                .build()
                .expect("failed to build reqwest client"),
        }
    }

    /// Build a GET request against the auth service with auth header.
    pub async fn get(&self, state: &AppState, path: &str) -> Result<RequestBuilder, String> {
        self.get_url(&state.base_url().await, state, path).await
    }

    /// Build a POST request against the auth service with auth header.
    pub async fn post(&self, state: &AppState, path: &str) -> Result<RequestBuilder, String> {
        self.post_url(&state.base_url().await, state, path).await
    }

    /// Build a PUT request against the auth service with auth header.
    pub async fn put(&self, state: &AppState, path: &str) -> Result<RequestBuilder, String> {
        self.put_url(&state.base_url().await, state, path).await
    }

    /// Build a DELETE request against the auth service with auth header.
    pub async fn delete(&self, state: &AppState, path: &str) -> Result<RequestBuilder, String> {
        self.delete_url(&state.base_url().await, state, path).await
    }

    /// Build a GET request against the signaling service with auth header.
    pub async fn sig_get(&self, state: &AppState, path: &str) -> Result<RequestBuilder, String> {
        self.get_url(&state.signaling_base_url().await, state, path).await
    }

    /// Build a POST request against the signaling service with auth header.
    pub async fn sig_post(&self, state: &AppState, path: &str) -> Result<RequestBuilder, String> {
        self.post_url(&state.signaling_base_url().await, state, path).await
    }

    /// Build a PUT request against the signaling service with auth header.
    pub async fn sig_put(&self, state: &AppState, path: &str) -> Result<RequestBuilder, String> {
        self.put_url(&state.signaling_base_url().await, state, path).await
    }

    /// Build a DELETE request against the signaling service with auth header.
    pub async fn sig_delete(&self, state: &AppState, path: &str) -> Result<RequestBuilder, String> {
        self.delete_url(&state.signaling_base_url().await, state, path).await
    }

    async fn get_url(&self, base: &str, state: &AppState, path: &str) -> Result<RequestBuilder, String> {
        let url = format!("{base}{path}");
        let mut req = self.inner.get(&url);
        if let Some(token) = state.access_token().await {
            req = req.bearer_auth(token);
        }
        Ok(req)
    }

    async fn post_url(&self, base: &str, state: &AppState, path: &str) -> Result<RequestBuilder, String> {
        let url = format!("{base}{path}");
        let mut req = self.inner.post(&url);
        if let Some(token) = state.access_token().await {
            req = req.bearer_auth(token);
        }
        Ok(req)
    }

    async fn put_url(&self, base: &str, state: &AppState, path: &str) -> Result<RequestBuilder, String> {
        let url = format!("{base}{path}");
        let mut req = self.inner.put(&url);
        if let Some(token) = state.access_token().await {
            req = req.bearer_auth(token);
        }
        Ok(req)
    }

    async fn delete_url(&self, base: &str, state: &AppState, path: &str) -> Result<RequestBuilder, String> {
        let url = format!("{base}{path}");
        let mut req = self.inner.delete(&url);
        if let Some(token) = state.access_token().await {
            req = req.bearer_auth(token);
        }
        Ok(req)
    }

    /// Send a request and deserialize the JSON body, mapping HTTP errors to user-friendly strings.
    pub async fn send_json<T: DeserializeOwned>(
        &self,
        req: RequestBuilder,
    ) -> Result<T, String> {
        let resp = req.send().await.map_err(|e| format!("Network error: {e}"))?;
        let status = resp.status();
        if status.is_success() {
            resp.json::<T>().await.map_err(|e| format!("Parse error: {e}"))
        } else {
            let body = resp.text().await.unwrap_or_default();
            Err(map_http_error(status, &body))
        }
    }

    /// Send a request expecting no response body.
    pub async fn send_empty(&self, req: RequestBuilder) -> Result<(), String> {
        let resp = req.send().await.map_err(|e| format!("Network error: {e}"))?;
        let status = resp.status();
        if status.is_success() {
            Ok(())
        } else {
            let body = resp.text().await.unwrap_or_default();
            Err(map_http_error(status, &body))
        }
    }
}

fn map_http_error(status: StatusCode, body: &str) -> String {
    match status.as_u16() {
        401 => "Session expired. Please log in again.".to_string(),
        403 => "You don't have permission.".to_string(),
        404 => "Not found.".to_string(),
        409 => {
            // Try to extract a conflict field from the body for inline errors
            body.to_string()
        }
        422 => format!("Validation error: {body}"),
        500..=599 => "Something went wrong. Try again later.".to_string(),
        _ => format!("Request failed ({status}): {body}"),
    }
}
