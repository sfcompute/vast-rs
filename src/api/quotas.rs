use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{client::VastClient, Result};

// ---------------------------------------------------------------------------
// Models
// ---------------------------------------------------------------------------

/// A VAST quota — capacity and/or inode limit attached to a path or entity.
///
/// `#[serde(default)]` lets partial responses (tests, older software) deserialise
/// without error; unknown fields flow into `extra`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Quota {
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub id: u64,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub guid: String,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub name: String,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub title: String,
    /// Self-link URL.
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub url: String,
    /// Filesystem path this quota is attached to.
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub path: String,
    /// Quota state, e.g. `"OK"` or `"EXCEEDED"`.
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub state: String,
    /// Human-readable state, e.g. `"OK"`.
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub pretty_state: String,

    // ---- Grace period -------------------------------------------------------
    /// Grace period for soft-limit overruns (null = none).
    #[serde(default)]
    pub grace_period: Option<Value>,
    #[serde(default)]
    pub pretty_grace_period: Option<Value>,
    #[serde(default)]
    pub pretty_grace_period_expiration: Option<Value>,
    /// Seconds until writes are blocked (null = no block pending).
    #[serde(default)]
    pub time_to_block: Option<Value>,

    // ---- Capacity limits ----------------------------------------------------
    /// Soft capacity limit in bytes (`None` if unset).
    #[serde(default)]
    pub soft_limit: Option<u64>,
    /// Hard capacity limit in bytes (`None` if unset — unlimited).
    #[serde(default)]
    pub hard_limit: Option<u64>,

    // ---- Inode limits -------------------------------------------------------
    /// Soft inode limit (`None` if unset).
    #[serde(default)]
    pub soft_limit_inodes: Option<u64>,
    /// Hard inode limit (`None` if unset).
    #[serde(default)]
    pub hard_limit_inodes: Option<u64>,

    // ---- Current usage ------------------------------------------------------
    /// Number of inodes currently consumed.
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub used_inodes: u64,
    /// Sync state, e.g. `"SYNCHRONIZED"`.
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub sync_state: String,
    /// Bytes consumed (logical).
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub used_capacity: u64,
    /// Bytes consumed (effective / after compression + deduplication).
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub used_effective_capacity: u64,
    /// Convenience field: `used_capacity` expressed in TiB.
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub used_capacity_tb: f64,
    /// Convenience field: `used_effective_capacity` expressed in TiB.
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub used_effective_capacity_tb: f64,
    /// Capacity consumed against the hard limit (0 if no limit).
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub used_limited_capacity: u64,
    /// Percentage of inode limit consumed (null if no limit).
    #[serde(default)]
    pub percent_inodes: Option<Value>,
    /// Percentage of capacity limit consumed (null if no limit).
    #[serde(default)]
    pub percent_capacity: Option<Value>,

    // ---- User / group sub-quotas --------------------------------------------
    /// Default per-user quota policy (null if none).
    #[serde(default)]
    pub default_user_quota: Option<Value>,
    /// Default per-group quota policy (null if none).
    #[serde(default)]
    pub default_group_quota: Option<Value>,
    /// Number of users currently over their soft limit.
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub num_exceeded_users: u64,
    /// Number of users currently blocked by their hard limit.
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub num_blocked_users: u64,
    /// Timestamp of last per-user quota stats update (null if never).
    #[serde(default)]
    pub last_user_quotas_update: Option<Value>,

    // ---- Notifications ------------------------------------------------------
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub enable_alarms: bool,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub enable_email_providers: bool,
    /// Default email for quota notifications (null if none).
    #[serde(default)]
    pub default_email: Option<String>,

    // ---- Association --------------------------------------------------------
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub cluster: String,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub cluster_id: u64,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub tenant_id: u64,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub tenant_name: String,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub internal: bool,
    /// System / hardware component ID this quota is attached to.
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub system_id: u64,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub is_user_quota: bool,

    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

/// Body for `POST /api/quotas/`.
#[derive(Debug, Serialize)]
pub struct CreateQuota {
    pub name: String,
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hard_limit: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub soft_limit: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hard_limit_inodes: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub soft_limit_inodes: Option<u64>,
}

/// Body for `PATCH /api/quotas/{id}/`.
#[derive(Debug, Default, Serialize)]
pub struct UpdateQuota {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hard_limit: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub soft_limit: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hard_limit_inodes: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub soft_limit_inodes: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enable_alarms: Option<bool>,
}

// ---------------------------------------------------------------------------
// API handle
// ---------------------------------------------------------------------------

/// Scoped access to the `/api/quotas/` resource.
pub struct QuotasApi<'c> {
    client: &'c VastClient,
}

impl<'c> QuotasApi<'c> {
    pub(crate) fn new(client: &'c VastClient) -> Self {
        Self { client }
    }

    /// List all quotas.
    ///
    /// `GET /api/quotas/`
    pub async fn list(&self) -> Result<Vec<Quota>> {
        self.client.get("quotas/").await
    }

    /// Get a single quota by ID.
    ///
    /// `GET /api/quotas/{id}/`
    pub async fn get(&self, id: u64) -> Result<Quota> {
        self.client.get(&format!("quotas/{id}/")).await
    }

    /// Create a new quota.
    ///
    /// `POST /api/quotas/`
    pub async fn create(&self, body: &CreateQuota) -> Result<Quota> {
        self.client.post("quotas/", body).await
    }

    /// Update an existing quota.
    ///
    /// `PATCH /api/quotas/{id}/`
    pub async fn update(&self, id: u64, body: &UpdateQuota) -> Result<Quota> {
        self.client.patch(&format!("quotas/{id}/"), body).await
    }

    /// Delete a quota by ID.
    ///
    /// `DELETE /api/quotas/{id}/`
    pub async fn delete(&self, id: u64) -> Result<()> {
        self.client.delete(&format!("quotas/{id}/")).await
    }
}
