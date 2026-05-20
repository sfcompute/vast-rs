//! Integration-style unit tests using [`wiremock`] to mock the VMS HTTP API.
//!
//! These tests spin up a local mock server and verify that the client sends
//! the correct HTTP requests and correctly deserialises responses.

// The view fixture JSON is deeply nested, pushing past serde_json's default
// macro expansion limit.
#![recursion_limit = "256"]

use serde_json::json;
use wiremock::matchers::{body_json, header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use vast_rs::VastClient;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Start a mock server and build a token-auth client pointed at it.
async fn setup(token: &str) -> (MockServer, VastClient) {
    let server = MockServer::start().await;
    let client = VastClient::builder()
        .address(server.uri())
        .token(token)
        .danger_accept_invalid_certs(true)
        .build()
        .expect("failed to build client");
    (server, client)
}

/// Build a credentials-auth client against `server`, optionally scoped to a tenant.
async fn setup_credentials(
    server: &MockServer,
    user: &str,
    pass: &str,
    tenant: Option<&str>,
) -> VastClient {
    let mut builder = VastClient::builder()
        .address(server.uri())
        .credentials(user, pass)
        .danger_accept_invalid_certs(true);
    if let Some(t) = tenant {
        builder = builder.tenant(t);
    }
    builder.build().expect("failed to build client")
}

// ---------------------------------------------------------------------------
// Auth — token
// ---------------------------------------------------------------------------

#[tokio::test]
async fn token_auth_sets_bearer_header() {
    let (server, client) = setup("test-token-abc").await;

    Mock::given(method("GET"))
        .and(path("/api/clusters/"))
        .and(header("authorization", "Bearer test-token-abc"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
        .expect(1)
        .mount(&server)
        .await;

    let result = client.clusters().list().await;
    assert!(result.is_ok(), "expected Ok, got {result:?}");
}

// ---------------------------------------------------------------------------
// Auth — credentials (cluster admin, no tenant)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn credentials_auth_posts_to_token_slash() {
    let server = MockServer::start().await;

    // Cluster admins hit POST /api/token/
    Mock::given(method("POST"))
        .and(path("/api/token/"))
        .and(body_json(json!({ "username": "admin", "password": "secret" })))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({ "access": "jwt-cluster", "refresh": null })),
        )
        .expect(1)
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path("/api/clusters/"))
        .and(header("authorization", "Bearer jwt-cluster"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
        .expect(1)
        .mount(&server)
        .await;

    let client = setup_credentials(&server, "admin", "secret", None).await;
    let result = client.clusters().list().await;
    assert!(result.is_ok(), "expected Ok, got {result:?}");
}

// ---------------------------------------------------------------------------
// Auth — credentials (tenant admin, with tenant)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn credentials_auth_tenant_uses_token_tenant_name_path() {
    let server = MockServer::start().await;

    // Tenant admins hit POST /api/token/{tenant_name} — a different URL, not a
    // body field. The body is the same as cluster admin (username + password only).
    Mock::given(method("POST"))
        .and(path("/api/token/acme"))
        .and(body_json(json!({ "username": "alice", "password": "secret" })))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({ "access": "jwt-tenant", "refresh": null })),
        )
        .expect(1)
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path("/api/volumes/"))
        .and(header("authorization", "Bearer jwt-tenant"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
        .expect(1)
        .mount(&server)
        .await;

    let client = setup_credentials(&server, "alice", "secret", Some("acme")).await;
    let result = client.volumes().list().await;
    assert!(result.is_ok(), "expected Ok, got {result:?}");
}

#[tokio::test]
async fn token_is_cached_and_token_endpoint_called_only_once() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/api/token/"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({ "access": "cached-jwt", "refresh": null })),
        )
        .expect(1) // Must be called exactly once despite two API calls.
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
        .mount(&server)
        .await;

    let client = setup_credentials(&server, "admin", "pass", None).await;
    let _ = client.clusters().list().await;
    let _ = client.volumes().list().await;

    server.verify().await; // asserts the expect(1) on the token endpoint
}

