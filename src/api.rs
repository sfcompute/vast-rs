//! Typed handles for each VMS resource family.
//!
//! Every resource follows the same shape:
//!
//! * a `Resource` struct with the stable fields you'll most often want, plus
//!   `extra: Map<String, Value>` capturing every other field the VMS returns
//!   (forward-compatible with newer cluster software);
//! * a `CreateResource` body for `POST` (where supported);
//! * an `UpdateResource` body for `PATCH` (where supported);
//! * a `Resources<'c>` newtype implementing `list`/`get`/`create`/`update`/`delete`
//!   as appropriate, reached via `VastClient::<resource>()`.
//!
//! Fields you need that aren't on the slim model are always one
//! `resource.extra.get("field_name")` away.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::{client::VastClient, Result};

/// Catch-all for fields not modeled explicitly.
pub type Extra = Map<String, Value>;

/// Serde helper: treat a JSON `null` as `T::default()` rather than erroring.
/// Combined with the struct-level `#[serde(default)]` on every model, this
/// covers both *missing* and *null* values uniformly. Apply to any non-Option
/// field that the VMS might return as `null` (e.g. text fields populated only
/// after cluster bootstrap).
fn null_default<'de, D, T>(d: D) -> std::result::Result<T, D::Error>
where D: serde::Deserializer<'de>, T: Default + Deserialize<'de>,
{
    Ok(Option::<T>::deserialize(d)?.unwrap_or_default())
}

// ---------------------------------------------------------------------------
// Macro: emit list/get/delete (and create/update/delete_with_body when needed)
// ---------------------------------------------------------------------------

macro_rules! crud {
    // Read-only resources: list + get.
    (ro $Handle:ident, $Resource:ty, $path:expr) => {
        pub struct $Handle<'c>(pub(crate) &'c VastClient);
        impl<'c> $Handle<'c> {
            pub async fn list(&self) -> Result<Vec<$Resource>> { self.0.get($path).await }
            pub async fn get(&self, id: u64) -> Result<$Resource> {
                self.0.get(&format!("{}{id}/", $path)).await
            }
        }
    };
    // Full CRUD: list/get/create/update/delete.
    ($Handle:ident, $Resource:ty, $Create:ty, $Update:ty, $path:expr) => {
        pub struct $Handle<'c>(pub(crate) &'c VastClient);
        impl<'c> $Handle<'c> {
            pub async fn list(&self) -> Result<Vec<$Resource>> { self.0.get($path).await }
            pub async fn get(&self, id: u64) -> Result<$Resource> {
                self.0.get(&format!("{}{id}/", $path)).await
            }
            pub async fn create(&self, body: &$Create) -> Result<$Resource> {
                self.0.post($path, body).await
            }
            pub async fn update(&self, id: u64, body: &$Update) -> Result<$Resource> {
                self.0.patch(&format!("{}{id}/", $path), body).await
            }
            pub async fn delete(&self, id: u64) -> Result<()> {
                self.0.delete(&format!("{}{id}/", $path)).await
            }
        }
    };
    // Create-only (no update): used for snapshots-style resources that don't update.
    (cd $Handle:ident, $Resource:ty, $Create:ty, $path:expr) => {
        pub struct $Handle<'c>(pub(crate) &'c VastClient);
        impl<'c> $Handle<'c> {
            pub async fn list(&self) -> Result<Vec<$Resource>> { self.0.get($path).await }
            pub async fn get(&self, id: u64) -> Result<$Resource> {
                self.0.get(&format!("{}{id}/", $path)).await
            }
            pub async fn create(&self, body: &$Create) -> Result<$Resource> {
                self.0.post($path, body).await
            }
            pub async fn delete(&self, id: u64) -> Result<()> {
                self.0.delete(&format!("{}{id}/", $path)).await
            }
        }
    };
}

// ===========================================================================
// Clusters
// ===========================================================================

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Cluster {
    pub id: u64,
    #[serde(deserialize_with = "null_default")] pub guid: String,
    #[serde(deserialize_with = "null_default")] pub name: String,
    #[serde(deserialize_with = "null_default")] pub state: String,
    #[serde(deserialize_with = "null_default")] pub sw_version: String,
    pub enabled: bool,
    #[serde(flatten)]
    pub extra: Extra,
}

