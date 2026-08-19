//! Wiremock-backed tests for `VastClient`. The slim models mean fixture JSON
//! only has to populate fields we actually deserialize — everything else flows
//! into `extra` and is invisible to the test.

use std::time::Duration;

use serde_json::json;
use wiremock::matchers::{body_json, header, method, path, query_param, query_param_is_missing};
use wiremock::{Mock, MockServer, ResponseTemplate};

use vast::VastClient;

/// Build a client with a custom retry budget and a near-zero backoff
/// so retry-loop tests don't sit on real wall-clock sleeps.
async fn setup_with_retry(server: &MockServer, token: &str, max_attempts: u32) -> VastClient {
    VastClient::builder()
        .address(server.uri())
        .token(token)
        .max_attempts(max_attempts)
        .retry_backoff(Duration::from_millis(1))
        .build()
        .unwrap()
}

async fn setup(token: &str) -> (MockServer, VastClient) {
    let server = MockServer::start().await;
    let client = VastClient::builder()
        .address(server.uri())
        .token(token)
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
async fn jwt_401_refreshes_credentials_and_replays_once() {
    // The refresh-and-replay contract has two observable
    // properties; verify both without depending on wiremock's
    // mock-selection behavior for two identical matchers:
    //
    //   1. The credential exchange runs exactly twice — once to
    //      populate the cache, once to replace the JWT the 401
    //      rejected.
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
    assert!(
        err.is_unauthorized(),
        "expected 401 after retry; got {err:?}"
    );

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

/// Build a credential-auth client with a real retry budget and a near-zero
/// backoff, so the retry-path tests don't sit on wall-clock sleeps.
async fn setup_credentials_with_retry(server: &MockServer, max_attempts: u32) -> VastClient {
    VastClient::builder()
        .address(server.uri())
        .credentials("alice", "pw")
        .danger_accept_invalid_certs(true)
        .max_attempts(max_attempts)
        .retry_backoff(Duration::from_millis(1))
        .build()
        .unwrap()
}

/// Mount a token endpoint that hands out `first` once and `rest`
/// thereafter, so a test can tell the pre- and post-refresh JWT apart.
///
/// `delay` widens the window during which an exchange is in flight — the
/// window concurrent 401s have to pile into.
async fn mount_rotating_token(server: &MockServer, first: &str, rest: &str, delay: Duration) {
    Mock::given(method("POST"))
        .and(path("/api/token/"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({ "access": first }))
                .set_delay(delay),
        )
        .up_to_n_times(1)
        .with_priority(1)
        .mount(server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/token/"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({ "access": rest }))
                .set_delay(delay),
        )
        .with_priority(2)
        .mount(server)
        .await;
}

async fn token_request_count(server: &MockServer) -> usize {
    server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .filter(|r| r.url.path() == "/api/token/")
        .count()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_401s_trigger_a_single_credential_exchange() {
    // A token expiring under concurrent load 401s every in-flight
    // request. Each of those tasks independently decides to refresh, and
    // their 401s arrive spread over time rather than all at once — so a
    // task that 401s late must NOT discard the token an earlier task
    // already fetched. Without generation tracking, every task in the
    // refresh window ran its own full credential exchange; the contract
    // is exactly two — one initial, one refresh — no matter how many
    // requests are in flight.
    //
    // The token endpoint is deliberately slow so all the staggered tasks
    // land inside the refresh window, which is what makes the assertion
    // sensitive rather than timing-dependent.
    let server = MockServer::start().await;
    mount_rotating_token(&server, "jwt-stale", "jwt-fresh", Duration::from_millis(50)).await;

    // The pre-refresh JWT is rejected; the post-refresh one works. This
    // models a real expiry, where the refreshed token actually resolves
    // the 401 — so any exchange beyond the second is pure waste.
    Mock::given(method("GET"))
        .and(path("/api/clusters/"))
        .and(header("authorization", "Bearer jwt-stale"))
        .respond_with(ResponseTemplate::new(401).set_body_json(json!({"detail": "expired"})))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/clusters/"))
        .and(header("authorization", "Bearer jwt-fresh"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
        .mount(&server)
        .await;

    let client = setup_credentials_with_retry(&server, 3).await;

    // Stagger the starts so the 401s land in separate windows — the
    // interleaving that defeats a plain double-checked lock.
    let mut set = tokio::task::JoinSet::new();
    for i in 0..16u64 {
        let c = client.clone();
        set.spawn(async move {
            tokio::time::sleep(Duration::from_millis(2 * i)).await;
            c.clusters().list().await
        });
    }
    while let Some(joined) = set.join_next().await {
        joined
            .unwrap()
            .expect("every request should succeed after one refresh");
    }

    let exchanges = token_request_count(&server).await;
    assert_eq!(
        exchanges, 2,
        "expected exactly one initial exchange + one refresh; got {exchanges} \
         (a count near the request total means refresh is not deduplicated)"
    );
}

#[tokio::test]
async fn credential_exchange_retries_transient_failure() {
    // A fleet of replicas sharing one service account can draw a 429 from
    // the token endpoint. That's transient, so it must be retried rather
    // than surfaced as a hard auth failure.
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/token/"))
        .respond_with(ResponseTemplate::new(429).set_body_json(json!({"detail": "throttled"})))
        .up_to_n_times(1)
        .with_priority(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/token/"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "access": "jwt" })))
        .with_priority(2)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/clusters/"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
        .mount(&server)
        .await;

    setup_credentials_with_retry(&server, 3)
        .await
        .clusters()
        .list()
        .await
        .expect("throttled token request should be retried");

    assert_eq!(token_request_count(&server).await, 2);
}

#[tokio::test]
async fn credential_exchange_does_not_retry_rejected_credentials() {
    // A wrong password is terminal. Retrying it would burn attempts
    // against whatever lockout policy the VMS enforces on the account,
    // so a 401 from the token endpoint must fail on the first try.
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/token/"))
        .respond_with(ResponseTemplate::new(401).set_body_json(json!({"detail": "no such user"})))
        .expect(1)
        .mount(&server)
        .await;

    let err = setup_credentials_with_retry(&server, 3)
        .await
        .clusters()
        .list()
        .await
        .unwrap_err();

    assert!(
        matches!(err, vast::Error::Auth(ref m) if m.contains("401")),
        "expected a terminal auth error; got {err:?}"
    );
    server.verify().await;
}

#[tokio::test]
async fn post_401_replay_retries_transient_failure() {
    // The replay that follows a refresh gets the same retry budget as any
    // other GET. Previously it was a single bare attempt, so a 5xx or 429
    // landing on the replay became a hard error for the caller.
    let server = MockServer::start().await;
    mount_rotating_token(&server, "jwt-stale", "jwt-fresh", Duration::ZERO).await;

    Mock::given(method("GET"))
        .and(path("/api/clusters/"))
        .and(header("authorization", "Bearer jwt-stale"))
        .respond_with(ResponseTemplate::new(401).set_body_json(json!({"detail": "expired"})))
        .mount(&server)
        .await;
    // The replay's first attempt hits a blip, its second succeeds.
    Mock::given(method("GET"))
        .and(path("/api/clusters/"))
        .and(header("authorization", "Bearer jwt-fresh"))
        .respond_with(ResponseTemplate::new(503))
        .up_to_n_times(1)
        .with_priority(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/clusters/"))
        .and(header("authorization", "Bearer jwt-fresh"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
        .with_priority(2)
        .mount(&server)
        .await;

    setup_credentials_with_retry(&server, 3)
        .await
        .clusters()
        .list()
        .await
        .expect("replay should retry through a transient 503");
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
            tenant_id: None,
            bucket_owner: None,
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
        ..Default::default()
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
async fn views_list_with_params_filters_by_path_and_bucket() {
    use vast::api::ListViewsParams;
    let (server, client) = setup("t").await;
    Mock::given(method("GET"))
        .and(path("/api/views/"))
        .and(query_param("path", "/buckets/x"))
        .and(query_param("bucket", "x"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([{
            "id": 5, "name": "s3", "path": "/buckets/x", "policy_id": 2,
            "protocols": ["S3"], "bucket": "x"
        }])))
        .expect(1)
        .mount(&server)
        .await;

    let views = client
        .views()
        .list_with_params(&ListViewsParams {
            path: Some("/buckets/x".into()),
            bucket: Some("x".into()),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(views[0].bucket, "x");
    server.verify().await;
}

#[tokio::test]
async fn users_list_with_params_filters_by_name() {
    use vast::api::ListUsersParams;
    let (server, client) = setup("t").await;
    Mock::given(method("GET"))
        .and(path("/api/users/"))
        .and(query_param("name", "alice"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([{
            "id": 4, "name": "alice", "uid": 1000
        }])))
        .expect(1)
        .mount(&server)
        .await;

    let users = client
        .users()
        .list_with_params(&ListUsersParams {
            name: Some("alice".into()),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(users[0].name, "alice");
    server.verify().await;
}

#[tokio::test]
async fn quotas_list_with_params_filters_by_path() {
    use vast::api::ListQuotasParams;
    let (server, client) = setup("t").await;
    Mock::given(method("GET"))
        .and(path("/api/quotas/"))
        .and(query_param("path", "/data"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([{
            "id": 3, "name": "data", "path": "/data", "used_capacity": 1024
        }])))
        .expect(1)
        .mount(&server)
        .await;

    let quotas = client
        .quotas()
        .list_with_params(&ListQuotasParams {
            path: Some("/data".into()),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(quotas[0].path, "/data");
    server.verify().await;
}

#[tokio::test]
async fn s3_policies_list_with_params_filters_by_name() {
    use vast::api::ListS3PoliciesParams;
    let (server, client) = setup("t").await;
    Mock::given(method("GET"))
        .and(path("/api/s3policies/"))
        .and(query_param("name", "read-only"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([{
            "id": 9, "name": "read-only", "policy": "{}", "enabled": true
        }])))
        .expect(1)
        .mount(&server)
        .await;

    let policies = client
        .s3_policies()
        .list_with_params(&ListS3PoliciesParams {
            name: Some("read-only".into()),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(policies[0].name, "read-only");
    server.verify().await;
}

#[tokio::test]
async fn delete_folder_posts_body_on_delete() {
    // `Clusters::delete_folder` sends a body on a DELETE — easy to break
    // without noticing. `Folders::delete` below does the same.
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
async fn folders_create_posts_to_create_folder_action() {
    use vast::api::CreateFolder;
    let (server, client) = setup("t").await;
    Mock::given(method("POST"))
        .and(path("/api/folders/create_folder/"))
        .and(body_json(json!({
            "path": "/sfc/chris/newdir",
            "tenant_id": 2,
            "owner_is_group": true,
            "user": "chris",
            "group": "sfc",
            "create_dir_mode": 493,
            "inherit_acl": true
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": 1, "guid": "g1", "name": "newdir", "path": "/sfc/chris/newdir"
        })))
        .expect(1)
        .mount(&server)
        .await;

    let folder = client
        .folders()
        .create(&CreateFolder {
            path: "/sfc/chris/newdir".into(),
            tenant_id: Some(2),
            owner_is_group: Some(true),
            user: Some("chris".into()),
            group: Some("sfc".into()),
            create_dir_mode: Some(0o755),
            inherit_acl: Some(true),
        })
        .await
        .unwrap();
    assert_eq!(folder.path, "/sfc/chris/newdir");
    server.verify().await;
}

#[tokio::test]
async fn folders_get_posts_to_stat_path_action() {
    let (server, client) = setup("t").await;
    Mock::given(method("POST"))
        .and(path("/api/folders/stat_path/"))
        .and(body_json(json!({ "path": "/sfc/chris", "tenant_id": 2 })))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({ "user": "chris", "group": "sfc" })),
        )
        .expect(1)
        .mount(&server)
        .await;

    let owner = client.folders().get("/sfc/chris", Some(2)).await.unwrap();
    assert_eq!(owner.user, "chris");
    assert_eq!(owner.group, "sfc");
    server.verify().await;
}

#[tokio::test]
async fn folders_get_omits_tenant_id_when_none() {
    let (server, client) = setup("t").await;
    Mock::given(method("POST"))
        .and(path("/api/folders/stat_path/"))
        .and(body_json(json!({ "path": "/sfc/chris" })))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({ "user": "chris", "group": "sfc" })),
        )
        .expect(1)
        .mount(&server)
        .await;

    client.folders().get("/sfc/chris", None).await.unwrap();
    server.verify().await;
}

#[tokio::test]
async fn folders_delete_sends_body_to_delete_folder_action() {
    let (server, client) = setup("t").await;
    Mock::given(method("DELETE"))
        .and(path("/api/folders/delete_folder/"))
        .and(body_json(
            json!({ "path": "/sfc/chris/newdir", "tenant_id": 2 }),
        ))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&server)
        .await;

    client
        .folders()
        .delete("/sfc/chris/newdir", Some(2))
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
            s3_policies_ids: Vec::new(),
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
async fn users_create_access_key_posts_tenant_id() {
    let (server, client) = setup("t").await;
    Mock::given(method("POST"))
        .and(path("/api/users/5/access_keys/"))
        .and(body_json(json!({ "tenant_id": 2 })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "access_key": "AKIA123", "secret_key": "s3cr3t"
        })))
        .expect(1)
        .mount(&server)
        .await;

    let keys = client.users().create_access_key(5, Some(2)).await.unwrap();
    assert_eq!(keys.access_key, "AKIA123");
    assert_eq!(keys.secret_key, "s3cr3t");
    server.verify().await;
}

#[tokio::test]
async fn users_create_access_key_omits_tenant_id_when_none() {
    let (server, client) = setup("t").await;
    Mock::given(method("POST"))
        .and(path("/api/users/5/access_keys/"))
        .and(body_json(json!({})))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "access_key": "AKIA123", "secret_key": "s3cr3t"
        })))
        .expect(1)
        .mount(&server)
        .await;

    client.users().create_access_key(5, None).await.unwrap();
    server.verify().await;
}

#[tokio::test]
async fn users_delete_access_key_sends_key_in_body() {
    let (server, client) = setup("t").await;
    Mock::given(method("DELETE"))
        .and(path("/api/users/5/access_keys/"))
        .and(body_json(json!({ "access_key": "AKIA123" })))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&server)
        .await;

    client
        .users()
        .delete_access_key(5, "AKIA123")
        .await
        .unwrap();
    server.verify().await;
}

#[tokio::test]
async fn user_key_pair_debug_redacts_secret_key() {
    let (server, client) = setup("t").await;
    Mock::given(method("POST"))
        .and(path("/api/users/5/access_keys/"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "access_key": "AKIA123", "secret_key": "s3cr3t"
        })))
        .mount(&server)
        .await;

    let keys = client.users().create_access_key(5, None).await.unwrap();
    let dbg = format!("{keys:?}");
    assert!(!dbg.contains("s3cr3t"), "secret leaked into Debug: {dbg}");
    assert!(dbg.contains("AKIA123"));
}

#[tokio::test]
async fn s3_policies_full_crud() {
    use vast::api::{CreateS3Policy, UpdateS3Policy};
    let (server, client) = setup("t").await;
    Mock::given(method("GET"))
        .and(path("/api/s3policies/"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([{
            "id": 1, "name": "read-only", "guid": "abc",
            "policy": "{\"Version\":\"2012-10-17\"}", "tenant_id": 1, "enabled": true
        }])))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/s3policies/"))
        .and(body_json(json!({
            "name": "full-access", "policy": "{\"Version\":\"2012-10-17\"}", "tenant_id": 1
        })))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!({
            "id": 2, "name": "full-access", "guid": "def",
            "policy": "{\"Version\":\"2012-10-17\"}", "tenant_id": 1, "enabled": true
        })))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("PATCH"))
        .and(path("/api/s3policies/2/"))
        .and(body_json(json!({ "enabled": false })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": 2, "name": "full-access", "guid": "def",
            "policy": "{\"Version\":\"2012-10-17\"}", "tenant_id": 1, "enabled": false
        })))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("DELETE"))
        .and(path("/api/s3policies/2/"))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&server)
        .await;

    let policies = client.s3_policies().list().await.unwrap();
    assert_eq!(policies[0].name, "read-only");
    assert_eq!(policies[0].tenant_id, Some(1));

    let created = client
        .s3_policies()
        .create(&CreateS3Policy {
            name: "full-access".into(),
            policy: "{\"Version\":\"2012-10-17\"}".into(),
            tenant_id: Some(1),
        })
        .await
        .unwrap();
    assert_eq!(created.id, 2);
    assert!(created.enabled);

    let updated = client
        .s3_policies()
        .update(
            2,
            &UpdateS3Policy {
                enabled: Some(false),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert!(!updated.enabled);

    client.s3_policies().delete(2).await.unwrap();
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
            tenant_id: None,
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
    //
    // Disable retries — this test is about how a single non-2xx
    // response is parsed, not about retry behavior. Skipping retries
    // keeps the test snappy.
    let server = MockServer::start().await;
    let client = setup_with_retry(&server, "t", 1).await;
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
    // No-retry setup for the same reason as above.
    let server = MockServer::start().await;
    let client = setup_with_retry(&server, "t", 1).await;
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

// ---------------------------------------------------------------------------
// Pagination
// ---------------------------------------------------------------------------

#[tokio::test]
async fn list_handles_drf_paginated_response_single_page() {
    // A single-page DRF response (next/previous both null) should be
    // unwrapped into the same Vec<T> a bare-array response produces.
    let (server, client) = setup("t").await;
    Mock::given(method("GET"))
        .and(path("/api/clusters/"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "count": 2,
            "next": null,
            "previous": null,
            "results": [
                { "id": 1, "name": "a", "sw_version": "5.0", "enabled": true, "state": "ONLINE", "guid": "g1" },
                { "id": 2, "name": "b", "sw_version": "5.0", "enabled": true, "state": "ONLINE", "guid": "g2" },
            ],
        })))
        .expect(1)
        .mount(&server)
        .await;

    let clusters = client.clusters().list().await.unwrap();
    assert_eq!(clusters.len(), 2);
    assert_eq!(clusters[0].name, "a");
    assert_eq!(clusters[1].name, "b");
    server.verify().await;
}

