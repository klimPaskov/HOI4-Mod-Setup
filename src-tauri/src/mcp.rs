//! Bounded health probes for manifest-declared MCP servers.
//!
//! The probe starts only the reviewed command recorded by the locked source
//! manifest, performs MCP initialization and (when the server advertises the
//! capability) a read-only tools/list request, then terminates the child.
//! It never calls an MCP tool and never receives credential environment
//! variables.

use crate::codex::{AppServerProtocol, ProcessJsonlTransport};
use crate::models::{ExternalAction, Platform, RemoteManifest};
use crate::process::find_path_executable;
use crate::security::{is_link_metadata, path_has_link_component, sha256_file};
use crate::AppError;
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::time::Duration;

pub const COMPONENT_ID: &str = "mcp.hoi4_agent_tools";
pub const HEALTH_RULE_ID: &str = "mcp.hoi4.health";
const MAX_SERVER_FIELD_BYTES: usize = 256;
const MAX_TOOL_COUNT: usize = 4096;
const MAX_TOOL_NAME_BYTES: usize = 256;

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
    pub runtime_entry: String,
    pub runtime_entry_sha256: String,
    pub runtime_entry_size: u64,
    pub required_tools: Vec<String>,
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
    for technology_tool in ["hoi4.tech_inspect", "hoi4.tech_render", "hoi4.tech_compare"] {
        if !unique.contains(technology_tool) {
            return Err(AppError::Source(format!(
                "the MCP package does not declare required Technology Tree route {technology_tool}"
            )));
        }
    }
    Ok(())
}

