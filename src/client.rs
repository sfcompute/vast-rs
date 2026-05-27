//! HTTP client and builder.

use std::sync::Arc;
use std::time::Duration;

use reqwest::{Method, header};
use secrecy::{ExposeSecret, SecretString};
use serde::{Serialize, de::DeserializeOwned};
use tokio::sync::RwLock;
use url::Url;

use crate::api;
use crate::auth::Auth;
use crate::error::{Error, Result};

/// Async client for the VAST Data Management System (VMS) REST API.
///
/// Cheap to clone — all clones share the same connection pool and cached JWT.
#[derive(Clone, Debug)]
pub struct VastClient {
    inner: Arc<Inner>,
}

#[derive(Debug)]
struct Inner {
    http: reqwest::Client,
    base: Url,
    auth: Auth,
    /// Cached bearer token. Held inside `SecretString` so it can't leak
    /// through `Debug` (`Inner` derives `Debug` for the public client),
    /// and behind `RwLock` so a `bearer_token()` slow path can hold the
    /// write guard across the credential exchange — that gives us
    /// single-flight initialisation without an extra primitive.
    cached_token: RwLock<Option<SecretString>>,
}

impl VastClient {
    /// Start building a new client.
    pub fn builder() -> Builder {
        Builder::default()
    }

    /// Construct a client directly from environment variables — see
    /// [`Builder::from_env`].
    pub fn from_env() -> Result<Self> {
        Builder::from_env()?.build()
    }

    // -- API namespaces --------------------------------------------------------