#[tokio::test]
async fn list_auto_paginates_across_pages() {
    // The first response carries a `next` link pointing at page 2;
    // list() should follow it and accumulate items from both pages.
    let (server, client) = setup("t").await;
    Mock::given(method("GET"))
        .and(path("/api/clusters/"))
        .and(query_param_is_missing("page"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "count": 3,
            "next": "https://vms.example.com/api/clusters/?page=2",
            "previous": null,
            "results": [
                { "id": 1, "name": "a", "sw_version": "5.0", "enabled": true, "state": "ONLINE", "guid": "g1" },
                { "id": 2, "name": "b", "sw_version": "5.0", "enabled": true, "state": "ONLINE", "guid": "g2" },
            ],
        })))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/clusters/"))
        .and(query_param("page", "2"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "count": 3,
            "next": null,
            "previous": "https://vms.example.com/api/clusters/",
            "results": [
                { "id": 3, "name": "c", "sw_version": "5.0", "enabled": true, "state": "ONLINE", "guid": "g3" },
            ],
        })))
        .expect(1)
        .mount(&server)
        .await;

    let clusters = client.clusters().list().await.unwrap();
    assert_eq!(clusters.len(), 3);
    assert_eq!(clusters[0].id, 1);
    assert_eq!(clusters[2].id, 3);
    server.verify().await;
}