/// Return the one command target declared by the verified manifest.
pub fn manifest_target(manifest: &RemoteManifest) -> Result<VerifiedMcpTarget, AppError> {
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
        .any(|platform| platform.supports(Platform::Windows))
    {
        return Err(AppError::UnsupportedPlatform(
            "the MCP component has no verified Windows route".into(),
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
    let target = rules[0]
        .target
        .as_deref()
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
        runtime_entry,
        &required_tools,
    )?;
    Ok(VerifiedMcpTarget {
        target: target.to_owned(),
        package_name: package_name.to_owned(),
        package_version: package_version.to_owned(),
        package_integrity: package_integrity.to_owned(),
        runtime_entry: runtime_entry.to_owned(),
        runtime_entry_sha256: runtime_entry_sha256.to_owned(),
        runtime_entry_size,
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
                && action.platform == Platform::Windows
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
    validate_package_identity(
        package_name,
        package_version,
        package_integrity,
        runtime_entry,
        &matches[0].required_tool_names,
    )?;
    Ok(VerifiedMcpTarget {
        target: target.to_owned(),
        package_name: package_name.to_owned(),
        package_version: package_version.to_owned(),
        package_integrity: package_integrity.to_owned(),
        runtime_entry: runtime_entry.to_owned(),
        runtime_entry_sha256: runtime_entry_sha256.to_owned(),
        runtime_entry_size,
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
    if Platform::current() != Platform::Windows {
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
    let wrapper = resolved_wrapper(&target.target)?;
    let metadata = std::fs::symlink_metadata(&wrapper)?;
    if is_link_metadata(&metadata) || !metadata.is_file() || path_has_link_component(&wrapper) {
        return Err(AppError::PathSecurity(
            "the resolved MCP executable is not a regular, link-free file".into(),
        ));
    }
    let bin_root = wrapper
        .parent()
        .ok_or_else(|| AppError::Process("the MCP wrapper has no containing directory".into()))?;
    let package_relative = format!("node_modules/{}/package.json", target.package_name);
    let package_bytes = crate::flatten::read_bounded_regular_file_no_follow_under_root(
        bin_root,
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
    let lock_bytes = crate::flatten::read_bounded_regular_file_no_follow_under_root(
        bin_root,
        "node_modules/.package-lock.json",
        16 * 1024 * 1024,
    )?;
    let lock: Value = serde_json::from_slice(&lock_bytes)?;
    let lock_key = format!("node_modules/{}", target.package_name);
    let installed_integrity = lock
        .get("packages")
        .and_then(|packages| packages.get(&lock_key))
        .and_then(|package| package.get("integrity"))
        .and_then(Value::as_str);
    if installed_integrity != Some(target.package_integrity.as_str()) {
        return Err(AppError::Credential(
            "the installed MCP package does not match the reviewed registry integrity".into(),
        ));
    }
    let entry_relative = format!(
        "node_modules/{}/{}",
        target.package_name, target.runtime_entry
    );
    let entry = bin_root.join(entry_relative.replace('/', std::path::MAIN_SEPARATOR_STR));
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
    crate::process::validate_executable_publisher(&node, "OpenJS Foundation")?;
    let node_directory = node.parent().map(Path::to_path_buf).ok_or_else(|| {
        AppError::Process("the resolved Node executable has no containing directory".into())
    })?;
    let transport = ProcessJsonlTransport::start_command_with_path_and_identity(
        node.clone(),
        vec![entry.display().to_string()],
        Some(project_root.to_path_buf()),
        Some(node_directory),
        &sha256_file(&node)?,
    )?;
    let mut protocol = AppServerProtocol::with_timeout(transport, Duration::from_secs(10));
    let initialized = protocol.initialize()?;
    let evidence = validate_initialize_result(&initialized)?;
    let tool_count = if initialized
        .get("capabilities")
        .and_then(Value::as_object)
        .is_some_and(|capabilities| capabilities.contains_key("tools"))
    {
        let listing = protocol.request("tools/list", json!({}))?;
        validate_tools_result(&listing, &target.required_tools)?
    } else {
        0
    };
    Ok(HealthEvidence {
        tool_count,
        required_tools: target.required_tools.clone(),
        ..evidence
    })
}

pub fn validate_initialize_result(value: &Value) -> Result<HealthEvidence, AppError> {
    let object = value
        .as_object()
        .ok_or_else(|| AppError::Process("MCP initialize returned a non-object result".into()))?;
    let protocol_version = bounded_string(object, "protocolVersion", "protocol version")?;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initialize_response_requires_server_and_capabilities() {
        let error = validate_initialize_result(&json!({
            "protocolVersion": "2025-11-25",
            "serverInfo": {"name": "hoi4-agent-tools", "version": "2.3.3"}
        }))
        .unwrap_err();
        assert!(error.to_string().contains("capabilities"));
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
    fn manifest_target_binds_the_exact_public_package_and_technology_routes() {
        let mut manifest: RemoteManifest = serde_json::from_slice(include_bytes!(
            "../../docs/source-manifest/hoi4-mod-setup.v2.manifest.json"
        ))
        .unwrap();
        let target = manifest_target(&manifest).unwrap();
        assert_eq!(target.package_name, "hoi4-agent-tools");
        assert_eq!(target.package_version, "2.5.2");
        for tool in ["hoi4.tech_inspect", "hoi4.tech_render", "hoi4.tech_compare"] {
            assert!(target
                .required_tools
                .iter()
                .any(|candidate| candidate == tool));
        }
        let component = manifest
            .components
            .iter_mut()
            .find(|component| component.id == COMPONENT_ID)
            .unwrap();
        let tools = component
            .validation
            .iter_mut()
            .find(|rule| rule.id == HEALTH_RULE_ID)
            .unwrap()
            .parameters
            .get_mut("required_tools")
            .unwrap()
            .as_array_mut()
            .unwrap();
        tools.retain(|tool| tool.as_str() != Some("hoi4.tech_render"));
        assert!(manifest_target(&manifest)
            .unwrap_err()
            .to_string()
            .contains("hoi4.tech_render"));
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
            verified_package_name: None,
            verified_package_version: None,
            verified_package_integrity: None,
            verified_runtime_entry: None,
            required_tool_names: vec![],
        };
        assert!(reviewed_plan_target(std::slice::from_ref(&action)).is_err());
        let mut tampered = action;
        tampered.arguments = vec!["C:\\outside\\evil.cmd".into()];
        assert!(reviewed_plan_target(&[tampered]).is_err());
    }
}
