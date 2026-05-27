# vast

A strongly-typed, async Rust client for the [VAST Data Management System (VMS) REST API](https://kb.vastdata.com/docs/vast-rest-api-overview).

The `vast` crate (repo: [`vast-rs`](https://github.com/sfcompute/vast-rs)) is a peer library to [vastpy](https://github.com/vast-data/vastpy), VAST's official Python SDK. API types are intended to be generated directly from your cluster's OpenAPI specification so they are always in sync with your installed VMS version.

---

## Features

- **Async-first** — built on [Tokio](https://tokio.rs/) and [reqwest](https://github.com/seanmonstar/reqwest)
- **Strongly typed** — serde models for clusters, nodes, users, volumes, and more; generated from the VMS OpenAPI spec
- **Two auth strategies** — long-lived API token or username/password (JWT obtained and cached automatically)
- **Secret-safe** — credentials are stored as [`SecretString`](https://docs.rs/secrecy) and never appear in logs or `Debug` output
- **Ergonomic builder API** — `VastClient::builder().address(...).token(...).build()?`
- **Cheap to clone** — `VastClient` is `Arc`-backed and shares a single connection pool
- **Graceful forward-compatibility** — unknown fields from newer API versions are captured in an `extra` map instead of failing deserialization

---

## Installation

Add to your `Cargo.toml`:

```toml
[dependencies]
vast = { git = "https://github.com/sfcompute/vast-rs" }
tokio = { version = "1", features = ["full"] }
```

> Rust 1.85 or later is required (Rust 2024 edition).

---

## Quick Start

```rust
use vast::VastClient;

#[tokio::main]
async fn main() -> vast::Result<()> {
    let client = VastClient::builder()
        .address("vms.example.com")
        .token("your-api-token")
        .build()?;

    let clusters = client.clusters().list().await?;
    for c in &clusters {
        println!("{}: {} ({})", c.id, c.name, c.sw_version);
    }

    Ok(())
}
```

### Authenticate with username and password

The client exchanges credentials for a JWT automatically and caches it for the session:

```rust
// Cluster admin account
let client = VastClient::builder()
    .address("vms.example.com")
    .credentials("admin", "secret")
    .build()?;

// Tenant admin account — tenant name is required, otherwise the VMS returns 401
let client = VastClient::builder()
    .address("vms.example.com")
    .credentials("alice", "secret")
    .tenant("acme")
    .build()?;
```

### Load configuration from environment variables

```rust
// Reads VMS_ADDRESS + VMS_TOKEN (or VMS_USER + VMS_PASSWORD) from the environment
let client = vast::VastClient::from_env()?;
```

---

## Environment Variables

| Variable       | Description                                                        |
|----------------|--------------------------------------------------------------------|
| `VMS_ADDRESS`  | Hostname or IP of the VMS (e.g. `vms.example.com`)                |
| `VMS_TOKEN`    | API token — takes precedence over user/password                    |
| `VMS_USER`     | Username for credential-based auth                                 |
| `VMS_PASSWORD` | Password for credential-based auth                                 |
| `VMS_TENANT`   | Tenant name — **required for tenant admin accounts**, omit for cluster admins |
| `VMS_DANGER_ACCEPT_INVALID_CERTS` | Set to `1` / `true` / `yes` / `on` to disable TLS certificate validation. **Development / self-signed VMS deployments only.** Equivalent to `.danger_accept_invalid_certs(true)` on the builder. |

---

## Usage

### Clusters

```rust
// List all clusters
let clusters = client.clusters().list().await?;

// Get a single cluster
let cluster = client.clusters().get(1).await?;
```

### Nodes

```rust
use vast::api::ListNodesParams;

// List all nodes
let nodes = client.nodes().list().await?;

// Filter by cluster
let params = ListNodesParams { cluster_id: Some(1), ..Default::default() };
let nodes = client.nodes().list_with_params(&params).await?;
```

### Volumes

```rust
use vast::api::{CreateVolume, UpdateVolume};

// List volumes
let volumes = client.volumes().list().await?;

// Create a volume
let vol = client.volumes().create(&CreateVolume {
    name: "data".into(),
    path: "/data".into(),
    quota: Some(10 * 1024 * 1024 * 1024), // 10 GiB
}).await?;

// Update a volume
client.volumes().update(vol.id, &UpdateVolume {
    quota: Some(20 * 1024 * 1024 * 1024),
    ..Default::default()
}).await?;

// Delete a volume
client.volumes().delete(vol.id).await?;
```

### Users

```rust
use vast::api::{CreateUser, UpdateUser};

let users = client.users().list().await?;

let user = client.users().create(&CreateUser {
    name: "alice".into(),
    uid: Some(1001),
    email: Some("alice@example.com".into()),
    enabled: Some(true),
}).await?;

client.users().delete(user.id).await?;
```

### Views

```rust
use vast::api::{CreateView, UpdateView};

let views = client.views().list().await?;

let view = client.views().create(&CreateView {
    name: "nfs-export".into(),
    path: "/data".into(),
    policy_id: 1,
    protocols: vec!["NFS".into()],
    create_dir: Some(true),
    alias: None,
    bucket: None,
    allow_anonymous_access: None,
    s3_versioning: None,
    s3_locks: None,
    s3_locks_retention_mode: None,
}).await?;

client.views().delete(view.id).await?;
```

### View Policies

```rust
let policies = client.view_policies().list().await?;
let policy = client.view_policies().get(1).await?;
```

### Quotas

```rust
use vast::api::{CreateQuota, UpdateQuota};

let quotas = client.quotas().list().await?;

let quota = client.quotas().create(&CreateQuota {
    name: "team-limit".into(),
    path: "/data".into(),
    hard_limit: Some(10 * 1024 * 1024 * 1024), // 10 GiB
    soft_limit: Some(8 * 1024 * 1024 * 1024),
    hard_limit_inodes: None,
    soft_limit_inodes: None,
}).await?;

client.quotas().update(quota.id, &UpdateQuota {
    hard_limit: Some(20 * 1024 * 1024 * 1024),
    ..Default::default()
}).await?;
```

### VIP Pools

```rust
let pools = client.vip_pools().list().await?;
```

### Snapshots

```rust
use vast::api::CreateSnapshot;

let snaps = client.snapshots().list().await?;

let snap = client.snapshots().create(&CreateSnapshot {
    name: "daily-backup".into(),
    path: "/data".into(),
}).await?;
```

### Error handling

```rust
use vast::Error;

match client.clusters().get(999).await {
    Ok(cluster) => println!("Found: {}", cluster.name),
    Err(e) if e.is_not_found() => println!("Cluster not found"),
    Err(e) if e.is_unauthorized() => println!("Check your credentials"),
    Err(e) => return Err(e),
}
```

---

## Builder Options

```rust
use std::time::Duration;

let client = VastClient::builder()
    .address("vms.example.com")                  // required — scheme added automatically
    .token("tok")                                 // or .credentials("user", "pass")
    .tenant("acme")                               // required for tenant admin accounts
    .timeout(Duration::from_secs(60))             // default: 30s
    .danger_accept_invalid_certs(true)            // for self-signed certs; dev only
    .build()?;
```

---

## Spec Refresh

The VMS exposes a Swagger 2.0 spec at `/api/?format=openapi` without authentication. `scripts/generate.sh` downloads it to `api-spec/vast-openapi.json` as a reference. Types in `src/api/` are hand-written from this spec — the API surface is small and changes additively between VAST versions.

```bash
# Refresh the local spec copy
./scripts/generate.sh -a vms.example.com

# Non-standard port or self-signed certificate
./scripts/generate.sh -a vms.example.com:8443 -k
```

After running, diff `api-spec/vast-openapi.json` against the previous version to spot new or changed endpoints, then update `src/api/` accordingly.

---

## API Coverage

| Resource             | list | get | create | update | delete |
|----------------------|:----:|:---:|:------:|:------:|:------:|
| Clusters             | ✓    | ✓   |        |        |        |
| Nodes                | ✓    | ✓   |        |        |        |
| Users                | ✓    | ✓   | ✓      | ✓      | ✓      |
| Volumes              | ✓    | ✓   | ✓      | ✓      | ✓      |
| Views                | ✓    | ✓   | ✓      | ✓      | ✓      |
| View Policies        | ✓    | ✓   | ✓      | ✓      | ✓      |
| Quotas               | ✓    | ✓   | ✓      | ✓      | ✓      |
| VIP Pools            | ✓    | ✓   | ✓      | ✓      | ✓      |
| Snapshots            | ✓    | ✓   | ✓      | ✓      | ✓      |
| Protection Policies  | ✓    | ✓   | ✓      | ✓      | ✓      |

---

## Running Tests

Unit tests use [wiremock](https://github.com/LukeMathWalker/wiremock-rs) to mock the VMS HTTP API — no cluster needed:

```bash
cargo test
```

The mock client (see [Mock client for consumer tests](#mock-client-for-consumer-tests) below) lives behind the `mock` feature. To exercise its own test suite, run:

```bash
cargo test --features mock
```

Integration tests against a real cluster live in `tests/integration.rs` and are gated behind the `integration` feature so the default `cargo test` doesn't touch the network. They're designed to be non-destructive — every resource they create uses a `vast-rs-test-` name prefix and is cleaned up at the end of each test. Set the environment variables and run:

```bash
VMS_ADDRESS=vms.example.com VMS_TOKEN=<token> cargo test --features integration
```

---

## Mock client for consumer tests

If you're building an application or library on top of `vast`, you can use `vast::mock::MockVastClient` to unit-test code that takes a `&VastClient` — no live cluster, no `wiremock` boilerplate. Enable the `mock` feature in your dev-dependencies:

```toml
[dev-dependencies]
vast = { version = "0.1", features = ["mock"] }
tokio = { version = "1", features = ["full"] }
serde_json = "1"
```

Then stub VMS responses with `json!` literals and exercise your code:

```rust
use serde_json::json;
use vast::{VastClient, mock::MockVastClient};

async fn count_online_clusters(client: &VastClient) -> usize {
    client.clusters().list().await.unwrap()
        .iter()
        .filter(|c| c.state == "ONLINE")
        .count()
}

#[tokio::test]
async fn counts_only_online() {
    let mock = MockVastClient::start().await;
    mock.stub_get("clusters/", json!([
        { "id": 1, "name": "a", "state": "ONLINE" },
        { "id": 2, "name": "b", "state": "OFFLINE" },
        { "id": 3, "name": "c", "state": "ONLINE" },
    ])).await;

    assert_eq!(count_online_clusters(&mock.client()).await, 2);
}
```

Helpers on `MockVastClient`:

| Helper                                  | What it does                                                |
|-----------------------------------------|-------------------------------------------------------------|
| `stub_get/post/patch/delete(path, ...)` | Stub the common verbs with sensible default statuses        |
| `stub_error(method, path, status, msg)` | Stub a `{"detail": msg}` error response                     |
| `stub_with(method, path, status, body, times)` | Stub with a call-count expectation (`u64` or range); enforce via `verify()` |
| `server()`                              | Underlying `wiremock::MockServer` for advanced matching     |
| `reset()` / `verify()`                  | Clear stubs / assert expectations were met                  |

Paths may be written as `"clusters/"`, `"/clusters/"`, or `"/api/clusters/"` — they all match the same endpoint.

---

## Contributing

Contributions are welcome. A few guidelines:

- **New API resources** — add the resource model and `Create`/`Update` structs in `src/api.rs`, register the handle via the `crud!` macro (or hand-code it if the endpoint has bespoke methods like `delete_folder`), then expose it from `VastClient` with a small accessor method. Follow the pattern of `Volumes` / `Tenants`.
- **Tests** — every new method should have at least one wiremock-backed unit test in `tests/client_tests.rs`.
- **No `unwrap()` in library code** — propagate errors with `?`.
- **Secrets discipline** — any new credential type must use `SecretString` (or equivalent) so it cannot leak into logs.

To run the full check suite before opening a PR:

```bash
cargo fmt --check
cargo clippy -- -D warnings
cargo test
```

---

## License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT License ([LICENSE-MIT](LICENSE-MIT))

at your option.

---

## Related Projects

- [vastpy](https://github.com/vast-data/vastpy) — VAST's official Python SDK
- [vast-admin-mcp](https://github.com/vast-data/vast-admin-mcp) — MCP server for VAST administration
- [vastdb_sdk](https://github.com/vast-data/vastdb_sdk) — Python SDK for VAST Database (columnar data, separate from VMS)
