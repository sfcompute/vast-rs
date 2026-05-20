# vast-rs

A strongly-typed, async Rust client for the [VAST Data Management System (VMS) REST API](https://kb.vastdata.com/docs/vast-rest-api-overview).

`vast-rs` is a peer library to [vastpy](https://github.com/vast-data/vastpy), VAST's official Python SDK. API types are intended to be generated directly from your cluster's OpenAPI specification so they are always in sync with your installed VMS version.

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
vast-rs = { git = "https://github.com/sfcompute/vast-rs" }
tokio = { version = "1", features = ["full"] }
```

> Rust 1.75 or later is required (async traits, Rust 2024 edition).

---

## Quick Start

```rust
use vast_rs::VastClient;

#[tokio::main]
async fn main() -> vast_rs::Result<()> {
    let client = VastClient::builder()
        .address("vms.example.com")
        .token("your-api-token")
        .build()?;

    let clusters = client.clusters().list().await?;
    for c in &clusters {
        println!("{}: {} ({})", c.id, c.name, c.sw_version.as_deref().unwrap_or("unknown"));
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
let client = vast_rs::VastClient::new(vast_rs::ClientConfig::from_env()?)?;
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
use vast_rs::api::nodes::ListNodesParams;

// List all nodes
let nodes = client.nodes().list().await?;

// Filter by cluster
let params = ListNodesParams { cluster_id: Some(1), ..Default::default() };
let nodes = client.nodes().list_with_params(&params).await?;
```

### Volumes

```rust
use vast_rs::api::volumes::{CreateVolume, UpdateVolume};

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
use vast_rs::api::users::{CreateUser, UpdateUser};

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
use vast_rs::api::views::{CreateView, UpdateView};

let views = client.views().list().await?;

let view = client.views().create(&CreateView {
    name: "nfs-export".into(),
    path: "/data".into(),
    policy_id: 1,
    protocols: vec!["NFS".into()],
    alias: None,
    bucket: None,
    allow_anonymous_access: None,
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
use vast_rs::api::quotas::{CreateQuota, UpdateQuota};

let quotas = client.quotas().list().await?;

let quota = client.quotas().create(&CreateQuota {
    name: "team-limit".into(),
    hard_limit: 10 * 1024 * 1024 * 1024, // 10 GiB
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
use vast_rs::api::snapshots::CreateSnapshot;

let snaps = client.snapshots().list().await?;

let snap = client.snapshots().create(&CreateSnapshot {
    name: "daily-backup".into(),
    path: "/data".into(),
}).await?;
```

### Error handling

```rust
use vast_rs::Error;

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
let client = VastClient::builder()
    .address("vms.example.com")         // required — scheme added automatically
    .token("tok")                        // or .credentials("user", "pass")
    .tenant("acme")                      // required for tenant admin accounts
    .timeout_secs(60)                    // default: 30
    .danger_accept_invalid_certs(true)   // for self-signed certs; dev only
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

## Running the Example

```bash
VMS_ADDRESS=vms.example.com \
VMS_TOKEN=your-token \
  cargo run --example list_clusters
```

---

## Running Tests

Unit tests use [wiremock](https://github.com/LukeMathWalker/wiremock-rs) to mock the VMS HTTP API — no cluster needed:

```bash
cargo test
```

Integration tests against a real cluster are in `tests/` and are gated behind the `integration` feature (coming soon). Set environment variables and run:

```bash
VMS_ADDRESS=vms.example.com VMS_TOKEN=<token> cargo test --features integration
```

---

## Contributing

Contributions are welcome. A few guidelines:

- **New API resources** — add a module under `src/api/`, register it in `src/api/mod.rs`, and expose it via a method on `VastClient`. Follow the pattern in `src/api/volumes.rs`.
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