pub struct Clusters<'c>(pub(crate) &'c VastClient);
impl<'c> Clusters<'c> {
    pub async fn list(&self) -> Result<Vec<Cluster>> { self.0.get("clusters/").await }
    pub async fn get(&self, id: u64) -> Result<Cluster> {
        self.0.get(&format!("clusters/{id}/")).await
    }
    /// Permanently delete a filesystem directory from the VAST namespace.
    /// `DELETE /api/clusters/{cluster_id}/delete_folder/` — requires the
    /// "Trash Folder Access" cluster setting.
    pub async fn delete_folder(&self, cluster_id: u64, path: &str, tenant_id: Option<u64>) -> Result<()> {
        let mut body = serde_json::json!({ "path": path });
        if let Some(t) = tenant_id { body["tenant_id"] = serde_json::json!(t); }
        self.0.delete_with_body(&format!("clusters/{cluster_id}/delete_folder/"), &body).await
    }
}

// ===========================================================================
// Nodes
// ===========================================================================

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Node {
    pub id: u64,
    pub name: String,
    pub state: Option<String>,
    pub ip: Option<String>,
    pub cluster: Option<u64>,
    #[serde(flatten)]
    pub extra: Extra,
}

#[derive(Debug, Default, Serialize)]
pub struct ListNodesParams {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cluster_id: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state: Option<String>,
}

pub struct Nodes<'c>(pub(crate) &'c VastClient);
impl<'c> Nodes<'c> {
    pub async fn list(&self) -> Result<Vec<Node>> { self.0.get("nodes/").await }
    pub async fn list_with_params(&self, params: &ListNodesParams) -> Result<Vec<Node>> {
        self.0.get_with_query("nodes/", params).await
    }
    pub async fn get(&self, id: u64) -> Result<Node> {
        self.0.get(&format!("nodes/{id}/")).await
    }
}

// ===========================================================================
// Users
// ===========================================================================

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct User {
    pub id: u64,
    pub name: String,
    pub uid: Option<u64>,
    pub email: Option<String>,
    pub enabled: Option<bool>,
    pub is_admin: Option<bool>,
    #[serde(flatten)]
    pub extra: Extra,
}

#[derive(Debug, Serialize)]
pub struct CreateUser {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")] pub uid: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")] pub email: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")] pub enabled: Option<bool>,
}

#[derive(Debug, Default, Serialize)]
pub struct UpdateUser {
    #[serde(skip_serializing_if = "Option::is_none")] pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")] pub email: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")] pub enabled: Option<bool>,
}

crud!(Users, User, CreateUser, UpdateUser, "users/");

// ===========================================================================
// Volumes
// ===========================================================================

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Volume {
    pub id: u64,
    pub name: String,
    pub path: Option<String>,
    pub quota: Option<u64>,
    pub enabled: Option<bool>,
    #[serde(flatten)]
    pub extra: Extra,
}

#[derive(Debug, Serialize)]
pub struct CreateVolume {
    pub name: String,
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")] pub quota: Option<u64>,
}

#[derive(Debug, Default, Serialize)]
pub struct UpdateVolume {
    #[serde(skip_serializing_if = "Option::is_none")] pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")] pub quota: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")] pub enabled: Option<bool>,
}

#[derive(Debug, Default, Serialize)]
pub struct ListVolumesParams {
    #[serde(skip_serializing_if = "Option::is_none")] pub path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")] pub is_snapshot: Option<bool>,
}

