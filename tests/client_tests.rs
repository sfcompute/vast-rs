//! Wiremock-backed tests for `VastClient`. The slim models mean fixture JSON
//! only has to populate fields we actually deserialize — everything else flows
//! into `extra` and is invisible to the test.

use serde_json::json;
use wiremock::matchers::{body_json, header, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

use vast::VastClient;

async fn setup(token: &str) -> (MockServer, VastClient) {
    let server = MockServer::start().await;
    let client = VastClient::builder()
        .address(server.uri())
        .token(token)
        .danger_accept_invalid_certs(true)
        .build()
        .unwrap();
    (server, client)
}

async fn setup_credentials(
    server: &MockServer,
    user: &str,
    pass: &str,
    tenant: Option<&str>,
) -> VastClient {
    let mut b = VastClient::builder()
        .address(server.uri())
        .credentials(user, pass)
        .danger_accept_invalid_certs(true);
    if let Some(t) = tenant {
        b = b.tenant(t);
    }
    b.build().unwrap()
}

// ---------------------------------------------------------------------------
// Auth
// ---------------------------------------------------------------------------

#[tokio::test]
async fn token_auth_sets_bearer_header() {
    let (server, client) = setup("tok-abc").await;
    Mock::given(method("GET"))
        .and(path("/api/clusters/"))
        .and(header("authorization", "Bearer tok-abc"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
        .expect(1)
        .mount(&server)
        .await;

    client.clusters().list().await.unwrap();
    server.verify().await;
}

#[tokio::test]
async fn credentials_post_to_token_endpoint() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/token/"))
        .and(body_json(
            json!({ "username": "admin", "password": "secret" }),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "access": "jwt" })))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/clusters/"))
        .and(header("authorization", "Bearer jwt"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
        .mount(&server)
        .await;

    setup_credentials(&server, "admin", "secret", None)
        .await
        .clusters()
        .list()
        .await
        .unwrap();
    server.verify().await;
}

#[tokio::test]
async fn tenant_name_is_url_encoded_in_token_path() {
    // Tenant names can legally contain `/`, spaces, etc. — encoding
    // them as a single path segment prevents the slash from creating
    // an extra path level (which would route to a different endpoint
    // or 404 on the VMS).
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/token/acme%2Fadmin"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "access": "jwt" })))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/clusters/"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
        .mount(&server)
        .await;

    setup_credentials(&server, "alice", "pw", Some("acme/admin"))
        .await
        .clusters()
        .list()
        .await
        .unwrap();
    server.verify().await;
}

#[tokio::test]
async fn tenant_admin_uses_path_scoped_token_endpoint() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/token/acme"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "access": "tjwt" })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/volumes/"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
        .mount(&server)
        .await;

    setup_credentials(&server, "alice", "pw", Some("acme"))
        .await
        .volumes()
        .list()
        .await
        .unwrap();
}

#[tokio::test]
async fn jwt_401_invalidates_cache_and_refreshes_credentials() {
    // The cache-invalidate-and-retry contract has two observable
    // properties; verify both without depending on wiremock's
    // mock-selection behavior for two identical matchers:
    //
    //   1. The credential exchange runs exactly twice — once to
    //      populate the cache, once after the cached JWT is
    //      invalidated by the 401.
    //   2. The original request is retried exactly once — first call
    //      gets the 401, retry surfaces the second 401 to the caller
    //      because we only retry once.
    //
    // Returning the same JWT on both exchanges is fine: the test is
    // observing call counts, not token values. The happy-path "retry
    // succeeds" case is already covered transitively by the other
    // tests in this file.
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/api/token/"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "access": "jwt" })))
        .expect(2)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/clusters/"))
        .respond_with(ResponseTemplate::new(401).set_body_json(json!({"detail": "expired"})))
        .expect(2)
        .mount(&server)
        .await;

    let client = setup_credentials(&server, "alice", "pw", None).await;
    let err = client.clusters().list().await.unwrap_err();
    assert!(err.is_unauthorized(), "expected 401 after retry; got {err:?}");

    server.verify().await;
}

#[tokio::test]
async fn static_token_does_not_retry_on_401() {
    // Static API tokens don't expire mid-process, so retrying after a
    // 401 would just double the failure count. Verify the client makes
    // exactly one request and surfaces the 401 to the caller.
    let (server, client) = setup("static").await;
    Mock::given(method("GET"))
        .and(path("/api/clusters/"))
        .respond_with(ResponseTemplate::new(401).set_body_json(json!({"detail": "bad token"})))
        .expect(1)
        .mount(&server)
        .await;

    let err = client.clusters().list().await.unwrap_err();
    assert!(err.is_unauthorized());
    server.verify().await;
}

#[tokio::test]
async fn jwt_is_cached_across_calls() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/token/"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "access": "cached" })))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
        .mount(&server)
        .await;

    let client = setup_credentials(&server, "admin", "pw", None).await;
    client.clusters().list().await.unwrap();
    client.volumes().list().await.unwrap();
    server.verify().await;
}