#[tokio::test]
async fn list_paged_exposes_page_metadata() {
    use vast::api::PageParams;
    let (server, client) = setup("t").await;
    Mock::given(method("GET"))
        .and(path("/api/clusters/"))
        .and(query_param("page_size", "10"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "count": 42,
            "next": "https://vms.example.com/api/clusters/?page=2",
            "previous": null,
            "results": [
                { "id": 1, "name": "a", "sw_version": "5.0", "enabled": true, "state": "ONLINE", "guid": "g1" },
            ],
        })))
        .expect(1)
        .mount(&server)
        .await;

    let page = client
        .clusters()
        .list_paged(&PageParams {
            page: None,
            page_size: Some(10),
        })
        .await
        .unwrap();
    assert_eq!(page.items.len(), 1);
    assert_eq!(page.count, Some(42));
    assert_eq!(page.next_page, Some(2));
    assert_eq!(page.previous_page, None);
    server.verify().await;
}

#[tokio::test]
async fn list_paged_with_bare_array_has_none_metadata() {
    // Endpoints that don't paginate still work through list_paged:
    // items are populated, metadata is None.
    use vast::api::PageParams;
    let (server, client) = setup("t").await;
    Mock::given(method("GET"))
        .and(path("/api/clusters/"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            { "id": 1, "name": "a", "sw_version": "5.0", "enabled": true, "state": "ONLINE", "guid": "g1" },
        ])))
        .mount(&server)
        .await;

    let page = client
        .clusters()
        .list_paged(&PageParams::default())
        .await
        .unwrap();
    assert_eq!(page.items.len(), 1);
    assert_eq!(page.count, None);
    assert_eq!(page.next_page, None);
    assert_eq!(page.previous_page, None);
}