// ---------------------------------------------------------------------------
// Clusters
// ---------------------------------------------------------------------------

#[tokio::test]
async fn clusters_list_returns_parsed_vec() {
    let (server, client) = setup("tok").await;

    let body = json!([
        {
            "id": 1, "guid": "aaaa-1111", "name": "prod-cluster", "title": "prod-cluster",
            "build": "release-5-3-2", "state": "ONLINE", "enabled": true,
            "sw_version": "5.2.0", "mgmt_vip": "10.0.0.1",
            "physical_space": 1000000000000u64, "physical_space_in_use": 100000000000u64,
            "logical_space": 900000000000u64, "logical_space_in_use": 50000000000u64,
            "usable_capacity_bytes": 900000000000u64,
            "free_physical_space": 900000000000u64, "free_logical_space": 850000000000u64,
            "physical_space_tb": 1.0, "logical_space_tb": 0.9,
            "free_physical_space_tb": 0.9, "free_logical_space_tb": 0.85,
            "rd_iops": 0.0, "wr_iops": 10.0, "rd_bw_mb": 0.0, "wr_bw_mb": 1.0,
            "rd_latency_ms": 0.0, "wr_latency_ms": 5.0,
            "drr": 1.0, "drr_text": "1.0:1"
        },
        {
            "id": 2, "guid": "bbbb-2222", "name": "dev-cluster", "title": "dev-cluster",
            "build": "release-5-1-0", "state": "ONLINE", "enabled": true,
            "sw_version": "5.1.0", "mgmt_vip": "10.0.0.2",
            "physical_space": 500000000000u64, "physical_space_in_use": 50000000000u64,
            "logical_space": 450000000000u64, "logical_space_in_use": 25000000000u64,
            "usable_capacity_bytes": 450000000000u64,
            "free_physical_space": 450000000000u64, "free_logical_space": 425000000000u64,
            "physical_space_tb": 0.5, "logical_space_tb": 0.45,
            "free_physical_space_tb": 0.45, "free_logical_space_tb": 0.43,
            "rd_iops": 0.0, "wr_iops": 5.0, "rd_bw_mb": 0.0, "wr_bw_mb": 0.5,
            "rd_latency_ms": 0.0, "wr_latency_ms": 4.0,
            "drr": 1.0, "drr_text": "1.0:1"
        },
    ]);

    Mock::given(method("GET"))
        .and(path("/api/clusters/"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&body))
        .mount(&server)
        .await;

    let clusters = client.clusters().list().await.unwrap();
    assert_eq!(clusters.len(), 2);
    assert_eq!(clusters[0].name, "prod-cluster");
    assert_eq!(clusters[1].id, 2);
}

#[tokio::test]
async fn clusters_get_single() {
    let (server, client) = setup("tok").await;

    let body = json!({
        "id": 42, "guid": "cccc-3333", "name": "my-cluster", "title": "my-cluster",
        "build": "release-5-3-0", "state": "ONLINE", "enabled": true,
        "sw_version": "5.3.0", "mgmt_vip": "10.0.0.42",
        "physical_space": 1000000000000u64, "physical_space_in_use": 0u64,
        "logical_space": 900000000000u64, "logical_space_in_use": 0u64,
        "usable_capacity_bytes": 900000000000u64,
        "free_physical_space": 1000000000000u64, "free_logical_space": 900000000000u64,
        "physical_space_tb": 1.0, "logical_space_tb": 0.9,
        "free_physical_space_tb": 1.0, "free_logical_space_tb": 0.9,
        "rd_iops": 0.0, "wr_iops": 0.0, "rd_bw_mb": 0.0, "wr_bw_mb": 0.0,
        "rd_latency_ms": 0.0, "wr_latency_ms": 0.0,
        "drr": 1.0, "drr_text": "1.0:1"
    });

    Mock::given(method("GET"))
        .and(path("/api/clusters/42/"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&body))
        .mount(&server)
        .await;

    let cluster = client.clusters().get(42).await.unwrap();
    assert_eq!(cluster.id, 42);
    assert_eq!(cluster.name, "my-cluster");
}

