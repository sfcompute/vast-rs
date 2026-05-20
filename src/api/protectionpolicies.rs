use serde::{Deserialize, Serialize};

use crate::{client::VastClient, Result};

// ---------------------------------------------------------------------------
// Models
// ---------------------------------------------------------------------------

/// A VAST protection policy — defines replication/snapshot schedules.
///
/// The exact field set depends on the VMS version; fields beyond the common
/// ones are captured in `extra` for forward-compatibility.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProtectionPolicy {
    pub id: u64,
    pub guid: String,
    pub name: String,
    pub title: String,
    pub cluster: String,
    pub cluster_id: u64,
    pub tenant_id: u64,

    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

/// Body for `POST /api/protectionpolicies/`.
#[derive(Debug, Serialize)]
pub struct CreateProtectionPolicy {
    pub name: String,
}

/// Body for `PATCH /api/protectionpolicies/{id}/`.
#[derive(Debug, Default, Serialize)]
pub struct UpdateProtectionPolicy {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

// ---------------------------------------------------------------------------
// API handle
// ---------------------------------------------------------------------------

/// Scoped access to the `/api/protectionpolicies/` resource.
pub struct ProtectionPoliciesApi<'c> {
    client: &'c VastClient,
}

impl<'c> ProtectionPoliciesApi<'c> {
    pub(crate) fn new(client: &'c VastClient) -> Self {
        Self { client }
    }

    /// List all protection policies.
    ///
    /// `GET /api/protectionpolicies/`
    pub async fn list(&self) -> Result<Vec<ProtectionPolicy>> {
        self.client.get("protectionpolicies/").await
    }

    /// Get a single protection policy by ID.
    ///
    /// `GET /api/protectionpolicies/{id}/`
    pub async fn get(&self, id: u64) -> Result<ProtectionPolicy> {
        self.client.get(&format!("protectionpolicies/{id}/")).await
    }

    /// Create a new protection policy.
    ///
    /// `POST /api/protectionpolicies/`
    pub async fn create(&self, body: &CreateProtectionPolicy) -> Result<ProtectionPolicy> {
        self.client.post("protectionpolicies/", body).await
    }

    /// Update an existing protection policy.
    ///
    /// `PATCH /api/protectionpolicies/{id}/`
    pub async fn update(&self, id: u64, body: &UpdateProtectionPolicy) -> Result<ProtectionPolicy> {
        self.client
            .patch(&format!("protectionpolicies/{id}/"), body)
            .await
    }

    /// Delete a protection policy by ID.
    ///
    /// `DELETE /api/protectionpolicies/{id}/`
    pub async fn delete(&self, id: u64) -> Result<()> {
        self.client
            .delete(&format!("protectionpolicies/{id}/"))
            .await
    }
}
