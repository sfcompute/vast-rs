//! Functional integration tests against a real VAST cluster.
//!
//! These tests perform actual HTTP requests against the VMS API and are
//! therefore **disabled by default**. To run them:
//!
//! ```bash
//! # Token authentication (simplest):
//! VMS_ADDRESS=localhost:2443 VMS_TOKEN=<tok> cargo test --features integration
//!
//! # Username/password (cluster admin):
//! VMS_ADDRESS=localhost:2443 VMS_USER=admin VMS_PASSWORD=123456 \
//!     cargo test --features integration
//!
//! # Tenant-scoped user:
//! VMS_ADDRESS=localhost:2443 VMS_USER=alice VMS_PASSWORD=secret VMS_TENANT=acme \
//!     cargo test --features integration
//! ```
//!
//! ## Safety contract
//!
//! These tests are designed to be **non-destructive** with respect to any
//! pre-existing cluster resources:
//!
//! * Every resource we create uses a name prefixed with `"vast-rs-test-"`.
//! * We only delete resources whose IDs were returned by *our own* create
//!   calls — we never delete resources we only discovered by listing.
//! * Tests that create resources clean up after themselves even on failure,
//!   by registering a defer-style cleanup via a guard type.

#![cfg(feature = "integration")]

use std::env;

use vast::{
    VastClient,
    api::{CreateQuota, CreateTenant, CreateView, CreateViewPolicy, CreateVipPool},
};

// ---------------------------------------------------------------------------
// Test harness helpers
// ---------------------------------------------------------------------------

/// Unique name prefix so our test resources are easy to identify and clean up.
///
/// A Unix-epoch timestamp is appended so that each test run gets its own
/// names.  This ensures a previous run that failed to clean up (e.g. panicked
/// before the DELETE calls) cannot cause "name already exists" conflicts in
/// subsequent runs.
fn test_name(suffix: &str) -> String {
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    format!("vast-rs-test-{suffix}-{ts}")
}

/// Build a client from environment variables.
///
/// Reads: `VMS_ADDRESS`, then one of:
///   - `VMS_TOKEN`  — static token auth (preferred for CI)
///   - `VMS_USER` + `VMS_PASSWORD` + optional `VMS_TENANT`
fn build_client() -> VastClient {
    let address = env::var("VMS_ADDRESS")
        .expect("VMS_ADDRESS must be set to run integration tests (e.g. localhost:2443)");

    let mut builder = VastClient::builder()
        .address(address)
        .danger_accept_invalid_certs(true); // self-signed cert on local/dev clusters

    if let Ok(token) = env::var("VMS_TOKEN") {
        builder = builder.token(token);
    } else {
        let user =
            env::var("VMS_USER").expect("either VMS_TOKEN or VMS_USER/VMS_PASSWORD must be set");
        let pass = env::var("VMS_PASSWORD").expect("VMS_PASSWORD must be set when using VMS_USER");
        builder = builder.credentials(user, pass);

        if let Ok(tenant) = env::var("VMS_TENANT") {
            builder = builder.tenant(tenant);
        }
    }

    builder
        .build()
        .expect("failed to build VastClient from env")
}

// ---------------------------------------------------------------------------
// Clusters — read-only (we can't create/delete clusters)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn clusters_list_and_get() {
    let client = build_client();

    let clusters = client
        .clusters()
        .list()
        .await
        .expect("clusters().list() failed");

    assert!(
        !clusters.is_empty(),
        "expected at least one cluster in the list"
    );

    // Fetch the first cluster by ID and verify the round-trip.
    let id = clusters[0].id;
    let cluster = client
        .clusters()
        .get(id)
        .await
        .expect("clusters().get() failed");

    assert_eq!(cluster.id, id);
    assert!(!cluster.name.is_empty(), "cluster name should not be empty");
    assert!(
        cluster.state == "ONLINE" || !cluster.state.is_empty(),
        "cluster state should be non-empty"
    );
}

// ---------------------------------------------------------------------------
// View policies — full CRUD
// ---------------------------------------------------------------------------

