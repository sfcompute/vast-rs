//! # vast-rs
//!
//! A strongly-typed async Rust client for the VAST Data Management System (VMS) REST API.
//!
//! This library is a peer to [vastpy](https://github.com/vast-data/vastpy), VAST's official
//! Python SDK. Client types are generated from the VMS OpenAPI specification and wrapped
//! in an ergonomic, idiomatic Rust API.
//!
//! ## Quick start
//!
//! ```rust,no_run
//! use vast_rs::{VastClient, ClientConfig};
//!
//! #[tokio::main]
//! async fn main() -> vast_rs::Result<()> {
//!     // Authenticate with an API token
//!     let client = VastClient::builder()
//!         .address("vms.example.com")
//!         .token("your-api-token")
//!         .build()?;
//!
//!     let clusters = client.clusters().list().await?;
//!     for cluster in &clusters {
//!         println!("{}: {}", cluster.id, cluster.name);
//!     }
//!     Ok(())
//! }
//! ```
//!
//! ## Authentication
//!
//! Two authentication strategies are supported:
//!
//! - **API token** — pass a token directly (recommended for automation):
//!   ```rust,no_run
//!   # use vast_rs::VastClient;
//!   let client = VastClient::builder()
//!       .address("vms.example.com")
//!       .token("your-api-token")
//!       .build()?;
//!   # Ok::<_, vast_rs::Error>(())
//!   ```
//!
//! - **Username + password** — the client obtains and refreshes a JWT automatically:
//!   ```rust,no_run
//!   # use vast_rs::VastClient;
//!   let client = VastClient::builder()
//!       .address("vms.example.com")
//!       .credentials("admin", "secret")
//!       .build()?;
//!   # Ok::<_, vast_rs::Error>(())
//!   ```
//!
//! ## Environment variables
//!
//! Configuration can also be loaded from the environment:
//!
//! | Variable       | Description                  |
//! |----------------|------------------------------|
//! | `VMS_ADDRESS`  | Hostname or IP of the VMS    |
//! | `VMS_TOKEN`    | API token                    |
//! | `VMS_USER`     | Username (if not using token)|
//! | `VMS_PASSWORD` | Password (if not using token)|

pub mod api;
pub mod auth;
pub mod client;
pub mod config;
pub mod error;

pub use client::VastClient;
pub use config::{ClientConfig, ClientConfigBuilder};
pub use error::{Error, Result};

/// Serde helper: deserialize `null` as `T::default()`, otherwise deserialize normally.
///
/// Use alongside `#[serde(default)]` on any non-`Option` field that the VAST API
/// may return as `null` instead of omitting:
///
/// ```ignore
/// #[serde(default, deserialize_with = "crate::null_as_default")]
/// pub name: String,
/// ```
///
/// With this pair, both missing fields and `null` fields produce `T::default()`.
pub fn null_as_default<'de, D, T>(deserializer: D) -> std::result::Result<T, D::Error>
where
    D: serde::de::Deserializer<'de>,
    T: Default + serde::de::Deserialize<'de>,
{
    Ok(<Option<T> as serde::Deserialize>::deserialize(deserializer)?.unwrap_or_default())
}