// ---------------------------------------------------------------------------
// Error handling
// ---------------------------------------------------------------------------

#[tokio::test]
async fn api_error_is_parsed_from_detail_field() {
    let (server, client) = setup("tok").await;

    Mock::given(method("GET"))
        .and(path("/api/clusters/999/"))
        .respond_with(
            ResponseTemplate::new(404)
                .set_body_json(json!({ "detail": "Not found." })),
        )
        .mount(&server)
        .await;

    let err = client.clusters().get(999).await.unwrap_err();
    assert!(err.is_not_found(), "expected is_not_found, got {err:?}");
    assert!(
        err.to_string().contains("Not found."),
        "message should be forwarded: {err}"
    );
}

#[tokio::test]
async fn api_error_401_is_unauthorized() {
    let (server, client) = setup("bad-token").await;

    Mock::given(method("GET"))
        .and(path("/api/users/"))
        .respond_with(
            ResponseTemplate::new(401).set_body_json(
                json!({ "detail": "Authentication credentials were not provided." }),
            ),
        )
        .mount(&server)
        .await;

    let err = client.users().list().await.unwrap_err();
    assert!(err.is_unauthorized(), "expected is_unauthorized, got {err:?}");
}

// ---------------------------------------------------------------------------
// Volumes
// ---------------------------------------------------------------------------

#[tokio::test]
async fn volumes_create_sends_post_with_body() {
    use vast_rs::api::volumes::CreateVolume;

    let (server, client) = setup("tok").await;

    let response_body = json!({
        "id": 10, "name": "data-vol", "path": "/data", "quota": 1073741824
    });

    Mock::given(method("POST"))
        .and(path("/api/volumes/"))
        .and(header("content-type", "application/json"))
        .respond_with(ResponseTemplate::new(201).set_body_json(&response_body))
        .mount(&server)
        .await;

    let vol = client
        .volumes()
        .create(&CreateVolume {
            name: "data-vol".into(),
            path: "/data".into(),
            quota: Some(1_073_741_824),
        })
        .await
        .unwrap();

    assert_eq!(vol.id, 10);
    assert_eq!(vol.name, "data-vol");
}

// ---------------------------------------------------------------------------
// Views
// ---------------------------------------------------------------------------

#[tokio::test]
async fn views_list_returns_parsed_vec() {
    use vast_rs::api::views::View;

    let (server, client) = setup("tok").await;

    let body = serde_json::json!([{
        "id": 1,
        "guid": "aaaa-1111",
        "name": "nfs-export",
        "title": "nfs-export",
        "url": "https://vms/api/views/1/",
        "path": "/data",
        "alias": "",
        "cluster": "prod",
        "cluster_id": 1,
        "policy": "default",
        "policy_id": 2,
        "protocols": ["NFS"],
        "allow_anonymous_access": false,
        "allow_s3_anonymous_access": false,
        "bucket": "",
        "bucket_creators": [],
        "bucket_creators_groups": [],
        "bucket_owner": null,
        "internal": false,
        "logical_capacity": 0,
        "physical_capacity": 0,
        "ignore_oos": false,
        "nfs_interop_flags": "BOTH_NFS3_AND_NFS4_INTEROP_DISABLED",
        "s3_locks_retention_mode": "NONE",
        "files_retention_mode": "NONE",
        "max_retention_period": "0d",
        "min_retention_period": "0d",
        "auto_commit": "0d",
        "s3_unverified_lookup": false,
        "s3_versioning": false,
        "share": "",
        "sync": "SYNCED",
        "sync_time": "2026-05-19T20:30:52Z",
        "tenant_id": 1,
        "created": "2026-05-19T20:21:13Z",
        "is_remote": false,
        "select_for_live_monitoring": false,
        "tenant_name": "default",
        "qos_policy": null,
        "qos_policy_id": null,
        "bulk_permission_update_state": null,
        "bulk_permission_update_progress": null,
        "abe_max_depth": null,
        "abe_protocols": [],
        "abac_tags": [],
        "is_seamless": false,
        "s3_locks": false,
        "s3_locks_retention_period": "",
        "share_acl": {"acl": [], "enabled": false},
        "enabled": true,
        "s3_object_ownership_rule": "ObjectWriter",
        "tenant_default_others_share_level_perm": "FULL",
        "is_indestructible_object_enabled": false,
        "indestructible_object_duration": 8,
        "user_impersonation": {"username": "", "enabled": false, "identifier": "", "identifier_type": "", "login_name": ""},
        "impersonation_username": ""
    }]);

    Mock::given(method("GET"))
        .and(path("/api/views/"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&body))
        .mount(&server)
        .await;

    let views: Vec<View> = client.views().list().await.unwrap();
    assert_eq!(views.len(), 1);
    assert_eq!(views[0].name, "nfs-export");
    assert_eq!(views[0].path, "/data");
    assert_eq!(views[0].protocols, vec!["NFS"]);
}

