//! Bounded health probes for manifest-declared MCP servers.
//!
//! The probe starts only the reviewed command recorded by the locked source
//! manifest, performs MCP initialization and a required read-only tools/list
//! request, then terminates the child.
//! It never calls an MCP tool and never receives credential environment
//! variables.

use crate::codex::{JsonlTransport, ProcessJsonlTransport};
use crate::models::{ExternalAction, Platform, RemoteManifest};
use crate::process::find_path_executable;
use crate::security::{is_link_metadata, path_has_link_component, safe_join, sha256_file};
use crate::AppError;
use serde::Serialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::time::Duration;

const MCP_PROTOCOL_VERSION: &str = "2025-11-25";

struct McpProtocol<T: JsonlTransport> {
    transport: T,
    next_id: u64,
    timeout: Duration,
}

impl<T: JsonlTransport> McpProtocol<T> {
    fn new(transport: T, timeout: Duration) -> Self {
        Self {
            transport,
            next_id: 1,
            timeout: timeout.max(Duration::from_millis(100)),
        }
    }

    fn request(&mut self, method: &str, params: Value) -> Result<Value, AppError> {
        let id = self.next_id;
        self.next_id = self.next_id.saturating_add(1);
        self.transport.send(&json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params
        }))?;
        let started = std::time::Instant::now();
        loop {
            let remaining = self.timeout.saturating_sub(started.elapsed());
            if remaining.is_zero() {
                return Err(AppError::Process(format!("MCP {method} timed out")));
            }
            let message = self
                .transport
                .receive(remaining)
                .map_err(|error| match error {
                    // The shared JSONL transport names the Codex App Server in
                    // its errors; report the MCP server instead.
                    AppError::Process(message) if message.contains("timed out") => {
                        AppError::Process(format!(
                            "the HOI4 Agent Tools MCP server did not answer {method} within {} seconds",
                            self.timeout.as_secs()
                        ))
                    }
                    other => other,
                })?
                .ok_or_else(|| AppError::Process(format!("MCP closed during {method}")))?;
            if message.get("id") != Some(&json!(id)) {
                continue;
            }
            if message.get("jsonrpc").and_then(Value::as_str) != Some("2.0") {
                return Err(AppError::Process(
                    "MCP returned an invalid JSON-RPC envelope".into(),
                ));
            }
            if message.get("error").is_some() {
                return Err(AppError::Process(format!("MCP {method} failed")));
            }
            return message
                .get("result")
                .cloned()
                .ok_or_else(|| AppError::Process(format!("MCP {method} omitted result")));
        }
    }

    fn initialize(&mut self) -> Result<Value, AppError> {
        let result = self.request(
            "initialize",
            json!({
                "protocolVersion": MCP_PROTOCOL_VERSION,
                "capabilities": {},
                "clientInfo": {
                    "name": "hoi4-mod-setup",
                    "version": env!("CARGO_PKG_VERSION")
                }
            }),
        )?;
        self.transport.send(&json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized",
            "params": {}
        }))?;
        Ok(result)
    }
}

impl<T: JsonlTransport> Drop for McpProtocol<T> {
    fn drop(&mut self) {
        self.transport.close();
    }
}

pub const COMPONENT_ID: &str = "mcp.hoi4_agent_tools";
/// The optional HOI4 Live Tools workflow, whose reviewed action installs the
/// pinned release for the current user (it is not an MCP package of this app).
pub const LIVE_TOOLS_COMPONENT_ID: &str = "workflow.live_tools";
/// A first start right after installation loads native libraries that
/// antivirus scanning may hold for many seconds.
const MCP_HEALTH_TIMEOUT: Duration = Duration::from_secs(60);
pub const HEALTH_RULE_ID: &str = "mcp.hoi4.health";
const MAX_SERVER_FIELD_BYTES: usize = 256;
const MAX_TOOL_COUNT: usize = 4096;
const MAX_TOOL_NAME_BYTES: usize = 256;
const MAX_PACKAGE_FILE_BYTES: u64 = 32 * 1024 * 1024;
const MAX_PACKAGE_TREE_BYTES: u64 = 512 * 1024 * 1024;

pub(crate) struct VerifiedPackageTree {
    _temporary: tempfile::TempDir,
    pub(crate) root: PathBuf,
    pub(crate) sha256: String,
    pub(crate) file_count: u64,
    symlinks: BTreeMap<String, String>,
}

fn read_installed_package_tree(root: &Path) -> Result<Vec<(String, Vec<u8>)>, AppError> {
    read_installed_package_tree_with_links(root, &BTreeMap::new())
}

fn contained_link_target(relative: &str, target: &str) -> Result<String, AppError> {
    crate::security::normalize_relative_path(relative)?;
    if target.is_empty()
        || target.len() > 4096
        || target.starts_with('/')
        || target.contains(['\\', '\0', ':'])
    {
        return Err(AppError::PathSecurity(
            "MCP package link target is invalid".into(),
        ));
    }
    let mut parts = relative.split('/').collect::<Vec<_>>();
    parts.pop();
    for part in target.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                if parts.pop().is_none() {
                    return Err(AppError::PathSecurity(
                        "MCP package link target escapes its root".into(),
                    ));
                }
            }
            part => parts.push(part),
        }
    }
    crate::security::normalize_relative_path(&parts.join("/"))
}

fn read_installed_package_tree_with_links(
    root: &Path,
    symlinks: &BTreeMap<String, String>,
) -> Result<Vec<(String, Vec<u8>)>, AppError> {
    if path_has_link_component(root) || !root.is_dir() {
        return Err(AppError::PathSecurity(
            "the installed MCP package root is not a link-free directory".into(),
        ));
    }
    let mut pending = vec![root.to_path_buf()];
    let mut files = Vec::new();
    let mut observed_links = BTreeSet::new();
    let mut retained_bytes = 0u64;
    let mut visited_entries = 0usize;
    while let Some(directory) = pending.pop() {
        for entry in std::fs::read_dir(&directory)? {
            visited_entries += 1;
            if visited_entries > 20_000 {
                return Err(AppError::PathSecurity(
                    "the installed MCP package exceeds its entry bound".into(),
                ));
            }
            let entry = entry?;
            let path = entry.path();
            let metadata = std::fs::symlink_metadata(&path)?;
            if is_link_metadata(&metadata) {
                let relative = path
                    .strip_prefix(root)
                    .map_err(|_| AppError::PathSecurity("MCP link escaped package".into()))?
                    .to_string_lossy()
                    .replace('\\', "/");
                let expected = symlinks.get(&relative).ok_or_else(|| {
                    AppError::PathSecurity(
                        "the installed MCP package contains an undeclared link".into(),
                    )
                })?;
                if !metadata.file_type().is_symlink()
                    || std::fs::read_link(&path)?.to_str() != Some(expected.as_str())
                {
                    return Err(AppError::PathSecurity(
                        "the installed MCP package link differs from the reviewed identity".into(),
                    ));
                }
                let destination = contained_link_target(&relative, expected)?;
                // This read proves that the declared target is a contained,
                // regular, link-free file; never read through the link itself.
                crate::flatten::read_bounded_regular_file_no_follow_under_root(
                    root,
                    &destination,
                    MAX_PACKAGE_FILE_BYTES,
                )?;
                observed_links.insert(relative.clone());
                let mut bytes = b"symlink\0".to_vec();
                bytes.extend(expected.as_bytes());
                retained_bytes = retained_bytes
                    .checked_add(bytes.len() as u64)
                    .ok_or_else(|| AppError::PathSecurity("MCP package size overflowed".into()))?;
                files.push((relative, bytes));
                continue;
            }
            if metadata.is_dir() {
                pending.push(path);
            } else if metadata.is_file() {
                let relative = path
                    .strip_prefix(root)
                    .map_err(|_| {
                        AppError::PathSecurity("MCP package path escaped its root".into())
                    })?
                    .to_string_lossy()
                    .replace('\\', "/");
                let bytes = crate::flatten::read_bounded_regular_file_no_follow_under_root(
                    root,
                    &relative,
                    MAX_PACKAGE_FILE_BYTES,
                )?;
                retained_bytes = retained_bytes
                    .checked_add(bytes.len() as u64)
                    .ok_or_else(|| AppError::PathSecurity("MCP package size overflowed".into()))?;
                if retained_bytes > MAX_PACKAGE_TREE_BYTES {
                    return Err(AppError::PathSecurity(
                        "the installed MCP package exceeds its verification byte bound".into(),
                    ));
                }
                files.push((relative, bytes));
            } else {
                return Err(AppError::PathSecurity(
                    "the installed MCP package contains a special file".into(),
                ));
            }
        }
        if files.len() > 10_000 {
            return Err(AppError::PathSecurity(
                "the installed MCP package exceeds its file bound".into(),
            ));
        }
    }
    if observed_links != symlinks.keys().cloned().collect() {
        return Err(AppError::PathSecurity(
            "a declared MCP package link is missing".into(),
        ));
    }
    files.sort_by(|left, right| left.0.cmp(&right.0));
    let total_bytes = files.iter().try_fold(0_u64, |total, (_, bytes)| {
        total.checked_add(bytes.len() as u64).ok_or_else(|| {
            AppError::PathSecurity("the installed MCP package size overflowed".into())
        })
    })?;
    if total_bytes > MAX_PACKAGE_TREE_BYTES || files.len() > 10_000 {
        return Err(AppError::PathSecurity(
            "the installed MCP package exceeds its verification bounds".into(),
        ));
    }
    Ok(files)
}

