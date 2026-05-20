//! Typed API namespaces — one module per VMS resource family.
//!
//! Each module exposes a `*Api` struct (e.g. [`clusters::ClustersApi`]) that
//! is constructed via the corresponding method on [`VastClient`](crate::VastClient).
//!
//! The structs and their fields in this module represent a best-effort
//! approximation of the VMS schema.  They are intended to be replaced (or
//! augmented) with types generated directly from your cluster's OpenAPI spec —
//! see `scripts/generate.sh` for instructions.

pub mod clusters;
pub mod folders;
pub mod nodes;
pub mod protectionpolicies;
pub mod quotas;
pub mod snapshots;
pub mod tenants;
pub mod users;
pub mod viewpolicies;
pub mod views;
pub mod vippools;
pub mod volumes;