#[tokio::test]
async fn views_create_sends_post_with_body() {
    use vast_rs::api::views::CreateView;

    let (server, client) = setup("tok").await;

    let response_body = serde_json::json!({
        "id": 5,
        "guid": "bbbb-2222",
        "name": "s3-bucket",
        "title": "s3-bucket",
        "url": "https://vms/api/views/5/",
        "path": "/buckets/mybucket",
        "alias": "",
        "cluster": "prod",
        "cluster_id": 1,
        "policy": "default",
        "policy_id": 2,
        "protocols": ["S3"],
        "allow_anonymous_access": false,
        "allow_s3_anonymous_access": false,
        "bucket": "mybucket",
        "bucket_creators": [],
        "bucket_creators_groups": [],
        "bucket_owner": null,
        "internal": false,
        "logical_capacity": 0,
        "physical_capacity": 0,
        "ignore_oos": false,
        "nfs_interop_flags": "",
        "s3_locks_retention_mode": "NONE",
        "files_retention_mode": "NONE",
        "max_retention_period": "0d",
        "min_retention_period": "0d",
        "auto_commit": "0d",
        "s3_unverified_lookup": false,
        "s3_versioning": false,
        "share": "",
        "sync": "SYNCED",
        "sync_time": "2026-05-19T20:30:52Z",
        "tenant_id": 1,
        "created": "2026-05-19T20:21:13Z",
        "is_remote": false,
        "select_for_live_monitoring": false,
        "tenant_name": "default",
        "qos_policy": null,
        "qos_policy_id": null,
        "bulk_permission_update_state": null,
        "bulk_permission_update_progress": null,
        "abe_max_depth": null,
        "abe_protocols": [],
        "abac_tags": [],
        "is_seamless": false,
        "s3_locks": false,
        "s3_locks_retention_period": "",
        "share_acl": {"acl": [], "enabled": false},
        "enabled": true,
        "s3_object_ownership_rule": "ObjectWriter",
        "tenant_default_others_share_level_perm": "FULL",
        "is_indestructible_object_enabled": false,
        "indestructible_object_duration": 8,
        "user_impersonation": {"username": "", "enabled": false, "identifier": "", "identifier_type": "", "login_name": ""},
        "impersonation_username": ""
    });

    Mock::given(method("POST"))
        .and(path("/api/views/"))
        .and(header("content-type", "application/json"))
        .respond_with(ResponseTemplate::new(201).set_body_json(&response_body))
        .mount(&server)
        .await;

    let view = client
        .views()
        .create(&CreateView {
            name: "s3-bucket".into(),
            path: "/buckets/mybucket".into(),
            policy_id: 2,
            protocols: vec!["S3".into()],
            create_dir: None,
            alias: None,
            bucket: Some("mybucket".into()),
            allow_anonymous_access: None,
            s3_versioning: None,
            s3_locks: None,
            s3_locks_retention_mode: None,
        })
        .await
        .unwrap();

    assert_eq!(view.id, 5);
    assert_eq!(view.bucket, "mybucket");
}