fn package_tree_identity(files: &[(String, Vec<u8>)]) -> (String, u64) {
    let mut digest = Sha256::new();
    for (relative, bytes) in files {
        digest.update(relative.as_bytes());
        digest.update([0]);
        digest.update(bytes.len().to_string().as_bytes());
        digest.update([0]);
        digest.update(bytes);
    }
    (format!("{:x}", digest.finalize()), files.len() as u64)
}

/// Hash a link-free installed package directory without copying it. This is
/// used for generated runtimes whose files are not part of the downloaded
/// component manifest, such as the GPTImage dependency tree.
pub(crate) fn package_tree_sha256(root: &Path) -> Result<String, AppError> {
    let files = read_installed_package_tree(root)?;
    Ok(package_tree_identity(&files).0)
}

fn verify_optional_npm_lock_integrity(
    bin_root: &Path,
    package_name: &str,
    package_integrity: &str,
) -> Result<(), AppError> {
    let lock_relative = "node_modules/.package-lock.json";
    let lock_path = safe_join(bin_root, lock_relative)?;
    match std::fs::symlink_metadata(&lock_path) {
        Ok(metadata) => {
            if is_link_metadata(&metadata)
                || !metadata.is_file()
                || path_has_link_component(&lock_path)
            {
                return Err(AppError::PathSecurity(
                    "the installed MCP npm lock is not a regular, link-free file".into(),
                ));
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
    }
    let lock_bytes = crate::flatten::read_bounded_regular_file_no_follow_under_root(
        bin_root,
        lock_relative,
        16 * 1024 * 1024,
    )?;
    let lock: Value = serde_json::from_slice(&lock_bytes)?;
    let lock_key = format!("node_modules/{package_name}");
    let installed_integrity = lock
        .get("packages")
        .and_then(|packages| packages.get(&lock_key))
        .and_then(|package| package.get("integrity"))
        .and_then(Value::as_str);
    if installed_integrity != Some(package_integrity) {
        return Err(AppError::Credential(
            "the installed MCP package does not match the reviewed registry integrity".into(),
        ));
    }
    Ok(())
}

pub(crate) fn materialize_verified_package_tree(
    root: &Path,
) -> Result<VerifiedPackageTree, AppError> {
    materialize_verified_package_tree_with_links(root, &BTreeMap::new())
}

fn materialize_verified_package_tree_with_links(
    root: &Path,
    symlinks: &BTreeMap<String, String>,
) -> Result<VerifiedPackageTree, AppError> {
    let files = read_installed_package_tree_with_links(root, symlinks)?;
    let (sha256, file_count) = package_tree_identity(&files);
    let temporary = tempfile::tempdir()?;
    let private_root = temporary.path().join("package");
    std::fs::create_dir(&private_root)?;
    for (relative, bytes) in &files {
        let destination = safe_join(&private_root, relative)?;
        if let Some(parent) = destination.parent() {
            std::fs::create_dir_all(parent)?;
        }
        if let Some(target) = symlinks.get(relative) {
            #[cfg(unix)]
            std::os::unix::fs::symlink(target, destination)?;
            #[cfg(not(unix))]
            {
                let _ = target;
                return Err(AppError::UnsupportedPlatform(
                    "declared package symlinks require Unix".into(),
                ));
            }
        } else {
            std::fs::write(destination, bytes)?;
        }
    }
    let copied = read_installed_package_tree_with_links(&private_root, symlinks)?;
    let copied_identity = package_tree_identity(&copied);
    if copied_identity != (sha256.clone(), file_count) {
        return Err(AppError::PathSecurity(
            "the private MCP runtime copy changed during materialization".into(),
        ));
    }
    Ok(VerifiedPackageTree {
        _temporary: temporary,
        root: private_root,
        sha256,
        file_count,
        symlinks: symlinks.clone(),
    })
}

pub(crate) fn verify_materialized_package_tree(tree: &VerifiedPackageTree) -> Result<(), AppError> {
    let copied = read_installed_package_tree_with_links(&tree.root, &tree.symlinks)?;
    if package_tree_identity(&copied) != (tree.sha256.clone(), tree.file_count) {
        return Err(AppError::PathSecurity(
            "the private MCP runtime tree changed before execution".into(),
        ));
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize)]
pub struct HealthEvidence {
    pub protocol_version: String,
    pub server_name: String,
    pub server_version: String,
    pub tool_count: usize,
    pub required_tools: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedMcpTarget {
    pub target: String,
    pub package_name: String,
    pub package_version: String,
    pub package_integrity: String,
    pub package_tree_sha256: String,
    pub package_file_count: u64,
    pub runtime_entry: String,
    pub runtime_entry_sha256: String,
    pub runtime_entry_size: u64,
    pub package_symlinks: BTreeMap<String, String>,
    pub required_tools: Vec<String>,
}

/// Parse only the managed HOI4 Agent Tools registration. Unrelated client
/// routes never authorize execution and are left to ordinary conflict review.
pub fn client_registration(
    bytes: &[u8],
    destination: &str,
) -> Result<Option<(String, Vec<String>)>, AppError> {
    let invalid = || AppError::Process("the managed MCP client registration is malformed".into());
    if bytes.len() > 1024 * 1024 {
        return Err(invalid());
    }
    let value: Value = if destination == ".codex/config.toml" {
        let config: toml::Value = std::str::from_utf8(bytes)
            .map_err(|_| invalid())?
            .parse()
            .map_err(|_| invalid())?;
        serde_json::to_value(config).map_err(|_| invalid())?
    } else {
        serde_json::from_slice(bytes).map_err(|_| invalid())?
    };
    let key = match destination {
        ".codex/config.toml" => "mcp_servers",
        "opencode.json" => "mcp",
        ".mcp.json" | ".cursor/mcp.json" | ".qoder/mcp.json" => "mcpServers",
        _ => return Err(invalid()),
    };
    let Some(route) = value.get(key).and_then(|v| v.get("hoi4_agent_tools")) else {
        return Ok(None);
    };
    for name in ["enabled", "disabled"] {
        if route.get(name).is_some_and(|v| !v.is_boolean()) {
            return Err(invalid());
        }
    }
    if route["enabled"] == false || route["disabled"] == true {
        return Ok(None);
    }
    let (command, args) = if key == "mcp" {
        let command_line = route["command"].as_array().ok_or_else(invalid)?;
        let command = command_line
            .first()
            .and_then(Value::as_str)
            .ok_or_else(invalid)?
            .to_owned();
        let args = command_line[1..]
            .iter()
            .map(|v| v.as_str().map(str::to_owned).ok_or_else(invalid))
            .collect::<Result<Vec<_>, _>>()?;
        (command, args)
    } else {
        let command = route["command"].as_str().ok_or_else(invalid)?.to_owned();
        let args = route
            .get("args")
            .map(|v| {
                v.as_array().ok_or_else(invalid).and_then(|items| {
                    items
                        .iter()
                        .map(|v| v.as_str().map(str::to_owned).ok_or_else(invalid))
                        .collect::<Result<Vec<_>, _>>()
                })
            })
            .transpose()?
            .unwrap_or_default();
        (command, args)
    };
    if command.is_empty()
        || command.len() > 4096
        || command.chars().any(char::is_control)
        || args.len() > 256
        || args
            .iter()
            .any(|v| v.len() > 4096 || v.chars().any(char::is_control))
    {
        return Err(invalid());
    }
    Ok(Some((command, args)))
}

fn required_parameter<'a>(
    rule: &'a crate::models::ValidationRule,
    key: &str,
) -> Result<&'a str, AppError> {
    rule.parameters
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| AppError::Source(format!("the MCP route is missing {key}")))
}

