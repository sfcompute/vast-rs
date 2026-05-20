use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{client::VastClient, Result};

// ---------------------------------------------------------------------------
// Models
// ---------------------------------------------------------------------------

/// A VAST VIP pool — a range of virtual IP addresses served by a set of CNodes.
///
/// `#[serde(default)]` lets partial responses (tests, older software) deserialise
/// without error; unknown fields flow into `extra`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VipPool {
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

    // ---- IP addressing ------------------------------------------------------
    /// First IPv4/IPv6 address in the pool range.
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub start_ip: String,
    /// Last IPv4/IPv6 address in the pool range.
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub end_ip: String,

    /// VLAN tag (0 = untagged).
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub vlan: u32,
    /// IPv4 subnet prefix length (0 if IPv6-only).
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub subnet_cidr: u32,
    /// IPv4 default gateway (empty string if IPv6-only).
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub gw_ip: String,

    /// IPv6 gateway address (empty string if IPv6 not configured).
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub gw_ipv6: String,
    /// IPv6 subnet prefix length (`None` if IPv6 not configured).
    #[serde(default)]
    pub subnet_cidr_ipv6: Option<u32>,

    // ---- Association --------------------------------------------------------
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub cluster: String,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub cluster_id: u64,
    /// Tenant this pool is scoped to (`None` = cluster-wide).
    #[serde(default)]
    pub tenant_id: Option<u64>,
    #[serde(default)]
    pub tenant_name: Option<String>,

    // ---- CNode membership ---------------------------------------------------
    /// IDs of CNodes currently serving VIPs from this pool.
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub active_cnode_ids: Vec<u64>,
    /// All CNode IDs associated with this pool (including standby).
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub cnode_ids: Vec<u64>,
    /// CNode detail objects (may be empty if not expanded).
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub cnodes: Vec<Value>,
    /// Number of active network interfaces serving this pool.
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub active_interfaces: u64,

    // ---- DNS / routing ------------------------------------------------------
    /// DNS domain name for reverse lookups on VIPs in this pool.
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub domain_name: String,
    /// Pool role, e.g. `"APPLICATION"`, `"PROTOCOLS"`.
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub role: String,
    /// IPv4/IPv6 address ranges as `[["start", "end"], ...]` pairs.
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub ip_ranges: Vec<Vec<String>>,
    /// Human-readable summary of the IP ranges.
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub ranges_summary: String,

    // ---- BGP configuration --------------------------------------------------
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub enable_l3: bool,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub vast_asn: u64,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub peer_asn: u64,
    #[serde(default)]
    pub bgp_config_id: Option<u64>,
    #[serde(default)]
    pub bgp_config_guid: Option<String>,
    #[serde(default)]
    pub bgp_config_name: Option<String>,

    // ---- Load balancing -----------------------------------------------------
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub enable_weighted_balancing: bool,
    /// VMS-preferred routing (prefer VMS traffic via this pool).
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub vms_preferred: bool,
    /// Port membership policy, e.g. `"ALL"`.
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub port_membership: String,

    // ---- Kafka integration --------------------------------------------------
    #[serde(default)]
    pub kafka_view_id: Option<Value>,

    // ---- State --------------------------------------------------------------
    /// Sync state, e.g. `"SYNCED"`.
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub sync: String,
    /// ISO 8601 timestamp of last sync.
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub sync_time: String,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub enabled: bool,

    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

/// Body for `POST /api/vippools/`.
#[derive(Debug, Serialize)]
pub struct CreateVipPool {
    pub name: String,
    pub start_ip: String,
    pub end_ip: String,
    pub gw_ip: String,
    pub subnet_cidr: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vlan: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cnode_ids: Option<Vec<u64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub domain_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tenant_id: Option<u64>,
}

/// Body for `PATCH /api/vippools/{id}/`.
#[derive(Debug, Default, Serialize)]
pub struct UpdateVipPool {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_ip: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_ip: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gw_ip: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subnet_cidr: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vlan: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cnode_ids: Option<Vec<u64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub domain_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
}

// ---------------------------------------------------------------------------
// API handle
// ---------------------------------------------------------------------------

/// Scoped access to the `/api/vippools/` resource.
pub struct VipPoolsApi<'c> {
    client: &'c VastClient,
}

impl<'c> VipPoolsApi<'c> {
    pub(crate) fn new(client: &'c VastClient) -> Self {
        Self { client }
    }

    /// List all VIP pools.
    ///
    /// `GET /api/vippools/`
    pub async fn list(&self) -> Result<Vec<VipPool>> {
        self.client.get("vippools/").await
    }

    /// Get a single VIP pool by ID.
    ///
    /// `GET /api/vippools/{id}/`
    pub async fn get(&self, id: u64) -> Result<VipPool> {
        self.client.get(&format!("vippools/{id}/")).await
    }

    /// Create a new VIP pool.
    ///
    /// `POST /api/vippools/`
    pub async fn create(&self, body: &CreateVipPool) -> Result<VipPool> {
        self.client.post("vippools/", body).await
    }

    /// Update an existing VIP pool.
    ///
    /// `PATCH /api/vippools/{id}/`
    pub async fn update(&self, id: u64, body: &UpdateVipPool) -> Result<VipPool> {
        self.client.patch(&format!("vippools/{id}/"), body).await
    }

    /// Delete a VIP pool by ID.
    ///
    /// `DELETE /api/vippools/{id}/`
    pub async fn delete(&self, id: u64) -> Result<()> {
        self.client.delete(&format!("vippools/{id}/")).await
    }
}
