//! Authentication strategy for the VMS API.
//!
//! [`Auth`] is an internal detail of [`crate::VastClient`] — construct it via
//! the builder ([`VastClient::builder().token(..)`] /
//! [`.credentials(..).tenant(..)`]) rather than directly.

use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};
use url::Url;

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
            return Some(Auth::Token(SecretString::new(tok)));
        }
        let username = std::env::var("VMS_USER").ok()?;
        let password = SecretString::new(std::env::var("VMS_PASSWORD").ok()?);
        let tenant = std::env::var("VMS_TENANT").ok();
        Some(Auth::Password { username, password, tenant })
    }

    /// Return a valid bearer token, fetching a JWT for password auth if needed.
    ///
    /// The token is wrapped in [`SecretString`] so it can't accidentally
    /// flow into `Debug` or tracing output; only call `.expose_secret()`
    /// at the immediate HTTP call site.
    pub(crate) async fn bearer_token(&self, http: &reqwest::Client, base: &Url) -> Result<SecretString> {
        match self {
            Auth::Token(t) => Ok(t.clone()),
            Auth::Password { username, password, tenant } => {
                // Cluster admins POST /api/token/; tenant admins POST /api/token/{name}.
                let url = base.join(&match tenant {
                    Some(t) => format!("token/{t}"),
                    None => "token/".into(),
                })?;
                let resp = http
                    .post(url)
                    .json(&TokenRequest { username, password: password.expose_secret() })
                    .send()
                    .await?;
                if !resp.status().is_success() {
                    let status = resp.status().as_u16();
                    let body = resp.text().await.unwrap_or_default();
                    let hint = if status == 401 && tenant.is_none() {
                        " (tenant-admin accounts must set .tenant(\"…\") or VMS_TENANT)"
                    } else { "" };
                    return Err(Error::Auth(format!("token request failed ({status}): {body}{hint}")));
                }
                Ok(SecretString::new(resp.json::<TokenResponse>().await?.access))
            }
        }
    }
}

#[derive(Serialize)]
struct TokenRequest<'a> { username: &'a str, password: &'a str }

#[derive(Deserialize)]
struct TokenResponse { access: String }