fn validate_package_identity(
    package_name: &str,
    package_version: &str,
    package_integrity: &str,
    package_tree_sha256: &str,
    package_file_count: u64,
    runtime_entry: &str,
    required_tools: &[String],
) -> Result<(), AppError> {
    let strict_version = package_version.split('.').collect::<Vec<_>>();
    let strict_version = strict_version.len() == 3
        && strict_version.iter().all(|part| {
            !part.is_empty()
                && part.chars().all(|character| character.is_ascii_digit())
                && (*part == "0" || !part.starts_with('0'))
        });
    if package_name != "hoi4-agent-tools"
        || !strict_version
        || !package_integrity.starts_with("sha512-")
        || package_integrity.len() > 256
        || !package_integrity[7..].chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '+' | '/' | '=')
        })
        || runtime_entry != "dist/bin/stdio.js"
        || crate::source::validate_sha256(package_tree_sha256).is_err()
        || !(1..=10_000).contains(&package_file_count)
        || required_tools.is_empty()
        || required_tools.len() > MAX_TOOL_COUNT
    {
        return Err(AppError::Source(
            "the MCP package identity or required tool declaration is invalid".into(),
        ));
    }
    let mut unique = BTreeSet::new();
    for tool in required_tools {
        if tool.len() > MAX_TOOL_NAME_BYTES
            || !tool.starts_with("hoi4.")
            || tool.chars().any(char::is_control)
            || !unique.insert(tool.as_str())
        {
            return Err(AppError::Source(
                "the MCP required tool declaration is invalid or duplicated".into(),
            ));
        }
    }
    // The manifest's required tool list belongs to the exact pinned package
    // version, so the app never adds its own route names: a later server
    // release may rename or consolidate routes.
    Ok(())
}

/// Return the one command target declared by the verified manifest.
pub fn manifest_target(manifest: &RemoteManifest) -> Result<VerifiedMcpTarget, AppError> {
    manifest_target_for_platform(manifest, Platform::Windows)
}

pub fn manifest_target_for_platform(
    manifest: &RemoteManifest,
    platform: Platform,
) -> Result<VerifiedMcpTarget, AppError> {
    let component = manifest
        .components
        .iter()
        .find(|component| component.id == COMPONENT_ID)
        .ok_or_else(|| {
            AppError::Source("the locked manifest does not declare the MCP component".into())
        })?;
    if !component
        .platforms
        .iter()
        .any(|declared| declared.supports(platform))
    {
        return Err(AppError::UnsupportedPlatform(
            "the MCP component has no verified route for this platform".into(),
        ));
    }
    let rules = component
        .validation
        .iter()
        .filter(|rule| rule.id == HEALTH_RULE_ID && rule.kind == "command")
        .collect::<Vec<_>>();
    if rules.len() != 1 {
        return Err(AppError::Source(
            "the MCP component does not declare one unambiguous health command".into(),
        ));
    }
    let mut rule = (*rules[0]).clone();
    if let Some(pins) = rule
        .parameters
        .get("platform_pins")
        .and_then(|v| v.get(platform.manifest_name()))
        .and_then(Value::as_object)
        .cloned()
    {
        for (key, value) in pins {
            rule.parameters.insert(key, value);
        }
    }
    let rules = [&rule];
    let target = rules[0]
        .parameters
        .get("platform_targets")
        .and_then(|v| v.get(platform.manifest_name()))
        .and_then(Value::as_str)
        .or_else(|| rules[0].target.as_deref())
        .ok_or_else(|| AppError::Source("the MCP health command target is missing".into()))?;
    validate_bare_command_name(target)?;
    let runtime_entry_sha256 = rules[0]
        .parameters
        .get("runtime_entry_sha256")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            AppError::UnsupportedPlatform(
                "the MCP route has no immutable runtime-entry SHA-256 evidence".into(),
            )
        })?;
    crate::source::validate_sha256(runtime_entry_sha256)?;
    let runtime_entry_size = rules[0]
        .parameters
        .get("runtime_entry_size")
        .and_then(Value::as_u64)
        .ok_or_else(|| {
            AppError::UnsupportedPlatform(
                "the MCP route has no immutable runtime-entry size evidence".into(),
            )
        })?;
    let package_name = required_parameter(rules[0], "package_name")?;
    let package_version = required_parameter(rules[0], "package_version")?;
    let package_integrity = required_parameter(rules[0], "package_integrity")?;
    let runtime_entry = required_parameter(rules[0], "runtime_entry")?;
    let package_tree_sha256 = required_parameter(rules[0], "package_tree_sha256")?;
    let package_file_count = rules[0]
        .parameters
        .get("package_file_count")
        .and_then(Value::as_u64)
        .ok_or_else(|| AppError::Source("the MCP route is missing package_file_count".into()))?;
    let required_tools = rules[0]
        .parameters
        .get("required_tools")
        .and_then(Value::as_array)
        .ok_or_else(|| AppError::Source("the MCP route has no required tool list".into()))?
        .iter()
        .map(|value| {
            value
                .as_str()
                .map(ToOwned::to_owned)
                .ok_or_else(|| AppError::Source("the MCP required tool list is invalid".into()))
        })
        .collect::<Result<Vec<_>, _>>()?;
    validate_package_identity(
        package_name,
        package_version,
        package_integrity,
        package_tree_sha256,
        package_file_count,
        runtime_entry,
        &required_tools,
    )?;
    let package_symlinks = rules[0]
        .parameters
        .get("package_symlinks")
        .cloned()
        .map(serde_json::from_value::<BTreeMap<String, String>>)
        .transpose()?
        .unwrap_or_default();
    if package_symlinks.len() > 32 {
        return Err(AppError::Source(
            "too many declared MCP package links".into(),
        ));
    }
    for (path, target) in &package_symlinks {
        contained_link_target(path, target)?;
    }
    Ok(VerifiedMcpTarget {
        target: target.to_owned(),
        package_name: package_name.to_owned(),
        package_version: package_version.to_owned(),
        package_integrity: package_integrity.to_owned(),
        package_tree_sha256: package_tree_sha256.to_owned(),
        package_file_count,
        runtime_entry: runtime_entry.to_owned(),
        runtime_entry_sha256: runtime_entry_sha256.to_owned(),
        runtime_entry_size,
        package_symlinks,
        required_tools,
    })
}

