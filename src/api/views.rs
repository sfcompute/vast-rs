use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{client::VastClient, Result};

// ---------------------------------------------------------------------------
// Models
// ---------------------------------------------------------------------------

/// A VAST view (an exported filesystem path, optionally with S3/NFS/SMB protocols).
///
/// `#[serde(default)]` lets partial responses (tests, older software) deserialise
/// without error; unknown fields flow into `extra`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct View {
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub id: u64,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub guid: String,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub name: String,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub title: String,
    /// Self-link URL, e.g. `"https://vms/api/views/1/"`.
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub url: String,
    /// Filesystem path this view exports.
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub path: String,
    /// NFS alias path (may be empty).
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub alias: String,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub cluster: String,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub cluster_id: u64,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub policy: String,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub policy_id: u64,
    /// Active protocols, e.g. `["NFS"]`, `["SMB"]`, `["S3"]`.
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub protocols: Vec<String>,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub allow_anonymous_access: bool,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub allow_s3_anonymous_access: bool,
    /// S3 bucket name (empty string if not an S3 view).
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub bucket: String,
    /// S3 principals allowed to create buckets.
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub bucket_creators: Vec<Value>,
    /// Groups allowed to create buckets.
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub bucket_creators_groups: Vec<Value>,
    /// S3 bucket owner (null if none set).
    #[serde(default)]
    pub bucket_owner: Option<Value>,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub internal: bool,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub logical_capacity: u64,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub physical_capacity: u64,
    /// NFS v3/v4 interop flag, e.g. `"BOTH_NFS3_AND_NFS4_INTEROP_DISABLED"`.
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub nfs_interop_flags: String,
    /// S3 object-lock retention mode, e.g. `"NONE"`.
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub s3_locks_retention_mode: String,
    /// File-level retention mode, e.g. `"NONE"`.
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub files_retention_mode: String,
    /// Maximum retention period duration string, e.g. `"0d"`.
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub max_retention_period: String,
    /// Minimum retention period duration string, e.g. `"0d"`.
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub min_retention_period: String,
    /// Auto-commit delay, e.g. `"0d"`.
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub auto_commit: String,
    /// Allow S3 list/head without auth (tenant setting).
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub s3_unverified_lookup: bool,
    /// S3 versioning enabled.
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub s3_versioning: bool,
    /// SMB share name (empty string if not an SMB view).
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub share: String,
    /// Sync state, e.g. `"SYNCED"`.
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub sync: String,
    /// ISO 8601 timestamp of last sync.
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub sync_time: String,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub tenant_id: u64,
    /// ISO 8601 creation timestamp.
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub created: String,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub is_remote: bool,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub select_for_live_monitoring: bool,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub tenant_name: String,

    /// Associated QoS policy object (null if none).
    #[serde(default)]
    pub qos_policy: Option<Value>,
    /// QoS policy ID (null if none).
    #[serde(default)]
    pub qos_policy_id: Option<u64>,

    /// In-progress bulk permission update state (null if idle).
    #[serde(default)]
    pub bulk_permission_update_state: Option<Value>,
    #[serde(default)]
    pub bulk_permission_update_progress: Option<Value>,

    /// Maximum ABE depth (null = unlimited).
    #[serde(default)]
    pub abe_max_depth: Option<Value>,
    /// ABE protocols list.
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub abe_protocols: Vec<Value>,

    /// ABAC tags attached to this view.
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub abac_tags: Vec<Value>,

    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub is_seamless: bool,

    /// S3 object lock enabled.
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub s3_locks: bool,
    /// S3 object lock default retention period (empty string = none).
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub s3_locks_retention_period: String,

    /// SMB share ACL configuration.
    #[serde(default)]
    pub share_acl: Value,

    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub enabled: bool,

    /// S3 object ownership rule, e.g. `"ObjectWriter"`.
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub s3_object_ownership_rule: String,
    /// Default share-level permission for others from tenant, e.g. `"FULL"`.
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub tenant_default_others_share_level_perm: String,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub is_indestructible_object_enabled: bool,
    /// Indestructible object hold duration in hours.
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub indestructible_object_duration: u64,

    /// S3 / NFS user-impersonation configuration.
    #[serde(default)]
    pub user_impersonation: Value,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub impersonation_username: String,

    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

/// Body for `POST /api/views/`.
#[derive(Debug, Serialize)]
pub struct CreateView {
    pub name: String,
    pub path: String,
    pub policy_id: u64,
    pub protocols: Vec<String>,
    /// If `true`, create the backing directory (and any missing parents)
    /// automatically.  Equivalent to `mkdir -p <path>` before attaching the
    /// export.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub create_dir: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alias: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bucket: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allow_anonymous_access: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub s3_versioning: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub s3_locks: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub s3_locks_retention_mode: Option<String>,
}

/// Body for `PATCH /api/views/{id}/`.
#[derive(Debug, Default, Serialize)]
pub struct UpdateView {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub policy_id: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub protocols: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alias: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allow_anonymous_access: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub s3_versioning: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
}

// ---------------------------------------------------------------------------
// API handle
// ---------------------------------------------------------------------------

/// Scoped access to the `/api/views/` resource.
pub struct ViewsApi<'c> {
    client: &'c VastClient,
}

impl<'c> ViewsApi<'c> {
    pub(crate) fn new(client: &'c VastClient) -> Self {
        Self { client }
    }

    /// List all views visible to this tenant.
    ///
    /// `GET /api/views/`
    pub async fn list(&self) -> Result<Vec<View>> {
        self.client.get("views/").await
    }

    /// Get a single view by ID.
    ///
    /// `GET /api/views/{id}/`
    pub async fn get(&self, id: u64) -> Result<View> {
        self.client.get(&format!("views/{id}/")).await
    }

    /// Create a new view.
    ///
    /// `POST /api/views/`
    pub async fn create(&self, body: &CreateView) -> Result<View> {
        self.client.post("views/", body).await
    }

    /// Update an existing view.
    ///
    /// `PATCH /api/views/{id}/`
    pub async fn update(&self, id: u64, body: &UpdateView) -> Result<View> {
        self.client.patch(&format!("views/{id}/"), body).await
    }

    /// Delete a view by ID.
    ///
    /// `DELETE /api/views/{id}/`
    pub async fn delete(&self, id: u64) -> Result<()> {
        self.client.delete(&format!("views/{id}/")).await
    }
}