// ---------------------------------------------------------------------------
// Quotas
// ---------------------------------------------------------------------------

#[tokio::test]
async fn quotas_list_returns_parsed_vec() {
    use vast_rs::api::quotas::Quota;

    let (server, client) = setup("tok").await;

    let body = serde_json::json!([{
        "id": 10,
        "guid": "cccc-3333",
        "name": "team-quota",
        "title": "team-quota",
        "url": "https://vms/api/quotas/10",
        "path": "/data",
        "state": "OK",
        "pretty_state": "OK",
        "grace_period": null,
        "pretty_grace_period": null,
        "pretty_grace_period_expiration": null,
        "time_to_block": null,
        "soft_limit": null,
        "hard_limit": null,
        "soft_limit_inodes": null,
        "hard_limit_inodes": null,
        "used_inodes": 1234,
        "sync_state": "SYNCHRONIZED",
        "used_capacity": 5368709120u64,
        "used_effective_capacity": 4294967296u64,
        "used_capacity_tb": 0.005,
        "used_effective_capacity_tb": 0.004,
        "used_limited_capacity": 0,
        "percent_inodes": null,
        "percent_capacity": null,
        "default_user_quota": null,
        "default_group_quota": null,
        "num_exceeded_users": 0,
        "num_blocked_users": 0,
        "last_user_quotas_update": null,
        "enable_email_providers": true,
        "default_email": null,
        "cluster": "prod",
        "cluster_id": 1,
        "tenant_id": 1,
        "tenant_name": "default",
        "internal": false,
        "system_id": 42,
        "enable_alarms": true,
        "is_user_quota": false
    }]);

    Mock::given(method("GET"))
        .and(path("/api/quotas/"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&body))
        .mount(&server)
        .await;

    let quotas: Vec<Quota> = client.quotas().list().await.unwrap();
    assert_eq!(quotas.len(), 1);
    assert_eq!(quotas[0].name, "team-quota");
    assert_eq!(quotas[0].hard_limit, None); // null in the API response
    assert_eq!(quotas[0].state, "OK");
}

// ---------------------------------------------------------------------------
// VIP Pools
// ---------------------------------------------------------------------------

#[tokio::test]
async fn vippools_list_returns_parsed_vec() {
    use vast_rs::api::vippools::VipPool;

    let (server, client) = setup("tok").await;

    let body = serde_json::json!([{
        "id": 1,
        "guid": "dddd-4444",
        "name": "app-pool",
        "title": "app-pool",
        "url": "https://vms/api/vippools/1",
        "start_ip": "10.0.1.1",
        "end_ip": "10.0.1.10",
        "vlan": 0,
        "subnet_cidr": 24,
        "gw_ip": "10.0.1.254",
        "gw_ipv6": "",
        "subnet_cidr_ipv6": null,
        "cluster": "prod",
        "cluster_id": 1,
        "tenant_id": null,
        "tenant_name": null,
        "active_cnode_ids": [1, 2],
        "cnode_ids": [1, 2, 3, 4],
        "cnodes": [],
        "active_interfaces": 2,
        "domain_name": "",
        "role": "APPLICATION",
        "ip_ranges": [["10.0.1.1", "10.0.1.10"]],
        "ranges_summary": "10.0.1.1-10.0.1.10",
        "sync": "SYNCED",
        "sync_time": "2026-05-19T20:30:15Z",
        "enabled": true,
        "vms_preferred": false,
        "port_membership": "ALL",
        "enable_l3": false,
        "vast_asn": 0,
        "peer_asn": 0,
        "enable_weighted_balancing": false,
        "kafka_view_id": null,
        "bgp_config_id": null,
        "bgp_config_guid": null,
        "bgp_config_name": null
    }]);

    Mock::given(method("GET"))
        .and(path("/api/vippools/"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&body))
        .mount(&server)
        .await;

    let pools: Vec<VipPool> = client.vip_pools().list().await.unwrap();
    assert_eq!(pools.len(), 1);
    assert_eq!(pools[0].name, "app-pool");
    assert_eq!(pools[0].start_ip, "10.0.1.1");
    assert_eq!(pools[0].active_cnode_ids, vec![1, 2]);
}