/// Extract the target from the external action captured in an installation
/// plan. This keeps transaction readiness bound to the exact source-declared
/// action that the user reviewed.
pub fn reviewed_plan_target(actions: &[ExternalAction]) -> Result<VerifiedMcpTarget, AppError> {
    let matches = actions
        .iter()
        .filter(|action| {
            action.component_id == COMPONENT_ID
                && action.id == format!("external.{COMPONENT_ID}.{HEALTH_RULE_ID}")
                && matches!(action.platform, Platform::Windows | Platform::Linux)
                && matches!(
                    action.command_source.as_str(),
                    "remote_manifest" | "repository_script"
                )
                && action.environment_names.is_empty()
                && action.arguments.len() == 1
        })
        .collect::<Vec<_>>();
    if matches.len() != 1 {
        return Err(AppError::Source(
            "the installation plan does not contain one reviewed MCP health action".into(),
        ));
    }
    let target = matches[0].arguments[0].as_str();
    validate_bare_command_name(target)?;
    let runtime_entry_sha256 = matches[0]
        .verified_executable_sha256
        .as_deref()
        .ok_or_else(|| {
            AppError::UnsupportedPlatform(
                "the reviewed MCP action has no immutable runtime-entry SHA-256 evidence".into(),
            )
        })?;
    crate::source::validate_sha256(runtime_entry_sha256)?;
    let runtime_entry_size = matches[0].verified_executable_size.ok_or_else(|| {
        AppError::UnsupportedPlatform(
            "the reviewed MCP action has no immutable runtime-entry size evidence".into(),
        )
    })?;
    let package_name = matches[0]
        .verified_package_name
        .as_deref()
        .ok_or_else(|| AppError::Source("the reviewed MCP action has no package name".into()))?;
    let package_version = matches[0]
        .verified_package_version
        .as_deref()
        .ok_or_else(|| AppError::Source("the reviewed MCP action has no package version".into()))?;
    let package_integrity = matches[0]
        .verified_package_integrity
        .as_deref()
        .ok_or_else(|| {
            AppError::Source("the reviewed MCP action has no package integrity".into())
        })?;
    let runtime_entry = matches[0]
        .verified_runtime_entry
        .as_deref()
        .ok_or_else(|| AppError::Source("the reviewed MCP action has no runtime entry".into()))?;
    let package_tree_sha256 = matches[0]
        .verified_package_tree_sha256
        .as_deref()
        .ok_or_else(|| {
            AppError::Source("the reviewed MCP action has no package-tree hash".into())
        })?;
    let package_file_count = matches[0].verified_package_file_count.ok_or_else(|| {
        AppError::Source("the reviewed MCP action has no package file count".into())
    })?;
    validate_package_identity(
        package_name,
        package_version,
        package_integrity,
        package_tree_sha256,
        package_file_count,
        runtime_entry,
        &matches[0].required_tool_names,
    )?;
    Ok(VerifiedMcpTarget {
        target: target.to_owned(),
        package_name: package_name.to_owned(),
        package_version: package_version.to_owned(),
        package_integrity: package_integrity.to_owned(),
        package_tree_sha256: package_tree_sha256.to_owned(),
        package_file_count,
        runtime_entry: runtime_entry.to_owned(),
        runtime_entry_sha256: runtime_entry_sha256.to_owned(),
        runtime_entry_size,
        package_symlinks: matches[0].verified_package_symlinks.clone(),
        required_tools: matches[0].required_tool_names.clone(),
    })
}

/// Start the exact package runtime behind the source-declared Windows wrapper.
/// The wrapper is used only to locate npm's global package root; it is never
/// executed. Node runs the manifest-hashed entry directly after package
/// version, npm integrity, entry size, and entry SHA-256 verification.
pub fn initialize_health(
    project_root: &Path,
    target: &VerifiedMcpTarget,
) -> Result<HealthEvidence, AppError> {
    if !matches!(Platform::current(), Platform::Windows | Platform::Linux) {
        return Err(AppError::UnsupportedPlatform(
            "the verified MCP route is currently supported only on Windows".into(),
        ));
    }
    validate_bare_command_name(&target.target)?;
    if !project_root.is_absolute() || !project_root.is_dir() {
        return Err(AppError::PathSecurity(
            "MCP health working directory must be an existing absolute directory".into(),
        ));
    }
    if path_has_link_component(project_root) {
        return Err(AppError::PathSecurity(
            "MCP health working directory contains a symlink or junction".into(),
        ));
    }
    let (bin_root, package_root) = if Platform::current() == Platform::Linux {
        let prefix = crate::paths::linux_npm_prefix()?;
        let package = prefix.join("lib/node_modules").join(&target.package_name);
        let locator = prefix.join("bin").join(&target.target);
        let expected = format!(
            "../lib/node_modules/{}/{}",
            target.package_name, target.runtime_entry
        );
        let metadata = std::fs::symlink_metadata(&locator)?;
        if !metadata.file_type().is_symlink()
            || std::fs::read_link(&locator)?.to_str() != Some(expected.as_str())
            || path_has_link_component(&prefix)
        {
            return Err(AppError::PathSecurity(
                "the native MCP locator differs from the reviewed package entry".into(),
            ));
        }
        (prefix.join("lib"), package)
    } else {
        let wrapper = resolved_wrapper(&target.target)?;
        let metadata = std::fs::symlink_metadata(&wrapper)?;
        if is_link_metadata(&metadata) || !metadata.is_file() || path_has_link_component(&wrapper) {
            return Err(AppError::PathSecurity(
                "the resolved MCP executable is not a regular, link-free file".into(),
            ));
        }
        let bin_root = wrapper
            .parent()
            .ok_or_else(|| AppError::Process("the MCP wrapper has no containing directory".into()))?
            .to_path_buf();
        let package = bin_root.join("node_modules").join(&target.package_name);
        (bin_root, package)
    };
    let package_relative = format!("node_modules/{}/package.json", target.package_name);
    let package_bytes = crate::flatten::read_bounded_regular_file_no_follow_under_root(
        &bin_root,
        &package_relative,
        1024 * 1024,
    )?;
    let package: Value = serde_json::from_slice(&package_bytes)?;
    if package.get("name").and_then(Value::as_str) != Some(target.package_name.as_str())
        || package.get("version").and_then(Value::as_str) != Some(target.package_version.as_str())
    {
        return Err(AppError::Credential(
            "the installed MCP package does not match the reviewed package identity".into(),
        ));
    }
    let verified_tree =
        materialize_verified_package_tree_with_links(&package_root, &target.package_symlinks)?;
    if verified_tree.sha256 != target.package_tree_sha256
        || verified_tree.file_count != target.package_file_count
    {
        return Err(AppError::Credential(
            "the installed MCP package tree does not match the reviewed release".into(),
        ));
    }
    verify_optional_npm_lock_integrity(&bin_root, &target.package_name, &target.package_integrity)?;
    let entry = safe_join(&verified_tree.root, &target.runtime_entry)?;
    let entry_metadata = std::fs::symlink_metadata(&entry)?;
    if is_link_metadata(&entry_metadata)
        || !entry_metadata.is_file()
        || path_has_link_component(&entry)
        || entry_metadata.len() != target.runtime_entry_size
        || sha256_file(&entry)? != target.runtime_entry_sha256
    {
        return Err(AppError::Credential(
            "the installed MCP runtime entry does not match the reviewed identity".into(),
        ));
    }
    let node = resolved_node()?;
    let node_sha256 = crate::process::verified_executable_sha256(&node, "OpenJS Foundation")?;
    let node_directory = node.parent().map(Path::to_path_buf).ok_or_else(|| {
        AppError::Process("the resolved Node executable has no containing directory".into())
    })?;
    let transport = ProcessJsonlTransport::start_command_with_path_and_identity(
        node.clone(),
        vec![entry.display().to_string()],
        Some(project_root.to_path_buf()),
        Some(node_directory),
        &node_sha256,
    )?;
    let mut protocol = McpProtocol::new(transport, MCP_HEALTH_TIMEOUT);
    let initialized = protocol.initialize()?;
    let evidence = validate_verified_initialize_result(&initialized, &target.package_version)?;
    let listing = protocol.request("tools/list", json!({}))?;
    let tool_count = validate_tools_result(&listing, &target.required_tools)?;
    Ok(HealthEvidence {
        tool_count,
        required_tools: target.required_tools.clone(),
        ..evidence
    })
}

