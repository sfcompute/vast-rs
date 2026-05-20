use serde::{Deserialize, Serialize};

use crate::{client::VastClient, Result};

// ---------------------------------------------------------------------------
// Models
// ---------------------------------------------------------------------------

/// A VMS user account.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub id: u64,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uid: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_admin: Option<bool>,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

/// Body for `POST /api/users/`.
#[derive(Debug, Serialize)]
pub struct CreateUser {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uid: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
}

/// Body for `PATCH /api/users/{id}/`.
#[derive(Debug, Default, Serialize)]
pub struct UpdateUser {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
}

// ---------------------------------------------------------------------------
// API handle
// ---------------------------------------------------------------------------

/// Scoped access to the `/api/users/` resource.
pub struct UsersApi<'c> {
    client: &'c VastClient,
}

impl<'c> UsersApi<'c> {
    pub(crate) fn new(client: &'c VastClient) -> Self {
        Self { client }
    }

    /// List all users.
    ///
    /// `GET /api/users/`
    pub async fn list(&self) -> Result<Vec<User>> {
        self.client.get("users/").await
    }

    /// Get a single user by ID.
    ///
    /// `GET /api/users/{id}/`
    pub async fn get(&self, id: u64) -> Result<User> {
        self.client.get(&format!("users/{id}/")).await
    }

    /// Create a new user.
    ///
    /// `POST /api/users/`
    pub async fn create(&self, body: &CreateUser) -> Result<User> {
        self.client.post("users/", body).await
    }

    /// Update an existing user.
    ///
    /// `PATCH /api/users/{id}/`
    pub async fn update(&self, id: u64, body: &UpdateUser) -> Result<User> {
        self.client.patch(&format!("users/{id}/"), body).await
    }

    /// Delete a user by ID.
    ///
    /// `DELETE /api/users/{id}/`
    pub async fn delete(&self, id: u64) -> Result<()> {
        self.client.delete(&format!("users/{id}/")).await
    }
}
