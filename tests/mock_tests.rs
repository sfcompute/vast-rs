//! Tests for `MockVastClient`. Run with `cargo test --features mock`.

#![recursion_limit = "256"]

use serde_json::json;
use vast_rs::mock::MockVastClient;

#[tokio::test]
async fn stub_get_roundtrip() {
    let mock = MockVastClient::start().await;
    mock.stub_get("clusters/", json!([
        { "id": 1, "name": "alpha", "state": "ONLINE" },
        { "id": 2, "name": "beta",  "state": "OFFLINE" },
    ])).await;

    let clusters = mock.client().clusters().list().await.unwrap();
    assert_eq!(clusters.len(), 2);
    assert_eq!(clusters[0].name, "alpha");
    assert_eq!(clusters[1].state, "OFFLINE");
}

#[tokio::test]
async fn stub_post_returns_created_resource() {
    use vast_rs::api::CreateVolume;

    let mock = MockVastClient::start().await;
    mock.stub_post("volumes/", json!({
        "id": 42, "name": "scratch", "path": "/scratch", "quota": 1_073_741_824u64
    })).await;

    let vol = mock.client().volumes().create(&CreateVolume {
        name: "scratch".into(),
        path: "/scratch".into(),
        quota: Some(1_073_741_824),
    }).await.unwrap();

    assert_eq!(vol.id, 42);
}

#[tokio::test]
async fn stub_delete_succeeds() {
    let mock = MockVastClient::start().await;
    mock.stub_delete("volumes/7/").await;
    mock.client().volumes().delete(7).await.unwrap();
}

#[tokio::test]
async fn stub_error_maps_to_typed_error() {
    let mock = MockVastClient::start().await;
    mock.stub_error("GET", "clusters/999/", 404, "Not found.").await;
    mock.stub_error("GET", "users/", 401, "No creds.").await;

    let nf = mock.client().clusters().get(999).await.unwrap_err();
    assert!(nf.is_not_found(), "{nf:?}");

    let un = mock.client().users().list().await.unwrap_err();
    assert!(un.is_unauthorized(), "{un:?}");
    assert!(un.to_string().contains("No creds."));
}

#[tokio::test]
async fn stub_with_range_verifies_call_count() {
    let mock = MockVastClient::start().await;
    mock.stub_with("GET", "clusters/", 200, json!([]), 1u64..=3u64).await;

    let client = mock.client();
    let _ = client.clusters().list().await.unwrap();
    let _ = client.clusters().list().await.unwrap();

    mock.verify().await;
}

#[tokio::test]
#[should_panic]
async fn verify_panics_on_unmet_expectation() {
    let mock = MockVastClient::start().await;
    mock.stub_with("GET", "clusters/", 200, json!([]), 1u64).await;
    mock.verify().await;
}

#[tokio::test]
async fn with_credentials_handles_cluster_and_tenant_admins() {
    for tenant in [None, Some("acme")] {
        let mock =
            MockVastClient::with_credentials("alice", "pw", tenant, "jwt").await;
        mock.stub_get("clusters/", json!([])).await;
        mock.client().clusters().list().await.unwrap();
    }
}

#[tokio::test]
async fn reset_clears_stubs() {
    let mock = MockVastClient::start().await;
    mock.stub_get("clusters/", json!([{ "id": 1, "name": "a" }])).await;
    assert_eq!(mock.client().clusters().list().await.unwrap().len(), 1);

    mock.reset().await;
    let err = mock.client().clusters().list().await.unwrap_err();
    assert_eq!(err.status_code(), Some(404));
}
