use serde::{Deserialize, Serialize};

use crate::{client::VastClient, Result};

// ---------------------------------------------------------------------------
// Models
// ---------------------------------------------------------------------------

/// A VAST filesystem directory (folder).
///
/// Folders are the backing filesystem objects for views and quotas.
/// Creating a folder via this API is equivalent to `mkdir -p`; deleting one
/// permanently removes it from the namespace.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Folder {
    #[serde(default)]
    pub id: u64,
    #[serde(default)]
    pub guid: String,
    #[serde(default)]
    pub name: String,
    /// Absolute filesystem path of this directory.
    #[serde(default)]
    pub path: String,

    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

/// Body for `POST /api/folders/`.
#[derive(Debug, Serialize)]
pub struct CreateFolder {
    /// Absolute path for the new directory.
    pub path: String,
    /// Create intermediate parent directories as needed (like `mkdir -p`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub create_dirs: Option<bool>,
}

// ---------------------------------------------------------------------------
// API handle
// ---------------------------------------------------------------------------

/// Scoped access to the `/api/folders/` resource.
pub struct FoldersApi<'c> {
    client: &'c VastClient,
}

impl<'c> FoldersApi<'c> {
    pub(crate) fn new(client: &'c VastClient) -> Self {
        Self { client }
    }

    /// List all folders.
    ///
    /// `GET /api/folders/`
    pub async fn list(&self) -> Result<Vec<Folder>> {
        self.client.get("folders/").await
    }

    /// Get a single folder by ID.
    ///
    /// `GET /api/folders/{id}/`
    pub async fn get(&self, id: u64) -> Result<Folder> {
        self.client.get(&format!("folders/{id}/")).await
    }

    /// Create a new folder (directory) in the VAST namespace.
    ///
    /// Set `create_dirs: Some(true)` to create intermediate parents
    /// automatically (equivalent to `mkdir -p`).
    ///
    /// `POST /api/folders/`
    pub async fn create(&self, body: &CreateFolder) -> Result<Folder> {
        self.client.post("folders/", body).await
    }

    /// Permanently delete a folder by ID.
    ///
    /// `DELETE /api/folders/{id}/`
    pub async fn delete(&self, id: u64) -> Result<()> {
        self.client.delete(&format!("folders/{id}/")).await
    }
}
