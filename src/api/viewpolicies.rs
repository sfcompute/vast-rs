use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{client::VastClient, Result};

// ---------------------------------------------------------------------------
// Models
// ---------------------------------------------------------------------------

/// A VAST view policy — controls NFS/SMB protocol behaviour and permissions.
///
/// `#[serde(default)]` lets partial responses (tests, older software) deserialise
/// without error; unknown fields flow into `extra`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ViewPolicy {
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
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub cluster: String,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub cluster_id: u64,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub tenant_id: u64,

    // ---- Access time --------------------------------------------------------
    /// Access-time update frequency (null = disabled).
    #[serde(default)]
    pub atime_frequency: Option<Value>,
    #[serde(default)]
    pub pretty_atime_frequency: Option<Value>,

    // ---- SMB mode bits (octal as decimal, e.g. 0o644 = 420) ----------------
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub smb_file_mode: u32,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub smb_directory_mode: u32,
    /// Human-readable octal string, e.g. `"644"`.
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub smb_file_mode_padded: String,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub smb_directory_mode_padded: String,

    // ---- Leases -------------------------------------------------------------
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub disable_read_lease: bool,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub disable_write_lease: bool,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub disable_handle_lease: bool,

    // ---- Auth / identity ----------------------------------------------------
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub use_auth_provider: bool,
    /// Authentication source, e.g. `"RPC"`, `"PROVIDERS"`.
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub auth_source: String,
    /// Human-readable auth source label.
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub pretty_auth_source: String,

    // ---- NFS options --------------------------------------------------------
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub nfs_return_open_permissions: bool,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub nfs_case_insensitive: bool,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub nfs_posix_acl: bool,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub nfs_enforce_tls: bool,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub nfs_enforce_tls_relaxed: bool,
    /// NFS minimum protection level, e.g. `"SYSTEM"`.
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub nfs_minimal_protection_level: String,

    // ---- NFS squash lists (IP / CIDR strings) --------------------------------
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub nfs_no_squash: Vec<String>,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub nfs_root_squash: Vec<String>,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub nfs_all_squash: Vec<String>,

    // ---- Access control lists -----------------------------------------------
    /// NFS read-write export list.
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub read_write: Vec<String>,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub read_only: Vec<String>,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub nfs_read_write: Vec<String>,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub nfs_read_only: Vec<String>,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub smb_read_write: Vec<String>,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub smb_read_only: Vec<String>,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub s3_read_write: Vec<String>,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub s3_read_only: Vec<String>,
    /// Principals with trash-access permission.
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub trash_access: Vec<String>,
    /// SMB Read share permission.
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub read: Vec<String>,
    /// SMB Change share permission.
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub change: Vec<String>,
    /// SMB Full Control share permission.
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub full: Vec<String>,

    // ---- GID / file-system flavour ------------------------------------------
    /// GID inheritance mode, e.g. `"LINUX"`.
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub gid_inheritance: String,
    /// Policy flavour, e.g. `"NFS"`, `"S3_NATIVE"`.
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub flavor: String,
    /// Path-length semantics, e.g. `"LCD"`, `"NPL"`.
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub path_length: String,
    /// Access flavour, e.g. `"ALL"`.
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub access_flavor: String,
    /// Allowed characters policy, e.g. `"LCD"`, `"NPL"`.
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub allowed_characters: String,

    // ---- Misc flags ---------------------------------------------------------
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub use_32bit_fileid: bool,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub expose_id_in_fsid: bool,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub apple_sid: bool,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub inherit_parent_mode_bits: bool,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub internal: bool,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub smb_is_ca: bool,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub is_s3_default_policy: bool,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub s3_special_chars_support: bool,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub s3_flavor_allow_free_listing: bool,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub s3_flavor_detect_full_pathname: bool,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub enable_snapshot_lookup: bool,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub enable_listing_of_snapshot_dir: bool,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub enable_access_to_snapshot_dir_in_subdirs: bool,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub enable_visibility_of_snapshot_dir: bool,

    // ---- S3 visibility ------------------------------------------------------
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub s3_visibility: Vec<Value>,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub s3_visibility_groups: Vec<Value>,

    // ---- VIP pools ----------------------------------------------------------
    /// VIP pools that serve this policy.
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub vip_pools: Vec<Value>,

    // ---- Protocols / audit --------------------------------------------------
    /// Protocols enabled for this policy (subset of view protocols).
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub protocols: Vec<String>,
    /// Per-protocol audit configuration.
    #[serde(default)]
    pub protocols_audit: Value,
    /// Remote mapping configuration.
    #[serde(default)]
    pub remote_mapping: Value,

    // ---- Sync / counts ------------------------------------------------------
    /// Sync state, e.g. `"SYNCED"`.
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub sync: String,
    /// ISO 8601 timestamp of last sync.
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub sync_time: String,
    /// ISO 8601 creation timestamp.
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub created: String,
    /// Number of views currently using this policy.
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub count_views: u64,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub tenant_name: String,

    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

/// Body for `POST /api/viewpolicies/`.
#[derive(Debug, Serialize)]
pub struct CreateViewPolicy {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth_source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flavor: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub smb_file_mode: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub smb_directory_mode: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nfs_posix_acl: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nfs_root_squash: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nfs_all_squash: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nfs_no_squash: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub read_write: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub read_only: Option<Vec<String>>,
}

/// Body for `PATCH /api/viewpolicies/{id}/`.
#[derive(Debug, Default, Serialize)]
pub struct UpdateViewPolicy {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub smb_file_mode: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub smb_directory_mode: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nfs_posix_acl: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nfs_root_squash: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nfs_all_squash: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nfs_no_squash: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub read_write: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub read_only: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disable_read_lease: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disable_write_lease: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disable_handle_lease: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth_source: Option<String>,
}

// ---------------------------------------------------------------------------
// API handle
// ---------------------------------------------------------------------------

/// Scoped access to the `/api/viewpolicies/` resource.
pub struct ViewPoliciesApi<'c> {
    client: &'c VastClient,
}

impl<'c> ViewPoliciesApi<'c> {
    pub(crate) fn new(client: &'c VastClient) -> Self {
        Self { client }
    }

    /// List all view policies.
    ///
    /// `GET /api/viewpolicies/`
    pub async fn list(&self) -> Result<Vec<ViewPolicy>> {
        self.client.get("viewpolicies/").await
    }

    /// Get a single view policy by ID.
    ///
    /// `GET /api/viewpolicies/{id}/`
    pub async fn get(&self, id: u64) -> Result<ViewPolicy> {
        self.client.get(&format!("viewpolicies/{id}/")).await
    }

    /// Create a new view policy.
    ///
    /// `POST /api/viewpolicies/`
    pub async fn create(&self, body: &CreateViewPolicy) -> Result<ViewPolicy> {
        self.client.post("viewpolicies/", body).await
    }

    /// Update an existing view policy.
    ///
    /// `PATCH /api/viewpolicies/{id}/`
    pub async fn update(&self, id: u64, body: &UpdateViewPolicy) -> Result<ViewPolicy> {
        self.client.patch(&format!("viewpolicies/{id}/"), body).await
    }

    /// Delete a view policy by ID.
    ///
    /// `DELETE /api/viewpolicies/{id}/`
    pub async fn delete(&self, id: u64) -> Result<()> {
        self.client.delete(&format!("viewpolicies/{id}/")).await
    }
}