fn validate_verified_initialize_result(
    initialized: &Value,
    expected_version: &str,
) -> Result<HealthEvidence, AppError> {
    let evidence = validate_initialize_result(initialized)?;
    if evidence.server_version != expected_version {
        return Err(AppError::Process(
            "MCP server version does not match the reviewed package identity".into(),
        ));
    }
    require_tools_capability(initialized)?;
    Ok(evidence)
}

fn require_tools_capability(initialized: &Value) -> Result<(), AppError> {
    if !initialized
        .get("capabilities")
        .and_then(Value::as_object)
        .is_some_and(|capabilities| capabilities.contains_key("tools"))
    {
        return Err(AppError::Process(
            "MCP initialize did not advertise the required tools capability".into(),
        ));
    }
    Ok(())
}

pub fn validate_initialize_result(value: &Value) -> Result<HealthEvidence, AppError> {
    let object = value
        .as_object()
        .ok_or_else(|| AppError::Process("MCP initialize returned a non-object result".into()))?;
    let protocol_version = bounded_string(object, "protocolVersion", "protocol version")?;
    if protocol_version != MCP_PROTOCOL_VERSION {
        return Err(AppError::Process(format!(
            "MCP negotiated unsupported protocol version {protocol_version}"
        )));
    }
    let server_info = object
        .get("serverInfo")
        .and_then(Value::as_object)
        .ok_or_else(|| AppError::Process("MCP initialize omitted serverInfo".into()))?;
    let server_name = bounded_string(server_info, "name", "server name")?;
    let server_version = bounded_string(server_info, "version", "server version")?;
    if object
        .get("capabilities")
        .and_then(Value::as_object)
        .is_none()
    {
        return Err(AppError::Process(
            "MCP initialize omitted capabilities".into(),
        ));
    }
    Ok(HealthEvidence {
        protocol_version,
        server_name,
        server_version,
        tool_count: 0,
        required_tools: Vec::new(),
    })
}

fn validate_tools_result(value: &Value, required_tools: &[String]) -> Result<usize, AppError> {
    let tools = value
        .get("tools")
        .and_then(Value::as_array)
        .ok_or_else(|| AppError::Process("MCP tools/list returned no tools array".into()))?;
    if tools.len() > MAX_TOOL_COUNT {
        return Err(AppError::Process(
            "MCP tools/list exceeded the bounded tool count".into(),
        ));
    }
    let mut names = BTreeSet::new();
    for tool in tools {
        let name = tool
            .get("name")
            .and_then(Value::as_str)
            .filter(|name| !name.trim().is_empty() && name.len() <= MAX_TOOL_NAME_BYTES)
            .ok_or_else(|| AppError::Process("MCP tools/list contained an invalid tool".into()))?;
        if name.contains('\0') {
            return Err(AppError::Process(
                "MCP tools/list contained an invalid tool name".into(),
            ));
        }
        if !names.insert(name) {
            return Err(AppError::Process(
                "MCP tools/list contained a duplicate tool name".into(),
            ));
        }
    }
    let missing = required_tools
        .iter()
        .filter(|required| !names.contains(required.as_str()))
        .cloned()
        .collect::<Vec<_>>();
    if !missing.is_empty() {
        return Err(AppError::Process(format!(
            "MCP tools/list omitted source-advertised routes: {}",
            missing.join(", ")
        )));
    }
    Ok(tools.len())
}

fn bounded_string(
    object: &serde_json::Map<String, Value>,
    key: &str,
    label: &str,
) -> Result<String, AppError> {
    let value = object
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty() && value.len() <= MAX_SERVER_FIELD_BYTES)
        .ok_or_else(|| AppError::Process(format!("MCP initialize returned an invalid {label}")))?;
    if value.contains('\0') {
        return Err(AppError::Process(format!(
            "MCP initialize returned an invalid {label}"
        )));
    }
    Ok(value.to_owned())
}

fn validate_bare_command_name(value: &str) -> Result<(), AppError> {
    if value.trim().is_empty()
        || value.len() > 256
        || value.contains('\0')
        || value
            .chars()
            .any(|character| matches!(character, '/' | '\\' | ':'))
        || matches!(value, "." | "..")
        || PathBuf::from(value).components().count() != 1
    {
        return Err(AppError::Source(
            "the MCP command target must be one bare executable name".into(),
        ));
    }
    Ok(())
}

fn resolved_wrapper(target: &str) -> Result<PathBuf, AppError> {
    if let Ok(path) = find_path_executable(&[target]) {
        return Ok(path);
    }
    #[cfg(target_os = "windows")]
    {
        let appdata = std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .ok_or_else(|| {
                AppError::Process("the current-user npm directory is unavailable".into())
            })?;
        let candidate = appdata.join("npm").join(target);
        let metadata = std::fs::symlink_metadata(&candidate)?;
        if !is_link_metadata(&metadata)
            && metadata.is_file()
            && !path_has_link_component(&candidate)
        {
            return std::fs::canonicalize(candidate).map_err(AppError::from);
        }
    }
    Err(AppError::Process(
        "the reviewed MCP package command is not installed".into(),
    ))
}

fn resolved_node() -> Result<PathBuf, AppError> {
    if let Ok(path) = find_path_executable(&["node.exe", "node"]) {
        return Ok(path);
    }
    #[cfg(target_os = "windows")]
    {
        let mut candidates = Vec::new();
        if let Some(local) = std::env::var_os("LOCALAPPDATA") {
            candidates.push(PathBuf::from(local).join("Programs/nodejs/node.exe"));
        }
        if let Some(program_files) = std::env::var_os("ProgramFiles") {
            candidates.push(PathBuf::from(program_files).join("nodejs/node.exe"));
        }
        for candidate in candidates {
            if let Ok(metadata) = std::fs::symlink_metadata(&candidate) {
                if !is_link_metadata(&metadata)
                    && metadata.is_file()
                    && !path_has_link_component(&candidate)
                {
                    return std::fs::canonicalize(candidate).map_err(AppError::from);
                }
            }
        }
    }
    Err(AppError::Process(
        "the reviewed MCP package requires Node.js LTS".into(),
    ))
}

#[cfg(any(target_os = "windows", test))]
const MAX_NATIVE_SCAN_ENTRIES: usize = 20_000;

