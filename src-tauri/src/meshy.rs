//! App-owned credential boundary for the exact supported Meshy MCP runtime.

use crate::mcp::{materialize_verified_package_tree, verify_materialized_package_tree};
use crate::security::{safe_join, sha256_file};
use crate::AppError;
use serde_json::Value;
use std::path::{Path, PathBuf};

const CLI_MARKER: &str = "--run-verified-meshy-mcp";
const PACKAGE_NAME: &str = "@meshy-ai/meshy-mcp-server";
const PACKAGE_VERSION: &str = "0.4.0";
const RUNTIME_TREE_SHA256: &str =
    "720075e2b1e266208f435b08d8ab81609f5e5e1a247ca4680c51b5a4f00f2011";
const RUNTIME_FILE_COUNT: u64 = 3916;
const RUNTIME_ENTRY: &str = "@meshy-ai/meshy-mcp-server/dist/index.js";
const MAX_NODE_BYTES: u64 = 128 * 1024 * 1024;

fn cli_requested(args: &[String]) -> bool {
    args.len() == 2 && args[1] == CLI_MARKER
}

fn installed_runtime_root() -> Result<PathBuf, AppError> {
    let local = std::env::var_os("LOCALAPPDATA")
        .filter(|value| !value.is_empty())
        .ok_or_else(|| AppError::Process("LOCALAPPDATA is unavailable".into()))?;
    let root = PathBuf::from(local)
        .join("HOI4 Mod Setup")
        .join("runtimes")
        .join(format!("meshy-{PACKAGE_VERSION}"))
        .join("node_modules");
    if !root.is_absolute() || !root.is_dir() {
        return Err(AppError::Process(
            "the reviewed Meshy runtime is not installed; run Repair".into(),
        ));
    }
    Ok(root)
}

fn copy_node_bytes(node: &Path, private_root: &Path) -> Result<PathBuf, AppError> {
    let parent = node
        .parent()
        .ok_or_else(|| AppError::Process("Node has no containing directory".into()))?;
    let filename = node
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| AppError::Process("Node has an invalid filename".into()))?;
    let bytes = crate::flatten::read_bounded_regular_file_no_follow_under_root(
        parent,
        filename,
        MAX_NODE_BYTES,
    )?;
    let private_node = private_root.join("node.exe");
    std::fs::write(&private_node, bytes)?;
    Ok(private_node)
}

fn copy_private_node(node: &Path, private_root: &Path) -> Result<PathBuf, AppError> {
    let private_node = copy_node_bytes(node, private_root)?;
    // Verify the exact copied bytes through the native Windows verifier route
    // and exact certificate simple-name comparison, then bind the spawn hash.
    crate::process::validate_executable_publisher(&private_node, "OpenJS Foundation")?;
    Ok(private_node)
}

fn launch() -> Result<i32, AppError> {
    if crate::models::Platform::current() != crate::models::Platform::Windows {
        return Err(AppError::UnsupportedPlatform(
            "the verified Meshy runtime is currently Windows-only".into(),
        ));
    }
    let key = std::env::var("MESHY_API_KEY")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| AppError::Credential("MESHY_API_KEY is missing".into()))?;
    std::env::remove_var("MESHY_API_KEY");

    let tree = materialize_verified_package_tree(&installed_runtime_root()?)?;
    if tree.sha256 != RUNTIME_TREE_SHA256 || tree.file_count != RUNTIME_FILE_COUNT {
        return Err(AppError::Credential(
            "the installed Meshy runtime does not match the reviewed lock".into(),
        ));
    }
    let package_path = safe_join(&tree.root, "@meshy-ai/meshy-mcp-server/package.json")?;
    let package: Value = serde_json::from_slice(&std::fs::read(package_path)?)?;
    if package.get("name").and_then(Value::as_str) != Some(PACKAGE_NAME)
        || package.get("version").and_then(Value::as_str) != Some(PACKAGE_VERSION)
    {
        return Err(AppError::Credential(
            "the private Meshy package identity is invalid".into(),
        ));
    }
    let node = crate::process::find_path_executable(&["node.exe", "node"])?;
    let private_root = tree
        .root
        .parent()
        .ok_or_else(|| AppError::Process("private Meshy root is invalid".into()))?;
    let private_node = copy_private_node(&node, private_root)?;
    let node_sha256 = sha256_file(&private_node)?;
    verify_materialized_package_tree(&tree)?;
    let entry = safe_join(&tree.root, RUNTIME_ENTRY)?;
    let result = crate::process::run_private_credential_stdio_proxy(
        &private_node,
        &node_sha256,
        &[entry.display().to_string()],
        "MESHY_API_KEY",
        &key,
    );
    drop(key);
    result
}

/// Return `None` for the desktop route or the MCP proxy exit code when the
/// signed app executable was explicitly invoked as the Meshy launcher.
pub fn run_cli_if_requested() -> Option<i32> {
    let args = std::env::args().collect::<Vec<_>>();
    if !cli_requested(&args) {
        return None;
    }
    Some(match launch() {
        Ok(code) => code,
        Err(error) => {
            eprintln!("Meshy MCP launch blocked: {error}");
            3
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn credential_route_accepts_only_the_exact_app_owned_cli_marker() {
        assert!(cli_requested(&["app.exe".into(), CLI_MARKER.into()]));
        assert!(!cli_requested(&[
            "app.exe".into(),
            CLI_MARKER.into(),
            "project-script.py".into()
        ]));
    }

    #[test]
    fn replacing_the_node_candidate_after_copy_cannot_change_private_bytes() {
        let source = tempfile::tempdir().unwrap();
        let private = tempfile::tempdir().unwrap();
        let node = source.path().join("node.exe");
        std::fs::write(&node, b"reviewed node bytes").unwrap();
        let copied = copy_node_bytes(&node, private.path()).unwrap();
        std::fs::write(&node, b"replacement node bytes").unwrap();
        assert_eq!(std::fs::read(copied).unwrap(), b"reviewed node bytes");
    }
}