// ---------------------------------------------------------------------------
// Config / builder
// ---------------------------------------------------------------------------

#[test]
fn config_from_builder_requires_address() {
    let err = VastClient::builder().token("x").build().unwrap_err();
    assert!(
        err.to_string().contains("address"),
        "should mention missing address: {err}"
    );
}

#[test]
fn config_from_builder_requires_auth() {
    let err = VastClient::builder()
        .address("vms.example.com")
        .build()
        .unwrap_err();
    assert!(
        err.to_string().contains("authentication"),
        "should mention missing auth: {err}"
    );
}

#[test]
fn config_normalises_address_without_scheme() {
    let client = VastClient::builder()
        .address("vms.example.com")
        .token("tok")
        .build();
    assert!(client.is_ok());
}

// ===========================================================================
// Tenants
// ===========================================================================

#[tokio::test]
async fn test_tenants_list_deserialises() {
    let (server, client) = setup("tok").await;

    Mock::given(method("GET"))
        .and(path("/api/tenants/"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([{
            "id": 1,
            "guid": "tttt-1111",
            "name": "acme",
            "title": "acme",
            "url": "https://vms/api/tenants/1/",
            "cluster": "mycluster",
            "cluster_id": 1,
            "is_default": false,
            "enabled": true,
            "vms_root_no_tenant_access": false,
            "s3_root_no_tenant_access": false,
            "sync": "SYNCED",
            "sync_time": "2024-01-01T00:00:00Z",
            "views_count": 0,
            "users_count": 0,
            "ssd_pools": [],
            "object_stores": [],
            "leaders": [],
            "encryption_crn": null
        }])))
        .mount(&server)
        .await;

    let tenants = client.tenants().list().await.unwrap();
    assert_eq!(tenants.len(), 1);
    assert_eq!(tenants[0].name, "acme");
    assert_eq!(tenants[0].id, 1);
    assert!(!tenants[0].is_default);
}

#[tokio::test]
async fn test_tenants_create() {
    use vast_rs::api::tenants::CreateTenant;

    let (server, client) = setup("tok").await;

    Mock::given(method("POST"))
        .and(path("/api/tenants/"))
        .and(body_json(json!({
            "name": "new-tenant"
        })))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!({
            "id": 2,
            "guid": "tttt-2222",
            "name": "new-tenant",
            "title": "new-tenant",
            "url": "https://vms/api/tenants/2/",
            "cluster": "mycluster",
            "cluster_id": 1,
            "is_default": false,
            "enabled": true,
            "vms_root_no_tenant_access": false,
            "s3_root_no_tenant_access": false,
            "sync": "SYNCED",
            "sync_time": "2024-01-01T00:00:00Z",
            "views_count": 0,
            "users_count": 0,
            "ssd_pools": [],
            "object_stores": [],
            "leaders": [],
            "encryption_crn": null
        })))
        .mount(&server)
        .await;

    let tenant = client
        .tenants()
        .create(&CreateTenant {
            name: "new-tenant".into(),
            vms_root_no_tenant_access: None,
            s3_root_no_tenant_access: None,
        })
        .await
        .unwrap();

    assert_eq!(tenant.id, 2);
    assert_eq!(tenant.name, "new-tenant");
    assert!(tenant.encryption_crn.is_none());
}

#[tokio::test]
async fn test_tenants_delete() {
    let (server, client) = setup("tok").await;

    Mock::given(method("DELETE"))
        .and(path("/api/tenants/1/"))
        .respond_with(ResponseTemplate::new(204))
        .mount(&server)
        .await;

    client.tenants().delete(1).await.unwrap();
}
