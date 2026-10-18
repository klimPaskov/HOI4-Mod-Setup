//! App-owned credential boundary for the exact supported Meshy MCP runtime.

use crate::mcp::{
    materialize_verified_package_tree, verify_materialized_package_tree, PrivateTreeLayout,
    VerifiedPackageTree,
};
use crate::security::{safe_join, sha256_file};
use crate::AppError;
use serde_json::Value;
use std::path::{Path, PathBuf};

const CLI_MARKER: &str = "--run-verified-meshy-mcp";
const PACKAGE_NAME: &str = "@meshy-ai/meshy-mcp-server";
const PACKAGE_VERSION: &str = "0.6.2";
const RUNTIME_TREE_SHA256: &str =
    "3ad84a9d8f6a1c4ccd97fe8cc15d710d9102911960664ece373d79f0b32430db";
const RUNTIME_FILE_COUNT: u64 = 3952;
const RUNTIME_ENTRY: &str = "@meshy-ai/meshy-mcp-server/dist/index.js";
const MAX_NODE_BYTES: u64 = 128 * 1024 * 1024;
/// Runtime-read variables pinned for the reviewed 0.6.2 runtime. It runs in
/// the project folder (downloads default to `<cwd>/meshy_output`) and calls
/// `dotenv.config()`, which adds a project `.env` to the environment but never
/// overrides a variable that already exists. Without these pins, project
/// content could switch the key-bearing server to its HTTP listener, route
/// authenticated requests through a proxy (`proxy-from-env` honors
/// `NO_PROXY=*` before any proxy variable), or disable TLS verification.
/// `MESHY_API_HOST` is read before `dotenv` runs and is pinned defensively.
pub(crate) const PINNED_RUNTIME_ENVIRONMENT: [(&str, &str); 4] = [
    ("TRANSPORT", "stdio"),
    ("MESHY_API_HOST", "https://api.meshy.ai"),
    ("NO_PROXY", "*"),
    ("NODE_TLS_REJECT_UNAUTHORIZED", "1"),
];

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

/// Copy the installed runtime into a private directory named `node_modules`,
/// because the reviewed tree is the hoisted npm dependency directory and its
/// entry package imports sibling packages such as `dotenv` by bare specifier.
/// The private copy must match the reviewed tree identity and package.
fn materialize_reviewed_runtime(
    runtime_root: &Path,
    tree_sha256: &str,
    file_count: u64,
) -> Result<VerifiedPackageTree, AppError> {
    let tree = materialize_verified_package_tree(runtime_root, PrivateTreeLayout::NodeModules)?;
    if tree.sha256 != tree_sha256 || tree.file_count != file_count {
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
    Ok(tree)
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

    let tree = materialize_reviewed_runtime(
        &installed_runtime_root()?,
        RUNTIME_TREE_SHA256,
        RUNTIME_FILE_COUNT,
    )?;
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
        &PINNED_RUNTIME_ENVIRONMENT,
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

    fn write_meshy_runtime_fixture(root: &Path) {
        crate::mcp::write_hoisted_dependency_fixture(root);
        let package = root.join("@meshy-ai/meshy-mcp-server");
        std::fs::create_dir_all(package.join("dist")).unwrap();
        std::fs::write(
            package.join("package.json"),
            format!(r#"{{"name":"{PACKAGE_NAME}","version":"{PACKAGE_VERSION}","type":"module"}}"#),
        )
        .unwrap();
        std::fs::write(
            package.join("dist/index.js"),
            "import value from \"sibling-dep\";
process.stdout.write(value);
",
        )
        .unwrap();
    }

    fn fixture_identity(root: &Path) -> (String, u64) {
        let tree = materialize_verified_package_tree(root, PrivateTreeLayout::Package).unwrap();
        (tree.sha256.clone(), tree.file_count)
    }

    #[test]
    fn reviewed_runtime_entry_resolves_hoisted_dependencies_under_real_node() {
        let runtime = tempfile::tempdir().unwrap();
        write_meshy_runtime_fixture(runtime.path());
        let (sha256, count) = fixture_identity(runtime.path());
        let tree = materialize_reviewed_runtime(runtime.path(), &sha256, count).unwrap();
        assert_eq!(tree.root.file_name().unwrap(), "node_modules");
        // The private Node copy lands beside the verified tree, so the
        // pre-spawn rehash still covers exactly the reviewed runtime.
        let source = tempfile::tempdir().unwrap();
        std::fs::write(source.path().join("node.exe"), b"node bytes").unwrap();
        let private_node =
            copy_node_bytes(&source.path().join("node.exe"), tree.root.parent().unwrap()).unwrap();
        assert!(!private_node.starts_with(&tree.root));
        verify_materialized_package_tree(&tree).unwrap();
        let output = crate::mcp::run_fixture_entry(&safe_join(&tree.root, RUNTIME_ENTRY).unwrap());
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(output.stdout, crate::mcp::HOISTED_FIXTURE_OUTPUT.as_bytes());
    }

    #[test]
    fn runtime_with_a_different_tree_identity_is_rejected() {
        let runtime = tempfile::tempdir().unwrap();
        write_meshy_runtime_fixture(runtime.path());
        let (sha256, count) = fixture_identity(runtime.path());
        let blocked = |sha: &str, files: u64| {
            materialize_reviewed_runtime(runtime.path(), sha, files)
                .err()
                .unwrap()
                .to_string()
        };
        assert!(blocked(&"0".repeat(64), count).contains("reviewed lock"));
        assert!(blocked(&sha256, count + 1).contains("reviewed lock"));
        std::fs::write(
            runtime.path().join("dotenv.js"),
            b"extra
",
        )
        .unwrap();
        assert!(blocked(&sha256, count).contains("reviewed lock"));
    }

    #[test]
    fn runtime_with_a_different_package_identity_is_rejected() {
        let runtime = tempfile::tempdir().unwrap();
        write_meshy_runtime_fixture(runtime.path());
        std::fs::write(
            runtime
                .path()
                .join("@meshy-ai/meshy-mcp-server/package.json"),
            format!(r#"{{"name":"{PACKAGE_NAME}","version":"0.4.0","type":"module"}}"#),
        )
        .unwrap();
        let (sha256, count) = fixture_identity(runtime.path());
        assert!(materialize_reviewed_runtime(runtime.path(), &sha256, count)
            .err()
            .unwrap()
            .to_string()
            .contains("package identity"));
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
