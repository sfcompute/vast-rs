//! # vast-rs
//!
//! A strongly-typed async Rust client for the VAST Data Management System (VMS)
//! REST API — peer to VAST's official Python SDK
//! ([vastpy](https://github.com/vast-data/vastpy)).
//!
//! ## Quick start
//!
//! ```rust,no_run
//! use vast_rs::VastClient;
//!
//! #[tokio::main]
//! async fn main() -> vast_rs::Result<()> {
//!     // Token auth (recommended for automation):
//!     let client = VastClient::builder()
//!         .address("vms.example.com")
//!         .token("your-api-token")
//!         .build()?;
//!
//!     for c in &client.clusters().list().await? {
//!         println!("{}: {} ({})", c.id, c.name, c.sw_version);
//!     }
//!     Ok(())
//! }
//! ```
//!
//! ### Username / password
//!
//! The client exchanges credentials for a JWT on first use and caches it.
//! Tenant-admin accounts must call `.tenant("name")` — cluster admins omit it.
//!
//! ```rust,no_run
//! # use vast_rs::VastClient;
//! let client = VastClient::builder()
//!     .address("vms.example.com")
//!     .credentials("alice", "secret")
//!     .tenant("acme")
//!     .build()?;
//! # Ok::<_, vast_rs::Error>(())
//! ```
//!
//! ### From the environment
//!
//! [`VastClient::from_env`] reads `VMS_ADDRESS` plus either `VMS_TOKEN` or
//! `VMS_USER` + `VMS_PASSWORD` (and optional `VMS_TENANT`).
//!
//! ## Models are forward-compatible
//!
//! Each resource model contains the stable fields you'll typically want
//! (`id`, `name`, etc.) plus an `extra: Map<String, Value>` capturing every
//! other field the VMS returns — so unknown / newer / cluster-specific fields
//! flow through unchanged. Reach for them via `resource.extra.get("field")`.

// Library-code discipline: callers should propagate errors with `?`, not
// panic. Tests and examples are unaffected (lints fire on lib code only).
#![warn(clippy::unwrap_used, clippy::expect_used)]

pub mod api;
mod auth;
mod client;
pub mod error;

#[cfg(feature = "mock")]
#[cfg_attr(docsrs, doc(cfg(feature = "mock")))]
pub mod mock;

pub use client::{Builder, VastClient};
pub use error::{Error, Result};