/// Native libraries of the installed MCP package that a running MCP client
/// has loaded, when the reviewed setup must replace that package. The
/// bootstrap keeps an installation whose complete tree already matches the
/// reviewed release and otherwise removes and reinstalls it, and npm cannot
/// replace a loaded library, so setup would otherwise fail only after the
/// project files were applied.
pub fn loaded_native_files_blocking_replacement(
    target: &VerifiedMcpTarget,
) -> Result<Vec<String>, AppError> {
    #[cfg(target_os = "windows")]
    {
        let Some(appdata) = std::env::var_os("APPDATA") else {
            return Ok(Vec::new());
        };
        let root = PathBuf::from(appdata)
            .join("npm")
            .join("node_modules")
            .join(&target.package_name);
        loaded_native_files_in(&root, target)
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = target;
        Ok(Vec::new())
    }
}

#[cfg(any(target_os = "windows", test))]
fn installed_package_version(package_root: &Path) -> Option<String> {
    let bytes = std::fs::read(package_root.join("package.json")).ok()?;
    if bytes.len() > 1024 * 1024 {
        return None;
    }
    let package: Value = serde_json::from_slice(&bytes).ok()?;
    package
        .get("version")
        .and_then(Value::as_str)
        .map(str::to_owned)
}

/// Whether the installed package is exactly the reviewed release: its version
/// and its complete tree identity and file count.
#[cfg(any(target_os = "windows", test))]
fn installed_package_matches(package_root: &Path, target: &VerifiedMcpTarget) -> bool {
    if installed_package_version(package_root).as_deref() != Some(target.package_version.as_str()) {
        return false;
    }
    read_installed_package_tree(package_root).is_ok_and(|files| {
        package_tree_identity(&files)
            == (
                target.package_tree_sha256.clone(),
                target.package_file_count,
            )
    })
}

#[cfg(any(target_os = "windows", test))]
fn loaded_native_files_in(
    package_root: &Path,
    target: &VerifiedMcpTarget,
) -> Result<Vec<String>, AppError> {
    let metadata = match std::fs::symlink_metadata(package_root) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error.into()),
    };
    if is_link_metadata(&metadata) || !metadata.is_dir() {
        return Ok(Vec::new());
    }
    if installed_package_matches(package_root, target) {
        return Ok(Vec::new());
    }
    let mut loaded = Vec::new();
    let mut pending = vec![package_root.to_path_buf()];
    let mut visited = 0usize;
    while let Some(directory) = pending.pop() {
        for entry in std::fs::read_dir(&directory)? {
            let entry = entry?;
            visited += 1;
            if visited > MAX_NATIVE_SCAN_ENTRIES {
                loaded.sort();
                return Ok(loaded);
            }
            let file_type = entry.file_type()?;
            if file_type.is_symlink() {
                continue;
            }
            let path = entry.path();
            if file_type.is_dir() {
                pending.push(path);
                continue;
            }
            let native = path
                .extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| {
                    extension.eq_ignore_ascii_case("node") || extension.eq_ignore_ascii_case("dll")
                });
            if native && is_loaded_by_another_process(&path) {
                let relative = path.strip_prefix(package_root).unwrap_or(&path);
                loaded.push(relative.to_string_lossy().replace('\\', "/"));
            }
        }
    }
    loaded.sort();
    Ok(loaded)
}

/// A mapped image refuses write access even when every sharing mode is
/// granted, so a sharing violation here means another process has loaded it.
#[cfg(target_os = "windows")]
fn is_loaded_by_another_process(path: &Path) -> bool {
    use std::os::windows::fs::OpenOptionsExt;
    const FILE_SHARE_ALL: u32 = 0x1 | 0x2 | 0x4;
    const ERROR_SHARING_VIOLATION: i32 = 32;
    match std::fs::OpenOptions::new()
        .write(true)
        .share_mode(FILE_SHARE_ALL)
        .open(path)
    {
        Ok(_) => false,
        Err(error) => error.raw_os_error() == Some(ERROR_SHARING_VIOLATION),
    }
}

