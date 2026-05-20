use serde::{Deserialize, Serialize};

use crate::{client::VastClient, Result};

// ---------------------------------------------------------------------------
// Models
// ---------------------------------------------------------------------------

/// A VAST cluster node.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Node {
    pub id: u64,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guid: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ip: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mgmt_ip: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cluster: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ssd_raids_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nvram_raids_count: Option<u64>,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

/// Query parameters accepted by `GET /api/nodes/`.
#[derive(Debug, Default, Serialize)]
pub struct ListNodesParams {
    /// Filter by cluster ID.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cluster_id: Option<u64>,
    /// Filter by node state (e.g. `"ACTIVE"`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state: Option<String>,
}

// ---------------------------------------------------------------------------
// API handle
// ---------------------------------------------------------------------------

/// Scoped access to the `/api/nodes/` resource.
pub struct NodesApi<'c> {
    client: &'c VastClient,
}

impl<'c> NodesApi<'c> {
    pub(crate) fn new(client: &'c VastClient) -> Self {
        Self { client }
    }

    /// List all nodes, optionally filtered.
    ///
    /// `GET /api/nodes/`
    pub async fn list(&self) -> Result<Vec<Node>> {
        self.client.get("nodes/").await
    }

    /// List nodes with query filters.
    ///
    /// `GET /api/nodes/?cluster_id=…`
    pub async fn list_with_params(&self, params: &ListNodesParams) -> Result<Vec<Node>> {
        self.client.get_with_query("nodes/", params).await
    }

    /// Retrieve a single node by its numeric ID.
    ///
    /// `GET /api/nodes/{id}/`
    pub async fn get(&self, id: u64) -> Result<Node> {
        self.client.get(&format!("nodes/{id}/")).await
    }
}
