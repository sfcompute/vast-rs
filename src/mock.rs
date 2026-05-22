//! In-memory mock client for testing consumer code without a live VMS.
//!
//! Enable in your `[dev-dependencies]`:
//!
//! ```toml
//! [dev-dependencies]
//! vast-rs = { version = "0.1", features = ["mock"] }
//! ```
//!
//! ```rust,no_run
//! # use serde_json::json;
//! # use vast_rs::{VastClient, mock::MockVastClient};
//! # async fn run() {
//! let mock = MockVastClient::start().await;
//! mock.stub_get("clusters/", json!([{ "id": 1, "name": "a" }])).await;
//!
//! let clusters = mock.client().clusters().list().await.unwrap();
//! assert_eq!(clusters.len(), 1);
//! # }
//! ```
//!
//! Paths are relative to `/api/`: `"clusters/"`, `"/clusters/"`, and
//! `"/api/clusters/"` all match the same endpoint.

use serde_json::{json, Value};
use wiremock::matchers::{method as wm_method, path as wm_path};
use wiremock::{Mock, MockServer, ResponseTemplate, Times};

use crate::VastClient;

/// In-process mock of the VAST VMS HTTP API.
#[derive(Debug)]
pub struct MockVastClient {
    server: MockServer,
    client: VastClient,
}

impl MockVastClient {
    /// Start a mock server with a dummy `"mock-token"` API token.
    pub async fn start() -> Self {
        Self::with_token("mock-token").await
    }

    /// Start a mock server with a specific API token. Use when your test
    /// asserts on the `Authorization` header.
    pub async fn with_token(token: impl Into<String>) -> Self {
        let server = MockServer::start().await;
        let client = VastClient::builder()
            .address(server.uri())
            .token(token)
            .danger_accept_invalid_certs(true)
            .build()
            .expect("mock client build never fails");
        Self { server, client }
    }

    /// Start a mock server, pre-stub the JWT exchange, and authenticate via
    /// username/password. Pass `Some("name")` for tenant-admin flows.
    pub async fn with_credentials(
        user: impl Into<String>,
        pass: impl Into<String>,
        tenant: Option<&str>,
        access_token: impl Into<String>,
    ) -> Self {
        let server = MockServer::start().await;
        let token_path = tenant.map_or("/api/token/".to_string(), |t| format!("/api/token/{t}"));
        Mock::given(wm_method("POST"))
            .and(wm_path(token_path))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "access": access_token.into(),
                "refresh": Value::Null,
            })))
            .mount(&server)
            .await;

        let mut b = VastClient::builder()
            .address(server.uri())
            .credentials(user, pass)
            .danger_accept_invalid_certs(true);
        if let Some(t) = tenant {
            b = b.tenant(t);
        }
        let client = b.build().expect("mock client build never fails");
        Self { server, client }
    }

    /// The mock-backed [`VastClient`]. Cheap to clone.
    pub fn client(&self) -> VastClient {
        self.client.clone()
    }

    /// Underlying [`MockServer`] for advanced matchers or request inspection.
    pub fn server(&self) -> &MockServer {
        &self.server
    }

    /// Panic if any [`stub_with`](Self::stub_with) expectation was not met.
    pub async fn verify(&self) {
        self.server.verify().await;
    }

    /// Drop all configured stubs.
    pub async fn reset(&self) {
        self.server.reset().await;
    }

    /// Stub `GET <path>` → `200` + JSON body.
    pub async fn stub_get(&self, path: &str, body: Value) {
        self.mount("GET", path, 200, body, None).await;
    }

    /// Stub `POST <path>` → `201` + JSON body.
    pub async fn stub_post(&self, path: &str, body: Value) {
        self.mount("POST", path, 201, body, None).await;
    }

    /// Stub `PATCH <path>` → `200` + JSON body.
    pub async fn stub_patch(&self, path: &str, body: Value) {
        self.mount("PATCH", path, 200, body, None).await;
    }

    /// Stub `DELETE <path>` → `204 No Content`.
    pub async fn stub_delete(&self, path: &str) {
        self.mount("DELETE", path, 204, Value::Null, None).await;
    }

    /// Stub `<method> <path>` → `status` with a VMS-style `{"detail": detail}`
    /// body. Compatible with `Error::is_not_found` / `is_unauthorized`.
    pub async fn stub_error(&self, method: &str, path: &str, status: u16, detail: &str) {
        self.mount(method, path, status, json!({ "detail": detail }), None).await;
    }

    /// Stub with a call-count expectation. `times` accepts a `u64` (exact) or
    /// any range like `1u64..=3u64`. Call [`verify`](Self::verify) to enforce.
    pub async fn stub_with(
        &self,
        method: &str,
        path: &str,
        status: u16,
        body: Value,
        times: impl Into<Times>,
    ) {
        self.mount(method, path, status, body, Some(times.into())).await;
    }

    async fn mount(&self, method: &str, path: &str, status: u16, body: Value, times: Option<Times>) {
        let resp = if body.is_null() {
            ResponseTemplate::new(status)
        } else {
            ResponseTemplate::new(status).set_body_json(body)
        };
        let mut m = Mock::given(wm_method(method))
            .and(wm_path(normalize_path(path)))
            .respond_with(resp);
        if let Some(t) = times {
            m = m.expect(t);
        }
        m.mount(&self.server).await;
    }
}

/// Map `clusters/`, `/clusters/`, `api/clusters/`, or `/api/clusters/`
/// all to `/api/clusters/`.
fn normalize_path(input: &str) -> String {
    let trimmed = input.trim_start_matches('/');
    let body = trimmed.strip_prefix("api/").unwrap_or(trimmed);
    format!("/api/{body}")
}

#[cfg(test)]
mod tests {
    use super::normalize_path;

    #[test]
    fn normalize_handles_all_prefixes() {
        for input in ["clusters/", "/clusters/", "api/clusters/", "/api/clusters/"] {
            assert_eq!(normalize_path(input), "/api/clusters/", "input: {input}");
        }
        assert_eq!(normalize_path("clusters/1/x/"), "/api/clusters/1/x/");
    }
}