pub struct Volumes<'c>(pub(crate) &'c VastClient);
impl<'c> Volumes<'c> {
    pub async fn list(&self) -> Result<Vec<Volume>> { self.0.get("volumes/").await }
    pub async fn list_with_params(&self, p: &ListVolumesParams) -> Result<Vec<Volume>> {
        self.0.get_with_query("volumes/", p).await
    }
    pub async fn get(&self, id: u64) -> Result<Volume> {
        self.0.get(&format!("volumes/{id}/")).await
    }
    pub async fn create(&self, body: &CreateVolume) -> Result<Volume> {
        self.0.post("volumes/", body).await
    }
    pub async fn update(&self, id: u64, body: &UpdateVolume) -> Result<Volume> {
        self.0.patch(&format!("volumes/{id}/"), body).await
    }
    pub async fn delete(&self, id: u64) -> Result<()> {
        self.0.delete(&format!("volumes/{id}/")).await
    }
}

// ===========================================================================
// Views
// ===========================================================================

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct View {
    pub id: u64,
    pub guid: String,
    pub name: String,
    pub path: String,
    pub policy_id: u64,
    pub bucket: String,
    pub protocols: Vec<String>,
    pub tenant_id: u64,
    pub enabled: bool,
    #[serde(flatten)]
    pub extra: Extra,
}

#[derive(Debug, Serialize)]
pub struct CreateView {
    pub name: String,
    pub path: String,
    pub policy_id: u64,
    pub protocols: Vec<String>,
    /// If `true`, create the backing directory and any missing parents.
    #[serde(skip_serializing_if = "Option::is_none")] pub create_dir: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")] pub alias: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")] pub bucket: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")] pub allow_anonymous_access: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")] pub s3_versioning: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")] pub s3_locks: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")] pub s3_locks_retention_mode: Option<String>,
}

#[derive(Debug, Default, Serialize)]
pub struct UpdateView {
    #[serde(skip_serializing_if = "Option::is_none")] pub policy_id: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")] pub protocols: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")] pub alias: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")] pub allow_anonymous_access: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")] pub s3_versioning: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")] pub enabled: Option<bool>,
}

crud!(Views, View, CreateView, UpdateView, "views/");

// ===========================================================================
// View policies
// ===========================================================================

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ViewPolicy {
    pub id: u64,
    pub guid: String,
    pub name: String,
    pub flavor: String,
    pub auth_source: String,
    #[serde(flatten)]
    pub extra: Extra,
}

#[derive(Debug, Serialize)]
pub struct CreateViewPolicy {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")] pub auth_source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")] pub flavor: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")] pub smb_file_mode: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")] pub smb_directory_mode: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")] pub nfs_posix_acl: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")] pub nfs_root_squash: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")] pub nfs_all_squash: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")] pub nfs_no_squash: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")] pub read_write: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")] pub read_only: Option<Vec<String>>,
}

#[derive(Debug, Default, Serialize)]
pub struct UpdateViewPolicy {
    #[serde(skip_serializing_if = "Option::is_none")] pub smb_file_mode: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")] pub smb_directory_mode: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")] pub nfs_posix_acl: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")] pub nfs_root_squash: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")] pub nfs_all_squash: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")] pub nfs_no_squash: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")] pub read_write: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")] pub read_only: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")] pub auth_source: Option<String>,
}

crud!(ViewPolicies, ViewPolicy, CreateViewPolicy, UpdateViewPolicy, "viewpolicies/");

// ===========================================================================
// Quotas
// ===========================================================================

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Quota {
    pub id: u64,
    pub guid: String,
    pub name: String,
    pub path: String,
    pub state: String,
    pub hard_limit: Option<u64>,
    pub soft_limit: Option<u64>,
    pub used_capacity: u64,
    #[serde(flatten)]
    pub extra: Extra,
}

#[derive(Debug, Serialize)]
pub struct CreateQuota {
    pub name: String,
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")] pub hard_limit: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")] pub soft_limit: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")] pub hard_limit_inodes: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")] pub soft_limit_inodes: Option<u64>,
}

#[derive(Debug, Default, Serialize)]
pub struct UpdateQuota {
    #[serde(skip_serializing_if = "Option::is_none")] pub hard_limit: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")] pub soft_limit: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")] pub hard_limit_inodes: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")] pub soft_limit_inodes: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")] pub enable_alarms: Option<bool>,
}

crud!(Quotas, Quota, CreateQuota, UpdateQuota, "quotas/");

