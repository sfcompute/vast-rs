use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{client::VastClient, Result};

// ---------------------------------------------------------------------------
// Models
// ---------------------------------------------------------------------------

/// A VAST cluster.
///
/// Fields use `#[serde(default)]` so that partial API responses (e.g. in tests
/// or from older cluster software) deserialise without error — missing fields
/// receive their Rust default value. Unknown fields flow into `extra`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Cluster {
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
    /// Software build string, e.g. `"release-5-3-2-sp4-1946523"`.
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub build: String,
    /// Cluster state, e.g. `"ONLINE"`.
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub state: String,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub enabled: bool,
    /// Full software version string.
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub sw_version: String,
    /// Management VIP — the IP address the VMS API is served from.
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub mgmt_vip: String,
    /// Primary CNode IP.
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub ip: String,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub psnt: String,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub system_name: String,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub ssh_user: String,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub loopback: bool,

    // ---- Protocols ----------------------------------------------------------
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub protocols: Vec<String>,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub enable_s3: bool,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub enable_smb: bool,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub enable_dr: bool,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub enable_trash: bool,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub enable_encryption: bool,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub allow_encryption: bool,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub encryption_type: String,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub enable_bucket_replication: bool,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub enable_bucket_db_replication: bool,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub s3_enable_v2_authentication: bool,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub s3_allow_cors: bool,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub allow_nfs3_over_udp: bool,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub showmount_suppressed: bool,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub showmount_hide_slash: bool,

    // ---- SMB ----------------------------------------------------------------
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub use_smb_privileged_group: bool,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub use_smb_privileged_user: bool,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub smb_privileged_group_full_access: bool,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub smb_privileged_user_name: String,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub smb_privileged_group_sid: String,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub smb_administrators_group_name: String,

    // ---- Capacity (bytes) ---------------------------------------------------
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub physical_space: u64,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub physical_space_in_use: u64,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub logical_space: u64,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub logical_space_in_use: u64,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub usable_capacity_bytes: u64,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub free_physical_space: u64,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub free_logical_space: u64,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub free_usable_capacity: u64,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub physical_space_wo_overhead: u64,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub physical_space_in_use_wo_overhead: u64,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub free_physical_space_wo_overhead: u64,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub auxiliary_space_in_use: u64,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub logical_auxiliary_space_in_use: u64,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub usable_auxiliary_space_in_use: u64,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub estore_capacity_in_use_bytes: u64,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub nvram_size: u64,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub drive_size: u64,

    // ---- Capacity (TiB) -----------------------------------------------------
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub physical_space_tb: f64,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub physical_space_in_use_tb: f64,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub logical_space_tb: f64,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub logical_space_in_use_tb: f64,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub free_physical_space_tb: f64,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub free_logical_space_tb: f64,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub free_usable_capacity_tb: f64,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub usable_capacity_tb: f64,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub free_physical_space_wo_overhead_tb: f64,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub auxiliary_space_in_use_tb: f64,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub logical_auxiliary_space_in_use_tb: f64,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub estore_capacity_in_use_tb: f64,

    // ---- Capacity (percent / ratios) ----------------------------------------
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub physical_space_in_use_percent: f64,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub logical_space_in_use_percent: f64,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub physical_drr_percent: f64,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub logical_drr_percent: f64,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub drr: f64,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub drr_text: String,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub general_md_usage: f64,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub use_spdk: bool,

    // ---- Performance counters -----------------------------------------------
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub rd_iops: f64,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub wr_iops: f64,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub iops: f64,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub rd_md_iops: f64,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub wr_md_iops: f64,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub md_iops: f64,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub rd_bw: f64,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub wr_bw: f64,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub bw: f64,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub rd_bw_mb: f64,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub wr_bw_mb: f64,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub bw_mb: f64,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub rd_latency: f64,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub wr_latency: f64,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub latency: f64,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub rd_latency_ms: f64,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub wr_latency_ms: f64,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub latency_ms: f64,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub replication_rd_iops: f64,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub replication_wr_iops: f64,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub replication_iops: f64,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub replication_rd_bw_mb: f64,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub replication_wr_bw_mb: f64,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub replication_bw_mb: f64,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub replication_rd_latency_ms: f64,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub replication_wr_latency_ms: f64,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub replication_latency_ms: f64,

    // ---- NDB (VAST Database) metrics ----------------------------------------
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub ndb_number_of_running_queries: f64,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub ndb_rows_scanned_per_second: f64,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub ndb_bandwidth: f64,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub ndb_bandwidth_read: f64,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub ndb_bandwidth_write: f64,

    // ---- Kafka metrics ------------------------------------------------------
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub fetch_msg_iops: f64,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub fetch_event_iops: f64,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub fetch_bw: f64,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub fetch_msg_latency: f64,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub fetch_event_latency: f64,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub produce_msg_iops: f64,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub produce_event_iops: f64,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub produce_bw: f64,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub produce_msg_latency: f64,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub produce_event_latency: f64,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub msg_iops: f64,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub event_iops: f64,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub kafka_bw: f64,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub msg_latency: f64,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub event_latency: f64,

    // ---- RAID / hardware state ----------------------------------------------
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub shards: u32,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub dr_shards: u32,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub dr_wb_shards: u32,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub stripe_groups: u32,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub micro_shards: u32,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub micro_dr_shards: u32,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub micro_dr_wb_shards: u32,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub micro_stripe_groups: u32,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub mega_shards: u32,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub mega_dr_shards: u32,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub mega_dr_wb_shards: u32,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub mega_stripe_groups: u32,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub cnode_cores: u32,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub max_nvram_replication_factor: u32,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub max_ssd_capacity_percent: u32,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub max_nvram_capacity_percent: u32,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub ssd_raid_state: String,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub nvram_raid_state: String,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub memory_raid_state: String,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub rio_nvram_state: String,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub mio_raid_state: String,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub wb_raid_layout: String,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub raid_drives_can_fail: u32,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub ssd_raid_rebuild_progress: u32,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub nvram_raid_rebuild_progress: u32,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub memory_raid_rebuild_progress: u32,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub rio_raid_rebuild_progress: u32,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub raid_rebuild_progress: String,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub nvram_raid_rebuild_progress_fraction: String,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub rio_raid_rebuild_progress_fraction: String,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub remaining_stripes_health: String,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub md_usage_health: String,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub is_wb_raid_enabled: bool,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub dbox_ha_support: bool,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub dr_hash_size: u64,

    // ---- Cluster topology ---------------------------------------------------
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub leader_cnode: String,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub mgmt_cnode: String,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub mgmt_inner_vip: String,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub mgmt_inner_vip_cnode: String,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub leader_state: String,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub is_indestructible_object_enabled: bool,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub enable_similarity: bool,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub half_system: bool,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub deep_stripe: bool,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub format_drives: bool,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub voc: bool,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub voc_ha_supported: bool,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub r#virtual: bool,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub ebox: bool,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub is_l3_internal_network: bool,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub enable_rack_level_resiliency: bool,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub triplication_enabled: bool,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub use_flash_write_buffers: bool,

    // ---- Upgrade / expansion ------------------------------------------------
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub upgrade_state: String,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub upgrade_phase: String,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub leader_upgrade_state: String,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub expansion_state: String,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub expansion_phase: u32,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub expansion_phase_description: String,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub rewrite_progress: u32,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub rewrite_phase: String,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub rewrite_type: Vec<Value>,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub dbox_migration_state: String,

    // ---- Quotas summary -----------------------------------------------------
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub quotas_used_percent: f64,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub quotas_used_capacity: u64,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub quotas_allocated_capacity: u64,

    // ---- Handle / inode counts ----------------------------------------------
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub used_handles_count: u64,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub max_handles_count: u64,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub used_handles_percent: f64,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub logical_inodes_in_use_num: u64,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub delete_snap_blocks_threshold: u64,

    // ---- Timing / status ----------------------------------------------------
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub uptime: String,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub online_start_time: String,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub deployment_time: String,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub burst_mode_state: String,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub perf_check: bool,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub dmsetup: bool,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub disable_metrics: bool,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub cnode_metrics: bool,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub provides_blocked: bool,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub turbo_boost_flag: bool,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub vast_install: bool,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub vast_audit_log_state: String,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub enable_vast_db_audit: bool,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub enable_json_audit: bool,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub mock_fanout_auth: bool,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub disable_dirsnap: bool,
    #[serde(default)]
    pub enable_json_audit_: Option<bool>, // guard for typo variants

    // ---- Retention / compliance ---------------------------------------------
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub default_others_share_level_perm: String,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub max_file_size: u64,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub max_retention_period: u64,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub max_retention_timeunit: String,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub trash_gid: u64,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub audit_dir_name: String,

    // ---- External version info ----------------------------------------------
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub external_version: String,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub external_build: String,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub external_sp_version: String,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub external_hotfix_version: String,

    // ---- EKM ----------------------------------------------------------------
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub ekm_address: String,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub ekm_port: u32,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub ekm_servers: String,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub ekm_auth_domain: String,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub ekm_proxy_address: String,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub secondary_ekm_port: u32,

    // ---- GN (Global Namespace) ----------------------------------------------
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub gn_max_size: String,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub gn_max_inode_count: u64,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub gn_used_size: String,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub gn_used_inode_count: u64,

    // ---- Capacity limits / misc ---------------------------------------------
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub max_cluster_write_bw_mb: u64,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub auto_logout_timeout: u64,
    #[serde(default)]
    pub s3_enable_http: Option<bool>,

    // ---- Nullable fields ----------------------------------------------------
    #[serde(default)]
    pub nfs4_certificate: Option<String>,
    #[serde(default)]
    pub nfs4_private_key: Option<String>,
    #[serde(default)]
    pub drive_pci_port_type: Option<String>,
    #[serde(default)]
    pub is_large_subnet: Option<bool>,
    #[serde(default)]
    pub secondary_ekm_address: Option<String>,
    #[serde(default)]
    pub voc_cluster_type: Option<String>,
    #[serde(default)]
    pub region: Option<String>,
    #[serde(default)]
    pub motd: Option<String>,
    #[serde(default)]
    pub motd_append_to_default: Option<String>,
    #[serde(default)]
    pub max_audit_dir_size: Option<u64>,
    #[serde(default)]
    pub rewrite_progress_current: Option<Value>,
    #[serde(default)]
    pub rewrite_progress_total: Option<Value>,
    #[serde(default)]
    pub defrag_threshold: Option<Value>,

    // ---- Complex / opaque nested objects ------------------------------------
    #[serde(default)]
    pub protocols_audit: Value,
    #[serde(default)]
    pub system_settings: Value,
    #[serde(default)]
    pub s3_certificate_info: Value,
    #[serde(default)]
    pub nfs4_certificate_info: Option<Value>,
    #[serde(default)]
    pub cluster_certificate_info: Value,
    #[serde(default)]
    pub upgrade_progress: Value,
    #[serde(default)]
    pub max_performance: Value,
    #[serde(default)]
    pub available_upgrade_version: Value,
    #[serde(default)]
    pub s3_new_version: Value,
    #[serde(default)]
    pub qos: Value,
    #[serde(default)]
    pub b2b_configuration: Value,
    #[serde(default)]
    pub bgp_session_state_per_port: Value,
    #[serde(default)]
    pub dbox_migration_validation_state: Value,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub access_ip_ranges: Vec<Value>,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub read_access_users: Vec<Value>,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub read_access_users_groups: Vec<Value>,

    // ---- Redacted secret fields (returned as "*****" strings) ---------------
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub s3_certificate: String,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub s3_private_key: String,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub cluster_certificate: String,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub cluster_private_key: String,
    #[serde(default, deserialize_with = "crate::null_as_default")]
    pub root_certificate: String,

    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

