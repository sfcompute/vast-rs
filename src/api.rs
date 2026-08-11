//! Typed handles for each VMS resource family.
//!
//! Every resource follows the same shape:
//!
//! * a `Resource` struct with the stable fields you'll most often want, plus
//!   `extra: Map<String, Value>` capturing every other field the VMS returns
//!   (forward-compatible with newer cluster software);
//! * a `CreateResource` body for `POST` (where supported);
//! * an `UpdateResource` body for `PATCH` (where supported);
//! * a `Resources<'c>` newtype implementing `list`/`get`/`create`/`update`/`delete`
//!   as appropriate, reached via `VastClient::<resource>()`.
//!
//! Fields you need that aren't on the slim model are always one
//! `resource.extra.get("field_name")` away.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::{Result, client::VastClient};

/// Catch-all for fields not modeled explicitly.
pub type Extra = Map<String, Value>;

/// Serde helper: treat a JSON `null` as `T::default()` rather than erroring.
/// Combined with the struct-level `#[serde(default)]` on every model, this
/// covers both *missing* and *null* values uniformly. Apply to any non-Option
/// field that the VMS might return as `null` (e.g. text fields populated only
/// after cluster bootstrap).
fn null_default<'de, D, T>(d: D) -> std::result::Result<T, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Default + Deserialize<'de>,
{
    Ok(Option::<T>::deserialize(d)?.unwrap_or_default())
}

// ---------------------------------------------------------------------------
// Pagination
// ---------------------------------------------------------------------------

/// Query parameters for paginated list endpoints. Pass to a resource's
/// `list_paged` method to fetch one specific page or set a page size.
///
/// `list()` accepts pagination implicitly — it auto-paginates and returns
/// the full collection regardless of whether the endpoint paginates the
/// response.
#[derive(Debug, Default, Clone, Copy, Serialize)]
pub struct PageParams {
    /// 1-indexed page number. Defaults to the server's first page.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page: Option<u32>,
    /// Items per page. Defaults to the server's `default_page_size`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page_size: Option<u32>,
}

/// One page of results from a list endpoint.
///
/// For endpoints that return a DRF paginated wrapper (`{count, next,
/// previous, results}`), all fields are populated. For endpoints that
/// return a bare JSON array, `count`/`next_page`/`previous_page` are
/// `None` and `items` holds the entire collection.
#[derive(Debug, Clone)]
pub struct Page<T> {
    /// Items on this page.
    pub items: Vec<T>,
    /// Total count across all pages, when known.
    pub count: Option<usize>,
    /// Page number of the next page, or `None` on the last page.
    pub next_page: Option<u32>,
    /// Page number of the previous page, or `None` on the first page.
    pub previous_page: Option<u32>,
}

/// Trait for query-parameter structs that can carry a page number, so
/// the auto-pagination helper can advance through pages without knowing
/// the concrete params type. Implemented by [`PageParams`] and the
/// filter structs for resources that support filtered listing
/// (e.g. [`ListNodesParams`], [`ListVolumesParams`]).
pub trait Paginate {
    fn set_page(&mut self, page: u32);
}

impl Paginate for PageParams {
    fn set_page(&mut self, page: u32) {
        self.page = Some(page);
    }
}

/// Untagged enum that decodes both the DRF paginated wrapper and a bare
/// array, so callers don't have to care which shape an endpoint returns
/// on a given cluster version or configuration.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub(crate) enum PaginatedResponse<T> {
    /// Standard DRF wrapper: `{count, next, previous, results}`.
    Paginated {
        count: usize,
        next: Option<String>,
        previous: Option<String>,
        results: Vec<T>,
    },
    /// Plain array (some endpoints don't paginate by default).
    Bare(Vec<T>),
}

impl<T> PaginatedResponse<T> {
    pub(crate) fn into_page(self) -> Page<T> {
        match self {
            Self::Bare(items) => Page {
                items,
                count: None,
                next_page: None,
                previous_page: None,
            },
            Self::Paginated {
                count,
                next,
                previous,
                results,
            } => Page {
                items: results,
                count: Some(count),
                next_page: page_from_link(next.as_deref()),
                previous_page: page_from_link(previous.as_deref()),
            },
        }
    }
}