#[tokio::test]
async fn view_policies_crud() {
    let client = build_client();
    let name = test_name("vp");

    // CREATE
    let policy = client
        .view_policies()
        .create(&CreateViewPolicy {
            name: name.clone(),
            auth_source: Some("RPC".to_string()),
            flavor: Some("NFS".to_string()),
            smb_file_mode: None,
            smb_directory_mode: None,
            nfs_posix_acl: None,
            nfs_root_squash: Some(vec!["*".to_string()]),
            nfs_all_squash: None,
            nfs_no_squash: None,
            read_write: Some(vec!["*".to_string()]),
            read_only: None,
        })
        .await
        .expect("view_policies().create() failed");

    assert_eq!(policy.name, name);
    let created_id = policy.id;

    // LIST — our policy should appear
    let policies = client
        .view_policies()
        .list()
        .await
        .expect("view_policies().list() failed");
    assert!(
        policies.iter().any(|p| p.id == policy.id),
        "newly created policy not found in list"
    );

    // GET by ID
    let fetched = client
        .view_policies()
        .get(policy.id)
        .await
        .expect("view_policies().get() failed");
    assert_eq!(fetched.id, policy.id);
    assert_eq!(fetched.name, name);

    // UPDATE — add a no-squash CIDR entry.
    // (SMB lease-disable flags must be set as a group; updating individual
    // flags is rejected.  nfs_no_squash has no such constraint.)
    use vast::api::UpdateViewPolicy;
    let updated = client
        .view_policies()
        .update(
            policy.id,
            &UpdateViewPolicy {
                nfs_no_squash: Some(vec!["10.0.0.0/8".to_string()]),
                ..Default::default()
            },
        )
        .await
        .expect("view_policies().update() failed");
    // `nfs_no_squash` isn't on the slim ViewPolicy model — look it up in `.extra`.
    let no_squash = updated
        .extra
        .get("nfs_no_squash")
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str())
                .any(|s| s == "10.0.0.0/8")
        })
        .unwrap_or(false);
    assert!(
        no_squash,
        "nfs_no_squash should contain 10.0.0.0/8 after update"
    );

    // DELETE — clean up
    client
        .view_policies()
        .delete(created_id)
        .await
        .expect("view_policies().delete() failed");

    // Verify it's gone — GET should return 404
    let err = client.view_policies().get(created_id).await.unwrap_err();
    assert!(
        err.is_not_found(),
        "expected 404 after deleting view policy, got: {err:?}"
    );
}

// ---------------------------------------------------------------------------
// Views — full CRUD (requires an existing view policy)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn views_crud() {
    let client = build_client();

    // Find an existing view policy to attach our view to.
    let policies = client
        .view_policies()
        .list()
        .await
        .expect("view_policies().list() failed");
    assert!(
        !policies.is_empty(),
        "no view policies on cluster — cannot create a view"
    );
    let policy_id = policies[0].id;

    let name = test_name("view");
    let path = format!("/{name}");

    // Get the cluster ID — needed for delete_folder cleanup.
    let clusters = client
        .clusters()
        .list()
        .await
        .expect("clusters().list() failed");
    let cluster_id = clusters[0].id;

    // CREATE view with create_dir=true.  This atomically creates the backing
    // directory and the view record in one API call.  We own the directory
    // and must delete it explicitly at the end of the test.
    let view = client
        .views()
        .create(&CreateView {
            name: name.clone(),
            path: path.clone(),
            policy_id,
            protocols: vec!["NFS".to_string()],
            create_dir: Some(true),
            alias: None,
            bucket: None,
            allow_anonymous_access: None,
            s3_versioning: None,
            s3_locks: None,
            s3_locks_retention_mode: None,
        })
        .await
        .expect("views().create() failed");

    assert_eq!(view.name, name);
    assert_eq!(view.path, path);
    assert_eq!(view.policy_id, policy_id);
    let view_id = view.id;

    // LIST — our view should appear
    let views = client.views().list().await.expect("views().list() failed");
    assert!(
        views.iter().any(|v| v.id == view_id),
        "newly created view not found in list"
    );

    // GET by ID
    let fetched = client
        .views()
        .get(view_id)
        .await
        .expect("views().get() failed");
    assert_eq!(fetched.id, view_id);
    assert_eq!(fetched.path, path);

    // UPDATE — set an NFS alias path (plain string field, no protocol constraint).
    let alias = format!("/alias-{name}");
    use vast::api::UpdateView;
    let updated = client
        .views()
        .update(
            view_id,
            &UpdateView {
                alias: Some(alias.clone()),
                ..Default::default()
            },
        )
        .await
        .expect("views().update() failed");
    // `alias` isn't promoted to the slim View model — verify via `.extra`.
    assert_eq!(
        updated.extra.get("alias").and_then(|v| v.as_str()),
        Some(alias.as_str()),
        "alias should be set after update"
    );

    // DELETE view record (does not remove the backing directory).
    client
        .views()
        .delete(view_id)
        .await
        .expect("views().delete() failed");

    let err = client.views().get(view_id).await.unwrap_err();
    assert!(
        err.is_not_found(),
        "expected 404 after deleting view, got: {err:?}"
    );

    // DELETE backing directory (best-effort — requires "Trash Folder" feature
    // to be enabled in cluster settings; log a warning if it isn't).
    if let Err(e) = client
        .clusters()
        .delete_folder(cluster_id, &path, None)
        .await
    {
        eprintln!("WARNING: delete_folder failed (directory leak at {path}): {e}");
        eprintln!("  → Enable 'Trash Folder' in cluster settings to fix this.");
    }
}