    /// `/api/clusters/`
    pub fn clusters(&self) -> api::Clusters<'_> {
        api::Clusters(self)
    }
    /// `/api/folders/`
    pub fn folders(&self) -> api::Folders<'_> {
        api::Folders(self)
    }
    /// `/api/nodes/`
    pub fn nodes(&self) -> api::Nodes<'_> {
        api::Nodes(self)
    }
    /// `/api/users/`
    pub fn users(&self) -> api::Users<'_> {
        api::Users(self)
    }
    /// `/api/volumes/`
    pub fn volumes(&self) -> api::Volumes<'_> {
        api::Volumes(self)
    }
    /// `/api/views/`
    pub fn views(&self) -> api::Views<'_> {
        api::Views(self)
    }
    /// `/api/viewpolicies/`
    pub fn view_policies(&self) -> api::ViewPolicies<'_> {
        api::ViewPolicies(self)
    }
    /// `/api/quotas/`
    pub fn quotas(&self) -> api::Quotas<'_> {
        api::Quotas(self)
    }
    /// `/api/vippools/`
    pub fn vip_pools(&self) -> api::VipPools<'_> {
        api::VipPools(self)
    }
    /// `/api/snapshots/`
    pub fn snapshots(&self) -> api::Snapshots<'_> {
        api::Snapshots(self)
    }
    /// `/api/tenants/`
    pub fn tenants(&self) -> api::Tenants<'_> {
        api::Tenants(self)
    }
    /// `/api/protectionpolicies/`
    pub fn protection_policies(&self) -> api::ProtectionPolicies<'_> {
        api::ProtectionPolicies(self)
    }

    // -- HTTP plumbing (used by api::*) ---------------------------------------

    pub(crate) async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        self.send(Method::GET, path, None::<&()>, None::<&()>).await
    }

    pub(crate) async fn get_with_query<T: DeserializeOwned, Q: Serialize + ?Sized>(
        &self,
        path: &str,
        query: &Q,
    ) -> Result<T> {
        self.send(Method::GET, path, Some(query), None::<&()>).await
    }

    /// Auto-paginate a list endpoint, returning every item across all
    /// pages. Handles both the DRF paginated wrapper and the bare-array
    /// shape via [`api::PaginatedResponse`].
    ///
    /// Takes `params` by value so the helper can advance the page number
    /// in-place between requests (requires `Q: Paginate`).
    pub(crate) async fn list_all<T, Q>(&self, path: &str, params: Q) -> Result<Vec<T>>
    where
        T: DeserializeOwned,
        Q: Serialize + api::Paginate,
    {
        let mut params = params;
        let mut all = Vec::new();
        loop {
            let resp: api::PaginatedResponse<T> = self.get_with_query(path, &params).await?;
            let page = resp.into_page();
            all.extend(page.items);
            match page.next_page {
                Some(n) => params.set_page(n),
                None => return Ok(all),
            }
        }
    }

    /// Fetch a single page of a list endpoint, returning the page
    /// metadata (`count`, `next_page`, `previous_page`) alongside the
    /// items. Handles both response shapes.
    pub(crate) async fn get_page<T, Q>(&self, path: &str, params: &Q) -> Result<api::Page<T>>
    where
        T: DeserializeOwned,
        Q: Serialize + ?Sized,
    {
        let resp: api::PaginatedResponse<T> = self.get_with_query(path, params).await?;
        Ok(resp.into_page())
    }

    pub(crate) async fn post<T: DeserializeOwned, B: Serialize + ?Sized>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T> {
        self.send(Method::POST, path, None::<&()>, Some(body)).await
    }

    pub(crate) async fn patch<T: DeserializeOwned, B: Serialize + ?Sized>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T> {
        self.send(Method::PATCH, path, None::<&()>, Some(body))
            .await
    }

    pub(crate) async fn delete(&self, path: &str) -> Result<()> {
        self.send_no_body(Method::DELETE, path, None::<&()>).await
    }

    pub(crate) async fn delete_with_body<B: Serialize + ?Sized>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<()> {
        self.send_no_body(Method::DELETE, path, Some(body)).await
    }

    async fn send<T, Q, B>(
        &self,
        method: Method,
        path: &str,
        query: Option<&Q>,
        body: Option<&B>,
    ) -> Result<T>
    where
        T: DeserializeOwned,
        Q: Serialize + ?Sized,
        B: Serialize + ?Sized,
    {
        let resp = self.request(method, path, query, body).await?;
        let status = resp.status();
        if status.is_success() {
            Ok(resp.json().await?)
        } else {
            Err(api_error(status.as_u16(), resp).await)
        }
    }

    async fn send_no_body<B>(&self, method: Method, path: &str, body: Option<&B>) -> Result<()>
    where
        B: Serialize + ?Sized,
    {
        let resp = self.request(method, path, None::<&()>, body).await?;
        if resp.status().is_success() {
            Ok(())
        } else {
            Err(api_error(resp.status().as_u16(), resp).await)
        }
    }

    async fn request<Q, B>(
        &self,
        method: Method,
        path: &str,
        query: Option<&Q>,
        body: Option<&B>,
    ) -> Result<reqwest::Response>
    where
        Q: Serialize + ?Sized,
        B: Serialize + ?Sized,
    {
        let resp = self.send_once(method.clone(), path, query, body).await?;

        // The VMS gates all routes behind the auth middleware, so 401 is
        // returned before any handler runs — retrying with a fresh JWT is
        // safe (no risk of doubled side effects). Only refresh if the
        // credentials can actually produce a new token; a static API
        // token will just 401 again.
        if resp.status() == reqwest::StatusCode::UNAUTHORIZED && self.inner.auth.is_refreshable() {
            tracing::debug!("got 401 from VMS; refreshing cached JWT and retrying once");
            self.invalidate_cached_token().await;
            return self.send_once(method, path, query, body).await;
        }

        Ok(resp)
    }

    #[tracing::instrument(
        name = "vast.request",
        skip_all,
        fields(
            http.method = %method,
            http.path = %path,
            http.status = tracing::field::Empty,
            duration_ms = tracing::field::Empty,
        ),
        err,
    )]
    async fn send_once<Q, B>(
        &self,
        method: Method,
        path: &str,
        query: Option<&Q>,
        body: Option<&B>,
    ) -> Result<reqwest::Response>
    where
        Q: Serialize + ?Sized,
        B: Serialize + ?Sized,
    {
        let token = self.bearer_token().await?;
        let url = self.inner.base.join(path)?;
        // Expose the secret only at the wire boundary. `bearer_auth`
        // marks the resulting `Authorization` header as sensitive, so
        // reqwest's tracing won't log it.
        let mut rb = self
            .inner
            .http
            .request(method, url)
            .bearer_auth(token.expose_secret());
        if let Some(q) = query {
            rb = rb.query(q);
        }
        if let Some(b) = body {
            rb = rb.json(b);
        }

        let start = std::time::Instant::now();
        let resp = rb.send().await?;
        let span = tracing::Span::current();
        span.record("http.status", resp.status().as_u16());
        span.record("duration_ms", start.elapsed().as_millis() as u64);
        Ok(resp)
    }

    /// Return a valid bearer token, performing the credential exchange
    /// on first use. Uses double-checked locking so concurrent first
    /// callers exchange credentials exactly once instead of racing
    /// to hit `/api/token/`.
    async fn bearer_token(&self) -> Result<SecretString> {
        // Fast path: another task already populated the cache.
        if let Some(t) = self.inner.cached_token.read().await.as_ref() {
            return Ok(t.clone());
        }
        // Slow path: take the write lock and re-check inside it. The
        // credential exchange happens with the lock held, which is what
        // makes this single-flight — concurrent callers wait on the
        // same exchange instead of issuing duplicates.
        let mut guard = self.inner.cached_token.write().await;
        if let Some(t) = guard.as_ref() {
            return Ok(t.clone());
        }
        tracing::debug!("performing credential exchange against VMS token endpoint");
        let fetched = self
            .inner
            .auth
            .bearer_token(&self.inner.http, &self.inner.base)
            .await?;
        *guard = Some(fetched.clone());
        Ok(fetched)
    }

    async fn invalidate_cached_token(&self) {
        *self.inner.cached_token.write().await = None;
    }
}