// ---------------------------------------------------------------------------
// API handle
// ---------------------------------------------------------------------------

/// Scoped access to the `/api/clusters/` resource.
pub struct ClustersApi<'c> {
    client: &'c VastClient,
}

impl<'c> ClustersApi<'c> {
    pub(crate) fn new(client: &'c VastClient) -> Self {
        Self { client }
    }

    /// List all clusters visible to this user.
    ///
    /// `GET /api/clusters/`
    pub async fn list(&self) -> Result<Vec<Cluster>> {
        self.client.get("clusters/").await
    }

    /// Retrieve a single cluster by its numeric ID.
    ///
    /// `GET /api/clusters/{id}/`
    pub async fn get(&self, id: u64) -> Result<Cluster> {
        self.client.get(&format!("clusters/{id}/")).await
    }

    /// Permanently delete a directory from the VAST namespace.
    ///
    /// `DELETE /api/clusters/{id}/delete_folder/`
    ///
    /// This removes the directory at `path` from the filesystem.  Note that
    /// some VAST deployments require the **Trash Folder Access** feature to be
    /// enabled (Settings → Cluster → Enable trash folder access) before this
    /// endpoint will succeed.
    pub async fn delete_folder(
        &self,
        cluster_id: u64,
        path: &str,
        tenant_id: Option<u64>,
    ) -> Result<()> {
        let mut body = serde_json::json!({ "path": path });
        if let Some(tid) = tenant_id {
            body["tenant_id"] = serde_json::json!(tid);
        }
        self.client
            .delete_with_body(
                &format!("clusters/{cluster_id}/delete_folder/"),
                &body,
            )
            .await
    }
}