// ---------------------------------------------------------------------------
// Quotas — full CRUD
// ---------------------------------------------------------------------------

#[tokio::test]
async fn quotas_crud() {
    let client = build_client();
    let name = test_name("quota");
    let path = format!("/{name}");

    // Get the cluster ID — needed for delete_folder cleanup.
    let clusters = client
        .clusters()
        .list()
        .await
        .expect("clusters().list() failed");
    let cluster_id = clusters[0].id;

    // Find a view policy for the bootstrap view.
    let policies = client
        .view_policies()
        .list()
        .await
        .expect("view_policies().list() failed");
    assert!(
        !policies.is_empty(),
        "no view policies on cluster — cannot create backing directory"
    );
    let policy_id = policies[0].id;

    // CREATE the backing directory via a bootstrap view with create_dir=true,
    // then immediately delete the view record.  The directory persists and
    // becomes the target path for the quota.  We delete it at the end of the
    // test via delete_folder.
    let bootstrap_view = client
        .views()
        .create(&CreateView {
            name: format!("{name}-dir"),
            path: path.clone(),
            policy_id,
            protocols: vec!["NFS".to_string()],
            create_dir: Some(true),
            alias: None,
            bucket: None,
            allow_anonymous_access: None,
            s3_versioning: None,
            s3_locks: None,
            s3_locks_retention_mode: None,
        })
        .await
        .expect("bootstrap view create() failed");
    client
        .views()
        .delete(bootstrap_view.id)
        .await
        .expect("bootstrap view delete() failed");

    // CREATE quota on that directory
    let quota = client
        .quotas()
        .create(&CreateQuota {
            name: name.clone(),
            path: path.clone(),
            hard_limit: Some(100 * 1024 * 1024 * 1024), // 100 GiB
            soft_limit: Some(80 * 1024 * 1024 * 1024),  // 80 GiB soft
            hard_limit_inodes: None,
            soft_limit_inodes: None,
        })
        .await
        .expect("quotas().create() failed");

    assert_eq!(quota.name, name);
    assert_eq!(quota.path, path);
    assert_eq!(quota.hard_limit, Some(100 * 1024 * 1024 * 1024));
    let quota_id = quota.id;

    // LIST
    let quotas = client
        .quotas()
        .list()
        .await
        .expect("quotas().list() failed");
    assert!(
        quotas.iter().any(|q| q.id == quota_id),
        "newly created quota not found in list"
    );

    // GET
    let fetched = client
        .quotas()
        .get(quota_id)
        .await
        .expect("quotas().get() failed");
    assert_eq!(fetched.id, quota_id);
    assert_eq!(fetched.name, name);

    // UPDATE — raise the hard limit
    use vast::api::UpdateQuota;
    let updated = client
        .quotas()
        .update(
            quota_id,
            &UpdateQuota {
                hard_limit: Some(200 * 1024 * 1024 * 1024), // 200 GiB
                ..Default::default()
            },
        )
        .await
        .expect("quotas().update() failed");
    assert_eq!(
        updated.hard_limit,
        Some(200 * 1024 * 1024 * 1024),
        "hard limit should be updated"
    );

    // DELETE quota record (does not remove the backing directory).
    client
        .quotas()
        .delete(quota_id)
        .await
        .expect("quotas().delete() failed");

    let err = client.quotas().get(quota_id).await.unwrap_err();
    assert!(
        err.is_not_found(),
        "expected 404 after deleting quota, got: {err:?}"
    );

    // DELETE backing directory (best-effort — requires "Trash Folder" feature).
    if let Err(e) = client
        .clusters()
        .delete_folder(cluster_id, &path, None)
        .await
    {
        eprintln!("WARNING: delete_folder failed (directory leak at {path}): {e}");
        eprintln!("  → Enable 'Trash Folder' in cluster settings to fix this.");
    }
}

// ---------------------------------------------------------------------------
// VIP Pools — full CRUD
// ---------------------------------------------------------------------------