/// Build an [`Error::Api`] from a non-2xx response.
///
/// VAST returns JSON `{"detail": "..."}` (DRF default) for most error
/// cases, but a misbehaving gateway, proxy, or upstream 5xx may return
/// HTML or an empty body. Read the body as text first so we always
/// surface something useful in those cases instead of dropping it.
async fn api_error(status: u16, resp: reqwest::Response) -> Error {
    let body = resp.text().await.unwrap_or_default();
    let message = if body.is_empty() {
        format!("HTTP {status}")
    } else {
        match serde_json::from_str::<serde_json::Value>(&body) {
            // Structured VMS error — prefer DRF's `detail`, fall back
            // to `message`, then to the raw serialised body.
            Ok(v) => v
                .get("detail")
                .or_else(|| v.get("message"))
                .and_then(|m| m.as_str())
                .map(str::to_string)
                .unwrap_or_else(|| body.clone()),
            // Non-JSON (HTML error pages, plain text, etc.) — surface
            // a truncated raw body so the user can still tell what
            // happened. Truncate by chars, not bytes, so a multi-byte
            // UTF-8 codepoint can't be split.
            Err(_) => {
                const MAX_LEN: usize = 512;
                let trimmed = body.trim();
                if trimmed.chars().count() > MAX_LEN {
                    let head: String = trimmed.chars().take(MAX_LEN).collect();
                    format!("{head}…")
                } else {
                    trimmed.to_string()
                }
            }
        }
    };
    Error::Api { status, message }
}

// ===========================================================================
// Builder
// ===========================================================================

const API_BASE_PATH: &str = "/api/";

/// Builder for [`VastClient`]. Obtain via [`VastClient::builder`].
#[derive(Default, Debug)]
pub struct Builder {
    address: Option<String>,
    auth: Option<Auth>,
    tenant: Option<String>,
    accept_invalid_certs: bool,
    timeout: Option<Duration>,
}

impl Builder {
    /// VMS hostname or IP (the `https://` scheme and `/api/` base path are
    /// added automatically if absent). Required.
    pub fn address(mut self, address: impl Into<String>) -> Self {
        self.address = Some(address.into());
        self
    }

    /// Authenticate with a long-lived API token (recommended for automation).
    pub fn token(mut self, token: impl Into<String>) -> Self {
        self.auth = Some(Auth::Token(SecretString::new(token.into())));
        self
    }