/// Extract `?page=N` from a paginated-response link URL. Tolerates an
/// unparseable URL by returning `None` — the auto-pagination loop will
/// then stop, which is the right thing if the server gave us a link we
/// can't follow.
fn page_from_link(link: Option<&str>) -> Option<u32> {
    let link = link?;
    let url = url::Url::parse(link).ok()?;
    url.query_pairs()
        .find(|(k, _)| k == "page")
        .and_then(|(_, v)| v.parse().ok())
}

/// Async iterator over a paginated list endpoint. Pages are fetched
/// lazily as items are consumed, so memory usage stays bounded to one
/// page regardless of collection size — useful when iterating over very
/// large quota or view sets without buffering them all.
///
/// Use [`next`](Self::next) to drive iteration:
///
/// ```rust,no_run
/// # use vast::VastClient;
/// # async fn run(client: VastClient) -> vast::Result<()> {
/// let mut iter = client.quotas().iter();
/// while let Some(quota) = iter.next().await {
///     let quota = quota?;
///     println!("{}: {}", quota.id, quota.path);
/// }
/// # Ok(()) }
/// ```
///
/// Retries from the underlying [`VastClient`] apply to each page fetch
/// independently, so a transient blip in the middle of iteration
/// recovers automatically. If retries are exhausted on a page, the
/// iterator yields `Some(Err(_))` and then `None` on subsequent calls.
pub struct PaginatedIter<T, Q> {
    client: VastClient,
    path: String,
    params: Q,
    buffer: std::collections::VecDeque<T>,
    done: bool,
}

impl<T, Q> PaginatedIter<T, Q>
where
    T: serde::de::DeserializeOwned,
    Q: Serialize + Paginate,
{
    pub(crate) fn new(client: VastClient, path: String, params: Q) -> Self {
        Self {
            client,
            path,
            params,
            buffer: std::collections::VecDeque::new(),
            done: false,
        }
    }

    /// Yield the next item, fetching another page from the VMS if the
    /// in-memory buffer is empty. Returns `None` when the collection
    /// is exhausted. On error, returns `Some(Err(_))` once and then
    /// `None` — the iterator transitions to a terminal state so the
    /// caller doesn't accidentally re-trigger the same failure in a
    /// tight loop.
    pub async fn next(&mut self) -> Option<Result<T>> {
        loop {
            if let Some(item) = self.buffer.pop_front() {
                return Some(Ok(item));
            }
            if self.done {
                return None;
            }
            let resp: PaginatedResponse<T> =
                match self.client.get_with_query(&self.path, &self.params).await {
                    Ok(r) => r,
                    Err(e) => {
                        self.done = true;
                        return Some(Err(e));
                    }
                };
            let page = resp.into_page();
            self.buffer.extend(page.items);
            match page.next_page {
                Some(n) => self.params.set_page(n),
                None => self.done = true,
            }
            // Loop: if the buffer now has items, pop one; if it's still
            // empty and `done`, return None; if empty and more pages to
            // fetch (rare — server returned an empty intermediate
            // page), the next iteration fetches the next page.
        }
    }
}

// ---------------------------------------------------------------------------
// Macros: the boilerplate every resource family shares.
//
// `list_params!` declares a resource's filter-params struct; `crud!`
// declares the handle struct and its methods. Every list endpoint gets
// three flavors:
//
//   * `list()`             — auto-paginates and returns the whole collection
//   * `list_paged(params)` — fetches one specific page
//   * `iter()`             — streams items, fetching pages lazily
//
// Declaring a resource with `filters = ListFooParams` adds a
// `_with_params` counterpart to each, taking the filter struct in place
// of `PageParams`.
// ---------------------------------------------------------------------------