#[tokio::test]
async fn list_with_params_paginates_while_preserving_filters() {
    // Filter params must be sent on every page request, not only the
    // first — otherwise page 2 would return unfiltered results.
    use vast::api::ListNodesParams;
    let (server, client) = setup("t").await;
    Mock::given(method("GET"))
        .and(path("/api/nodes/"))
        .and(query_param("cluster_id", "7"))
        .and(query_param("page_size", "100"))
        .and(query_param_is_missing("page"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "count": 2,
            "next": "https://vms.example.com/api/nodes/?cluster_id=7&page=2&page_size=100",
            "previous": null,
            "results": [{ "id": 1, "name": "n1" }],
        })))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/nodes/"))
        .and(query_param("cluster_id", "7"))
        .and(query_param("page", "2"))
        .and(query_param("page_size", "100"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "count": 2,
            "next": null,
            "previous": "https://vms.example.com/api/nodes/?cluster_id=7&page_size=100",
            "results": [{ "id": 2, "name": "n2" }],
        })))
        .expect(1)
        .mount(&server)
        .await;

    let nodes = client
        .nodes()
        .list_with_params(&ListNodesParams {
            cluster_id: Some(7),
            page_size: Some(100),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(nodes.len(), 2);
    server.verify().await;
}

#[tokio::test]
async fn list_unwraps_unparseable_next_link_safely() {
    // If the VMS returns a `next` link we can't parse a page number out
    // of, list() should stop instead of looping forever or panicking.
    let (server, client) = setup("t").await;
    Mock::given(method("GET"))
        .and(path("/api/clusters/"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "count": 1,
            "next": "not-a-valid-url",
            "previous": null,
            "results": [
                { "id": 1, "name": "a", "sw_version": "5.0", "enabled": true, "state": "ONLINE", "guid": "g1" },
            ],
        })))
        .expect(1)
        .mount(&server)
        .await;

    let clusters = client.clusters().list().await.unwrap();
    assert_eq!(clusters.len(), 1);
    server.verify().await;
}