// ===========================================================================
// VIP pools
// ===========================================================================

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct VipPool {
    pub id: u64,
    pub guid: String,
    pub name: String,
    pub start_ip: String,
    pub end_ip: String,
    pub active_cnode_ids: Vec<u64>,
    pub role: String,
    pub enabled: bool,
    #[serde(flatten)]
    pub extra: Extra,
}

#[derive(Debug, Serialize)]
pub struct CreateVipPool {
    pub name: String,
    pub start_ip: String,
    pub end_ip: String,
    pub gw_ip: String,
    pub subnet_cidr: u32,
    #[serde(skip_serializing_if = "Option::is_none")] pub vlan: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")] pub role: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")] pub cnode_ids: Option<Vec<u64>>,
    #[serde(skip_serializing_if = "Option::is_none")] pub domain_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")] pub tenant_id: Option<u64>,
}

#[derive(Debug, Default, Serialize)]
pub struct UpdateVipPool {
    #[serde(skip_serializing_if = "Option::is_none")] pub start_ip: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")] pub end_ip: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")] pub gw_ip: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")] pub subnet_cidr: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")] pub vlan: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")] pub cnode_ids: Option<Vec<u64>>,
    #[serde(skip_serializing_if = "Option::is_none")] pub domain_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")] pub enabled: Option<bool>,
}

crud!(VipPools, VipPool, CreateVipPool, UpdateVipPool, "vippools/");

// ===========================================================================
// Tenants
// ===========================================================================

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Tenant {
    pub id: u64,
    pub guid: String,
    pub name: String,
    pub is_default: bool,
    pub enabled: bool,
    pub encryption_crn: Option<String>,
    #[serde(flatten)]
    pub extra: Extra,
}

#[derive(Debug, Serialize)]
pub struct CreateTenant {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")] pub vms_root_no_tenant_access: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")] pub s3_root_no_tenant_access: Option<bool>,
}

#[derive(Debug, Default, Serialize)]
pub struct UpdateTenant {
    #[serde(skip_serializing_if = "Option::is_none")] pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")] pub vms_root_no_tenant_access: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")] pub s3_root_no_tenant_access: Option<bool>,
}

crud!(Tenants, Tenant, CreateTenant, UpdateTenant, "tenants/");

// ===========================================================================
// Snapshots
// ===========================================================================

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Snapshot {
    pub id: u64,
    pub guid: String,
    pub name: String,
    pub path: String,
    #[serde(flatten)]
    pub extra: Extra,
}

#[derive(Debug, Serialize)]
pub struct CreateSnapshot {
    pub name: String,
    pub path: String,
}

#[derive(Debug, Default, Serialize)]
pub struct UpdateSnapshot {
    #[serde(skip_serializing_if = "Option::is_none")] pub name: Option<String>,
}

crud!(Snapshots, Snapshot, CreateSnapshot, UpdateSnapshot, "snapshots/");

// ===========================================================================
// Protection policies
// ===========================================================================

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ProtectionPolicy {
    pub id: u64,
    pub guid: String,
    pub name: String,
    #[serde(flatten)]
    pub extra: Extra,
}

#[derive(Debug, Serialize)]
pub struct CreateProtectionPolicy { pub name: String }

#[derive(Debug, Default, Serialize)]
pub struct UpdateProtectionPolicy {
    #[serde(skip_serializing_if = "Option::is_none")] pub name: Option<String>,
}

crud!(ProtectionPolicies, ProtectionPolicy, CreateProtectionPolicy, UpdateProtectionPolicy, "protectionpolicies/");

// ===========================================================================
// Folders
// ===========================================================================

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Folder {
    pub id: u64,
    pub guid: String,
    pub name: String,
    pub path: String,
    #[serde(flatten)]
    pub extra: Extra,
}

#[derive(Debug, Serialize)]
pub struct CreateFolder {
    pub path: String,
    /// Create intermediate parent directories as needed (`mkdir -p`).
    #[serde(skip_serializing_if = "Option::is_none")] pub create_dirs: Option<bool>,
}

crud!(cd Folders, Folder, CreateFolder, "folders/");
