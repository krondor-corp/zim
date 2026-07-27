//! Typed hub HTTP client — a module per subject, a file per request.
//!
//! Each request type documents the hub route it targets. The hub server
//! (`zim-hub`) serializes the **same wire types** and compiles the
//! **same request impls** defined here, so the shapes can't drift; the
//! route *paths* still have no compile-time link — if you change a path
//! here, change the matching `zim-hub` route.

#[cfg(feature = "admin")]
pub mod admin;
pub mod auth;
// Part of the `vault` capability (same speakers: wasm SDK + hub server)
// even though the route is vault-agnostic — blobs are content-addressed.
#[cfg(feature = "vault")]
pub mod blob;
pub mod client;
pub mod devices;
pub mod did_doc;
pub mod jwt;
pub mod resolver;
#[cfg(feature = "vault")]
pub mod vault;
pub mod vaults;

#[cfg(feature = "admin")]
pub use admin::{AdminActionRequest, AdminUser, AdminUsers, AdminUsersRequest};
#[cfg(feature = "grant")]
pub use auth::{GrantApproveRequest, GrantInfo, GrantInfoRequest};
pub use auth::{Me, MeRequest};
pub use client::HubClient;
pub use devices::{
    device_did, device_nick, Device, DevicesRequest, DevicesResponse, RemoveDeviceRequest,
};
pub use did_doc::{DidDoc, DidDocRequest};
pub use resolver::HttpDidResolver;
pub use vaults::{VaultItem, VaultsRequest, VaultsResponse};