// ---------------------------------------------------------------------------
// Retries
// ---------------------------------------------------------------------------

#[tokio::test]
async fn get_retries_5xx_then_succeeds() {
    // First call: 503. Second call: 200 with body. list() should return
    // the body after one retry; we observe both stubs firing exactly once.
    let server = MockServer::start().await;
    let client = setup_with_retry(&server, "t", 3).await;

    // wiremock dispatches mocks in registration order, with the
    // **earlier** mock taking precedence at equal priority — so
    // register the 503 first (with `up_to_n_times(1)` so it stops
    // matching after one hit) and the 200 second so the retry falls
    // through to it.
    Mock::given(method("GET"))
        .and(path("/api/clusters/"))
        .respond_with(ResponseTemplate::new(503).set_body_json(json!({"detail": "transient"})))
        .up_to_n_times(1)
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/clusters/"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            { "id": 1, "name": "a", "sw_version": "5.0", "enabled": true, "state": "ONLINE", "guid": "g1" },
        ])))
        .expect(1)
        .mount(&server)
        .await;

    let clusters = client.clusters().list().await.unwrap();
    assert_eq!(clusters.len(), 1);
    server.verify().await;
}

#[tokio::test]
async fn get_retries_429_then_succeeds() {
    // 429 (rate-limited) is in the retryable set alongside 5xx.
    let server = MockServer::start().await;
    let client = setup_with_retry(&server, "t", 3).await;

    // Register the 429 first so it wins the first request, then
    // exhausts via `up_to_n_times(1)` and the retry falls through to
    // the 200 mock. (Earlier-mounted mocks take precedence at equal
    // priority in wiremock-rs.)
    Mock::given(method("GET"))
        .and(path("/api/clusters/"))
        .respond_with(ResponseTemplate::new(429).set_body_json(json!({"detail": "slow down"})))
        .up_to_n_times(1)
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/clusters/"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
        .expect(1)
        .mount(&server)
        .await;

    client.clusters().list().await.unwrap();
    server.verify().await;
}

