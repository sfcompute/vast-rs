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
///
/// The paths named on the accessors below are relative to the API root, which is
/// `/api/` unless [`Builder::api_version`] sets a version, then `/api/<version>/`.
#[derive(Clone, Debug)]
pub struct VastClient {
    inner: Arc<Inner>,
}

#[derive(Debug)]
struct Inner {
    http: reqwest::Client,
    /// API root every request path is joined onto: `<scheme>://<host>/api/`,
    /// carrying the configured API version as a further segment when one is set
    /// (`…/api/v7/`).
    base: Url,
    auth: Auth,
    /// Cached bearer token. Held inside `SecretString` so it can't leak
    /// through `Debug` (`Inner` derives `Debug` for the public client),
    /// and behind `RwLock` so a `bearer_token()` slow path can hold the
    /// write guard across the credential exchange — that gives us
    /// single-flight initialisation without an extra primitive.
    cached_token: RwLock<Option<CachedToken>>,
    /// Max attempts per **GET** request (initial + retries). POST /
    /// PATCH / DELETE are never retried because they may be
    /// non-idempotent.
    max_attempts: u32,
    /// Backoff base. Sleeps grow as `base * 2^(attempt-1)` between
    /// attempts (1s → 2s → 4s with the default base).
    retry_backoff: Duration,
}

/// A cached bearer token, tagged with the generation it was issued under.
///
/// The generation is what makes refresh idempotent under concurrency. A
/// request that 401s records the generation of the token it *used*; the
/// refresh then runs only if the cache still holds that same generation.
/// Without it, every task in a batch of concurrent 401s would blindly
/// null the cache — discarding a token a sibling task had just fetched —
/// and trigger one full credential exchange per in-flight request.
#[derive(Clone, Debug)]
struct CachedToken {
    secret: SecretString,
    generation: u64,
}

/// Retry budget shared by the request path and the credential exchange.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Retry {
    max_attempts: u32,
    backoff_base: Duration,
}

impl Retry {
    pub(crate) fn max_attempts(&self) -> u32 {
        self.max_attempts.max(1)
    }

