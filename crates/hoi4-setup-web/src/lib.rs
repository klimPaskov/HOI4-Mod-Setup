//! WebAssembly bindings for the HOI4 Mod Setup web edition.
//!
//! The browser keeps the dropped mod folder in a [`Project`], downloads the
//! manifest and selected source files itself, and hands them to [`Source`],
//! which verifies every file against the manifest's SHA-256 evidence before
//! anything is adapted. [`Export`] compares the result with the dropped folder
//! and its installation lock, offers the desktop's conflict choices, and
//! writes the ZIP. Everything runs in the page; nothing is uploaded.
//!
//! Values cross the boundary as JSON strings and byte arrays.

mod assistant;
mod build;
mod context_files;
mod export;
mod scan;
mod sign_in;
mod source;

pub use assistant::{analysis_turn, description_turn, read_analysis_reply, read_description_reply};
pub use context_files::context_document;
pub use export::Export;
pub use scan::Project;
pub use sign_in::{open_router_key, open_router_key_request, open_router_sign_in};
pub use source::Source;

use hoi4_setup_core::models::Platform;
use hoi4_setup_core::AppError;
use serde::Serialize;
use wasm_bindgen::JsError;

pub(crate) fn js(error: AppError) -> JsError {
    JsError::new(&error.to_string())
}

pub(crate) fn to_json<T: Serialize>(value: &T) -> Result<String, JsError> {
    serde_json::to_string(value).map_err(|error| JsError::new(&error.to_string()))
}

pub(crate) fn platform_of(name: &str) -> Result<Platform, AppError> {
    match name {
        "windows" => Ok(Platform::Windows),
        "macos" => Ok(Platform::Macos),
        "linux" => Ok(Platform::Linux),
        _ => Err(AppError::InvalidInput(
            "choose Windows, macOS, or Linux".into(),
        )),
    }
}

/// Components that need the desktop app's installer or launcher: the 3D
/// workflow runs Meshy through the app, and Live Tools installs its MCP server.
pub(crate) const DESKTOP_ONLY_COMPONENTS: &[&str] = &["workflow.3d", "workflow.live_tools"];