    /// Authenticate with a username/password pair. For tenant admins also call
    /// [`tenant`](Self::tenant); cluster admins can omit it.
    pub fn credentials(mut self, user: impl Into<String>, pass: impl Into<String>) -> Self {
        self.auth = Some(Auth::Password {
            username: user.into(),
            password: SecretString::new(pass.into()),
            tenant: None,
        });
        self
    }

    /// Tenant name for tenant-admin credential auth. Without this, the VMS
    /// returns 401 for tenant-scoped users.
    pub fn tenant(mut self, tenant: impl Into<String>) -> Self {
        self.tenant = Some(tenant.into());
        self
    }

    /// Accept self-signed / invalid TLS certificates. **Development only.**
    pub fn danger_accept_invalid_certs(mut self, yes: bool) -> Self {
        self.accept_invalid_certs = yes;
        self
    }

    /// Per-request timeout (default: 30 seconds).
    pub fn timeout(mut self, t: Duration) -> Self {
        self.timeout = Some(t);
        self
    }

    /// Build from environment variables: `VMS_ADDRESS` plus either `VMS_TOKEN`
    /// or `VMS_USER`+`VMS_PASSWORD` (and optional `VMS_TENANT`).
    ///
    /// Also reads `VMS_DANGER_ACCEPT_INVALID_CERTS` — set to a truthy
    /// value (`"1"`, `"true"`, `"yes"`, `"on"`, case-insensitive) to
    /// disable TLS certificate validation. **Development / self-signed
    /// VMS deployments only.** Equivalent to calling
    /// [`danger_accept_invalid_certs(true)`](Self::danger_accept_invalid_certs)
    /// on the builder.
    pub fn from_env() -> Result<Self> {
        let address = std::env::var("VMS_ADDRESS")
            .map_err(|_| Error::Config("VMS_ADDRESS must be set".into()))?;
        let auth = Auth::from_env()
            .ok_or_else(|| Error::Config("set VMS_TOKEN or VMS_USER + VMS_PASSWORD".into()))?;
        let accept_invalid_certs = std::env::var("VMS_DANGER_ACCEPT_INVALID_CERTS")
            .ok()
            .map(|v| truthy(&v))
            .unwrap_or(false);
        if accept_invalid_certs {
            tracing::warn!(
                "VMS_DANGER_ACCEPT_INVALID_CERTS is set — TLS certificate validation is disabled"
            );
        }
        Ok(Self {
            address: Some(address),
            auth: Some(auth),
            accept_invalid_certs,
            ..Default::default()
        })
    }

    /// Consume the builder and produce a [`VastClient`].
    pub fn build(self) -> Result<VastClient> {
        let address = self
            .address
            .ok_or_else(|| Error::Config("address is required".into()))?;
        let auth = match self.auth {
            None => return Err(Error::Config("call .token() or .credentials()".into())),
            Some(Auth::Password {
                username,
                password,
                tenant,
            }) => Auth::Password {
                username,
                password,
                tenant: self.tenant.or(tenant),
            },
            Some(other) => {
                if self.tenant.is_some() {
                    tracing::warn!(".tenant() has no effect with token auth");
                }
                other
            }
        };

        let base = normalize_base_url(&address)?;
        let http = reqwest::Client::builder()
            .timeout(self.timeout.unwrap_or(Duration::from_secs(30)))
            .danger_accept_invalid_certs(self.accept_invalid_certs)
            .default_headers({
                let mut h = header::HeaderMap::new();
                h.insert(
                    header::ACCEPT,
                    header::HeaderValue::from_static("application/json"),
                );
                h.insert(
                    header::CONTENT_TYPE,
                    header::HeaderValue::from_static("application/json"),
                );
                h
            })
            .build()?;

        Ok(VastClient {
            inner: Arc::new(Inner {
                http,
                base,
                auth,
                cached_token: RwLock::new(None),
            }),
        })
    }
}