#[cfg(all(not(target_os = "windows"), test))]
fn is_loaded_by_another_process(_path: &Path) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;
    use std::sync::{Arc, Mutex};

    #[test]
    fn client_registration_preserves_exact_argument_arrays_and_rejects_malformed_or_disabled_routes(
    ) {
        for destination in [".mcp.json", ".cursor/mcp.json", ".qoder/mcp.json"] {
            let bytes = serde_json::to_vec(&json!({"mcpServers":{"hoi4_agent_tools":{"command":"/usr/bin/node","args":["/private/entry.js"]},"custom":{"command":"ignored"}}})).unwrap();
            assert_eq!(
                client_registration(&bytes, destination).unwrap(),
                Some(("/usr/bin/node".into(), vec!["/private/entry.js".into()]))
            );
            for route in [
                json!({"command":"/usr/bin/node","args":[1]}),
                json!({"command":[],"args":[]}),
                json!({"command":"/usr/bin/node","enabled":"true"}),
            ] {
                assert!(client_registration(
                    &serde_json::to_vec(&json!({"mcpServers":{"hoi4_agent_tools":route}})).unwrap(),
                    destination
                )
                .is_err());
            }
            let disabled = serde_json::to_vec(&json!({"mcpServers":{"hoi4_agent_tools":{"command":"/usr/bin/node","enabled":false}}})).unwrap();
            assert!(client_registration(&disabled, destination)
                .unwrap()
                .is_none());
        }
        let codex = b"[mcp_servers.hoi4_agent_tools]\ncommand='/usr/bin/node'\nargs=['/private/entry.js']\n";
        assert_eq!(
            client_registration(codex, ".codex/config.toml").unwrap(),
            Some(("/usr/bin/node".into(), vec!["/private/entry.js".into()]))
        );
        let opencode = serde_json::to_vec(&json!({"mcp":{"hoi4_agent_tools":{"type":"local","command":["/usr/bin/node","/private/entry.js"]}}})).unwrap();
        assert_eq!(
            client_registration(&opencode, "opencode.json").unwrap(),
            Some(("/usr/bin/node".into(), vec!["/private/entry.js".into()]))
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    #[ignore = "requires an explicitly installed manifest-bound native MCP package"]
    fn native_linux_mcp_health_verifies_private_tree_and_lists_tools_only() {
        let manifest_path = std::env::var_os("HOI4_TEST_LINUX_MCP_MANIFEST")
            .expect("provide an explicit verified source manifest");
        let manifest =
            crate::source::parse_manifest(&std::fs::read(manifest_path).unwrap(), None).unwrap();
        let target = manifest_target_for_platform(&manifest, Platform::Linux).unwrap();
        assert_eq!(target.target, "hoi4-agent-tools");
        let project = tempfile::tempdir().unwrap();
        let evidence = initialize_health(&project.path().canonicalize().unwrap(), &target).unwrap();
        assert_eq!(evidence.required_tools, target.required_tools);
        assert!(evidence.tool_count >= target.required_tools.len());
    }

    #[test]
    fn package_link_targets_must_remain_inside_the_verified_tree() {
        assert_eq!(
            contained_link_target("node_modules/.bin/semver", "../semver/bin/semver.js").unwrap(),
            "node_modules/semver/bin/semver.js"
        );
        for target in [
            "../../../escape",
            "/absolute",
            "C:\\escape",
            "../other/../../../../escape",
        ] {
            assert!(contained_link_target("node_modules/.bin/semver", target).is_err());
        }
    }

    #[cfg(unix)]
    #[test]
    fn only_exact_declared_internal_links_are_copied_and_reverified() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(directory.path().join("node_modules/.bin")).unwrap();
        std::fs::create_dir_all(directory.path().join("node_modules/semver/bin")).unwrap();
        std::fs::write(
            directory.path().join("node_modules/semver/bin/semver.js"),
            b"verified target",
        )
        .unwrap();
        let relative = "node_modules/.bin/semver";
        let target = "../semver/bin/semver.js";
        std::os::unix::fs::symlink(target, directory.path().join(relative)).unwrap();
        assert!(read_installed_package_tree(directory.path()).is_err());
        let declared = [(relative.into(), target.into())].into_iter().collect();
        let tree =
            materialize_verified_package_tree_with_links(directory.path(), &declared).unwrap();
        assert_eq!(tree.file_count, 2);
        verify_materialized_package_tree(&tree).unwrap();
        std::fs::remove_file(tree.root.join(relative)).unwrap();
        std::os::unix::fs::symlink("../outside", tree.root.join(relative)).unwrap();
        assert!(verify_materialized_package_tree(&tree).is_err());
        let missing = [("node_modules/.bin/missing".into(), target.into())]
            .into_iter()
            .collect();
        assert!(read_installed_package_tree_with_links(directory.path(), &missing).is_err());
    }

    fn native_scan_target(version: &str, tree: &str, files: u64) -> VerifiedMcpTarget {
        VerifiedMcpTarget {
            target: "hoi4-agent-tools.cmd".into(),
            package_name: "hoi4-agent-tools".into(),
            package_version: version.into(),
            package_integrity: "sha512-reviewed".into(),
            package_tree_sha256: tree.into(),
            package_file_count: files,
            runtime_entry: "dist/bin/stdio.js".into(),
            runtime_entry_sha256: "0".repeat(64),
            runtime_entry_size: 1,
            package_symlinks: BTreeMap::new(),
            required_tools: Vec::new(),
        }
    }

    #[test]
    fn loaded_native_files_block_only_a_replacement_of_a_package_that_does_not_match() {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path().join("hoi4-agent-tools");
        let newer = native_scan_target("3.8.1", &"a".repeat(64), 1);
        assert!(loaded_native_files_in(&root, &newer).unwrap().is_empty());

        let library = root.join("node_modules/@img/sharp-win32-x64/lib");
        std::fs::create_dir_all(&library).unwrap();
        std::fs::write(root.join("package.json"), r#"{"version":"3.6.0"}"#).unwrap();
        std::fs::write(library.join("libvips-42.dll"), b"image").unwrap();
        std::fs::write(library.join("sharp.node"), b"addon").unwrap();
        std::fs::write(library.join("notes.txt"), b"text").unwrap();
        assert!(loaded_native_files_in(&root, &newer).unwrap().is_empty());

        let (tree, count) = package_tree_identity(&read_installed_package_tree(&root).unwrap());
        let installed = native_scan_target("3.6.0", &tree, count);
        #[cfg(target_os = "windows")]
        let same_version_other_tree = native_scan_target("3.6.0", &"b".repeat(64), count);

        #[cfg(target_os = "windows")]
        {
            use std::os::windows::fs::OpenOptionsExt;
            // An exclusive handle stands in for a loader mapping: both
            // refuse a second write handle with a sharing violation.
            let held = std::fs::OpenOptions::new()
                .read(true)
                .share_mode(0)
                .open(library.join("libvips-42.dll"))
                .unwrap();
            let expected = vec!["node_modules/@img/sharp-win32-x64/lib/libvips-42.dll".to_string()];
            assert_eq!(loaded_native_files_in(&root, &newer).unwrap(), expected);
            assert_eq!(
                loaded_native_files_in(&root, &same_version_other_tree).unwrap(),
                expected,
                "a same-version tree with stale files is still replaced"
            );
            drop(held);
        }
        assert!(
            loaded_native_files_in(&root, &installed)
                .unwrap()
                .is_empty(),
            "the exact reviewed tree is kept, so nothing is replaced"
        );
    }

    struct FakeTransport {
        sent: Arc<Mutex<Vec<Value>>>,
        responses: VecDeque<Value>,
    }

    impl JsonlTransport for FakeTransport {
        fn send(&mut self, value: &Value) -> Result<(), AppError> {
            self.sent.lock().unwrap().push(value.clone());
            Ok(())
        }

        fn receive(&mut self, _timeout: Duration) -> Result<Option<Value>, AppError> {
            Ok(self.responses.pop_front())
        }

        fn close(&mut self) {}
    }

    #[test]
    fn mcp_handshake_uses_json_rpc_protocol_version_and_initialized_notification() {
        let sent = Arc::new(Mutex::new(Vec::new()));
        let transport = FakeTransport {
            sent: sent.clone(),
            responses: VecDeque::from([
                json!({
                    "jsonrpc": "2.0",
                    "id": 1,
                    "result": {
                        "protocolVersion": MCP_PROTOCOL_VERSION,
                        "serverInfo": {"name": "hoi4-agent-tools", "version": "4.0.3"},
                        "capabilities": {"tools": {}}
                    }
                }),
                json!({"jsonrpc": "2.0", "id": 2, "result": {"tools": []}}),
            ]),
        };
        let mut protocol = McpProtocol::new(transport, Duration::from_secs(1));
        protocol.initialize().unwrap();
        protocol.request("tools/list", json!({})).unwrap();
        let sent = sent.lock().unwrap();
        assert_eq!(sent[0]["jsonrpc"], "2.0");
        assert_eq!(sent[0]["method"], "initialize");
        assert_eq!(sent[0]["params"]["protocolVersion"], MCP_PROTOCOL_VERSION);
        assert!(sent[0]["params"]["capabilities"].is_object());
        assert_eq!(sent[1]["method"], "notifications/initialized");
        assert_eq!(sent[2]["method"], "tools/list");
    }

    #[test]
    fn initialize_response_requires_server_and_capabilities() {
        let error = validate_initialize_result(&json!({
            "protocolVersion": "2025-11-25",
            "serverInfo": {"name": "hoi4-agent-tools", "version": "2.3.3"}
        }))
        .unwrap_err();
        assert!(error.to_string().contains("capabilities"));
        let error = validate_initialize_result(&json!({
            "protocolVersion": "2024-11-05",
            "serverInfo": {"name": "hoi4-agent-tools", "version": "4.0.3"},
            "capabilities": {"tools": {}}
        }))
        .unwrap_err();
        assert!(error.to_string().contains("unsupported protocol version"));
    }

    #[test]
    fn tools_list_is_bounded_and_structured() {
        let required = vec!["hoi4.tech_inspect".to_string()];
        assert_eq!(
            validate_tools_result(
                &json!({"tools": [{"name": "inspect"}, {"name": "hoi4.tech_inspect"}]}),
                &required,
            )
            .unwrap(),
            2
        );
        assert!(validate_tools_result(
            &json!({"tools": [{"description": "missing name"}]}),
            &required,
        )
        .is_err());
        assert!(
            validate_tools_result(&json!({"tools": [{"name": "inspect"}]}), &required)
                .unwrap_err()
                .to_string()
                .contains("hoi4.tech_inspect")
        );
    }

    #[test]
    fn package_tree_identity_detects_a_mutated_imported_module() {
        let package = tempfile::tempdir().unwrap();
        let bin = package.path().join("dist/bin");
        std::fs::create_dir_all(&bin).unwrap();
        std::fs::write(bin.join("stdio.js"), b"import '../core.js';\n").unwrap();
        let sibling = package.path().join("dist/core.js");
        std::fs::write(&sibling, b"export const value = 1;\n").unwrap();
        let before = package_tree_identity(&read_installed_package_tree(package.path()).unwrap());
        std::fs::write(&sibling, b"export const value = 2;\n").unwrap();
        let after = package_tree_identity(&read_installed_package_tree(package.path()).unwrap());
        assert_ne!(before, after);
    }

    #[test]
    fn package_reader_accepts_the_current_native_library_size_within_the_bound() {
        let package = tempfile::tempdir().unwrap();
        let native_library = package.path().join("libvips-42.dll");
        std::fs::File::create(&native_library)
            .unwrap()
            .set_len(18_406_400)
            .unwrap();
        let files = read_installed_package_tree(package.path()).unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].1.len(), 18_406_400);
    }

    #[test]
    fn package_reader_rejects_a_file_above_the_bounded_limit() {
        let package = tempfile::tempdir().unwrap();
        std::fs::File::create(package.path().join("oversized.bin"))
            .unwrap()
            .set_len(MAX_PACKAGE_FILE_BYTES + 1)
            .unwrap();
        assert!(read_installed_package_tree(package.path()).is_err());
    }

    #[test]
    fn missing_optional_npm_lock_does_not_replace_full_tree_identity() {
        let prefix = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(prefix.path().join("node_modules")).unwrap();
        verify_optional_npm_lock_integrity(prefix.path(), "hoi4-agent-tools", "sha512-reviewed")
            .unwrap();

        std::fs::write(
            prefix.path().join("node_modules/.package-lock.json"),
            serde_json::to_vec(&json!({
                "packages": {
                    "node_modules/hoi4-agent-tools": {"integrity": "sha512-wrong"}
                }
            }))
            .unwrap(),
        )
        .unwrap();
        assert!(verify_optional_npm_lock_integrity(
            prefix.path(),
            "hoi4-agent-tools",
            "sha512-reviewed",
        )
        .unwrap_err()
        .to_string()
        .contains("registry integrity"));
    }

    #[test]
    fn verified_runtime_copy_is_immune_to_later_source_mutation() {
        let package = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(package.path().join("dist")).unwrap();
        let source = package.path().join("dist/index.js");
        std::fs::write(&source, b"export const value = 1;\n").unwrap();
        let verified = materialize_verified_package_tree(package.path()).unwrap();
        std::fs::write(&source, b"export const value = 2;\n").unwrap();
        assert_eq!(
            std::fs::read(verified.root.join("dist/index.js")).unwrap(),
            b"export const value = 1;\n"
        );
    }

    #[test]
    fn private_runtime_mutation_is_rejected_before_spawn() {
        let package = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(package.path().join("dist")).unwrap();
        std::fs::write(package.path().join("dist/index.js"), b"reviewed\n").unwrap();
        let verified = materialize_verified_package_tree(package.path()).unwrap();
        std::fs::write(verified.root.join("dist/index.js"), b"tampered\n").unwrap();
        assert!(verify_materialized_package_tree(&verified)
            .unwrap_err()
            .to_string()
            .contains("changed before execution"));
    }

    #[test]
    fn required_tools_capability_is_not_optional() {
        let initialized = json!({
            "protocolVersion": MCP_PROTOCOL_VERSION,
            "serverInfo": {"name": "hoi4-agent-tools", "version": "4.0.3"},
            "capabilities": {}
        });
        assert!(require_tools_capability(&initialized)
            .unwrap_err()
            .to_string()
            .contains("required tools capability"));
    }

    #[test]
    fn running_server_must_advertise_the_reviewed_package_version() {
        let manifest: RemoteManifest = serde_json::from_slice(include_bytes!(
            "../../docs/source-manifest/hoi4-mod-setup.manifest.json"
        ))
        .unwrap();
        let target = manifest_target(&manifest).unwrap();
        let mut initialized = json!({
            "protocolVersion": MCP_PROTOCOL_VERSION,
            "serverInfo": {
                "name": "hoi4-agent-tools",
                "version": target.package_version.clone()
            },
            "capabilities": {"tools": {}}
        });
        let evidence =
            validate_verified_initialize_result(&initialized, &target.package_version).unwrap();
        assert_eq!(evidence.server_version, target.package_version);
        initialized["serverInfo"]["version"] = json!("unreviewed-version");
        assert!(
            validate_verified_initialize_result(&initialized, &target.package_version)
                .unwrap_err()
                .to_string()
                .contains("reviewed package identity")
        );
    }

    #[test]
    fn manifest_target_binds_the_exact_public_package_and_its_declared_routes() {
        let manifest: RemoteManifest = serde_json::from_slice(include_bytes!(
            "../../docs/source-manifest/hoi4-mod-setup.manifest.json"
        ))
        .unwrap();
        let target = manifest_target(&manifest).unwrap();
        assert_eq!(target.package_name, "hoi4-agent-tools");
        let declared = manifest
            .components
            .iter()
            .find(|component| component.id == COMPONENT_ID)
            .and_then(|component| {
                component
                    .validation
                    .iter()
                    .find(|rule| rule.id == HEALTH_RULE_ID)
            })
            .and_then(|rule| rule.parameters.get("required_tools"))
            .and_then(Value::as_array)
            .unwrap()
            .iter()
            .filter_map(Value::as_str)
            .map(ToOwned::to_owned)
            .collect::<Vec<_>>();
        assert!(!declared.is_empty());
        assert_eq!(target.required_tools, declared);
    }

    #[test]
    fn removed_compare_route_cannot_satisfy_current_technology_inspectors() {
        let manifest: RemoteManifest = serde_json::from_slice(include_bytes!(
            "../../docs/source-manifest/hoi4-mod-setup.manifest.json"
        ))
        .unwrap();
        let target = manifest_target(&manifest).unwrap();
        let tools = target
            .required_tools
            .iter()
            .filter(|tool| !matches!(tool.as_str(), "hoi4.tech_inspect" | "hoi4.tech_render"))
            .map(|tool| json!({"name": tool}))
            .chain(std::iter::once(json!({"name": "hoi4.tech_compare"})))
            .collect::<Vec<_>>();
        let error = validate_tools_result(&json!({"tools": tools}), &target.required_tools)
            .unwrap_err()
            .to_string();
        assert!(error.contains("hoi4.tech_inspect"));
        assert!(error.contains("hoi4.tech_render"));
        assert!(!target
            .required_tools
            .iter()
            .any(|tool| tool == "hoi4.tech_compare"));
    }

    #[test]
    fn reviewed_plan_target_rejects_an_unreviewed_path() {
        let action = ExternalAction {
            id: format!("external.{COMPONENT_ID}.{HEALTH_RULE_ID}"),
            component_id: COMPONENT_ID.into(),
            platform: Platform::Windows,
            command_source: "repository_script".into(),
            executable: Some("manifest-declared executable".into()),
            arguments: vec!["hoi4-agent-tools.cmd".into()],
            working_directory: Some("<project_root>".into()),
            environment_names: vec![],
            network_access: "not_declared".into(),
            expected_writes: vec![],
            privilege: "not_declared".into(),
            rollback_boundary: "not_declared_by_source".into(),
            display_command: Some("Repository-declared validation target".into()),
            risk: "high".into(),
            requires_approval: true,
            contains_secret: false,
            verified_executable_sha256: None,
            verified_executable_size: None,
            verified_interpreter_sha256: None,
            verified_interpreter_size: None,
            verified_runtime_sha256: None,
            verified_runtime_size: None,
            verified_package_symlinks: Default::default(),
            verified_package_name: None,
            verified_package_version: None,
            verified_package_integrity: None,
            verified_package_tree_sha256: None,
            verified_package_file_count: None,
            verified_runtime_entry: None,
            required_tool_names: vec![],
        };
        assert!(reviewed_plan_target(std::slice::from_ref(&action)).is_err());
        let mut tampered = action;
        tampered.arguments = vec!["C:\\outside\\evil.cmd".into()];
        assert!(reviewed_plan_target(&[tampered]).is_err());
    }
}