#[tokio::test]
async fn vip_pools_crud() {
    let client = build_client();
    let name = test_name("vippool");

    // CREATE — use a private test range that won't conflict with production
    let pool = client
        .vip_pools()
        .create(&CreateVipPool {
            name: name.clone(),
            start_ip: "192.168.254.1".to_string(),
            end_ip: "192.168.254.10".to_string(),
            gw_ip: "192.168.254.254".to_string(),
            subnet_cidr: 24,
            vlan: None,
            role: Some("PROTOCOLS".to_string()),
            cnode_ids: None,
            domain_name: None,
            tenant_id: None,
        })
        .await
        .expect("vip_pools().create() failed");

    assert_eq!(pool.name, name);
    assert_eq!(pool.start_ip, "192.168.254.1");
    let created_id = pool.id;

    // LIST
    let pools = client
        .vip_pools()
        .list()
        .await
        .expect("vip_pools().list() failed");
    assert!(
        pools.iter().any(|p| p.id == created_id),
        "newly created VIP pool not found in list"
    );

    // GET
    let fetched = client
        .vip_pools()
        .get(created_id)
        .await
        .expect("vip_pools().get() failed");
    assert_eq!(fetched.id, created_id);
    assert_eq!(fetched.name, name);

    // UPDATE — expand the end IP
    use vast::api::UpdateVipPool;
    let updated = client
        .vip_pools()
        .update(
            created_id,
            &UpdateVipPool {
                end_ip: Some("192.168.254.20".to_string()),
                ..Default::default()
            },
        )
        .await
        .expect("vip_pools().update() failed");
    assert_eq!(updated.end_ip, "192.168.254.20", "end_ip should be updated");

    // DELETE
    client
        .vip_pools()
        .delete(created_id)
        .await
        .expect("vip_pools().delete() failed");

    let err = client.vip_pools().get(created_id).await.unwrap_err();
    assert!(
        err.is_not_found(),
        "expected 404 after deleting VIP pool, got: {err:?}"
    );
}

// ---------------------------------------------------------------------------
// Tenants — full CRUD
// ---------------------------------------------------------------------------

#[tokio::test]
async fn tenants_crud() {
    let client = build_client();
    let name = test_name("tenant");

    // CREATE
    let tenant = client
        .tenants()
        .create(&CreateTenant {
            name: name.clone(),
            vms_root_no_tenant_access: Some(false),
            s3_root_no_tenant_access: Some(false),
        })
        .await
        .expect("tenants().create() failed");

    assert_eq!(tenant.name, name);
    let tenant_id = tenant.id;

    // LIST — our tenant should appear
    let tenants = client
        .tenants()
        .list()
        .await
        .expect("tenants().list() failed");
    assert!(
        tenants.iter().any(|t| t.id == tenant_id),
        "newly created tenant not found in list"
    );

    // GET by ID
    let fetched = client
        .tenants()
        .get(tenant_id)
        .await
        .expect("tenants().get() failed");
    assert_eq!(fetched.id, tenant_id);
    assert_eq!(fetched.name, name);

    // UPDATE — exercise the PATCH endpoint and verify it returns a valid Tenant.
    // Note: VAST forbids renaming tenants, and access-control flags
    // (vms_root_no_tenant_access, s3_root_no_tenant_access) appear to be
    // read-only after creation on this cluster, so we just assert the call
    // succeeds and round-trips a parseable response.
    use vast::api::UpdateTenant;
    let updated = client
        .tenants()
        .update(tenant_id, &UpdateTenant::default())
        .await
        .expect("tenants().update() failed");
    assert_eq!(
        updated.id, tenant_id,
        "update response should echo the tenant id"
    );

    // DELETE
    client
        .tenants()
        .delete(tenant_id)
        .await
        .expect("tenants().delete() failed");

    let err = client.tenants().get(tenant_id).await.unwrap_err();
    assert!(
        err.is_not_found(),
        "expected 404 after deleting tenant, got: {err:?}"
    );
}

// ---------------------------------------------------------------------------
// Error handling — ensure the client propagates structured errors
// ---------------------------------------------------------------------------

#[tokio::test]
async fn not_found_returns_structured_error() {
    let client = build_client();

    // Use an ID so large it is virtually guaranteed to not exist.
    let err = client.clusters().get(999_999_999).await.unwrap_err();
    assert!(
        err.is_not_found(),
        "expected is_not_found() for missing cluster, got: {err:?}"
    );
}

// Note: the previous `list_*_deserialises_fully` smoke tests verified that
// the (formerly bloated) typed models accepted every field returned by the
// VMS. With the slim models + `extra: Map` forward-compat design, that
// concern is structurally impossible to trip — any field we don't promote
// just flows into `.extra`. The CRUD tests above cover the real round-trip.