#[tokio::test]
async fn get_does_not_retry_4xx() {
    // 4xx other than 429 is treated as a real client error — no retry.
    // (Auth 401 with refreshable credentials is a separate path with
    // its own one-shot retry — covered by `jwt_401_invalidates_cache_...`.)
    let server = MockServer::start().await;
    let client = setup_with_retry(&server, "static", 3).await;

    Mock::given(method("GET"))
        .and(path("/api/clusters/1/"))
        .respond_with(
            ResponseTemplate::new(404).set_body_json(json!({"detail": "no such cluster"})),
        )
        .expect(1)
        .mount(&server)
        .await;

    let err = client.clusters().get(1).await.unwrap_err();
    assert!(err.is_not_found());
    server.verify().await;
}

#[tokio::test]
async fn get_exhausts_attempts_then_returns_error() {
    // 5xx that never recovers — after `max_attempts` tries we should
    // surface the error to the caller, not loop forever.
    let server = MockServer::start().await;
    let client = setup_with_retry(&server, "t", 3).await;

    Mock::given(method("GET"))
        .and(path("/api/clusters/"))
        .respond_with(ResponseTemplate::new(503).set_body_json(json!({"detail": "still bad"})))
        .expect(3)
        .mount(&server)
        .await;

    let err = client.clusters().list().await.unwrap_err();
    // The error should reflect the final 503.
    let msg = format!("{err}");
    assert!(
        msg.contains("503") || msg.contains("still bad"),
        "got: {msg}"
    );
    server.verify().await;
}

