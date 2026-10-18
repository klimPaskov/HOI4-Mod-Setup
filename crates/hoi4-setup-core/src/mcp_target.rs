//! The reviewed HOI4 Agent Tools target a manifest declares for a platform.

use crate::models::*;
use crate::AppError;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

pub const COMPONENT_ID: &str = "mcp.hoi4_agent_tools";

pub const HEALTH_RULE_ID: &str = "mcp.hoi4.health";

/// The optional HOI4 Live Tools workflow, whose reviewed action installs the
/// pinned release for the current user (it is not an MCP package of this app).
pub const LIVE_TOOLS_COMPONENT_ID: &str = "workflow.live_tools";

pub const MAX_TOOL_COUNT: usize = 4096;

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
    crate::manifest::validate_sha256(runtime_entry_sha256)?;
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

pub fn contained_link_target(relative: &str, target: &str) -> Result<String, AppError> {
    crate::text::normalize_relative_path(relative)?;
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
    crate::text::normalize_relative_path(&parts.join("/"))
}

pub fn required_parameter<'a>(
    rule: &'a crate::models::ValidationRule,
    key: &str,
) -> Result<&'a str, AppError> {
    rule.parameters
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| AppError::Source(format!("the MCP route is missing {key}")))
}

pub fn validate_package_identity(
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
        || crate::manifest::validate_sha256(package_tree_sha256).is_err()
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

pub fn validate_bare_command_name(value: &str) -> Result<(), AppError> {
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
