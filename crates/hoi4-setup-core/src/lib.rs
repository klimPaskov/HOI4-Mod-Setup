//! Portable setup core shared by the HOI4 Mod Setup desktop app and its web
//! edition (`docs/38_web_edition_and_improvement_plan.md`).
//!
//! Code here builds for native targets and for `wasm32-unknown-unknown`. It
//! never touches the filesystem, processes, the network, the OS clock, or OS
//! randomness directly; hosts supply those through the traits in [`adapters`].

pub mod adapters;
pub mod error;
pub mod memory_fs;
pub mod native_mcp;
pub mod schemas;
pub mod text;

pub use error::AppError;
