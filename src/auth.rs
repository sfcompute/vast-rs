//! Authentication strategy for the VMS API.
//!
//! [`Auth`] is an internal detail of [`crate::VastClient`] — construct it via
//! the builder ([`VastClient::builder().token(..)`] /
//! [`.credentials(..).tenant(..)`]) rather than directly.

use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};
use url::Url;

use crate::client::Retry;
use crate::error::{Error, Result};

/// How to authenticate with the VMS. Secrets are wrapped in [`SecretString`]
/// so they never leak through `Debug` or tracing.
#[derive(Debug, Clone)]
pub(crate) enum Auth {
    /// Long-lived API token; sent as `Authorization: Bearer …` on every request.
    Token(SecretString),
    /// Username + password exchanged for a JWT on first use.
    /// `tenant` must be set for tenant-admin accounts (cluster admins leave it `None`).
    Password {
        username: String,
        password: SecretString,
        tenant: Option<String>,
    },
}

impl Auth {
    /// `true` when the cached bearer token could plausibly become valid
    /// again by re-running the credential exchange — i.e. for password
    /// auth, whose JWT expires. Static API tokens never expire mid-process
    /// so retrying after a 401 is pointless.
    pub(crate) fn is_refreshable(&self) -> bool {
        matches!(self, Auth::Password { .. })
    }

    /// Build from environment variables: `VMS_TOKEN`, or `VMS_USER` + `VMS_PASSWORD`
    /// (+ optional `VMS_TENANT`).
    pub(crate) fn from_env() -> Option<Self> {
        if let Ok(tok) = std::env::var("VMS_TOKEN") {
            return Some(Auth::Token(SecretString::from(tok)));
        }
        let username = std::env::var("VMS_USER").ok()?;
        let password = SecretString::from(std::env::var("VMS_PASSWORD").ok()?);
        let tenant = std::env::var("VMS_TENANT").ok();
        Some(Auth::Password {
            username,
            password,
            tenant,
        })
    }

    /// Return a valid bearer token, fetching a JWT for password auth if needed.
    ///
    /// The token is wrapped in [`SecretString`] so it can't accidentally
    /// flow into `Debug` or tracing output; only call `.expose_secret()`
    /// at the immediate HTTP call site.
    ///
    /// Transient exchange failures are retried per `retry`. Minting a JWT
    /// has no side effect beyond issuing a new one, so the request is safe
    /// to repeat — but only for *transient* failures. Rejected credentials
    /// are terminal and should not be hammered.
    pub(crate) async fn bearer_token(
        &self,
        http: &reqwest::Client,
        base: &Url,
        retry: Retry,
    ) -> Result<SecretString> {
        match self {
            Auth::Token(t) => Ok(t.clone()),
            Auth::Password {
                username,
                password,
                tenant,
            } => {
                // Cluster admins POST /api/token/; tenant admins POST /api/token/{name}.
                // Tenant names may legally contain characters (`/`, ` `,
                // `?`, `#`) that would otherwise break out of the path
                // segment, so use `path_segments_mut().push()` to do the
                // percent-encoding rather than `format!`.
                let url = match tenant {
                    Some(t) => {
                        let mut u = base.join("token/")?;
                        u.path_segments_mut()
                            .map_err(|()| Error::Auth("base URL has no path segments".into()))?
                            .pop_if_empty()
                            .push(t);
                        u
                    }
                    None => base.join("token/")?,
                };

                let mut attempt: u32 = 0;
                loop {
                    attempt += 1;
                    match exchange(http, &url, username, password, tenant.as_deref()).await {
                        Ok(token) => return Ok(token),
                        Err(f) if f.transient && attempt < retry.max_attempts() => {
                            let delay = retry.delay(attempt);
                            tracing::warn!(
                                attempt,
                                max_attempts = retry.max_attempts(),
                                retry_in_ms = delay.as_millis() as u64,
                                error = %f.error,
                                "credential exchange failed transiently; sleeping and retrying",
                            );
                            tokio::time::sleep(delay).await;
                        }
                        Err(f) => return Err(f.error),
                    }
                }
            }
        }
    }
}

/// A failed credential exchange, tagged with whether repeating it could
/// plausibly succeed.
struct ExchangeFailure {
    transient: bool,
    error: Error,
}

/// One `POST /api/token/` round-trip.
async fn exchange(
    http: &reqwest::Client,
    url: &Url,
    username: &str,
    password: &SecretString,
    tenant: Option<&str>,
) -> std::result::Result<SecretString, ExchangeFailure> {
    let resp = http
        .post(url.clone())
        .json(&TokenRequest {
            username,
            password: password.expose_secret(),
        })
        .send()
        .await
        // Connection refused / timeout / TLS reset — a VMS failover or
        // restart looks exactly like this and clears on its own.
        .map_err(|e| ExchangeFailure {
            transient: true,
            error: Error::Http(e),
        })?;

    let status = resp.status();
    if !status.is_success() {
        let code = status.as_u16();
        let body = resp.text().await.unwrap_or_default();
        let hint = if code == 401 && tenant.is_none() {
            " (tenant-admin accounts must set .tenant(\"…\") or VMS_TENANT)"
        } else {
            ""
        };
        return Err(ExchangeFailure {
            // 429 is the VMS throttling us — the case a fleet of replicas
            // sharing one service account is most likely to hit. 5xx is a
            // VMS-side blip. Everything else, notably 401/403 rejected
            // credentials, is a decision the VMS won't reverse on retry.
            transient: code == 429 || status.is_server_error(),
            error: Error::Auth(format!("token request failed ({code}): {body}{hint}")),
        });
    }

    resp.json::<TokenResponse>()
        .await
        .map(|r| SecretString::from(r.access))
        // Covers both a truncated body read and a shape we can't parse.
        // The former is transient; the latter will just fail again and
        // surface the same error once the budget is spent.
        .map_err(|e| ExchangeFailure {
            transient: true,
            error: Error::Http(e),
        })
}

#[derive(Serialize)]
struct TokenRequest<'a> {
    username: &'a str,
    password: &'a str,
}

#[derive(Deserialize)]
struct TokenResponse {
    access: String,
}