fn normalize_base_url(addr: &str) -> Result<Url> {
    let with_scheme = if addr.starts_with("http://") || addr.starts_with("https://") {
        addr.to_string()
    } else {
        format!("https://{addr}")
    };
    let parsed = Url::parse(&with_scheme)?;
    if parsed.path().ends_with(API_BASE_PATH) {
        return Ok(parsed);
    }
    let host = format!(
        "{}://{}{}",
        parsed.scheme(),
        parsed.host_str().unwrap_or(""),
        parsed.port().map(|p| format!(":{p}")).unwrap_or_default()
    );
    Ok(Url::parse(&format!("{host}{API_BASE_PATH}"))?)
}

/// Parse an env-var-style boolean. Accepts the usual unix-ish synonyms
/// (`"1"`, `"true"`, `"yes"`, `"on"`, case-insensitive); everything else
/// — including the explicit negatives `"0"` / `"false"` and any
/// unrecognized junk — is treated as `false`. Whitespace is trimmed so
/// that a YAML-rendered ` true\n` from a k8s ConfigMap still works.
fn truthy(v: &str) -> bool {
    matches!(
        v.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "on"
    )
}

#[cfg(test)]
mod tests {
    // Test code uses `.unwrap()` freely on `Result`s that can't fail
    // under the inputs given. The crate-level `#![warn(clippy::unwrap_used,
    // clippy::expect_used)]` in `lib.rs` is meant for library code, not
    // tests, so opt out locally.
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    #[test]
    fn normalize_base_url_host_only_adds_https_and_api_path() {
        let u = normalize_base_url("vms.example.com").unwrap();
        assert_eq!(u.as_str(), "https://vms.example.com/api/");
    }

    #[test]
    fn normalize_base_url_host_port_preserves_port() {
        let u = normalize_base_url("vms.example.com:8443").unwrap();
        assert_eq!(u.as_str(), "https://vms.example.com:8443/api/");
    }

    #[test]
    fn normalize_base_url_full_https_url_with_api_path_passes_through() {
        let u = normalize_base_url("https://vms.example.com/api/").unwrap();
        assert_eq!(u.as_str(), "https://vms.example.com/api/");
    }

    #[test]
    fn normalize_base_url_http_scheme_preserved() {
        // Plain HTTP is used by the wiremock-backed tests and shouldn't
        // be rewritten to https.
        let u = normalize_base_url("http://127.0.0.1:12345").unwrap();
        assert_eq!(u.as_str(), "http://127.0.0.1:12345/api/");
    }

    #[test]
    fn normalize_base_url_drops_extraneous_path_components() {
        // If the caller passes a URL with a non-`/api/` path, we replace
        // it with `/api/` rather than appending. This matches the
        // expected base for `Url::join` to produce `<base>/clusters/`.
        let u = normalize_base_url("https://vms.example.com/legacy/").unwrap();
        assert_eq!(u.as_str(), "https://vms.example.com/api/");
    }

    #[test]
    fn debug_output_redacts_password() {
        // README declares: secrets discipline — passwords must not flow
        // into Debug. The `SecretString` wrapper enforces this.
        let builder = Builder::default()
            .address("vms.example.com")
            .credentials("alice", "hunter2");
        let dbg = format!("{builder:?}");
        assert!(
            !dbg.contains("hunter2"),
            "password leaked into Debug output: {dbg}"
        );
        // The username and address are not secrets and SHOULD appear
        // so operators can tell which client they're looking at.
        assert!(
            dbg.contains("alice"),
            "username should appear in Debug: {dbg}"
        );
    }

    #[test]
    fn debug_output_redacts_token() {
        let builder = Builder::default()
            .address("vms.example.com")
            .token("super-secret-token-value");
        let dbg = format!("{builder:?}");
        assert!(
            !dbg.contains("super-secret-token-value"),
            "token leaked into Debug output: {dbg}"
        );
    }

    #[test]
    fn truthy_accepts_common_yes_synonyms() {
        for v in [
            "1", "true", "TRUE", "True", "yes", "YES", "on", "ON", " true ", "\ttrue\n",
        ] {
            assert!(truthy(v), "{v:?} should be truthy");
        }
    }

    #[test]
    fn truthy_rejects_no_synonyms_and_junk() {
        for v in ["", "0", "false", "FALSE", "no", "off", "anything-else", " "] {
            assert!(!truthy(v), "{v:?} should not be truthy");
        }
    }
}