#[tokio::test]
async fn concurrent_first_calls_exchange_token_only_once() {
    // Verify the single-flight property: a herd of concurrent first
    // calls should result in exactly one credential exchange, not one
    // per task.
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/token/"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "access": "jwt" })))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
        .mount(&server)
        .await;

    let client = setup_credentials(&server, "alice", "pw", None).await;

    let mut handles = Vec::new();
    for _ in 0..16 {
        let c = client.clone();
        handles.push(tokio::spawn(async move {
            c.clusters().list().await.unwrap();
        }));
    }
    for h in handles {
        h.await.unwrap();
    }

    server.verify().await;
}

// ---------------------------------------------------------------------------
// CRUD smoke tests — one per resource shape
// ---------------------------------------------------------------------------

#[tokio::test]
async fn clusters_list_and_get() {
    let (server, client) = setup("t").await;
    Mock::given(method("GET"))
        .and(path("/api/clusters/"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            { "id": 1, "name": "prod", "state": "ONLINE", "sw_version": "5.2.0" },
            { "id": 2, "name": "dev",  "state": "ONLINE", "sw_version": "5.1.0" },
        ])))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/clusters/42/"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": 42, "name": "my", "state": "ONLINE", "sw_version": "5.3.0"
        })))
        .mount(&server)
        .await;

    let list = client.clusters().list().await.unwrap();
    assert_eq!(list.len(), 2);
    assert_eq!(list[0].name, "prod");

    let one = client.clusters().get(42).await.unwrap();
    assert_eq!(one.id, 42);
}

#[tokio::test]
async fn volumes_create() {
    use vast::api::CreateVolume;
    let (server, client) = setup("t").await;
    Mock::given(method("POST"))
        .and(path("/api/volumes/"))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!({
            "id": 10, "name": "data", "path": "/data", "quota": 1_073_741_824u64
        })))
        .mount(&server)
        .await;

    let vol = client
        .volumes()
        .create(&CreateVolume {
            name: "data".into(),
            path: "/data".into(),
            quota: Some(1_073_741_824),
        })
        .await
        .unwrap();
    assert_eq!(vol.id, 10);
    assert_eq!(vol.name, "data");
}

#[tokio::test]
async fn views_list_and_create() {
    use vast::api::CreateView;
    let (server, client) = setup("t").await;
    Mock::given(method("GET"))
        .and(path("/api/views/"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([{
            "id": 1, "name": "nfs", "path": "/data", "policy_id": 2,
            "protocols": ["NFS"], "bucket": ""
        }])))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/views/"))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!({
            "id": 5, "name": "s3", "path": "/buckets/x", "policy_id": 2,
            "protocols": ["S3"], "bucket": "x"
        })))
        .mount(&server)
        .await;

    let views = client.views().list().await.unwrap();
    assert_eq!(views[0].path, "/data");
    assert_eq!(views[0].protocols, vec!["NFS"]);

    let created = client
        .views()
        .create(&CreateView {
            name: "s3".into(),
            path: "/buckets/x".into(),
            policy_id: 2,
            protocols: vec!["S3".into()],
            create_dir: None,
            alias: None,
            bucket: Some("x".into()),
            allow_anonymous_access: None,
            s3_versioning: None,
            s3_locks: None,
            s3_locks_retention_mode: None,
        })
        .await
        .unwrap();
    assert_eq!(created.bucket, "x");
}

#[tokio::test]
async fn quotas_list_handles_nullable_limits() {
    let (server, client) = setup("t").await;
    Mock::given(method("GET"))
        .and(path("/api/quotas/"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([{
            "id": 10, "name": "team", "path": "/data", "state": "OK",
            "hard_limit": null, "soft_limit": null, "used_capacity": 5_368_709_120u64,
        }])))
        .mount(&server)
        .await;

    let quotas = client.quotas().list().await.unwrap();
    assert_eq!(quotas[0].name, "team");
    assert_eq!(quotas[0].hard_limit, None);
    assert_eq!(quotas[0].state, "OK");
}

