use serde::{Deserialize, Serialize};

use crate::{client::VastClient, Result};

// ---------------------------------------------------------------------------
// Models
// ---------------------------------------------------------------------------

/// A VAST snapshot.
///
/// The exact field set depends on the VMS version; fields beyond the common
/// ones are captured in `extra` for forward-compatibility.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snapshot {
    pub id: u64,
    pub guid: String,
    pub name: String,
    pub title: String,
    pub path: String,
    pub cluster: String,
    pub cluster_id: u64,
    pub tenant_id: u64,

    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

/// Body for `POST /api/snapshots/`.
#[derive(Debug, Serialize)]
pub struct CreateSnapshot {
    pub name: String,
    /// Filesystem path to snapshot.
    pub path: String,
}

/// Body for `PATCH /api/snapshots/{id}/`.
#[derive(Debug, Default, Serialize)]
pub struct UpdateSnapshot {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

// ---------------------------------------------------------------------------
// API handle
// ---------------------------------------------------------------------------

/// Scoped access to the `/api/snapshots/` resource.
pub struct SnapshotsApi<'c> {
    client: &'c VastClient,
}

impl<'c> SnapshotsApi<'c> {
    pub(crate) fn new(client: &'c VastClient) -> Self {
        Self { client }
    }

    /// List all snapshots.
    ///
    /// `GET /api/snapshots/`
    pub async fn list(&self) -> Result<Vec<Snapshot>> {
        self.client.get("snapshots/").await
    }

    /// Get a single snapshot by ID.
    ///
    /// `GET /api/snapshots/{id}/`
    pub async fn get(&self, id: u64) -> Result<Snapshot> {
        self.client.get(&format!("snapshots/{id}/")).await
    }

    /// Create a new snapshot.
    ///
    /// `POST /api/snapshots/`
    pub async fn create(&self, body: &CreateSnapshot) -> Result<Snapshot> {
        self.client.post("snapshots/", body).await
    }

    /// Update an existing snapshot.
    ///
    /// `PATCH /api/snapshots/{id}/`
    pub async fn update(&self, id: u64, body: &UpdateSnapshot) -> Result<Snapshot> {
        self.client.patch(&format!("snapshots/{id}/"), body).await
    }

    /// Delete a snapshot by ID.
    ///
    /// `DELETE /api/snapshots/{id}/`
    pub async fn delete(&self, id: u64) -> Result<()> {
        self.client.delete(&format!("snapshots/{id}/")).await
    }
}
