use serde::{Deserialize, Serialize};

use crate::{client::VastClient, Result};

// ---------------------------------------------------------------------------
// Models
// ---------------------------------------------------------------------------

/// A VAST volume.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Volume {
    pub id: u64,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guid: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quota: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logical_capacity: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_snapshot: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

/// Body for `POST /api/volumes/`.
#[derive(Debug, Serialize)]
pub struct CreateVolume {
    pub name: String,
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quota: Option<u64>,
}

/// Body for `PATCH /api/volumes/{id}/`.
#[derive(Debug, Default, Serialize)]
pub struct UpdateVolume {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quota: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
}

/// Query parameters for `GET /api/volumes/`.
#[derive(Debug, Default, Serialize)]
pub struct ListVolumesParams {
    /// Only return volumes under this path prefix.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_snapshot: Option<bool>,
}

// ---------------------------------------------------------------------------
// API handle
// ---------------------------------------------------------------------------

/// Scoped access to the `/api/volumes/` resource.
pub struct VolumesApi<'c> {
    client: &'c VastClient,
}

impl<'c> VolumesApi<'c> {
    pub(crate) fn new(client: &'c VastClient) -> Self {
        Self { client }
    }

    /// List all volumes.
    ///
    /// `GET /api/volumes/`
    pub async fn list(&self) -> Result<Vec<Volume>> {
        self.client.get("volumes/").await
    }

    /// List volumes with query filters.
    pub async fn list_with_params(&self, params: &ListVolumesParams) -> Result<Vec<Volume>> {
        self.client.get_with_query("volumes/", params).await
    }

    /// Get a single volume by ID.
    ///
    /// `GET /api/volumes/{id}/`
    pub async fn get(&self, id: u64) -> Result<Volume> {
        self.client.get(&format!("volumes/{id}/")).await
    }

    /// Create a new volume.
    ///
    /// `POST /api/volumes/`
    pub async fn create(&self, body: &CreateVolume) -> Result<Volume> {
        self.client.post("volumes/", body).await
    }

    /// Update an existing volume.
    ///
    /// `PATCH /api/volumes/{id}/`
    pub async fn update(&self, id: u64, body: &UpdateVolume) -> Result<Volume> {
        self.client.patch(&format!("volumes/{id}/"), body).await
    }

    /// Delete a volume by ID.
    ///
    /// `DELETE /api/volumes/{id}/`
    pub async fn delete(&self, id: u64) -> Result<()> {
        self.client.delete(&format!("volumes/{id}/")).await
    }
}