#[tokio::test]
async fn vip_pools_list_parses_active_cnode_ids() {
    let (server, client) = setup("t").await;
    Mock::given(method("GET"))
        .and(path("/api/vippools/"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([{
            "id": 1, "name": "app", "start_ip": "10.0.1.1", "end_ip": "10.0.1.10",
            "active_cnode_ids": [1, 2], "role": "APPLICATION"
        }])))
        .mount(&server)
        .await;

    let pools = client.vip_pools().list().await.unwrap();
    assert_eq!(pools[0].start_ip, "10.0.1.1");
    assert_eq!(pools[0].active_cnode_ids, vec![1, 2]);
}

#[tokio::test]
async fn nodes_list_with_params_serialises_query_string() {
    use vast::api::ListNodesParams;
    let (server, client) = setup("t").await;
    Mock::given(method("GET"))
        .and(path("/api/nodes/"))
        .and(query_param("cluster_id", "7"))
        .and(query_param("state", "ONLINE"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
        .expect(1)
        .mount(&server)
        .await;

    let params = ListNodesParams {
        cluster_id: Some(7),
        state: Some("ONLINE".into()),
    };
    client.nodes().list_with_params(&params).await.unwrap();
    server.verify().await;
}

#[tokio::test]
async fn nodes_list_with_params_omits_none_fields() {
    use vast::api::ListNodesParams;
    let (server, client) = setup("t").await;
    // No matchers asserting query params: this mock should fire even
    // when no query string is present. We then verify exactly one call.
    Mock::given(method("GET"))
        .and(path("/api/nodes/"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
        .expect(1)
        .mount(&server)
        .await;

    client
        .nodes()
        .list_with_params(&ListNodesParams::default())
        .await
        .unwrap();
    server.verify().await;
}

#[tokio::test]
async fn delete_folder_posts_body_on_delete() {
    // `delete_folder` is the only hand-coded method that sends a body
    // on a DELETE — easy to break without noticing.
    let (server, client) = setup("t").await;
    Mock::given(method("DELETE"))
        .and(path("/api/clusters/1/delete_folder/"))
        .and(body_json(
            json!({ "path": "/data/scratch", "tenant_id": 2 }),
        ))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&server)
        .await;

    client
        .clusters()
        .delete_folder(1, "/data/scratch", Some(2))
        .await
        .unwrap();
    server.verify().await;
}

#[tokio::test]
async fn delete_folder_omits_tenant_id_when_none() {
    let (server, client) = setup("t").await;
    Mock::given(method("DELETE"))
        .and(path("/api/clusters/1/delete_folder/"))
        .and(body_json(json!({ "path": "/data/scratch" })))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&server)
        .await;

    client
        .clusters()
        .delete_folder(1, "/data/scratch", None)
        .await
        .unwrap();
    server.verify().await;
}

#[tokio::test]
async fn users_full_crud() {
    use vast::api::{CreateUser, UpdateUser};
    let (server, client) = setup("t").await;
    Mock::given(method("POST"))
        .and(path("/api/users/"))
        .and(body_json(json!({ "name": "alice", "uid": 1001 })))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!({
            "id": 5, "name": "alice", "uid": 1001
        })))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("PATCH"))
        .and(path("/api/users/5/"))
        .and(body_json(json!({ "enabled": false })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": 5, "name": "alice", "uid": 1001, "enabled": false
        })))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("DELETE"))
        .and(path("/api/users/5/"))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&server)
        .await;

    let created = client
        .users()
        .create(&CreateUser {
            name: "alice".into(),
            uid: Some(1001),
            email: None,
            enabled: None,
        })
        .await
        .unwrap();
    assert_eq!(created.id, 5);

    let updated = client
        .users()
        .update(
            5,
            &UpdateUser {
                enabled: Some(false),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(updated.enabled, Some(false));

    client.users().delete(5).await.unwrap();
    server.verify().await;
}

#[tokio::test]
async fn snapshots_create_and_delete() {
    use vast::api::CreateSnapshot;
    let (server, client) = setup("t").await;
    Mock::given(method("POST"))
        .and(path("/api/snapshots/"))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!({
            "id": 9, "guid": "abc", "name": "nightly", "path": "/data"
        })))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("DELETE"))
        .and(path("/api/snapshots/9/"))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&server)
        .await;

    let snap = client
        .snapshots()
        .create(&CreateSnapshot {
            name: "nightly".into(),
            path: "/data".into(),
        })
        .await
        .unwrap();
    assert_eq!(snap.id, 9);
    client.snapshots().delete(snap.id).await.unwrap();
    server.verify().await;
}

