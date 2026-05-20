use crate::auth::Auth;
use crate::error::{Error, Result};
use url::Url;

/// Default VMS API base path.
const DEFAULT_BASE_PATH: &str = "/api/";

/// Configuration for a [`VastClient`](crate::VastClient).
///
/// Build one via [`ClientConfig::builder()`] or load it from the environment
/// with [`ClientConfig::from_env()`].
#[derive(Debug, Clone)]
pub struct ClientConfig {
    /// Base URL of the VMS API, e.g. `https://vms.example.com/api/`.
    pub(crate) base_url: Url,

    /// Authentication strategy.
    pub(crate) auth: Auth,

    /// Accept invalid / self-signed TLS certificates.
    ///
    /// **Warning:** this disables certificate verification and should only be
    /// used in development or test environments.
    pub(crate) danger_accept_invalid_certs: bool,

    /// Request timeout in seconds (default: 30).
    pub(crate) timeout_secs: u64,
}

impl ClientConfig {
    /// Start building a new configuration.
    pub fn builder() -> ClientConfigBuilder {
        ClientConfigBuilder::default()
    }

    /// Load configuration from environment variables.
    ///
    /// | Variable       | Required?                              |
    /// |----------------|----------------------------------------|
    /// | `VMS_ADDRESS`  | Always                                 |
    /// | `VMS_TOKEN`    | One of TOKEN or USER+PASSWORD          |
    /// | `VMS_USER`     | One of TOKEN or USER+PASSWORD          |
    /// | `VMS_PASSWORD` | One of TOKEN or USER+PASSWORD          |
    /// | `VMS_TENANT`   | Only for tenant admin accounts         |
    pub fn from_env() -> Result<Self> {
        let address = std::env::var("VMS_ADDRESS")
            .map_err(|_| Error::Config("`VMS_ADDRESS` environment variable is not set".into()))?;

        let auth = Auth::from_env().ok_or_else(|| {
            Error::Config(
                "set `VMS_TOKEN` or both `VMS_USER` and `VMS_PASSWORD`".into(),
            )
        })?;

        ClientConfig::builder().address(address).auth(auth).into_config()
    }
}

// ---------------------------------------------------------------------------
// Builder
// ---------------------------------------------------------------------------

/// Builder for [`ClientConfig`] / [`VastClient`].
///
/// Obtain one via [`VastClient::builder()`] or [`ClientConfig::builder()`].
#[derive(Debug, Default)]
pub struct ClientConfigBuilder {
    address: Option<String>,
    auth: Option<Auth>,
    tenant: Option<String>,
    danger_accept_invalid_certs: bool,
    timeout_secs: Option<u64>,
}

impl ClientConfigBuilder {
    /// Set the VMS hostname or IP address (required).
    ///
    /// The scheme (`https://`) is added automatically if omitted. The API base
    /// path (`/api/`) is always appended.
    pub fn address(mut self, address: impl Into<String>) -> Self {
        self.address = Some(address.into());
        self
    }

    /// Use an API token for authentication.
    pub fn token(mut self, token: impl Into<String>) -> Self {
        self.auth = Some(Auth::token(token));
        self
    }

    /// Use username/password credentials.
    ///
    /// For **tenant admin** accounts also call `.tenant("tenant-name")`.
    /// Cluster admins can omit it.
    pub fn credentials(mut self, username: impl Into<String>, password: impl Into<String>) -> Self {
        self.auth = Some(Auth::password(username, password));
        self
    }

    /// Scope credential auth to a specific tenant.
    ///
    /// **Required for tenant admin accounts.** The tenant name is included in
    /// the `POST /api/token/` request body; without it the VMS returns 401 for
    /// tenant-level users.
    ///
    /// ```rust,no_run
    /// use vast_rs::VastClient;
    ///
    /// # fn main() -> vast_rs::Result<()> {
    /// let client = VastClient::builder()
    ///     .address("vms.example.com")
    ///     .credentials("alice", "secret")
    ///     .tenant("acme")
    ///     .build()?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn tenant(mut self, tenant: impl Into<String>) -> Self {
        self.tenant = Some(tenant.into());
        self
    }

    /// Set an explicit [`Auth`] strategy (advanced use).
    pub fn auth(mut self, auth: Auth) -> Self {
        self.auth = Some(auth);
        self
    }

    /// Accept self-signed or otherwise invalid TLS certificates.
    ///
    /// **This disables certificate verification.** Use only in development.
    pub fn danger_accept_invalid_certs(mut self, yes: bool) -> Self {
        self.danger_accept_invalid_certs = yes;
        self
    }

    /// Set the per-request timeout in seconds (default: 30).
    pub fn timeout_secs(mut self, secs: u64) -> Self {
        self.timeout_secs = Some(secs);
        self
    }

    /// Consume the builder and produce a [`ClientConfig`].
    pub(crate) fn into_config(self) -> Result<ClientConfig> {
        let raw_address = self
            .address
            .ok_or_else(|| Error::Config("address is required".into()))?;

        // If .tenant() was set, inject it into a UsernamePassword auth.
        let auth = match self.auth {
            Some(Auth::UsernamePassword { username, password, tenant: existing }) => {
                let effective_tenant = self.tenant.or(existing);
                Auth::UsernamePassword { username, password, tenant: effective_tenant }
            }
            Some(other) => {
                if self.tenant.is_some() {
                    tracing::warn!(
                        "`.tenant()` has no effect when using token-based authentication"
                    );
                }
                other
            }
            None => return Err(Error::Config(
                "authentication is required — call .token() or .credentials()".into(),
            )),
        };

        // Normalise address: add scheme if missing, then append base path.
        let with_scheme =
            if raw_address.starts_with("http://") || raw_address.starts_with("https://") {
                raw_address
            } else {
                format!("https://{raw_address}")
            };

        let mut base = Url::parse(&with_scheme)?;

        // Ensure the path ends with /api/.
        if !base.path().ends_with(DEFAULT_BASE_PATH) {
            let host_root = format!(
                "{}://{}{}",
                base.scheme(),
                base.host_str().unwrap_or(""),
                base.port().map(|p| format!(":{p}")).unwrap_or_default()
            );
            base = Url::parse(&format!("{host_root}{DEFAULT_BASE_PATH}"))?;
        }

        Ok(ClientConfig {
            base_url: base,
            auth,
            danger_accept_invalid_certs: self.danger_accept_invalid_certs,
            timeout_secs: self.timeout_secs.unwrap_or(30),
        })
    }
}