#[tokio::test]
async fn max_attempts_one_disables_retry() {
    let server = MockServer::start().await;
    let client = setup_with_retry(&server, "t", 1).await;

    Mock::given(method("GET"))
        .and(path("/api/clusters/"))
        .respond_with(ResponseTemplate::new(503))
        .expect(1) // exactly one — no retries
        .mount(&server)
        .await;

    let _ = client.clusters().list().await.unwrap_err();
    server.verify().await;
}

#[tokio::test]
async fn post_is_never_retried_even_on_5xx() {
    // POST may be non-idempotent — we send it at most once regardless
    // of the retry budget, so a flaky create endpoint surfaces the
    // first failure rather than silently creating duplicates.
    use vast::api::CreateUser;
    let server = MockServer::start().await;
    let client = setup_with_retry(&server, "t", 5).await;

    Mock::given(method("POST"))
        .and(path("/api/users/"))
        .respond_with(ResponseTemplate::new(503).set_body_json(json!({"detail": "boom"})))
        .expect(1) // exactly one
        .mount(&server)
        .await;

    let _ = client
        .users()
        .create(&CreateUser {
            name: "alice".into(),
            uid: None,
            email: None,
            enabled: None,
            s3_policies_ids: Vec::new(),
        })
        .await
        .unwrap_err();
    server.verify().await;
}

