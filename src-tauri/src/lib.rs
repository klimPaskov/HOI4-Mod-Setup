//! Platform-neutral domain core for HOI4 Mod Setup.
//!
//! The React surface is intentionally a client of these types and commands.
//! It never receives filesystem or credential authority directly.

pub mod account_cli;
pub mod agent_setup;
pub mod ai;
pub mod claude_code;
pub mod codex;
pub use hoi4_setup_core::coding_environment;
pub mod context;
pub mod credentials;
pub mod descriptors;
pub mod flatten;
pub mod git;
#[cfg(any(target_os = "linux", test))]
mod linux_trust;
pub mod local_source;
pub mod mcp;
pub mod openrouter;
pub use hoi4_setup_core::merge;
pub mod meshy;
pub mod migrations;
pub use hoi4_setup_core::models;
pub use hoi4_setup_core::native_mcp;
pub mod paths;
pub mod portraits;
pub mod process;
mod project_profile;
pub mod python_lookup;
pub mod readiness;
mod safe_fs;
pub mod scanner;
pub use hoi4_setup_core::schemas;
pub mod security;
pub mod service;
pub mod source;
pub mod transaction;

pub use models::*;

pub use hoi4_setup_core::AppError;

#[cfg(feature = "desktop")]
pub fn run_desktop() {
    commands::run();
}

#[cfg(feature = "desktop")]
mod commands;

pub use hoi4_setup_core::live_testing;
