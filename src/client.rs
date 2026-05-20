use std::sync::Arc;

use reqwest::{header, Method, RequestBuilder, Response, StatusCode};
use serde::{de::DeserializeOwned, Serialize};
use tokio::sync::RwLock;
use tracing::instrument;
use url::Url;

use crate::{
    api::{
        clusters::ClustersApi,
        folders::FoldersApi,
        nodes::NodesApi,
        protectionpolicies::ProtectionPoliciesApi,
        quotas::QuotasApi,
        snapshots::SnapshotsApi,
        tenants::TenantsApi,
        users::UsersApi,
        viewpolicies::ViewPoliciesApi,
        views::ViewsApi,
        vippools::VipPoolsApi,
        volumes::VolumesApi,
    },
    config::{ClientConfig, ClientConfigBuilder},
    error::{Error, Result},
};

// ---------------------------------------------------------------------------
// Inner state shared by all API handles
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub(crate) struct Inner {
    pub(crate) http: reqwest::Client,
    pub(crate) base_url: Url,
    pub(crate) config: ClientConfig,
    /// Cached bearer token (populated lazily for password-auth or on first
    /// token-auth request).
    pub(crate) cached_token: RwLock<Option<String>>,
}

// ---------------------------------------------------------------------------
// VastClient
// ---------------------------------------------------------------------------

/// Async client for the VAST Data Management System (VMS) REST API.
///
/// `VastClient` is cheap to clone — all instances share the same underlying
/// connection pool and configuration.
///
/// # Example
///
/// ```rust,no_run
/// use vast_rs::VastClient;
///
/// # #[tokio::main]
/// # async fn main() -> vast_rs::Result<()> {
/// let client = VastClient::builder()
///     .address("vms.example.com")
///     .token("my-api-token")
///     .build()?;
///
/// let clusters = client.clusters().list().await?;
/// # Ok(())
/// # }
/// ```
#[derive(Clone, Debug)]
pub struct VastClient {
    pub(crate) inner: Arc<Inner>,
}

impl VastClient {
    /// Start building a new client.
    ///
    /// This is a shorthand for [`ClientConfig::builder()`] followed by
    /// [`VastClient::new()`].
    pub fn builder() -> ClientConfigBuilder {
        ClientConfig::builder()
    }

    /// Construct a client from a pre-built [`ClientConfig`].
    pub fn new(config: ClientConfig) -> Result<Self> {
        let builder = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(config.timeout_secs))
            .danger_accept_invalid_certs(config.danger_accept_invalid_certs)
            .default_headers({
                let mut headers = header::HeaderMap::new();
                headers.insert(
                    header::ACCEPT,
                    header::HeaderValue::from_static("application/json"),
                );
                headers.insert(
                    header::CONTENT_TYPE,
                    header::HeaderValue::from_static("application/json"),
                );
                headers
            });

        let http = builder.build().map_err(Error::Http)?;

        let base_url = config.base_url.clone();