// ---------------------------------------------------------------------------
// PaginatedIter
// ---------------------------------------------------------------------------

#[tokio::test]
async fn iter_streams_items_across_pages() {
    // Two-page DRF response; iter() should walk through every item
    // without buffering the entire collection.
    let (server, client) = setup("t").await;
    Mock::given(method("GET"))
        .and(path("/api/clusters/"))
        .and(query_param_is_missing("page"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "count": 3,
            "next": "https://vms.example.com/api/clusters/?page=2",
            "previous": null,
            "results": [
                { "id": 1, "name": "a", "sw_version": "5.0", "enabled": true, "state": "ONLINE", "guid": "g1" },
                { "id": 2, "name": "b", "sw_version": "5.0", "enabled": true, "state": "ONLINE", "guid": "g2" },
            ],
        })))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/clusters/"))
        .and(query_param("page", "2"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "count": 3,
            "next": null,
            "previous": "https://vms.example.com/api/clusters/",
            "results": [
                { "id": 3, "name": "c", "sw_version": "5.0", "enabled": true, "state": "ONLINE", "guid": "g3" },
            ],
        })))
        .expect(1)
        .mount(&server)
        .await;

    let mut iter = client.clusters().iter();
    let mut ids = Vec::new();
    while let Some(item) = iter.next().await {
        ids.push(item.unwrap().id);
    }
    assert_eq!(ids, vec![1, 2, 3]);
    server.verify().await;
}

#[tokio::test]
async fn iter_works_against_bare_array_responses() {
    // Endpoints that don't paginate still flow through iter() — it
    // yields the whole bare array, then returns None.
    let (server, client) = setup("t").await;
    Mock::given(method("GET"))
        .and(path("/api/clusters/"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            { "id": 1, "name": "a", "sw_version": "5.0", "enabled": true, "state": "ONLINE", "guid": "g1" },
            { "id": 2, "name": "b", "sw_version": "5.0", "enabled": true, "state": "ONLINE", "guid": "g2" },
        ])))
        .mount(&server)
        .await;

    let mut iter = client.clusters().iter();
    let mut count = 0;
    while let Some(item) = iter.next().await {
        item.unwrap();
        count += 1;
    }
    assert_eq!(count, 2);
}

#[tokio::test]
async fn iter_propagates_terminal_error_then_returns_none() {
    // A page fetch that exhausts retries should surface as
    // `Some(Err(_))` once; subsequent calls return `None` so the
    // caller's `while let Some(...)` loop terminates cleanly without
    // re-triggering the same failure.
    let server = MockServer::start().await;
    let client = setup_with_retry(&server, "t", 1).await;
    Mock::given(method("GET"))
        .and(path("/api/clusters/"))
        .respond_with(ResponseTemplate::new(503))
        .expect(1)
        .mount(&server)
        .await;

    let mut iter = client.clusters().iter();
    let first = iter.next().await.expect("expected Some(Err)");
    assert!(first.is_err());
    let second = iter.next().await;
    assert!(second.is_none(), "iter should be terminal after error");
    server.verify().await;
}
