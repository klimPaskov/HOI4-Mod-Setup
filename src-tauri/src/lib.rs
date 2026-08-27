//! Platform-neutral domain core for HOI4 Mod Setup.
//!
//! The React surface is intentionally a client of these types and commands.
//! It never receives filesystem or credential authority directly.

pub mod ai;
pub mod chat_sources;
pub mod codex;
pub mod coding_environment;
pub mod credentials;
pub mod descriptors;
pub mod flatten;
pub mod git;
pub mod mcp;
pub mod merge;
pub mod meshy;
pub mod migrations;
pub mod models;
pub mod paths;
pub mod portraits;
pub mod process;
pub mod readiness;
pub mod scanner;
pub mod security;
pub mod source;
pub mod transaction;

pub use models::*;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("invalid input: {0}")]
    InvalidInput(String),
    #[error("path security error: {0}")]
    PathSecurity(String),
    #[error("source error: {0}")]
    Source(String),
    #[error("scan error: {0}")]
    Scan(String),
    #[error("merge error: {0}")]
    Merge(String),
    #[error("transaction error: {0}")]
    Transaction(String),
    #[error("credential error: {0}")]
    Credential(String),
    #[error("process error: {0}")]
    Process(String),
    #[error("serialization error: {0}")]
    Serialization(String),
    #[error("unsupported platform: {0}")]
    UnsupportedPlatform(String),
}

impl From<std::io::Error> for AppError {
    fn from(value: std::io::Error) -> Self {
        Self::Transaction(value.to_string())
    }
}

impl From<serde_json::Error> for AppError {
    fn from(value: serde_json::Error) -> Self {
        Self::Serialization(value.to_string())
    }
}

#[cfg(feature = "desktop")]
pub fn run_desktop() {
    commands::run();
}

#[cfg(feature = "desktop")]
mod commands;
