use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};

/// The authentication strategy to use when communicating with the VMS API.
///
/// Tokens are stored as [`SecretString`] so they never appear in `Debug` output
/// or tracing spans.
#[derive(Clone)]
pub enum Auth {
    /// A long-lived API token issued by the VMS.
    /// Sent as `Authorization: Bearer <token>` on every request.
    Token(SecretString),

    /// Username and password credentials.
    ///
    /// The client POSTs these to `POST /api/token/` on first use and caches the
    /// resulting JWT, refreshing it transparently when it expires.
    ///
    /// For **tenant admins**, `tenant` must be set to the tenant name, otherwise
    /// the VMS returns 401. Cluster admins leave `tenant` as `None`.
    UsernamePassword {
        username: String,
        password: SecretString,
        /// Tenant name — required for tenant-scoped (tenant admin) authentication.
        /// Leave `None` for cluster-level admin accounts.
        tenant: Option<String>,
    },
}

impl std::fmt::Debug for Auth {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Auth::Token(_) => f.debug_tuple("Auth::Token").field(&"[redacted]").finish(),
            Auth::UsernamePassword { username, tenant, .. } => f
                .debug_struct("Auth::UsernamePassword")
                .field("username", username)
                .field("password", &"[redacted]")
                .field("tenant", tenant)
                .finish(),
        }
    }
}

impl Auth {
    /// Create token-based authentication.
    pub fn token(token: impl Into<String>) -> Self {
        Auth::Token(SecretString::new(token.into()))
    }

    /// Create username/password authentication for a **cluster admin** account.
    pub fn password(username: impl Into<String>, password: impl Into<String>) -> Self {
        Auth::UsernamePassword {
            username: username.into(),
            password: SecretString::new(password.into()),
            tenant: None,
        }
    }

    /// Create username/password authentication for a **tenant admin** account.
    ///
    /// The `tenant` name is included in the token request body so the VMS
    /// can scope the resulting JWT to that tenant.
    pub fn tenant_password(
        username: impl Into<String>,
        password: impl Into<String>,
        tenant: impl Into<String>,
    ) -> Self {
        Auth::UsernamePassword {
            username: username.into(),
            password: SecretString::new(password.into()),
            tenant: Some(tenant.into()),
        }
    }

    /// Build from environment variables.
    ///
    /// | Variable         | Purpose                             |
    /// |------------------|-------------------------------------|
    /// | `VMS_TOKEN`      | API token (takes precedence)        |
    /// | `VMS_USER`       | Username for credential auth        |
    /// | `VMS_PASSWORD`   | Password for credential auth        |
    /// | `VMS_TENANT`     | Tenant name (for tenant admin auth) |
    pub fn from_env() -> Option<Self> {
        if let Ok(token) = std::env::var("VMS_TOKEN") {
            return Some(Auth::token(token));
        }
        let user = std::env::var("VMS_USER").ok()?;
        let pass = std::env::var("VMS_PASSWORD").ok()?;
        let tenant = std::env::var("VMS_TENANT").ok();
        Some(Auth::UsernamePassword {
            username: user,
            password: SecretString::new(pass),
            tenant,
        })
    }
}

// ---------------------------------------------------------------------------
// Token request / response
// ---------------------------------------------------------------------------

/// JSON body sent to `POST /api/token/` or `POST /api/token/{tenant_name}`.
#[derive(Debug, Serialize)]
pub(crate) struct TokenRequest<'a> {
    pub username: &'a str,
    pub password: &'a str,
}

/// JSON response from `POST /api/token/`.
#[derive(Debug, Deserialize)]
pub(crate) struct TokenResponse {
    pub access: String,
    #[allow(dead_code)]
    pub refresh: Option<String>,
}

impl Auth {
    /// Returns a valid bearer token, fetching one from the API if needed.
    pub(crate) async fn bearer_token(
        &self,
        http: &reqwest::Client,
        base_url: &url::Url,
    ) -> crate::Result<String> {
        match self {
            Auth::Token(t) => Ok(t.expose_secret().to_string()),

            Auth::UsernamePassword { username, password, tenant } => {
                // Cluster admins:  POST /api/token/
                // Tenant admins:   POST /api/token/{tenant_name}
                // These are distinct endpoints, not a body parameter difference.
                let url = match tenant.as_deref() {
                    Some(t) => base_url.join(&format!("token/{t}"))?,
                    None    => base_url.join("token/")?,
                };

                tracing::debug!(
                    %url,
                    user = %username,
                    tenant = ?tenant,
                    "Fetching JWT token",
                );

                let body = TokenRequest {
                    username: username.as_str(),
                    password: password.expose_secret(),
                };

                let resp = http
                    .post(url)
                    .json(&body)
                    .send()
                    .await
                    .map_err(crate::Error::Http)?;

                if !resp.status().is_success() {
                    let status = resp.status().as_u16();
                    let text = resp.text().await.unwrap_or_default();

                    // Give a targeted hint for the tenant admin 401 case.
                    let hint = if status == 401 && tenant.is_none() {
                        " (if this is a tenant admin account, set the tenant name \
                         with .tenant(\"...\") or the VMS_TENANT environment variable)"
                    } else {
                        ""
                    };

                    return Err(crate::Error::Auth(format!(
                        "token request failed ({status}): {text}{hint}"
                    )));
                }

                let token_resp: TokenResponse =
                    resp.json().await.map_err(crate::Error::Http)?;
                Ok(token_resp.access)
            }
        }
    }
}