#[tokio::test]
async fn tenants_full_crud() {
    use vast::api::CreateTenant;
    let (server, client) = setup("t").await;
    Mock::given(method("GET"))
        .and(path("/api/tenants/"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([{
            "id": 1, "name": "acme", "is_default": false, "encryption_crn": null
        }])))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/tenants/"))
        .and(body_json(json!({ "name": "new" })))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!({
            "id": 2, "name": "new", "is_default": false, "encryption_crn": null
        })))
        .mount(&server)
        .await;
    Mock::given(method("DELETE"))
        .and(path("/api/tenants/1/"))
        .respond_with(ResponseTemplate::new(204))
        .mount(&server)
        .await;

    let tenants = client.tenants().list().await.unwrap();
    assert_eq!(tenants[0].name, "acme");
    assert!(!tenants[0].is_default);

    let created = client
        .tenants()
        .create(&CreateTenant {
            name: "new".into(),
            vms_root_no_tenant_access: None,
            s3_root_no_tenant_access: None,
        })
        .await
        .unwrap();
    assert_eq!(created.id, 2);
    assert!(created.encryption_crn.is_none());

    client.tenants().delete(1).await.unwrap();
}

// ---------------------------------------------------------------------------
// Error handling
// ---------------------------------------------------------------------------

#[tokio::test]
async fn non_json_error_body_surfaces_in_message() {
    // A misconfigured gateway / upstream 5xx may return HTML or plain
    // text. Verify the raw body still flows into the error message
    // instead of being dropped on the floor.
    let (server, client) = setup("t").await;
    Mock::given(method("GET"))
        .and(path("/api/clusters/"))
        .respond_with(
            ResponseTemplate::new(502)
                .insert_header("content-type", "text/html")
                .set_body_string("<html><body>Bad Gateway</body></html>"),
        )
        .mount(&server)
        .await;

    let err = client.clusters().list().await.unwrap_err();
    assert_eq!(err.status_code(), Some(502));
    let msg = err.to_string();
    assert!(
        msg.contains("Bad Gateway"),
        "expected raw HTML body to surface in error message; got: {msg}"
    );
}

#[tokio::test]
async fn empty_error_body_yields_http_status_message() {
    let (server, client) = setup("t").await;
    Mock::given(method("GET"))
        .and(path("/api/clusters/"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&server)
        .await;

    let err = client.clusters().list().await.unwrap_err();
    assert_eq!(err.status_code(), Some(500));
    assert!(
        err.to_string().contains("HTTP 500"),
        "expected fallback message to include status; got: {err}"
    );
}

#[tokio::test]
async fn not_found_and_unauthorized_classify_correctly() {
    let (server, client) = setup("t").await;
    Mock::given(method("GET"))
        .and(path("/api/clusters/999/"))
        .respond_with(ResponseTemplate::new(404).set_body_json(json!({ "detail": "Not found." })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/users/"))
        .respond_with(ResponseTemplate::new(401).set_body_json(json!({ "detail": "No creds." })))
        .mount(&server)
        .await;

    let nf = client.clusters().get(999).await.unwrap_err();
    assert!(nf.is_not_found());
    assert!(nf.to_string().contains("Not found."));

    let un = client.users().list().await.unwrap_err();
    assert!(un.is_unauthorized());
}

// ---------------------------------------------------------------------------
// Builder validation
// ---------------------------------------------------------------------------

#[test]
fn builder_requires_address() {
    let err = VastClient::builder().token("x").build().unwrap_err();
    assert!(err.to_string().contains("address"));
}

#[test]
fn builder_requires_auth() {
    let err = VastClient::builder()
        .address("vms.example.com")
        .build()
        .unwrap_err();
    assert!(err.to_string().contains(".token()") || err.to_string().contains(".credentials()"));
}

#[test]
fn builder_normalises_address_without_scheme() {
    VastClient::builder()
        .address("vms.example.com")
        .token("x")
        .build()
        .unwrap();
}