    /// Backoff before the attempt following `attempt` (1-based): grows as
    /// `base * 2^(attempt-1)`. The shift is clamped so an unusually large
    /// `max_attempts` can't overflow the multiplication.
    pub(crate) fn delay(&self, attempt: u32) -> Duration {
        self.backoff_base * 2u32.saturating_pow(attempt.saturating_sub(1).min(16))
    }
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
    /// `/api/folders/{create_folder,stat_path,delete_folder}/` — action
    /// endpoints keyed by path, not a listable resource.
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
    /// `/api/s3policies/`
    pub fn s3_policies(&self) -> api::S3Policies<'_> {
        api::S3Policies(self)
    }

    // -- HTTP plumbing (used by api::*) ---------------------------------------

    pub(crate) async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        self.send(Method::GET, path, None::<&()>, None::<&()>).await
    }

    pub async fn get_with_query<T: DeserializeOwned, Q: Serialize + ?Sized>(
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
            let bytes = resp.bytes().await?;
            serde_json::from_slice(&bytes).map_err(|source| Error::Decode {
                path: path.to_string(),
                source,
            })
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
        // GETs are idempotent so we retry them on transient failures.
        // POST/PATCH/DELETE may be non-idempotent and are sent at most
        // once. The 401 refresh-and-replay below is independent of this
        // budget — it's a single logical retry triggered by a known-safe
        // condition (the VMS rejects with 401 before any handler runs).
        let retry = Retry {
            max_attempts: if method == Method::GET {
                self.inner.max_attempts
            } else {
                1
            },
            backoff_base: self.inner.retry_backoff,
        };

        let (resp, generation) = self
            .send_with_retries(method.clone(), path, query, body, retry)
            .await?;

        // The VMS gates all routes behind the auth middleware, so an auth
        // rejection is returned before any handler runs — retrying with a
        // fresh JWT is safe (no risk of doubled side effects). Only refresh
        // if the credentials can actually produce a new token; a static API
        // token will just be rejected again.
        let status = resp.status();
        if self.inner.auth.is_refreshable() && is_auth_rejection(status) {
            // Read the body before deciding: the decision depends on it,
            // and a rejection we *don't* refresh becomes an error whose
            // message comes from that same body.
            let err_body = resp.text().await.unwrap_or_default();

            // A 401 on a bearer-authenticated route is unambiguous — the
            // token was rejected. A 403 is not: DRF returns it both for a
            // rejected JWT (the VMS surfaces simplejwt's "Given token not
            // valid for any token type") and for a plain permission denial.
            // Refreshing on a permission denial would mint a token per
            // request to fix something no token can, so a 403 has to name a
            // token problem before we act on it.
            if status == reqwest::StatusCode::UNAUTHORIZED || reports_token_rejected(&err_body) {
                tracing::debug!(
                    generation,
                    http.status = status.as_u16(),
                    "VMS rejected the JWT; refreshing and retrying once"
                );
                self.refresh_token(generation).await?;
                // Replay under the same budget rather than as a single bare
                // attempt: a burst of rejections across replicas sharing one
                // service account can draw a 429, and the replay deserves
                // the same backoff any other request gets. This still can't
                // loop — the replay's own rejection goes to the caller
                // unrefreshed.
                let (resp, _) = self
                    .send_with_retries(method, path, query, body, retry)
                    .await?;
                return Ok(resp);
            }

            tracing::debug!(
                http.status = status.as_u16(),
                "got 403 from VMS that does not report a token problem; \
                 treating as a permission denial rather than refreshing"
            );
            return Err(api_error_from_body(status.as_u16(), &err_body));
        }

        Ok(resp)
    }

    /// Send a request, retrying transient failures per `retry`. Returns the
    /// response alongside the generation of the token it was sent with, so
    /// a 401 can be attributed to a specific cached token.
    async fn send_with_retries<Q, B>(
        &self,
        method: Method,
        path: &str,
        query: Option<&Q>,
        body: Option<&B>,
        retry: Retry,
    ) -> Result<(reqwest::Response, u64)>
    where
        Q: Serialize + ?Sized,
        B: Serialize + ?Sized,
    {
        let max_attempts = retry.max_attempts();
        let mut attempt: u32 = 0;
        loop {
            attempt += 1;
            // Fetch the token outside the retried region. A failed
            // credential exchange has already spent its own budget inside
            // `Auth::bearer_token`, which retries only transient
            // conditions — folding it into this loop as well would
            // multiply token requests, so a rejected password would spend
            // `max_attempts` tries against the account's lockout policy
            // instead of one. On the happy path this is a cache read.
            let token = self.bearer_token().await?;
            let generation = token.generation;
            match self
                .send_once(method.clone(), path, query, body, &token)
                .await
            {
                Ok(resp) => {
                    // Retry 5xx and 429 (rate-limited) responses if we
                    // have budget; everything else (2xx, 3xx, 4xx other
                    // than 429) flows through to the caller's 401 check.
                    let s = resp.status();
                    let retryable_status =
                        s.is_server_error() || s == reqwest::StatusCode::TOO_MANY_REQUESTS;
                    if retryable_status && attempt < max_attempts {
                        let delay = retry.delay(attempt);
                        tracing::warn!(
                            attempt,
                            max_attempts,
                            http.method = %method,
                            http.path = %path,
                            http.status = s.as_u16(),
                            retry_in_ms = delay.as_millis() as u64,
                            "request returned retryable status; sleeping and retrying",
                        );
                        tokio::time::sleep(delay).await;
                        continue;
                    }
                    return Ok((resp, generation));
                }
                Err(e) if attempt < max_attempts => {
                    let delay = retry.delay(attempt);
                    tracing::warn!(
                        attempt,
                        max_attempts,
                        http.method = %method,
                        http.path = %path,
                        retry_in_ms = delay.as_millis() as u64,
                        error = %e,
                        "request failed at transport layer; sleeping and retrying",
                    );
                    tokio::time::sleep(delay).await;
                    continue;
                }
                Err(e) => return Err(e),
            }
        }
    }

    #[tracing::instrument(
        name = "vast.request",
        skip_all,
        fields(
            http.method = %method,
            http.path = %path,
            http.url = tracing::field::Empty,
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
        token: &CachedToken,
    ) -> Result<reqwest::Response>
    where
        Q: Serialize + ?Sized,
        B: Serialize + ?Sized,
    {
        let url = self.inner.base.join(path)?;
        tracing::Span::current().record("http.url", url.as_str());
        // Expose the secret only at the wire boundary. `bearer_auth`
        // marks the resulting `Authorization` header as sensitive, so
        // reqwest's tracing won't log it.
        let mut rb = self
            .inner
            .http
            .request(method, url)
            .bearer_auth(token.secret.expose_secret());
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
    /// to hit the token endpoint.
    async fn bearer_token(&self) -> Result<CachedToken> {
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
        self.exchange_locked(&mut guard).await
    }

    /// Replace the cached token after a 401, but only if it hasn't already
    /// been replaced since `failed_generation` was handed out.
    ///
    /// When many in-flight requests 401 together — the normal shape of a
    /// token expiring under concurrent load — their 401s arrive spread over
    /// time. Whichever task gets there first exchanges credentials; every
    /// later task finds a newer generation than the one it used and adopts
    /// that token instead of minting another. That collapses a stampede of
    /// up to one exchange *per in-flight request* into exactly one.
    async fn refresh_token(&self, failed_generation: u64) -> Result<CachedToken> {
        let mut guard = self.inner.cached_token.write().await;
        if let Some(t) = guard.as_ref().filter(|t| t.generation != failed_generation) {
            tracing::debug!(
                failed_generation,
                current_generation = t.generation,
                "cached JWT was already refreshed by another task; reusing it"
            );
            return Ok(t.clone());
        }
        self.exchange_locked(&mut guard).await
    }

    /// Exchange credentials and store the result under the next
    /// generation. The caller must hold the write guard: the exchange runs
    /// with the lock held, which is what makes both initialisation and
    /// refresh single-flight.
    async fn exchange_locked(&self, guard: &mut Option<CachedToken>) -> Result<CachedToken> {
        tracing::debug!("performing credential exchange against VMS token endpoint");
        let secret = self
            .inner
            .auth
            .bearer_token(&self.inner.http, &self.inner.base, self.retry())
            .await?;
        // Derive the generation from the cache rather than a parameter, so
        // it stays monotonic regardless of which path got here.
        let generation = guard.as_ref().map_or(0, |t| t.generation).wrapping_add(1);
        let fresh = CachedToken { secret, generation };
        *guard = Some(fresh.clone());
        Ok(fresh)
    }

    /// The configured retry budget, used for the credential exchange and
    /// as the basis for the per-request budget.
    fn retry(&self) -> Retry {
        Retry {
            max_attempts: self.inner.max_attempts,
            backoff_base: self.inner.retry_backoff,
        }
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
    api_error_from_body(status, &body)
}

/// `true` if the status is one the VMS uses to reject a bearer token.
///
/// 401 is the textbook answer, but DRF coerces `AuthenticationFailed` to
/// 403 whenever the active authenticator supplies no `WWW-Authenticate`
/// header, so a rejected JWT can arrive as either.
fn is_auth_rejection(status: reqwest::StatusCode) -> bool {
    matches!(
        status,
        reqwest::StatusCode::UNAUTHORIZED | reqwest::StatusCode::FORBIDDEN
    )
}

/// `true` if an error body reports that the *token* was rejected, as
/// opposed to the request being forbidden on its merits.
///
/// This has to key off the body because the status code alone can't tell
/// the two apart (see [`is_auth_rejection`]). simplejwt — which the VMS's
/// error shapes match — reports an unusable token as
/// `{"detail": "Given token not valid for any token type", "messages": [...]}`,
/// sometimes with `"code": "token_not_valid"`. A permission denial instead
/// reads `{"detail": "You do not have permission to perform this action."}`,
/// which matches none of the markers below.
///
/// Matching on message text is inherently brittle, so this errs toward *not*
/// refreshing: an unrecognised 403 surfaces to the caller rather than
/// spending a credential exchange. Proactively refreshing before `exp`
/// would remove the dependence on error shapes altogether.
fn reports_token_rejected(body: &str) -> bool {
    /// Lowercase fragments that appear in token-rejection messages but not
    /// in permission denials.
    const TOKEN_MARKERS: [&str; 4] = [
        "token_not_valid",
        "token not valid",
        "token is invalid",
        "token is expired",
    ];

    let matches_marker = |s: &str| {
        TOKEN_MARKERS
            .iter()
            .any(|m| s.to_ascii_lowercase().contains(m))
    };

    if let Ok(v) = serde_json::from_str::<serde_json::Value>(body) {
        // simplejwt attaches per-token diagnostics under `messages` only
        // when the token itself failed to validate.
        if v.get("messages").is_some_and(|m| m.is_array()) {
            return true;
        }
        if v.get("code")
            .and_then(|c| c.as_str())
            .is_some_and(matches_marker)
        {
            return true;
        }
        if v.get("detail")
            .and_then(|d| d.as_str())
            .is_some_and(matches_marker)
        {
            return true;
        }
        // Structured body with no token marker — a permission denial.
        return false;
    }
    // Non-JSON (gateway HTML, plain text): fall back to a raw scan.
    matches_marker(body)
}

/// Build an [`Error::Api`] from a status and an already-read body.
fn api_error_from_body(status: u16, body: &str) -> Error {
    let message = if body.is_empty() {
        format!("HTTP {status}")
    } else {
        match serde_json::from_str::<serde_json::Value>(body) {
            // Structured VMS error — prefer DRF's `detail`, fall back
            // to `message`, then to the raw serialised body.
            Ok(v) => v
                .get("detail")
                .or_else(|| v.get("message"))
                .and_then(|m| m.as_str())
                .map(str::to_string)
                .unwrap_or_else(|| body.to_string()),
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

/// Default total attempts (initial + retries) per GET request. Applies
/// only to GETs; POST / PATCH / DELETE are sent at most once.
const DEFAULT_MAX_ATTEMPTS: u32 = 3;
/// Default backoff base. Successive retries wait `BASE * 2^(attempt-1)`.
const DEFAULT_RETRY_BACKOFF: Duration = Duration::from_secs(1);

/// Env var naming a PEM file of extra CA certificates to trust. See
/// [`Builder::ca_certificate`].
const CA_CERT_FILE_VAR: &str = "VMS_CA_CERT_FILE";

/// Env var naming the REST API version to address. See
/// [`Builder::api_version`].
const API_VERSION_VAR: &str = "VMS_API_VERSION";

/// Builder for [`VastClient`]. Obtain via [`VastClient::builder`].
#[derive(Default, Debug)]
pub struct Builder {
    address: Option<String>,
    auth: Option<Auth>,
    tenant: Option<String>,
    api_version: Option<String>,
    accept_invalid_certs: bool,
    ca_certs: Vec<CaPem>,
    timeout: Option<Duration>,
    max_attempts: Option<u32>,
    retry_backoff: Option<Duration>,
}

/// A PEM bundle held until [`Builder::build`] parses it, tagged with where it
/// came from so a parse failure can name the file (or say it was passed
/// in-process) rather than just "invalid certificate".
///
/// `Debug` is hand-written because the derived one would print the PEM as a few
/// thousand integers, drowning the address and username an operator debugging a
/// `Builder` is actually looking for.
#[derive(Clone)]
struct CaPem {
    pem: Vec<u8>,
    source: String,
}

impl std::fmt::Debug for CaPem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "CaPem({}, {} bytes)", self.source, self.pem.len())
    }
}

impl Builder {
    /// VMS hostname or IP (the `https://` scheme and `/api/` base path are
    /// added automatically if absent). Required.
    ///
    /// The API version does not go here — an address carrying a version segment
    /// is rejected at [`build`](Self::build). Use
    /// [`api_version`](Self::api_version) instead.
    pub fn address(mut self, address: impl Into<String>) -> Self {
        self.address = Some(address.into());
        self
    }

    /// Authenticate with a long-lived API token (recommended for automation).
    pub fn token(mut self, token: impl Into<String>) -> Self {
        self.auth = Some(Auth::Token(SecretString::from(token.into())));
        self
    }

    /// Authenticate with a username/password pair. For tenant admins also call
    /// [`tenant`](Self::tenant); cluster admins can omit it.
    pub fn credentials(mut self, user: impl Into<String>, pass: impl Into<String>) -> Self {
        self.auth = Some(Auth::Password {
            username: user.into(),
            password: SecretString::from(pass.into()),
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

    /// REST API version to address, as the path segment it becomes: `"v7"`
    /// sends every request to `https://<address>/api/v7/…`. Omitted — or given
    /// as an empty string — the client addresses the unversioned `/api/…`
    /// routes, which is what a VMS resolves to its oldest available version.
    ///
    /// [`build`](Self::build) rejects anything that is not a `v`-and-digits
    /// segment, since the value goes straight into the request path.
    pub fn api_version(mut self, version: impl Into<String>) -> Self {
        self.api_version = Some(version.into());
        self
    }

    /// Accept self-signed / invalid TLS certificates. **Development only.**
    ///
    /// For a VMS presenting a certificate from a private CA, prefer
    /// [`ca_certificate`](Self::ca_certificate): it keeps the connection
    /// authenticated instead of trusting whatever answers.
    pub fn danger_accept_invalid_certs(mut self, yes: bool) -> Self {
        self.accept_invalid_certs = yes;
        self
    }

    /// Trust `pem` — a PEM-encoded CA certificate, or a bundle of several —
    /// when validating the VMS certificate, in addition to the public roots.
    ///
    /// This is what a VMS with a certificate from a private CA needs, and it is
    /// not optional there: the client is built on reqwest's `rustls-tls`
    /// feature, whose root store is the Mozilla set compiled into the binary.
    /// It reads neither the host's certificate directory nor `SSL_CERT_FILE`,
    /// so installing the CA on the machine has no effect and a certificate that
    /// does not chain to a public root fails the handshake.
    ///
    /// Repeated calls accumulate rather than replace, so a VMS and an S3
    /// endpoint anchored to different roots can both be added. The PEM is
    /// parsed by [`build`](Self::build), which rejects one containing no
    /// certificate.
    ///
    /// ```rust,no_run
    /// # use vast::VastClient;
    /// let pem = std::fs::read("/etc/vast-ca/ca.crt")?;
    /// let client = VastClient::builder()
    ///     .address("vms.example.com")
    ///     .token("tok")
    ///     .ca_certificate(pem)
    ///     .build()?;
    /// # Ok::<_, Box<dyn std::error::Error>>(())
    /// ```
    pub fn ca_certificate(mut self, pem: impl Into<Vec<u8>>) -> Self {
        self.ca_certs.push(CaPem {
            pem: pem.into(),
            source: "ca_certificate()".into(),
        });
        self
    }

    /// Per-request timeout (default: 30 seconds).
    pub fn timeout(mut self, t: Duration) -> Self {
        self.timeout = Some(t);
        self
    }

    /// Maximum number of attempts per **GET** request (initial + retries).
    /// Default 3. Set to 1 to disable retries.
    ///
    /// Retries fire on transport-level failures (network, timeout) and
    /// on retryable status codes (`5xx`, `429`). POST / PATCH / DELETE
    /// are sent at most once regardless of this setting, since they
    /// may be non-idempotent.
    ///
    /// The same budget applies to the credential exchange against the token
    /// endpoint, which is retried on `5xx` / `429` / transport failures.
    /// Rejected credentials (`401` / `403`) are never retried.
    pub fn max_attempts(mut self, n: u32) -> Self {
        self.max_attempts = Some(n.max(1));
        self
    }

    /// Backoff base for retries. Successive retries wait
    /// `base * 2^(attempt-1)` — with the default 1-second base, that's
    /// 1s, 2s, 4s, ... Default: 1 second.
    pub fn retry_backoff(mut self, base: Duration) -> Self {
        self.retry_backoff = Some(base);
        self
    }

    /// Build from environment variables: `VMS_ADDRESS` plus either `VMS_TOKEN`
    /// or `VMS_USER`+`VMS_PASSWORD` (and optional `VMS_TENANT`).
    ///
    /// Also reads:
    ///
    /// * `VMS_API_VERSION` — REST API version to address, as
    ///   [`api_version`](Self::api_version): `v7` sends every request to
    ///   `/api/v7/…`. Unset or empty addresses the unversioned `/api/…` routes.
    /// * `VMS_CA_CERT_FILE` — path to a PEM CA certificate or bundle to trust
    ///   in addition to the public roots, as
    ///   [`ca_certificate`](Self::ca_certificate). A path that cannot be read
    ///   is an error, not a fallback to the public roots: silently continuing
    ///   would turn a typo'd mount path into a handshake failure on the first
    ///   request instead of a clear one at startup.
    /// * `VMS_DANGER_ACCEPT_INVALID_CERTS` — set to a truthy value (`"1"`,
    ///   `"true"`, `"yes"`, `"on"`, case-insensitive) to disable TLS
    ///   certificate validation. **Development / self-signed VMS deployments
    ///   only.** Equivalent to calling
    ///   [`danger_accept_invalid_certs(true)`](Self::danger_accept_invalid_certs)
    ///   on the builder.
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
        let ca_certs = match std::env::var(CA_CERT_FILE_VAR) {
            Ok(path) if !path.trim().is_empty() => vec![read_ca_pem(path.trim())?],
            _ => Vec::new(),
        };
        Ok(Self {
            address: Some(address),
            auth: Some(auth),
            api_version: std::env::var(API_VERSION_VAR).ok(),
            accept_invalid_certs,
            ca_certs,
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

        if self.accept_invalid_certs && !self.ca_certs.is_empty() {
            tracing::warn!(
                "TLS certificate validation is disabled, so the CA certificate(s) supplied \
                 are not used; drop danger_accept_invalid_certs to validate against them"
            );
        }

        let api_version = match self.api_version.as_deref() {
            Some(raw) => normalize_api_version(raw)?,
            None => None,
        };
        let base = normalize_base_url(&address, api_version.as_deref())?;
        let mut http = reqwest::Client::builder()
            .timeout(self.timeout.unwrap_or(Duration::from_secs(30)))
            .danger_accept_invalid_certs(self.accept_invalid_certs);
        for ca in &self.ca_certs {
            for cert in parse_ca_bundle(&ca.pem, &ca.source)? {
                http = http.add_root_certificate(cert);
            }
        }
        let http = http
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
            // Config, not Http: a failure here is the TLS backend refusing
            // what we handed it — most often a supplied CA whose PEM body is
            // not a certificate — and never a transport problem. reqwest's own
            // Display is a bare "builder error", so append the cause chain,
            // which is where the detail actually lives.
            .build()
            .map_err(|e| {
                let mut msg = format!("failed to construct the HTTP client: {e}");
                let mut cause: Option<&dyn std::error::Error> = std::error::Error::source(&e);
                while let Some(c) = cause {
                    msg.push_str(&format!(": {c}"));
                    cause = c.source();
                }
                Error::Config(msg)
            })?;

        Ok(VastClient {
            inner: Arc::new(Inner {
                http,
                base,
                auth,
                cached_token: RwLock::new(None),
                max_attempts: self.max_attempts.unwrap_or(DEFAULT_MAX_ATTEMPTS),
                retry_backoff: self.retry_backoff.unwrap_or(DEFAULT_RETRY_BACKOFF),
            }),
        })
    }
}

/// Resolve the API root every request path is joined onto.
///
/// `addr` may be a bare host, a `host:port`, or a full URL; the scheme defaults
/// to https. Its path is replaced with `/api/`, except a path that already ends
/// there, which is kept — that is what lets a VMS behind a reverse-proxy prefix
/// (`https://gw/vast/api/`) resolve. `version`, when set, becomes one further
/// segment: `/api/v7/`.
///
/// The trailing slash is load-bearing: every request path is resolved with
/// `Url::join`, which replaces the last segment of a base that lacks one.
fn normalize_base_url(addr: &str, version: Option<&str>) -> Result<Url> {
    let with_scheme = if addr.starts_with("http://") || addr.starts_with("https://") {
        addr.to_string()
    } else {
        format!("https://{addr}")
    };
    let parsed = Url::parse(&with_scheme)?;

    let path = parsed.path().trim_end_matches('/');
    // An address carrying its own version segment is a misconfiguration, not a
    // second way to set one: two sources for one value is a precedence rule
    // nobody remembers.
    if path.rsplit('/').next().is_some_and(is_version_segment) {
        return Err(Error::Config(format!(
            "address {addr:?} carries an API version in its path; \
             set the version with {API_VERSION_VAR} (or .api_version(\"…\")) instead"
        )));
    }
    let prefix = path.strip_suffix("/api").unwrap_or("");

    let origin = format!(
        "{}://{}{}",
        parsed.scheme(),
        parsed.host_str().unwrap_or(""),
        parsed.port().map(|p| format!(":{p}")).unwrap_or_default()
    );
    let version = version.map(|v| format!("{v}/")).unwrap_or_default();
    Ok(Url::parse(&format!(
        "{origin}{prefix}{API_BASE_PATH}{version}"
    ))?)
}

/// Validate a configured API version and normalize it to a bare path segment.
///
/// Empty — or whitespace, or a lone `/` — reads as unset. A Kubernetes env var
/// rendered from an absent value arrives as `""`, and that has to mean "address
/// the unversioned routes" rather than build a `/api//` path no VMS serves.
///
/// Anything else must look like `v7`, because the value is interpolated
/// straight into the request path: rejecting it at `build()` names the bad
/// config, where accepting it would send every request to a path the VMS has
/// never heard of and report it as a 404 per call.
fn normalize_api_version(raw: &str) -> Result<Option<String>> {
    let version = raw.trim().trim_matches('/');
    if version.is_empty() {
        return Ok(None);
    }
    if !is_version_segment(version) {
        return Err(Error::Config(format!(
            "invalid API version {raw:?}: expected a segment like \"v7\""
        )));
    }
    Ok(Some(version.to_string()))
}

/// `true` for a VMS API version path segment: `v` followed by digits.
fn is_version_segment(s: &str) -> bool {
    s.len() > 1 && s.starts_with('v') && s[1..].bytes().all(|b| b.is_ascii_digit())
}

/// Read a PEM CA file named by `VMS_CA_CERT_FILE`, tagging it with the path so
/// a later parse failure can say which file was wrong.
fn read_ca_pem(path: &str) -> Result<CaPem> {
    let pem = std::fs::read(path)
        .map_err(|e| Error::Config(format!("{CA_CERT_FILE_VAR}: cannot read {path}: {e}")))?;
    Ok(CaPem {
        pem,
        source: path.to_string(),
    })
}

/// Read the PEM framing of a bundle into the certificates reqwest will trust as
/// roots. Each certificate's DER body stays unparsed here; rustls validates it
/// when [`Builder::build`] assembles the client, which maps that failure to
/// [`Error::Config`] too.
///
/// The empty case is an error rather than an empty list. `from_pem_bundle`
/// reports success on input holding no PEM block at all — a placeholder never
/// filled in, an empty ConfigMap key, an HTML error page saved as a `.crt` —
/// and silently trusting nothing extra would surface only as a handshake
/// failure on the first request, pointing at the network rather than at the
/// file.
fn parse_ca_bundle(pem: &[u8], source: &str) -> Result<Vec<reqwest::Certificate>> {
    let certs = reqwest::Certificate::from_pem_bundle(pem)
        .map_err(|e| Error::Config(format!("{source}: invalid PEM certificate: {e}")))?;
    if certs.is_empty() {
        return Err(Error::Config(format!(
            "{source}: no PEM certificate found (expected a -----BEGIN CERTIFICATE----- block)"
        )));
    }
    Ok(certs)
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
        let u = normalize_base_url("vms.example.com", None).unwrap();
        assert_eq!(u.as_str(), "https://vms.example.com/api/");
    }

    #[test]
    fn normalize_base_url_host_port_preserves_port() {
        let u = normalize_base_url("vms.example.com:8443", None).unwrap();
        assert_eq!(u.as_str(), "https://vms.example.com:8443/api/");
    }

    #[test]
    fn normalize_base_url_full_https_url_with_api_path_passes_through() {
        let u = normalize_base_url("https://vms.example.com/api/", None).unwrap();
        assert_eq!(u.as_str(), "https://vms.example.com/api/");
    }

    #[test]
    fn normalize_base_url_http_scheme_preserved() {
        // Plain HTTP is used by the wiremock-backed tests and shouldn't
        // be rewritten to https.
        let u = normalize_base_url("http://127.0.0.1:12345", None).unwrap();
        assert_eq!(u.as_str(), "http://127.0.0.1:12345/api/");
    }

    #[test]
    fn normalize_base_url_drops_extraneous_path_components() {
        // If the caller passes a URL with a non-`/api/` path, we replace
        // it with `/api/` rather than appending. This matches the
        // expected base for `Url::join` to produce `<base>/clusters/`.
        let u = normalize_base_url("https://vms.example.com/legacy/", None).unwrap();
        assert_eq!(u.as_str(), "https://vms.example.com/api/");
    }

    #[test]
    fn normalize_base_url_appends_the_api_version() {
        for addr in [
            "vms.example.com",
            "https://vms.example.com",
            "https://vms.example.com/api/",
        ] {
            let u = normalize_base_url(addr, Some("v7")).unwrap();
            assert_eq!(u.as_str(), "https://vms.example.com/api/v7/", "addr {addr}");
        }
    }

    #[test]
    fn normalize_base_url_keeps_a_reverse_proxy_prefix_ahead_of_the_version() {
        let u = normalize_base_url("https://gw.example.com/vast/api/", Some("v7")).unwrap();
        assert_eq!(u.as_str(), "https://gw.example.com/vast/api/v7/");
    }

    /// The version has exactly one source, so an address carrying its own is
    /// refused rather than honoured or quietly stripped — either would make the
    /// version depend on which of the two config values won.
    #[test]
    fn normalize_base_url_rejects_a_version_in_the_address() {
        for version in [None, Some("v7")] {
            let err = normalize_base_url("https://vms.example.com/api/v5/", version)
                .expect_err("a version in the address must not be accepted");
            assert!(matches!(err, Error::Config(_)), "got {err:?}");
            assert!(
                err.to_string().contains(API_VERSION_VAR),
                "error should name the var to use: {err}"
            );
        }
    }

    /// A version resolves as one path segment, so `Url::join` on a request path
    /// must land under it rather than replacing it.
    #[test]
    fn versioned_base_joins_request_paths_under_the_version() {
        let base = normalize_base_url("vms.example.com", Some("v7")).unwrap();
        assert_eq!(
            base.join("users/12/access_keys/").unwrap().as_str(),
            "https://vms.example.com/api/v7/users/12/access_keys/"
        );
        assert_eq!(
            base.join("token/").unwrap().as_str(),
            "https://vms.example.com/api/v7/token/"
        );
    }

    #[test]
    fn normalize_api_version_reads_blank_as_unversioned() {
        // A k8s env var rendered from an absent value arrives as "".
        for raw in ["", "   ", "\n", "/"] {
            assert_eq!(normalize_api_version(raw).unwrap(), None, "raw {raw:?}");
        }
    }

    #[test]
    fn normalize_api_version_trims_to_a_bare_segment() {
        for raw in ["v7", " v7 ", "v7/", "/v7/", "\tv7\n"] {
            assert_eq!(
                normalize_api_version(raw).unwrap().as_deref(),
                Some("v7"),
                "raw {raw:?}"
            );
        }
        assert_eq!(
            normalize_api_version("v12").unwrap().as_deref(),
            Some("v12")
        );
    }

    #[test]
    fn normalize_api_version_rejects_anything_else() {
        // Each of these would otherwise be interpolated into every request
        // path: a bad segment, an escape out of it, or a whole extra level.
        for raw in [
            "7", "V7", "v", "vv7", "v7beta", "latest", "v7/users", "../v7", "v 7", "v7?x=1",
        ] {
            let err = normalize_api_version(raw)
                .expect_err(&format!("{raw:?} should not be accepted as a version"));
            assert!(matches!(err, Error::Config(_)), "{raw:?} gave {err:?}");
        }
    }

    #[test]
    fn build_rejects_an_invalid_api_version() {
        // The rejection has to survive the trip through build(), not just
        // exist as a helper.
        let err = Builder::default()
            .address("vms.example.com")
            .token("tok")
            .api_version("seven")
            .build()
            .expect_err("builder must refuse a malformed version");
        assert!(matches!(err, Error::Config(_)), "got {err:?}");
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

    /// A self-signed CA, valid until 2126, generated purely for these tests:
    /// `openssl req -x509 -newkey rsa:2048 -keyout /dev/null -nodes
    ///  -subj "/CN=vast-rs test CA" -days 36500`. Nothing trusts it.
    const TEST_CA_PEM: &str = "\
-----BEGIN CERTIFICATE-----
MIIDFzCCAf+gAwIBAgIUJpxwTFqrX/16i0njladfR5Q3nzowDQYJKoZIhvcNAQEL
BQAwGjEYMBYGA1UEAwwPdmFzdC1ycyB0ZXN0IENBMCAXDTI2MDgyNTE5MzIyMloY
DzIxMjYwODAxMTkzMjIyWjAaMRgwFgYDVQQDDA92YXN0LXJzIHRlc3QgQ0EwggEi
MA0GCSqGSIb3DQEBAQUAA4IBDwAwggEKAoIBAQCsIPh1QlswgmSDVth9XwD7m6cM
ruKNkZKThKbG8xLm43n/aw+h4BAZCIAVX3q/ccTymGLaUyeGQJ+nReVTpO0dXNCT
HqdDlfspUaPtFw83F01DCniqI5riUzTD5GA7mg4NJ5hpxpeyaDfvU/iziyM+nLSu
SqDpQ2wDY4UEa9C2BHRDXiy3EjsM4hQhOfCgA7pttty0qOxWVXClQiAEStugn/Rf
TP/iaA5Igpmqr/w3Tk4MxCAfiVSrVGeyu4VCA5XQ4rbOXqxmwp1zZ3szodmenqYJ
BUN9kWU1tPV4cXjaVi138Huclpx9qtd3V6YZQ+pSDwMuvi0rxOi95BeGSuUJAgMB
AAGjUzBRMB0GA1UdDgQWBBSG+J0ZGa5Z3GPSaHQtksCRVUYV+zAfBgNVHSMEGDAW
gBSG+J0ZGa5Z3GPSaHQtksCRVUYV+zAPBgNVHRMBAf8EBTADAQH/MA0GCSqGSIb3
DQEBCwUAA4IBAQB5SvhVadIL7Sh7OWkpqTVuX9qBKjD+0HfNDnpNPBHG+u225tVI
YtutYWf7R6o4kuxqseVqiXzYwMuDZUVn58q9oKW5SrqHwBXOVpdy0TGxxhH54wXJ
S4IwYTK/OhGBT9DPrZU67feaB7uHh8sEY8Ylroa10CsX+c1VvHcKKtTytIClKd8i
uKkNXFI3uSARJmMGkY+pjl9Z/1kn13LzgTYy/CxAz49mfELMDqBGPQ07P0n0GKNl
5YkwZuz2mU/jzNKv0LhtiZDXYgXALoZfNmp2WnJGo1xoie/+YWZT8qHDYpb+b6Nu
JvLMX92kU9FM/XiQMQjc2G8K1uis9IW+k4X+
-----END CERTIFICATE-----
";

    /// A path in the temp dir unique to this process and call site. Cheaper
    /// than a `tempfile` dev-dependency for the two tests that need a file,
    /// and `set_var` is not an option: edition 2024 makes it `unsafe`, which
    /// `unsafe_code = "forbid"` rules out crate-wide.
    fn temp_path(tag: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("vast-rs-{}-{tag}.pem", std::process::id()))
    }

    #[test]
    fn ca_certificate_accepts_a_pem_bundle() {
        // Two concatenated certs, since a bundle is the shape a k8s
        // ConfigMap or an /etc/ssl bundle actually holds.
        let bundle = format!("{TEST_CA_PEM}{TEST_CA_PEM}");
        let certs = parse_ca_bundle(bundle.as_bytes(), "test").unwrap();
        assert_eq!(certs.len(), 2);

        Builder::default()
            .address("vms.example.com")
            .token("tok")
            .ca_certificate(TEST_CA_PEM)
            .build()
            .expect("a valid CA should build");
    }

    #[test]
    fn ca_certificate_rejects_pem_holding_no_certificate() {
        // The branch that exists because from_pem_bundle returns Ok(vec![])
        // here: an unfilled placeholder trusts nothing and must not build.
        for junk in [
            "",
            "   \n",
            "REPLACE-ME: paste the PEM of the issuing CA here\n",
            "<html><body>403 Forbidden</body></html>",
        ] {
            let err = parse_ca_bundle(junk.as_bytes(), "some-file.crt")
                .expect_err("input with no PEM block must be rejected");
            assert!(
                matches!(err, Error::Config(_)),
                "expected a Config error, got {err:?}"
            );
            assert!(
                err.to_string().contains("some-file.crt"),
                "error should name the source: {err}"
            );

            // And the rejection must survive the trip through the builder,
            // rather than being dropped somewhere in build().
            let err = Builder::default()
                .address("vms.example.com")
                .token("tok")
                .ca_certificate(junk)
                .build()
                .expect_err("builder must refuse a PEM with no certificate");
            assert!(matches!(err, Error::Config(_)), "got {err:?}");
        }
    }

    #[test]
    fn ca_certificate_rejects_a_pem_body_that_is_not_a_certificate() {
        // Distinct from the empty case, and caught one layer later: the PEM
        // framing is well-formed, so from_pem_bundle hands back a Certificate
        // holding unparsed DER. rustls rejects it when reqwest assembles the
        // root store, which is inside our build(). Assert that arrives as a
        // Config error and not as Http — the `#[from] reqwest::Error` route
        // would file this misconfiguration under "HTTP error: builder error".
        let corrupt =
            "-----BEGIN CERTIFICATE-----\nbm90IGEgY2VydGlmaWNhdGU=\n-----END CERTIFICATE-----\n";
        assert_eq!(
            parse_ca_bundle(corrupt.as_bytes(), "corrupt.crt")
                .unwrap()
                .len(),
            1
        );

        let err = Builder::default()
            .address("vms.example.com")
            .token("tok")
            .ca_certificate(corrupt)
            .build()
            .expect_err("a non-certificate PEM body must not build");
        assert!(matches!(err, Error::Config(_)), "got {err:?}");
        assert!(
            err.to_string().contains("HTTP client"),
            "message should say what failed: {err}"
        );
    }

    #[test]
    fn read_ca_pem_reports_a_missing_file_with_its_path() {
        let missing = temp_path("definitely-absent");
        let _ = std::fs::remove_file(&missing);
        let err = read_ca_pem(missing.to_str().unwrap())
            .expect_err("an unreadable path must not fall back to the public roots");
        assert!(matches!(err, Error::Config(_)), "got {err:?}");
        let msg = err.to_string();
        assert!(msg.contains(CA_CERT_FILE_VAR), "should name the var: {msg}");
        assert!(
            msg.contains(missing.to_str().unwrap()),
            "should name the path: {msg}"
        );
    }

    #[test]
    fn read_ca_pem_loads_a_file_and_tags_it_with_the_path() {
        let path = temp_path("valid-ca");
        std::fs::write(&path, TEST_CA_PEM).unwrap();

        let ca = read_ca_pem(path.to_str().unwrap()).unwrap();
        assert_eq!(ca.source, path.to_str().unwrap());
        assert_eq!(parse_ca_bundle(&ca.pem, &ca.source).unwrap().len(), 1);

        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn debug_output_summarizes_ca_pem_instead_of_dumping_bytes() {
        let builder = Builder::default()
            .address("vms.example.com")
            .token("tok")
            .ca_certificate(TEST_CA_PEM);
        let dbg = format!("{builder:?}");
        assert!(
            dbg.contains("CaPem(ca_certificate(), "),
            "CA should be summarized: {dbg}"
        );
        // The derived Debug on Vec<u8> renders bytes as decimal integers;
        // "45, 45, 45" is the "---" that opens every PEM.
        assert!(!dbg.contains("45, 45, 45"), "PEM bytes leaked: {dbg}");
        assert!(
            dbg.contains("vms.example.com"),
            "address should show: {dbg}"
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
    fn reports_token_rejected_matches_simplejwt_rejection() {
        // Verbatim body observed from a VMS returning 403 for an expired
        // access JWT — the case that made refresh silently never fire.
        assert!(reports_token_rejected(
            r#"{"detail":"Given token not valid for any token type","messages":[{"token_class":"AccessToken","token_type":"access","message":"Token is invalid or expired"}]}"#
        ));
        // Same condition, leaner shapes.
        assert!(reports_token_rejected(
            r#"{"detail":"Given token not valid for any token type"}"#
        ));
        assert!(reports_token_rejected(
            r#"{"detail":"Token is invalid or expired","code":"token_not_valid"}"#
        ));
    }

    #[test]
    fn reports_token_rejected_ignores_permission_denials() {
        // These arrive with the same 403 as a rejected token. Refreshing
        // on them would mint a credential exchange per request to fix
        // something no token can fix.
        for body in [
            r#"{"detail":"You do not have permission to perform this action."}"#,
            r#"{"detail":"Authentication credentials were not provided."}"#,
            r#"{"detail":"Tenant admins may not modify cluster-wide settings."}"#,
            "",
        ] {
            assert!(
                !reports_token_rejected(body),
                "should not be treated as a token rejection: {body:?}"
            );
        }
    }

    #[test]
    fn reports_token_rejected_scans_non_json_bodies() {
        // A gateway can replace the JSON body with HTML or plain text.
        assert!(reports_token_rejected(
            "<html><body>Given token not valid for any token type</body></html>"
        ));
        assert!(!reports_token_rejected(
            "<html><body>Forbidden</body></html>"
        ));
    }

    #[test]
    fn truthy_rejects_no_synonyms_and_junk() {
        for v in ["", "0", "false", "FALSE", "no", "off", "anything-else", " "] {
            assert!(!truthy(v), "{v:?} should not be truthy");
        }
    }
}
