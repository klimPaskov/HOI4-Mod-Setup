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
use crate::security::path_has_link_component;
use crate::AppError;
use serde::Serialize;
use serde_json::{json, Value};
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
}

/// Return the one command target declared by the verified manifest.
pub fn manifest_target(manifest: &RemoteManifest) -> Result<String, AppError> {
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
    Ok(target.to_owned())
}

/// Extract the target from the external action captured in an installation
/// plan. This keeps transaction readiness bound to the exact source-declared
/// action that the user reviewed.
pub fn reviewed_plan_target(actions: &[ExternalAction]) -> Result<String, AppError> {
    let matches = actions
        .iter()
        .filter(|action| {
            action.component_id == COMPONENT_ID
                && action.id == format!("external.{COMPONENT_ID}.{HEALTH_RULE_ID}")
                && action.platform == Platform::Windows
                && action.command_source == "repository_script"
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
    Ok(target.to_owned())
}

/// Start the source-declared Windows wrapper through the canonical Windows
/// command interpreter. The `/c` value is assembled only from a canonical,
/// PATH-resolved executable and is never supplied by the renderer.
pub fn initialize_health(project_root: &Path, target: &str) -> Result<HealthEvidence, AppError> {
    if Platform::current() != Platform::Windows {
        return Err(AppError::UnsupportedPlatform(
            "the verified MCP route is currently supported only on Windows".into(),
        ));
    }
    validate_bare_command_name(target)?;
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
    let wrapper = find_path_executable(&[target])?;
    let command_interpreter = find_path_executable(&["cmd.exe"])?;
    let node = find_path_executable(&["node.exe", "node"])?;
    let node_directory = node.parent().map(Path::to_path_buf).ok_or_else(|| {
        AppError::Process("the resolved Node executable has no containing directory".into())
    })?;
    let command_line = format!("\"{}\"", wrapper.display());
    let transport = ProcessJsonlTransport::start_command_with_path(
        command_interpreter,
        vec!["/d".into(), "/s".into(), "/c".into(), command_line],
        Some(project_root.to_path_buf()),
        Some(node_directory),
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
        validate_tools_result(&listing)?
    } else {
        0
    };
    Ok(HealthEvidence {
        tool_count,
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
    })
}

fn validate_tools_result(value: &Value) -> Result<usize, AppError> {
    let tools = value
        .get("tools")
        .and_then(Value::as_array)
        .ok_or_else(|| AppError::Process("MCP tools/list returned no tools array".into()))?;
    if tools.len() > MAX_TOOL_COUNT {
        return Err(AppError::Process(
            "MCP tools/list exceeded the bounded tool count".into(),
        ));
    }
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
        assert_eq!(
            validate_tools_result(&json!({"tools": [{"name": "inspect"}]})).unwrap(),
            1
        );
        assert!(
            validate_tools_result(&json!({"tools": [{"description": "missing name"}]})).is_err()
        );
    }

    #[test]
    fn manifest_target_requires_the_source_declared_rule() {
        let manifest: RemoteManifest = serde_json::from_slice(include_bytes!(
            "../../source-manifest/hoi4-mod-setup.manifest.json"
        ))
        .unwrap();
        assert_eq!(manifest_target(&manifest).unwrap(), "hoi4-agent-tools.cmd");
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
        };
        assert_eq!(
            reviewed_plan_target(std::slice::from_ref(&action)).unwrap(),
            "hoi4-agent-tools.cmd"
        );
        let mut tampered = action;
        tampered.arguments = vec!["C:\\outside\\evil.cmd".into()];
        assert!(reviewed_plan_target(&[tampered]).is_err());
    }
}