        Ok(Self {
            inner: Arc::new(Inner {
                http,
                base_url,
                config,
                cached_token: RwLock::new(None),
            }),
        })
    }

    // -----------------------------------------------------------------------
    // Auth helpers
    // -----------------------------------------------------------------------

    /// Returns a valid bearer token, fetching one from the API if needed.
    pub(crate) async fn bearer_token(&self) -> Result<String> {
        // Fast path: already have one cached.
        {
            let guard = self.inner.cached_token.read().await;
            if let Some(tok) = guard.as_deref() {
                return Ok(tok.to_string());
            }
        }

        // Slow path: fetch from auth strategy and cache.
        let token = self
            .inner
            .config
            .auth
            .bearer_token(&self.inner.http, &self.inner.base_url)
            .await?;

        {
            let mut guard = self.inner.cached_token.write().await;
            *guard = Some(token.clone());
        }

        Ok(token)
    }

    // -----------------------------------------------------------------------
    // Low-level request helpers
    // -----------------------------------------------------------------------

    /// Build a [`RequestBuilder`] for the given method and relative path.
    ///
    /// `path` should be relative to the API base URL, e.g. `"clusters/"`.
    pub(crate) async fn request(&self, method: Method, path: &str) -> Result<RequestBuilder> {
        let url = self.inner.base_url.join(path)?;
        let token = self.bearer_token().await?;

        let rb = self
            .inner
            .http
            .request(method, url)
            .bearer_auth(token);

        Ok(rb)
    }

    /// Perform a GET and deserialize the JSON response.
    #[instrument(skip(self), fields(path))]
    pub(crate) async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        let resp = self.request(Method::GET, path).await?.send().await?;
        self.parse_response(resp).await
    }

    /// Perform a GET with query parameters and deserialize the response.
    #[instrument(skip(self, params), fields(path))]
    pub(crate) async fn get_with_query<T, Q>(&self, path: &str, params: &Q) -> Result<T>
    where
        T: DeserializeOwned,
        Q: Serialize + ?Sized,
    {
        let resp = self
            .request(Method::GET, path)
            .await?
            .query(params)
            .send()
            .await?;
        self.parse_response(resp).await
    }

    /// Perform a POST with a JSON body and deserialize the response.
    #[instrument(skip(self, body), fields(path))]
    pub(crate) async fn post<T, B>(&self, path: &str, body: &B) -> Result<T>
    where
        T: DeserializeOwned,
        B: Serialize + ?Sized,
    {
        let resp = self
            .request(Method::POST, path)
            .await?
            .json(body)
            .send()
            .await?;
        self.parse_response(resp).await
    }

    /// Perform a PATCH with a JSON body and deserialize the response.
    #[instrument(skip(self, body), fields(path))]
    pub(crate) async fn patch<T, B>(&self, path: &str, body: &B) -> Result<T>
    where
        T: DeserializeOwned,
        B: Serialize + ?Sized,
    {
        let resp = self
            .request(Method::PATCH, path)
            .await?
            .json(body)
            .send()
            .await?;
        self.parse_response(resp).await
    }

    /// Perform a DELETE and return nothing on success.
    #[instrument(skip(self), fields(path))]
    pub(crate) async fn delete(&self, path: &str) -> Result<()> {
        let resp = self.request(Method::DELETE, path).await?.send().await?;
        if resp.status().is_success() {
            return Ok(());
        }
        Err(self.api_error(resp).await)
    }

    /// Perform a DELETE with a JSON body and return nothing on success.
    #[instrument(skip(self, body), fields(path))]
    pub(crate) async fn delete_with_body<B>(&self, path: &str, body: &B) -> Result<()>
    where
        B: Serialize + ?Sized,
    {
        let resp = self
            .request(Method::DELETE, path)
            .await?
            .json(body)
            .send()
            .await?;
        if resp.status().is_success() {
            return Ok(());
        }
        Err(self.api_error(resp).await)
    }

    /// Parse a response: return deserialized body on 2xx, or an [`Error::Api`]
    /// on any non-success status.
    async fn parse_response<T: DeserializeOwned>(&self, resp: Response) -> Result<T> {
        let status = resp.status();
        if status.is_success() {
            Ok(resp.json::<T>().await?)
        } else {
            Err(self.api_error_from_status_and_body(status, resp).await)
        }
    }

    async fn api_error(&self, resp: Response) -> Error {
        let status = resp.status();
        self.api_error_from_status_and_body(status, resp).await
    }

    async fn api_error_from_status_and_body(&self, status: StatusCode, resp: Response) -> Error {
        // Try to extract a human-readable message from the JSON body.
        let message = match resp.json::<serde_json::Value>().await {
            Ok(v) => {
                // VAST typically returns {"detail": "..."} or {"message": "..."}
                v.get("detail")
                    .or_else(|| v.get("message"))
                    .and_then(|m| m.as_str())
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| v.to_string())
            }
            Err(_) => status.to_string(),
        };
        Error::Api {
            status: status.as_u16(),
            message,
        }
    }

    // -----------------------------------------------------------------------
    // API namespaces
    // -----------------------------------------------------------------------

    /// Access the Clusters API.
    pub fn clusters(&self) -> ClustersApi<'_> {
        ClustersApi::new(self)
    }

    /// Access the Folders API.
    pub fn folders(&self) -> FoldersApi<'_> {
        FoldersApi::new(self)
    }

    /// Access the Nodes API.
    pub fn nodes(&self) -> NodesApi<'_> {
        NodesApi::new(self)
    }

    /// Access the Users API.
    pub fn users(&self) -> UsersApi<'_> {
        UsersApi::new(self)
    }

    /// Access the Volumes API.
    pub fn volumes(&self) -> VolumesApi<'_> {
        VolumesApi::new(self)
    }

    /// Access the Views API.
    pub fn views(&self) -> ViewsApi<'_> {
        ViewsApi::new(self)
    }

    /// Access the View Policies API.
    pub fn view_policies(&self) -> ViewPoliciesApi<'_> {
        ViewPoliciesApi::new(self)
    }

    /// Access the Quotas API.
    pub fn quotas(&self) -> QuotasApi<'_> {
        QuotasApi::new(self)
    }

    /// Access the VIP Pools API.
    pub fn vip_pools(&self) -> VipPoolsApi<'_> {
        VipPoolsApi::new(self)
    }

    /// Access the Snapshots API.
    pub fn snapshots(&self) -> SnapshotsApi<'_> {
        SnapshotsApi::new(self)
    }

    /// Access the Tenants API.
    pub fn tenants(&self) -> TenantsApi<'_> {
        TenantsApi::new(self)
    }

    /// Access the Protection Policies API.
    pub fn protection_policies(&self) -> ProtectionPoliciesApi<'_> {
        ProtectionPoliciesApi::new(self)
    }
}

// Allow ClientConfigBuilder to produce a VastClient directly via .build().
impl ClientConfigBuilder {
    /// Consume this builder and produce a [`VastClient`].
    pub fn build(self) -> Result<VastClient> {
        let config = self.into_config()?;
        VastClient::new(config)
    }
}
