use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{client::VastClient, Result};

// ---------------------------------------------------------------------------
// Models
// ---------------------------------------------------------------------------

/// A VAST tenant — a logical namespace with its own users, views, and quotas.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tenant {
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub id: u64,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub guid: String,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub name: String,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub title: String,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub url: String,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub cluster: String,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub cluster_id: u64,

    // ---- Access control -----------------------------------------------------
    /// Deny VMS root access to the tenant namespace.
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub vms_root_no_tenant_access: bool,
    /// Deny S3 root access to the tenant namespace.
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub s3_root_no_tenant_access: bool,

    // ---- Defaults -----------------------------------------------------------
    /// Whether this is the default tenant.
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub is_default: bool,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub enabled: bool,

    // ---- Encryption ---------------------------------------------------------
    #[serde(default)]
    pub encryption_crn: Option<String>,

    // ---- Sync ---------------------------------------------------------------
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub sync: String,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub sync_time: String,

    // ---- Counts -------------------------------------------------------------
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub views_count: u64,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub users_count: u64,

    // ---- Associations -------------------------------------------------------
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub ssd_pools: Vec<Value>,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub object_stores: Vec<Value>,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub leaders: Vec<Value>,

    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

/// Body for `POST /api/tenants/`.
#[derive(Debug, Serialize)]
pub struct CreateTenant {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vms_root_no_tenant_access: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub s3_root_no_tenant_access: Option<bool>,
}

/// Body for `PATCH /api/tenants/{id}/`.
#[derive(Debug, Default, Serialize)]
pub struct UpdateTenant {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vms_root_no_tenant_access: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub s3_root_no_tenant_access: Option<bool>,
}

// ---------------------------------------------------------------------------
// API handle
// ---------------------------------------------------------------------------

/// Scoped access to the `/api/tenants/` resource.
pub struct TenantsApi<'c> {
    client: &'c VastClient,
}

impl<'c> TenantsApi<'c> {
    pub(crate) fn new(client: &'c VastClient) -> Self {
        Self { client }
    }

    /// List all tenants.
    ///
    /// `GET /api/tenants/`
    pub async fn list(&self) -> Result<Vec<Tenant>> {
        self.client.get("tenants/").await
    }

    /// Get a single tenant by ID.
    ///
    /// `GET /api/tenants/{id}/`
    pub async fn get(&self, id: u64) -> Result<Tenant> {
        self.client.get(&format!("tenants/{id}/")).await
    }

    /// Create a new tenant.
    ///
    /// `POST /api/tenants/`
    pub async fn create(&self, body: &CreateTenant) -> Result<Tenant> {
        self.client.post("tenants/", body).await
    }

    /// Update an existing tenant.
    ///
    /// `PATCH /api/tenants/{id}/`
    pub async fn update(&self, id: u64, body: &UpdateTenant) -> Result<Tenant> {
        self.client.patch(&format!("tenants/{id}/"), body).await
    }

    /// Delete a tenant by ID.
    ///
    /// `DELETE /api/tenants/{id}/`
    pub async fn delete(&self, id: u64) -> Result<()> {
        self.client.delete(&format!("tenants/{id}/")).await
    }
}