/// Declare a filter-params struct for a list endpoint: one `Option` field
/// per query parameter the endpoint filters on, plus `page` / `page_size`.
///
/// `None` fields are omitted from the query string entirely, so
/// `Default::default()` filters nothing and lists the whole collection.
macro_rules! list_params {
    (
        $(#[$attr:meta])*
        $Params:ident { $($(#[$field_attr:meta])* $field:ident: $ty:ty),+ $(,)? }
    ) => {
        $(#[$attr])*
        #[derive(Debug, Default, Clone, Serialize)]
        pub struct $Params {
            $(
                $(#[$field_attr])*
                #[serde(skip_serializing_if = "Option::is_none")]
                pub $field: Option<$ty>,
            )+
            /// 1-indexed page number. Defaults to the server's first page.
            #[serde(skip_serializing_if = "Option::is_none")]
            pub page: Option<u32>,
            /// Items per page. Defaults to the server's `default_page_size`.
            #[serde(skip_serializing_if = "Option::is_none")]
            pub page_size: Option<u32>,
        }

        impl Paginate for $Params {
            fn set_page(&mut self, page: u32) {
                self.page = Some(page);
            }
        }
    };
}

/// The unfiltered list methods every listable resource gets.
macro_rules! list_methods {
    ($Resource:ty, $path:expr) => {
        pub async fn list(&self) -> Result<Vec<$Resource>> {
            self.0.list_all($path, PageParams::default()).await
        }
        pub async fn list_paged(&self, params: &PageParams) -> Result<Page<$Resource>> {
            self.0.get_page($path, params).await
        }
        /// Stream items one at a time, fetching pages lazily as the
        /// in-memory buffer drains. See [`PaginatedIter`].
        pub fn iter(&self) -> PaginatedIter<$Resource, PageParams> {
            PaginatedIter::new(self.0.clone(), $path.to_string(), PageParams::default())
        }
    };
}

/// Filtered counterparts of [`list_methods`], emitted for resources
/// declared with `filters = ...`. Filters are carried across every page
/// request, not just the first.
macro_rules! filtered_list_methods {
    ($Resource:ty, $Params:ty, $path:expr) => {
        /// Auto-paginate, returning every item matching `params`.
        pub async fn list_with_params(&self, params: &$Params) -> Result<Vec<$Resource>> {
            self.0.list_all($path, params.clone()).await
        }
        /// Fetch one page of the items matching `params`.
        pub async fn list_paged_with_params(&self, params: &$Params) -> Result<Page<$Resource>> {
            self.0.get_page($path, params).await
        }
        /// Stream the items matching `params`, fetching pages lazily as
        /// the in-memory buffer drains. See [`PaginatedIter`].
        pub fn iter_with_params(&self, params: &$Params) -> PaginatedIter<$Resource, $Params> {
            PaginatedIter::new(self.0.clone(), $path.to_string(), params.clone())
        }
    };
}

macro_rules! crud {
    // Full CRUD: list/list_paged/iter/get/create/update/delete.
    ($Handle:ident, $Resource:ty, $Create:ty, $Update:ty, $path:expr
     $(, filters = $Params:ty)?) => {
        pub struct $Handle<'c>(pub(crate) &'c VastClient);
        impl<'c> $Handle<'c> {
            list_methods!($Resource, $path);
            $( filtered_list_methods!($Resource, $Params, $path); )?
            pub async fn get(&self, id: u64) -> Result<$Resource> {
                self.0.get(&format!("{}{id}/", $path)).await
            }
            pub async fn create(&self, body: &$Create) -> Result<$Resource> {
                self.0.post($path, body).await
            }
            pub async fn update(&self, id: u64, body: &$Update) -> Result<$Resource> {
                self.0.patch(&format!("{}{id}/", $path), body).await
            }
            pub async fn delete(&self, id: u64) -> Result<()> {
                self.0.delete(&format!("{}{id}/", $path)).await
            }
        }
    };
    // Create-only (no update): used for snapshots-style resources that don't update.
    (cd $Handle:ident, $Resource:ty, $Create:ty, $path:expr
     $(, filters = $Params:ty)?) => {
        pub struct $Handle<'c>(pub(crate) &'c VastClient);
        impl<'c> $Handle<'c> {
            list_methods!($Resource, $path);
            $( filtered_list_methods!($Resource, $Params, $path); )?
            pub async fn get(&self, id: u64) -> Result<$Resource> {
                self.0.get(&format!("{}{id}/", $path)).await
            }
            pub async fn create(&self, body: &$Create) -> Result<$Resource> {
                self.0.post($path, body).await
            }
            pub async fn delete(&self, id: u64) -> Result<()> {
                self.0.delete(&format!("{}{id}/", $path)).await
            }
        }
    };
    // Read-only: resources the VMS exposes for listing only (nodes,
    // clusters). Extra endpoints go in a separate `impl` block.
    (ro $Handle:ident, $Resource:ty, $path:expr $(, filters = $Params:ty)?) => {
        pub struct $Handle<'c>(pub(crate) &'c VastClient);
        impl<'c> $Handle<'c> {
            list_methods!($Resource, $path);
            $( filtered_list_methods!($Resource, $Params, $path); )?
            pub async fn get(&self, id: u64) -> Result<$Resource> {
                self.0.get(&format!("{}{id}/", $path)).await
            }
        }
    };
}

// ===========================================================================
// Clusters
// ===========================================================================

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Cluster {
    pub id: u64,
    #[serde(deserialize_with = "null_default")]
    pub guid: String,
    #[serde(deserialize_with = "null_default")]
    pub name: String,
    #[serde(deserialize_with = "null_default")]
    pub state: String,
    #[serde(deserialize_with = "null_default")]
    pub sw_version: String,
    pub enabled: bool,
    #[serde(flatten)]
    pub extra: Extra,
}

crud!(ro Clusters, Cluster, "clusters/");

impl<'c> Clusters<'c> {
    /// Permanently delete a filesystem directory from the VAST namespace.
    /// `DELETE /api/clusters/{cluster_id}/delete_folder/` — requires the
    /// "Trash Folder Access" cluster setting.
    pub async fn delete_folder(
        &self,
        cluster_id: u64,
        path: &str,
        tenant_id: Option<u64>,
    ) -> Result<()> {
        let mut body = serde_json::json!({ "path": path });
        if let Some(t) = tenant_id {
            body["tenant_id"] = serde_json::json!(t);
        }
        self.0
            .delete_with_body(&format!("clusters/{cluster_id}/delete_folder/"), &body)
            .await
    }
}

// ===========================================================================
// Nodes
// ===========================================================================

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Node {
    pub id: u64,
    pub name: String,
    pub state: Option<String>,
    pub ip: Option<String>,
    pub cluster: Option<u64>,
    #[serde(flatten)]
    pub extra: Extra,
}

list_params! {
    /// Filters for `GET /nodes/`.
    ListNodesParams {
        /// Only nodes belonging to this cluster.
        cluster_id: u64,
        /// Only nodes in this state (e.g. `"ACTIVE"`).
        state: String,
    }
}

crud!(ro Nodes, Node, "nodes/", filters = ListNodesParams);

// ===========================================================================
// Users
// ===========================================================================

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct UserAccessKey(pub String, pub String);

impl UserAccessKey {
    pub fn id(&self) -> &str {
        &self.0
    }
    pub fn status(&self) -> &str {
        &self.1
    }
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct User {
    pub id: u64,
    pub name: String,
    pub uid: Option<u64>,
    pub email: Option<String>,
    pub enabled: Option<bool>,
    pub is_admin: Option<bool>,
    pub s3_policies_ids: Vec<u64>,
    pub access_keys: Vec<UserAccessKey>,
    #[serde(flatten)]
    pub extra: Extra,
}

#[derive(Debug, Serialize)]
pub struct CreateUser {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uid: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub s3_policies_ids: Vec<u64>,
}

#[derive(Debug, Default, Serialize)]
pub struct UpdateUser {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
}

crud!(Users, User, CreateUser, UpdateUser, "users/");

/// An S3 access key pair belonging to a local user.
///
/// `secret_key` is returned **only** at creation time — the VMS never
/// discloses it again, so persist it when you receive it. The `Debug` impl
/// redacts it so it can't leak into logs; read it via the field directly.
#[derive(Default, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct UserKeyPair {
    /// S3 access key, needed to authenticate S3 client requests.
    #[serde(deserialize_with = "null_default")]
    pub access_key: String,
    /// S3 secret key, needed to authenticate S3 client requests.
    #[serde(deserialize_with = "null_default")]
    pub secret_key: String,
    #[serde(flatten)]
    pub extra: Extra,
}

impl std::fmt::Debug for UserKeyPair {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UserKeyPair")
            .field("access_key", &self.access_key)
            .field("secret_key", &"[REDACTED]")
            .field("extra", &self.extra)
            .finish()
    }
}

/// Body for `POST /users/{id}/access_keys/`.
#[derive(Debug, Serialize)]
struct CreateAccessKey {
    #[serde(skip_serializing_if = "Option::is_none")]
    tenant_id: Option<u64>,
}

/// Body for `DELETE /users/{id}/access_keys/`.
#[derive(Debug, Serialize)]
struct DeleteAccessKey<'a> {
    access_key: &'a str,
}

impl<'c> Users<'c> {
    /// `POST /users/{user_id}/access_keys/` — generate an S3 access key
    /// pair for a local user.
    ///
    /// The returned [`UserKeyPair::secret_key`] is only ever shown here;
    /// store it now or generate a new pair later.
    pub async fn create_access_key(
        &self,
        user_id: u64,
        tenant_id: Option<u64>,
    ) -> Result<UserKeyPair> {
        self.0
            .post(
                &format!("users/{user_id}/access_keys/"),
                &CreateAccessKey { tenant_id },
            )
            .await
    }

    /// `DELETE /users/{user_id}/access_keys/` — remove one S3 access key
    /// pair from a local user. The key to remove goes in the request body,
    /// not the path.
    pub async fn delete_access_key(&self, user_id: u64, access_key: &str) -> Result<()> {
        self.0
            .delete_with_body(
                &format!("users/{user_id}/access_keys/"),
                &DeleteAccessKey { access_key },
            )
            .await
    }
}

// ===========================================================================
// Volumes
// ===========================================================================

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Volume {
    pub id: u64,
    pub name: String,
    pub path: Option<String>,
    pub quota: Option<u64>,
    pub enabled: Option<bool>,
    #[serde(flatten)]
    pub extra: Extra,
}

#[derive(Debug, Serialize)]
pub struct CreateVolume {
    pub name: String,
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quota: Option<u64>,
}

#[derive(Debug, Default, Serialize)]
pub struct UpdateVolume {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quota: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
}

list_params! {
    /// Filters for `GET /volumes/`.
    ListVolumesParams {
        /// Only the volume at this exact filesystem path.
        path: String,
        /// Restrict to snapshot volumes (`true`) or live volumes (`false`).
        is_snapshot: bool,
    }
}

crud!(
    Volumes,
    Volume,
    CreateVolume,
    UpdateVolume,
    "volumes/",
    filters = ListVolumesParams
);

// ===========================================================================
// Views
// ===========================================================================

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct View {
    pub id: u64,
    pub guid: String,
    pub name: String,
    pub path: String,
    pub policy_id: u64,
    pub bucket: String,
    pub protocols: Vec<String>,
    pub tenant_id: Option<u64>,
    pub enabled: bool,
    pub bucket_owner: Option<String>,
    #[serde(flatten)]
    pub extra: Extra,
}

#[derive(Debug, Serialize)]
pub struct CreateView {
    pub name: String,
    pub path: String,
    pub policy_id: u64,
    pub protocols: Vec<String>,
    /// If `true`, create the backing directory and any missing parents.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub create_dir: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alias: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bucket: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allow_anonymous_access: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub s3_versioning: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub s3_locks: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub s3_locks_retention_mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tenant_id: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bucket_owner: Option<String>,
}

#[derive(Debug, Default, Serialize)]
pub struct UpdateView {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub policy_id: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub protocols: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alias: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allow_anonymous_access: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub s3_versioning: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
}

list_params! {
    /// Filters for `GET /views/`.
    ListViewsParams {
        /// Only the view at this exact filesystem path.
        path: String,
        /// Only the view exporting this S3 bucket name.
        bucket: String,
    }
}

crud!(
    Views,
    View,
    CreateView,
    UpdateView,
    "views/",
    filters = ListViewsParams
);

// ===========================================================================
// View policies
// ===========================================================================

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ViewPolicy {
    pub id: u64,
    pub guid: String,
    pub name: String,
    pub flavor: String,
    pub auth_source: String,
    #[serde(flatten)]
    pub extra: Extra,
}

#[derive(Debug, Serialize)]
pub struct CreateViewPolicy {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth_source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flavor: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub smb_file_mode: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub smb_directory_mode: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nfs_posix_acl: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nfs_root_squash: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nfs_all_squash: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nfs_no_squash: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub read_write: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub read_only: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tenant_id: Option<u64>,
}

#[derive(Debug, Default, Serialize)]
pub struct UpdateViewPolicy {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub smb_file_mode: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub smb_directory_mode: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nfs_posix_acl: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nfs_root_squash: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nfs_all_squash: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nfs_no_squash: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub read_write: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub read_only: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth_source: Option<String>,
}

crud!(
    ViewPolicies,
    ViewPolicy,
    CreateViewPolicy,
    UpdateViewPolicy,
    "viewpolicies/"
);

// ===========================================================================
// Quotas
// ===========================================================================

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Quota {
    pub id: u64,
    pub guid: String,
    pub name: String,
    pub path: String,
    pub state: String,
    pub hard_limit: Option<u64>,
    pub soft_limit: Option<u64>,
    pub used_capacity: u64,
    #[serde(flatten)]
    pub extra: Extra,
}

#[derive(Debug, Serialize)]
pub struct CreateQuota {
    pub name: String,
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hard_limit: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub soft_limit: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hard_limit_inodes: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub soft_limit_inodes: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tenant_id: Option<u64>,
}

#[derive(Debug, Default, Serialize)]
pub struct UpdateQuota {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hard_limit: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub soft_limit: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hard_limit_inodes: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub soft_limit_inodes: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enable_alarms: Option<bool>,
}

list_params! {
    /// Filters for `GET /quotas/`.
    ListQuotasParams {
        /// Only the quota on this exact filesystem path.
        path: String,
    }
}

crud!(
    Quotas,
    Quota,
    CreateQuota,
    UpdateQuota,
    "quotas/",
    filters = ListQuotasParams
);

// ===========================================================================
// VIP pools
// ===========================================================================

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct VipPool {
    pub id: u64,
    pub guid: String,
    pub name: String,
    pub start_ip: String,
    pub end_ip: String,
    pub active_cnode_ids: Vec<u64>,
    pub role: String,
    pub enabled: bool,
    #[serde(flatten)]
    pub extra: Extra,
}

#[derive(Debug, Serialize)]
pub struct CreateVipPool {
    pub name: String,
    pub start_ip: String,
    pub end_ip: String,
    pub gw_ip: String,
    pub subnet_cidr: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vlan: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cnode_ids: Option<Vec<u64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub domain_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tenant_id: Option<u64>,
}

#[derive(Debug, Default, Serialize)]
pub struct UpdateVipPool {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_ip: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_ip: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gw_ip: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subnet_cidr: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vlan: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cnode_ids: Option<Vec<u64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub domain_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
}

crud!(VipPools, VipPool, CreateVipPool, UpdateVipPool, "vippools/");

// ===========================================================================
// Tenants
// ===========================================================================

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Tenant {
    pub id: u64,
    pub guid: String,
    pub name: String,
    pub is_default: bool,
    pub enabled: bool,
    pub encryption_crn: Option<String>,
    #[serde(flatten)]
    pub extra: Extra,
}

#[derive(Debug, Serialize)]
pub struct CreateTenant {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vms_root_no_tenant_access: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub s3_root_no_tenant_access: Option<bool>,
}

#[derive(Debug, Default, Serialize)]
pub struct UpdateTenant {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vms_root_no_tenant_access: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub s3_root_no_tenant_access: Option<bool>,
}

crud!(Tenants, Tenant, CreateTenant, UpdateTenant, "tenants/");

// ===========================================================================
// Snapshots
// ===========================================================================

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Snapshot {
    pub id: u64,
    pub guid: String,
    pub name: String,
    pub path: String,
    #[serde(flatten)]
    pub extra: Extra,
}

#[derive(Debug, Serialize)]
pub struct CreateSnapshot {
    pub name: String,
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tenant_id: Option<u64>,
}

#[derive(Debug, Default, Serialize)]
pub struct UpdateSnapshot {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

crud!(
    Snapshots,
    Snapshot,
    CreateSnapshot,
    UpdateSnapshot,
    "snapshots/"
);

// ===========================================================================
// Protection policies
// ===========================================================================

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ProtectionPolicy {
    pub id: u64,
    pub guid: String,
    pub name: String,
    #[serde(flatten)]
    pub extra: Extra,
}

#[derive(Debug, Serialize)]
pub struct CreateProtectionPolicy {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tenant_id: Option<u64>,
}

#[derive(Debug, Default, Serialize)]
pub struct UpdateProtectionPolicy {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

crud!(
    ProtectionPolicies,
    ProtectionPolicy,
    CreateProtectionPolicy,
    UpdateProtectionPolicy,
    "protectionpolicies/"
);

// ===========================================================================
// Folders
// ===========================================================================
//
// Folders aren't a REST resource with stable IDs — the VMS exposes them as
// three POST/DELETE *actions* keyed by filesystem path, not by an
// enumerable list. There is no `GET /folders/` to list against, so
// `get`/`delete` can't be expressed as `GET|DELETE /folders/{id}/` the way
// the `crud!` macro assumes; both take `{path, tenant_id}` as a request
// body instead.

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Folder {
    pub id: u64,
    pub guid: String,
    pub name: String,
    pub path: String,
    #[serde(flatten)]
    pub extra: Extra,
}

#[derive(Debug, Serialize)]
pub struct CreateFolder {
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tenant_id: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner_is_group: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    /// Octal directory creation mode (e.g. `0o755`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub create_dir_mode: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inherit_acl: Option<bool>,
}

/// Body shared by `stat_path` and `delete_folder`, which both key off a
/// path (optionally scoped to a tenant) rather than an ID.
#[derive(Debug, Serialize)]
struct FolderPathBody<'a> {
    path: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    tenant_id: Option<u64>,
}

/// Response from `stat_path`: the owning user and group for a path.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct FolderOwner {
    #[serde(deserialize_with = "null_default")]
    pub user: String,
    #[serde(deserialize_with = "null_default")]
    pub group: String,
    #[serde(flatten)]
    pub extra: Extra,
}

pub struct Folders<'c>(pub(crate) &'c VastClient);
impl<'c> Folders<'c> {
    /// `POST /folders/create_folder/` — create a folder in the Element
    /// Store under the given tenant, owning group and user.
    pub async fn create(&self, body: &CreateFolder) -> Result<Folder> {
        self.0.post("folders/create_folder/", body).await
    }
    /// `POST /folders/stat_path/` — look up the owning user and group for
    /// an Element Store path.
    pub async fn get(&self, path: &str, tenant_id: Option<u64>) -> Result<FolderOwner> {
        self.0
            .post("folders/stat_path/", &FolderPathBody { path, tenant_id })
            .await
    }
    /// `DELETE /folders/delete_folder/` — delete a folder from the
    /// Element Store.
    pub async fn delete(&self, path: &str, tenant_id: Option<u64>) -> Result<()> {
        self.0
            .delete_with_body(
                "folders/delete_folder/",
                &FolderPathBody { path, tenant_id },
            )
            .await
    }
}

// ===========================================================================
// S3 policies
// ===========================================================================

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct S3Policy {
    pub id: u64,
    pub guid: String,
    pub name: String,
    pub policy: String,
    pub users: Vec<String>,
    pub groups: Vec<String>,
    pub enabled: bool,
    pub tenant_id: Option<u64>,
    #[serde(flatten)]
    pub extra: Extra,
}

#[derive(Debug, Serialize)]
pub struct CreateS3Policy {
    pub name: String,
    /// The S3 identity policy document, as a JSON string.
    pub policy: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tenant_id: Option<u64>,
}

#[derive(Debug, Default, Serialize)]
pub struct UpdateS3Policy {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub policy: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
}

list_params! {
    /// Filters for `GET /s3policies/`.
    ListS3PoliciesParams {
        /// Only the policy with this exact name.
        name: String,
    }
}

crud!(
    S3Policies,
    S3Policy,
    CreateS3Policy,
    UpdateS3Policy,
    "s3policies/",
    filters = ListS3PoliciesParams
);
