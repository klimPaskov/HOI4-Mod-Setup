//! Portable setup core shared by the HOI4 Mod Setup desktop app and its web
//! edition (`docs/38_web_edition_and_improvement_plan.md`).
//!
//! Code here builds for native targets and for `wasm32-unknown-unknown`. It
//! never touches the filesystem, processes, the network, the OS clock, or OS
//! randomness directly; hosts supply those through the traits in [`adapters`].

pub mod adapters;
pub mod coding_environment;
pub mod descriptors;
pub mod error;
pub mod live_testing;
pub mod manifest;
pub mod mcp_target;
pub mod memory_fs;
pub mod merge;
pub mod models;
pub mod native_mcp;
pub mod profile;
pub mod schemas;
pub mod text;

pub use error::AppError;
