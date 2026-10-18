//! The setup manifest: parsing, validation, dependency expansion, platform
//! support, file selection, and download verification. Everything here is
//! pure and runs unchanged in the desktop app and the web edition.

use crate::models::*;
use crate::text::{canonical_relative_key, normalize_relative_path, sha256_bytes};
use crate::AppError;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeSet, HashMap, HashSet};

pub const SOURCE_REPOSITORY: &str = "https://github.com/klimPaskov/Agentic-HOI4-Modding";
pub const SOURCE_OWNER: &str = "klimPaskov";
pub const SOURCE_NAME: &str = "Agentic-HOI4-Modding";
pub const MANIFEST_PATH: &str = "hoi4-mod-setup.manifest.json";
pub const MAX_SELECTED_FILES: usize = 20_000;
pub const MAX_SELECTED_BYTES: u64 = 1024 * 1024 * 1024;

/// The environment variable that carries the Meshy API key to its reviewed
/// launcher; the key itself never appears in a manifest or plan.
pub const MESHY_ENVIRONMENT_NAME: &str = "MESHY_API_KEY";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentSupport {
    pub component_id: String,
    pub selected: bool,
    pub state: String,
    pub reason: Option<String>,
    #[serde(default)]
    pub dependents: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelectedSourceFile {
    pub component_id: String,
    pub source_path: String,
    pub destination: String,
    pub ownership: Ownership,
    pub expected_sha256: Option<String>,
    pub expected_size: Option<u64>,
    pub executable: bool,
    pub platform: ManifestPlatform,
}

pub fn validate_manifest_destinations(components: &[ComponentDefinition]) -> Result<(), AppError> {
    let mut owners: std::collections::HashMap<String, Vec<&ComponentDefinition>> =
        std::collections::HashMap::new();
    for component in components {
        let key = canonical_relative_key(&component.destination.path)?;
        owners.entry(key).or_default().push(component);
    }
    for (key, components) in owners {
        // Multiple tree components may contribute disjoint files to one
        // managed directory. Concrete selected destinations are still
        // canonicalized and rejected unconditionally by select_component_files.
        let shared_tree_root = components
            .iter()
            .all(|component| component.source.kind == SourceKind::Tree);
        let structured_merge = components
            .iter()
            .all(|component| component.destination.ownership == crate::models::Ownership::Merged);
        if components.len() > 1
            && !shared_tree_root
            && !structured_merge
            && components.iter().any(|component| {
                component.destination.ownership != crate::models::Ownership::Merged
            })
        {
            return Err(AppError::PathSecurity(format!(
                "duplicate destination: {key}"
            )));
        }
    }
    Ok(())
}

pub fn parse_manifest(bytes: &[u8], revision: Option<&str>) -> Result<RemoteManifest, AppError> {
    let value: Value = serde_json::from_slice(bytes)
        .map_err(|error| AppError::Source(format!("manifest JSON is invalid: {error}")))?;
    // A newer format is reported as needing an app update before any
    // checked-in schema is applied to it.
    if let Some(major) = value
        .get("schema_version")
        .and_then(Value::as_str)
        .and_then(|version| version.split('.').next())
        .and_then(|version| version.parse::<u64>().ok())
        .filter(|major| *major > CURRENT_MANIFEST_MAJOR)
    {
        return Err(AppError::Source(format!(
            "the setup package uses manifest format {major}, which needs a newer HOI4 Mod Setup. Update the app, then try again"
        )));
    }
    validate_manifest_schema(&value)?;
    let manifest: RemoteManifest = serde_json::from_value(value)
        .map_err(|error| AppError::Source(format!("manifest model is invalid: {error}")))?;
    validate_manifest(&manifest, revision)?;
    Ok(manifest)
}

pub fn validate_manifest_schema(value: &Value) -> Result<(), AppError> {
    let major = value
        .get("schema_version")
        .and_then(Value::as_str)
        .and_then(|version| version.split('.').next())
        .and_then(|version| version.parse::<u64>().ok());
    let schema = match major {
        Some(6) => include_str!("../../../docs/schemas/remote-manifest.v6.schema.json"),
        Some(5) => include_str!("../../../docs/schemas/remote-manifest.v5.schema.json"),
        Some(4) => include_str!("../../../docs/schemas/remote-manifest.v4.schema.json"),
        _ => include_str!("../../../docs/schemas/remote-manifest.schema.json"),
    };
    let validator = crate::schemas::validator(schema).map_err(|error| {
        AppError::Source(format!("remote manifest schema is unavailable: {error}"))
    })?;
    validator.validate(value).map_err(|error| {
        AppError::Source(format!(
            "manifest does not satisfy the authoritative JSON Schema at {}: {}",
            error.instance_path(),
            error
        ))
    })
}

pub fn wiki_install_metadata(manifest: &RemoteManifest) -> WikiInstallMetadata {
    WikiInstallMetadata {
        snapshot_marker: manifest.wiki.snapshot_marker.clone(),
        required_media_policy: manifest.wiki.required_media_policy.clone(),
        source_status: manifest.wiki.provenance.source_status.clone(),
        license_status: manifest.wiki.provenance.license_status.clone(),
        repository_license_status: manifest
            .repository
            .license_evidence
            .clone()
            .unwrap_or_else(|| "unknown".into()),
        notes: manifest.wiki.provenance.notes.clone(),
    }
}

pub fn require_canonical_manifest_path(raw: &str, label: &str) -> Result<String, AppError> {
    let normalized = normalize_relative_path(raw)
        .map_err(|error| AppError::Source(format!("invalid {label} path: {error}")))?;
    let canonical_raw = raw.replace('\\', "/").trim_end_matches('/').to_owned();
    if canonical_raw != normalized {
        return Err(AppError::Source(format!(
            "{label} path is not in canonical slash-separated form: {raw}"
        )));
    }
    Ok(normalized)
}

/// Format 6 declares how each client registers MCP. A project route must name
/// a file the client's own component installs, so the installer never writes
/// a client setting outside that client's package.
pub fn validate_environment_mcp_declarations(
    manifest: &RemoteManifest,
    major: u64,
) -> Result<(), AppError> {
    for environment in &manifest.coding_environments {
        let invalid = |detail: &str| {
            AppError::Source(format!(
                "coding environment {} has an invalid MCP declaration: {detail}",
                environment.id
            ))
        };
        if major < 6 {
            if environment.mcp_support.is_some()
                || environment.mcp_registration.is_some()
                || environment.documentation_url.is_some()
            {
                return Err(invalid("MCP declarations need manifest format 6"));
            }
            continue;
        }
        if environment
            .documentation_url
            .as_deref()
            .is_some_and(|url| !url.starts_with("https://") || url.len() > 300)
        {
            return Err(invalid("documentation must use an HTTPS address"));
        }
        match (
            environment.mcp_support,
            environment.mcp_registration.as_ref(),
        ) {
            (None, _) => return Err(invalid("mcp_support is required")),
            (Some(McpSupport::Project), None) => {
                return Err(invalid("a project route needs mcp_registration"))
            }
            (Some(McpSupport::Manual | McpSupport::None), Some(_)) => {
                return Err(invalid("only a project route has an MCP file"))
            }
            (Some(McpSupport::Project), Some(registration)) => {
                let path = require_canonical_manifest_path(&registration.path, "MCP registration")?;
                let component = manifest
                    .components
                    .iter()
                    .find(|component| component.id == registration.component_id)
                    .ok_or_else(|| invalid("the registration component is not declared"))?;
                if component.coding_environment.as_deref() != Some(environment.id.as_str()) {
                    return Err(invalid(
                        "the registration component belongs to another client",
                    ));
                }
                let destination = component.destination.path.replace('\\', "/");
                let installs_path = destination.trim_end_matches('/') == path
                    || component
                        .expected_files
                        .iter()
                        .any(|file| file.path.replace('\\', "/") == path);
                if !installs_path {
                    return Err(invalid(
                        "the registration component does not install the MCP file",
                    ));
                }
            }
            (Some(McpSupport::Manual | McpSupport::None), None) => {}
        }
    }
    Ok(())
}

pub fn validate_manifest(
    manifest: &RemoteManifest,
    revision: Option<&str>,
) -> Result<(), AppError> {
    let version_pattern =
        Regex::new(r"^[0-9]+\.[0-9]+\.[0-9]+$").expect("static manifest version regex");
    if !version_pattern.is_match(&manifest.schema_version) {
        return Err(AppError::Source(
            "manifest schema_version must be a full semantic version".into(),
        ));
    }
    let major = manifest
        .schema_version
        .split('.')
        .next()
        .and_then(|value| value.parse::<u64>().ok())
        .ok_or_else(|| AppError::Source("manifest schema_version is invalid".into()))?;
    if !SUPPORTED_MANIFEST_MAJORS.contains(&major) {
        return Err(AppError::Source(if major > CURRENT_MANIFEST_MAJOR {
            format!(
                "the setup package uses manifest format {major}, which needs a newer HOI4 Mod Setup. Update the app, then try again"
            )
        } else {
            format!(
                "the setup package uses manifest format {major}, which this app no longer reads"
            )
        }));
    }
    let declared_environments = manifest
        .coding_environments
        .iter()
        .map(|environment| environment.id.as_str())
        .collect::<HashSet<_>>();
    if declared_environments.len() != manifest.coding_environments.len()
        || manifest.coding_environments.iter().any(|environment| {
            !crate::coding_environment::is_supported(&environment.id)
                || environment.display_name.trim().is_empty()
                || environment.display_name.chars().count() > 64
        })
        || manifest
            .coding_environments
            .iter()
            .filter(|environment| environment.default_primary)
            .count()
            > 1
    {
        return Err(AppError::Source(
            "the manifest coding-environment declarations are invalid".into(),
        ));
    }
    if major >= 3 && manifest.coding_environments.is_empty() {
        return Err(AppError::Source(
            "the manifest does not declare its coding environments".into(),
        ));
    }
    validate_environment_mcp_declarations(manifest, major)?;
    if manifest.manifest_id.trim().is_empty()
        || !Regex::new(r"^[a-z0-9][a-z0-9._-]*$")
            .expect("static manifest ID regex")
            .is_match(&manifest.manifest_id)
    {
        return Err(AppError::Source("manifest_id is invalid".into()));
    }
    if manifest.repository.default_branch.trim().is_empty() {
        return Err(AppError::Source(
            "manifest repository default_branch is empty".into(),
        ));
    }
    if manifest.repository.provider != "github"
        || manifest.repository.owner != SOURCE_OWNER
        || manifest.repository.name != SOURCE_NAME
    {
        return Err(AppError::Source(
            "manifest repository does not match the approved source".into(),
        ));
    }
    if !manifest.update_policy.latest.resolve_default_branch
        || !manifest.update_policy.latest.record_commit
    {
        return Err(AppError::Source(
            "manifest latest policy must resolve the default branch and record the exact commit"
                .into(),
        ));
    }
    if !matches!(
        manifest.repository.license_evidence.as_deref(),
        None | Some("verified" | "declared_unverified" | "not_found" | "unknown")
    ) {
        return Err(AppError::Source(
            "manifest repository license evidence has an unsupported status".into(),
        ));
    }
    if let Some(revision) = revision {
        validate_commit(revision)?;
        match manifest.generated_for_revision.as_deref() {
            Some(declared) => {
                validate_commit(declared)?;
            }
            None => {
                return Err(AppError::Source(
                    "manifest must declare the immutable revision used to generate its evidence"
                        .into(),
                ));
            }
        }
    }
    let id_pattern = Regex::new(r"^[a-z0-9][a-z0-9._-]*$").expect("static component id regex");
    let mut components = HashMap::new();
    for component in &manifest.components {
        if component.id.len() > 128 || !id_pattern.is_match(&component.id) {
            return Err(AppError::Source(format!(
                "invalid component id: {}",
                component.id
            )));
        }
        if components.insert(component.id.clone(), component).is_some() {
            return Err(AppError::Source(format!(
                "duplicate component id: {}",
                component.id
            )));
        }
        if let Some(engine) = component.source.template_engine.as_deref() {
            let supported_engine = match (component.id.as_str(), engine) {
                ("core.agents" | "core.claude.instructions", "agents_adaptation_v1") => {
                    component.source.kind == SourceKind::File
                }
                ("mcp.hoi4_agent_tools", "mcp_config_v1") => {
                    component.source.kind == SourceKind::Generated
                }
                _ => false,
            };
            if !supported_engine {
                return Err(AppError::Source(format!(
                    "component {} declares an unsupported template engine: {engine}",
                    component.id
                )));
            }
        }
        if let Some(environment) = component.coding_environment.as_deref() {
            if !crate::coding_environment::is_supported(environment)
                || (!declared_environments.is_empty()
                    && !declared_environments.contains(environment))
            {
                return Err(AppError::Source(format!(
                    "component {} declares an unsupported coding environment: {environment}",
                    component.id
                )));
            }
            if component.category != ComponentCategory::Environment {
                return Err(AppError::Source(format!(
                    "coding-environment component {} must use the environment category",
                    component.id
                )));
            }
        } else if component.category == ComponentCategory::Environment {
            return Err(AppError::Source(format!(
                "environment component {} must declare coding_environment",
                component.id
            )));
        }
        if !component.shared_coding_environments.is_empty() {
            let owner = component.coding_environment.as_deref().ok_or_else(|| {
                AppError::Source(format!(
                    "component {} shares files without an owning coding environment",
                    component.id
                ))
            })?;
            let mut shared = HashSet::new();
            for environment in &component.shared_coding_environments {
                if major < 6
                    || environment == owner
                    || !declared_environments.contains(environment.as_str())
                    || !shared.insert(environment.as_str())
                {
                    return Err(AppError::Source(format!(
                        "component {} declares an invalid shared coding environment: {environment}",
                        component.id
                    )));
                }
            }
        }
        if component.platforms.is_empty() {
            return Err(AppError::Source(format!(
                "component {} has no platform declarations",
                component.id
            )));
        }
        if component.destination.outside_project
            || component.destination.ownership == Ownership::External
        {
            return Err(AppError::Source(format!(
                "component {} declares an unsupported external destination",
                component.id
            )));
        }
        require_canonical_manifest_path(&component.source.path, "component source")?;
        require_canonical_manifest_path(&component.destination.path, "component destination")?;
        if !matches!(component.source.kind, SourceKind::Generated)
            && component.expected_files.is_empty()
        {
            return Err(AppError::Source(format!(
                "component {} has no complete file evidence",
                component.id
            )));
        }
        let mut evidence_paths = HashSet::new();
        for expected in &component.expected_files {
            require_canonical_manifest_path(&expected.path, "expected file")?;
            let evidence_key = canonical_relative_key(&expected.path)
                .map_err(|error| AppError::Source(error.to_string()))?;
            if !evidence_paths.insert(evidence_key) {
                return Err(AppError::Source(format!(
                    "component {} has duplicate file evidence: {}",
                    component.id, expected.path
                )));
            }
            let hash = expected.sha256.as_deref().ok_or_else(|| {
                AppError::Source(format!(
                    "component {} has a file without SHA-256 evidence",
                    component.id
                ))
            })?;
            validate_sha256(hash)?;
            if expected.size.is_none() {
                return Err(AppError::Source(format!(
                    "component {} has a file without size evidence",
                    component.id
                )));
            }
        }
        for environment in &component.environment {
            crate::text::validate_env_name(&environment.name)?;
            if environment.secret && environment.storage.as_deref() != Some("os_credential_vault") {
                return Err(AppError::Source(format!(
                    "secret environment {} must use the OS credential vault",
                    environment.name
                )));
            }
            if environment.secret && environment.name != MESHY_ENVIRONMENT_NAME {
                return Err(AppError::Source(format!(
                    "unsupported secret environment {}; only {} is supported",
                    environment.name, MESHY_ENVIRONMENT_NAME
                )));
            }
        }
        for tool in &component.required_tools {
            if tool.version_policy.as_deref().is_some_and(|policy| {
                !matches!(
                    policy,
                    "manifest"
                        | "repository_script"
                        | "latest_at_execution"
                        | "user_managed"
                        | "any"
                )
            }) {
                return Err(AppError::Source(format!(
                    "component {} declares an unsupported tool version policy",
                    component.id
                )));
            }
        }
        for rule in &component.validation {
            if !matches!(rule.severity.as_str(), "block" | "warn" | "info")
                || !matches!(
                    rule.kind.as_str(),
                    "exists"
                        | "sha256"
                        | "json_schema"
                        | "toml_parse"
                        | "yaml_bom"
                        | "command"
                        | "directory_coverage"
                        | "custom"
                )
            {
                return Err(AppError::Source(format!(
                    "component {} declares an unsupported validation rule",
                    component.id
                )));
            }
            if rule.kind == "command" {
                for identity in ["executable", "interpreter", "runtime"] {
                    let hash_key = format!("{identity}_sha256");
                    let size_key = format!("{identity}_size");
                    let hash_value = rule.parameters.get(&hash_key);
                    let size_value = rule.parameters.get(&size_key);
                    if hash_value.is_none() != size_value.is_none() {
                        return Err(AppError::Source(format!(
                            "component {} command identity must declare both {hash_key} and {size_key}",
                            component.id
                        )));
                    }
                    if let Some(value) = hash_value {
                        let hash = value.as_str().ok_or_else(|| {
                            AppError::Source(format!(
                                "component {} command identity SHA-256 must be a string",
                                component.id
                            ))
                        })?;
                        validate_sha256(hash)?;
                    }
                    if let Some(value) = size_value {
                        if value.as_u64().is_none() {
                            return Err(AppError::Source(format!(
                                "component {} command identity size must be a non-negative integer",
                                component.id
                            )));
                        }
                    }
                }
            }
        }
        if component.id == "workflow.3d" {
            let actions = component
                .validation
                .iter()
                .filter(|rule| rule.kind == "command" && rule.id == "3d.bootstrap")
                .collect::<Vec<_>>();
            if actions.len() != 1 {
                return Err(AppError::Source(
                    "workflow.3d must declare exactly one reviewed bootstrap action".into(),
                ));
            }
            let action = actions[0];
            let arguments = action.parameters.get("arguments").and_then(Value::as_array);
            let expected_writes = action
                .parameters
                .get("expected_writes")
                .and_then(Value::as_array);
            let declared_text = |key: &str| {
                action
                    .parameters
                    .get(key)
                    .and_then(Value::as_str)
                    .is_some_and(|value| !value.trim().is_empty())
            };
            if action.target.as_deref() != Some(".tools/3d_pipeline/bootstrap_3d_workflow.py")
                || arguments.is_none_or(Vec::is_empty)
                || expected_writes.is_none_or(Vec::is_empty)
                || !declared_text("network_access")
                || action.parameters.get("privilege").and_then(Value::as_str)
                    != Some("current_user")
                || !declared_text("rollback_boundary")
            {
                return Err(AppError::Source(
                    "workflow.3d bootstrap action evidence is incomplete".into(),
                ));
            }
        }
        if component.id == "mcp.hoi4_agent_tools.bootstrap" {
            let actions = component
                .validation
                .iter()
                .filter(|rule| rule.kind == "command" && rule.id == "mcp.bootstrap")
                .collect::<Vec<_>>();
            if actions.len() != 1 {
                return Err(AppError::Source(
                    "the MCP bootstrap component must declare exactly one reviewed action".into(),
                ));
            }
            let action = actions[0];
            let arguments = action.parameters.get("arguments").and_then(Value::as_array);
            let expected_writes = action
                .parameters
                .get("expected_writes")
                .and_then(Value::as_array);
            let declared_text = |key: &str| {
                action
                    .parameters
                    .get(key)
                    .and_then(Value::as_str)
                    .is_some_and(|value| !value.trim().is_empty())
            };
            if component.source.path != ".tools/mcp/bootstrap_hoi4_agent_tools.py"
                || action.target.as_deref() != Some(".tools/mcp/bootstrap_hoi4_agent_tools.py")
                || arguments
                    .is_none_or(|values| values.as_slice() != [Value::String("--quiet".into())])
                || expected_writes.is_none_or(Vec::is_empty)
                || !declared_text("network_access")
                || action.parameters.get("privilege").and_then(Value::as_str)
                    != Some("current_user")
                || !declared_text("rollback_boundary")
            {
                return Err(AppError::Source(
                    "the MCP bootstrap reviewed action evidence is incomplete".into(),
                ));
            }
        }
        if component.id == crate::mcp_target::LIVE_TOOLS_COMPONENT_ID
            && component
                .validation
                .iter()
                .any(|rule| rule.kind == "command")
        {
            // A manifest that predates the installer declares no command and
            // stays a skill-only workflow. One that declares it must pin the
            // release by URL, SHA-256 and size and run only the managed script.
            let actions = component
                .validation
                .iter()
                .filter(|rule| rule.kind == "command" && rule.id == "live.bootstrap")
                .collect::<Vec<_>>();
            let invalid = |detail: &str| {
                AppError::Source(format!(
                    "the HOI4 Live Tools bootstrap action evidence is incomplete: {detail}"
                ))
            };
            if actions.len() != 1
                || component
                    .validation
                    .iter()
                    .filter(|rule| rule.kind == "command")
                    .count()
                    != 1
            {
                return Err(invalid("exactly one reviewed action is required"));
            }
            let action = actions[0];
            let text = |key: &str| action.parameters.get(key).and_then(Value::as_str);
            let declared_text = |key: &str| text(key).is_some_and(|value| !value.trim().is_empty());
            if action.target.as_deref() != Some(".tools/mcp/bootstrap_hoi4_live_tools.py")
                || action
                    .parameters
                    .get("arguments")
                    .and_then(Value::as_array)
                    .is_none_or(|values| {
                        values.as_slice()
                            != [
                                Value::String("--quiet".into()),
                                Value::String("--register".into()),
                                Value::String("none".into()),
                            ]
                            && (manifest.live_testing.is_some()
                                || values.as_slice() != [Value::String("--quiet".into())])
                    })
                || action
                    .parameters
                    .get("expected_writes")
                    .and_then(Value::as_array)
                    .is_none_or(Vec::is_empty)
                || !declared_text("network_access")
                || !declared_text("rollback_boundary")
                || text("privilege") != Some("current_user")
            {
                return Err(invalid("target, arguments, writes, network, or privilege"));
            }
            let url = text("release_url").unwrap_or_default();
            if !url.starts_with("https://github.com/")
                || url.contains("..")
                || !url.ends_with(".zip")
            {
                return Err(invalid(
                    "release_url must be an https://github.com release archive",
                ));
            }
            validate_sha256(text("release_sha256").unwrap_or_default())?;
            if action
                .parameters
                .get("release_size")
                .and_then(Value::as_u64)
                .is_none_or(|size| size == 0)
                || !declared_text("release_version")
            {
                return Err(invalid("release_size and release_version are required"));
            }
            if !manifest
                .components
                .iter()
                .any(|candidate| candidate.id == "mcp.hoi4_live.bootstrap")
                || !component
                    .dependencies
                    .iter()
                    .any(|dependency| dependency == "mcp.hoi4_live.bootstrap")
            {
                return Err(invalid(
                    "the managed bootstrap script component is not a dependency",
                ));
            }
        }
        for platform in &component.platforms {
            if matches!(platform, ManifestPlatform::All)
                && (component
                    .required_tools
                    .iter()
                    .any(|tool| !tool.commands.is_empty())
                    || component
                        .validation
                        .iter()
                        .any(|rule| rule.kind == "command"))
            {
                return Err(AppError::Source(format!(
                    "command-bearing component {} must declare explicit platform routes",
                    component.id
                )));
            }
        }
    }
    for component in &manifest.components {
        for dependency in &component.dependencies {
            if !components.contains_key(dependency) {
                return Err(AppError::Source(format!(
                    "component {} depends on unknown component {dependency}",
                    component.id
                )));
            }
        }
    }
    crate::mcp_target::manifest_target(manifest)?;
    let component_ids: HashSet<&str> = components.keys().map(|id| id.as_str()).collect();
    for profile in &manifest.profiles {
        if profile.id.trim().is_empty() || profile.components.is_empty() {
            return Err(AppError::Source(format!("profile {} is empty", profile.id)));
        }
        for component in &profile.components {
            if !component_ids.contains(component.as_str()) {
                return Err(AppError::Source(format!(
                    "profile {} references unknown component {component}",
                    profile.id
                )));
            }
        }
    }
    if manifest
        .profiles
        .iter()
        .filter(|profile| profile.default)
        .count()
        != 1
    {
        return Err(AppError::Source(
            "manifest must declare exactly one default profile".into(),
        ));
    }
    validate_dependency_cycles(&components)?;
    crate::live_testing::validate_catalog(manifest)?;
    validate_manifest_destinations(&manifest.components)?;
    let wiki_component = components
        .get(&manifest.wiki.component_id)
        .ok_or_else(|| AppError::Source("wiki component_id is not declared".into()))?;
    if wiki_component.category != ComponentCategory::Wiki {
        return Err(AppError::Source(
            "wiki component_id must identify a wiki component".into(),
        ));
    }
    if !matches!(
        manifest.wiki.required_media_policy.as_str(),
        "all_declared" | "referenced_only" | "none"
    ) {
        return Err(AppError::Source("unsupported wiki media policy".into()));
    }
    if !matches!(
        manifest.wiki.provenance.source_status.as_str(),
        "verified" | "repository_only" | "unknown"
    ) || !matches!(
        manifest.wiki.provenance.license_status.as_str(),
        "verified" | "not_found" | "unknown"
    ) {
        return Err(AppError::Source(
            "wiki provenance contains an unsupported status".into(),
        ));
    }
    let mut required_pages = HashSet::new();
    for page in &manifest.wiki.required_pages {
        let page = require_canonical_manifest_path(page, "required wiki page")
            .map_err(|error| AppError::Source(format!("invalid required wiki page: {error}")))?;
        if !required_pages.insert(page.clone()) {
            return Err(AppError::Source(format!(
                "duplicate required wiki page: {page}"
            )));
        }
        let expected_path = format!("paradox_wiki/{page}");
        if !wiki_component
            .expected_files
            .iter()
            .any(|file| file.path == expected_path)
        {
            return Err(AppError::Source(format!(
                "required wiki page lacks manifest evidence: {page}"
            )));
        }
    }
    if manifest.wiki.destination != "paradox_wiki/" {
        return Err(AppError::Source(
            "wiki destination must be paradox_wiki/".into(),
        ));
    }
    if manifest.wiki.required_pages.is_empty() {
        return Err(AppError::Source("wiki must declare required pages".into()));
    }
    if manifest.update_policy.rollback_retention == 0 {
        return Err(AppError::Source(
            "rollback retention must be positive".into(),
        ));
    }
    if manifest
        .signing
        .as_ref()
        .is_some_and(|policy| policy.required)
    {
        return Err(AppError::Source(
            "signed manifests are declared but signature verification is not supported by this build"
                .into(),
        ));
    }
    Ok(())
}

pub fn validate_commit(value: &str) -> Result<(), AppError> {
    let valid = value.len() == 40 && value.chars().all(|character| character.is_ascii_hexdigit());
    if valid {
        Ok(())
    } else {
        Err(AppError::Source(format!(
            "expected a full immutable commit SHA: {value}"
        )))
    }
}

pub fn validate_sha256(value: &str) -> Result<(), AppError> {
    if value.len() == 64
        && value.chars().all(|character| character.is_ascii_hexdigit())
        && value == value.to_ascii_lowercase()
    {
        Ok(())
    } else {
        Err(AppError::Source(format!(
            "invalid lowercase SHA-256: {value}"
        )))
    }
}

pub fn validate_dependency_cycles(
    components: &HashMap<String, &ComponentDefinition>,
) -> Result<(), AppError> {
    fn visit(
        id: &str,
        components: &HashMap<String, &ComponentDefinition>,
        visiting: &mut HashSet<String>,
        visited: &mut HashSet<String>,
    ) -> Result<(), AppError> {
        if visited.contains(id) {
            return Ok(());
        }
        if !visiting.insert(id.to_string()) {
            return Err(AppError::Source(format!("dependency cycle includes {id}")));
        }
        let component = components
            .get(id)
            .ok_or_else(|| AppError::Source(format!("unknown component {id}")))?;
        for dependency in &component.dependencies {
            visit(dependency, components, visiting, visited)?;
        }
        visiting.remove(id);
        visited.insert(id.to_string());
        Ok(())
    }

    let mut visiting = HashSet::new();
    let mut visited = HashSet::new();
    for id in components.keys() {
        visit(id, components, &mut visiting, &mut visited)?;
    }
    Ok(())
}

pub fn expand_components(
    manifest: &RemoteManifest,
    requested: &[String],
) -> Result<Vec<String>, AppError> {
    let by_id: HashMap<&str, &ComponentDefinition> = manifest
        .components
        .iter()
        .map(|component| (component.id.as_str(), component))
        .collect();
    let mut selected = BTreeSet::new();
    fn add(
        id: &str,
        by_id: &HashMap<&str, &ComponentDefinition>,
        selected: &mut BTreeSet<String>,
    ) -> Result<(), AppError> {
        let component = by_id
            .get(id)
            .ok_or_else(|| AppError::Source(format!("unknown selected component: {id}")))?;
        if selected.insert(id.to_string()) {
            for dependency in &component.dependencies {
                add(dependency, by_id, selected)?;
            }
        }
        Ok(())
    }
    for id in requested {
        add(id, &by_id, &mut selected)?;
    }
    let mut ordered = Vec::new();
    fn append_topological(
        id: &str,
        by_id: &HashMap<&str, &ComponentDefinition>,
        selected: &BTreeSet<String>,
        appended: &mut HashSet<String>,
        ordered: &mut Vec<String>,
    ) {
        if !selected.contains(id) || !appended.insert(id.to_string()) {
            return;
        }
        for dependency in &by_id[id].dependencies {
            append_topological(dependency, by_id, selected, appended, ordered);
        }
        ordered.push(id.to_string());
    }
    let mut appended = HashSet::new();
    for id in requested {
        append_topological(id, &by_id, &selected, &mut appended, &mut ordered);
    }
    Ok(ordered)
}

/// Return the reverse dependency graph for the verified manifest. The map is
/// used to explain removal/update impact; it never grants a dependent the
/// right to bypass the forward dependency closure.
pub fn reverse_dependencies(manifest: &RemoteManifest) -> HashMap<String, Vec<String>> {
    let mut reverse = manifest
        .components
        .iter()
        .map(|component| (component.id.clone(), Vec::new()))
        .collect::<HashMap<_, _>>();
    for component in &manifest.components {
        for dependency in &component.dependencies {
            reverse
                .entry(dependency.clone())
                .or_default()
                .push(component.id.clone());
        }
    }
    for dependents in reverse.values_mut() {
        dependents.sort();
        dependents.dedup();
    }
    reverse
}

pub fn resolve_platform_support(
    manifest: &RemoteManifest,
    selected: &[String],
    platform: Platform,
) -> Result<Vec<ComponentSupport>, AppError> {
    let by_id: HashMap<&str, &ComponentDefinition> = manifest
        .components
        .iter()
        .map(|component| (component.id.as_str(), component))
        .collect();
    let reverse = reverse_dependencies(manifest);
    selected
        .iter()
        .map(|id| {
            let component = by_id
                .get(id.as_str())
                .ok_or_else(|| AppError::Source(format!("unknown selected component: {id}")))?;
            let dependents = reverse.get(id).cloned().unwrap_or_default();
            if component
                .platforms
                .iter()
                .any(|value| value.supports(platform))
            {
                Ok(ComponentSupport {
                    component_id: id.clone(),
                    selected: true,
                    state: "supported".into(),
                    reason: None,
                    dependents,
                })
            } else if component.optional {
                Ok(ComponentSupport {
                    component_id: id.clone(),
                    selected: true,
                    state: "unsupported_platform".into(),
                    reason: Some(format!(
                        "{} has no verified {} route",
                        component.display_name,
                        platform.manifest_name()
                    )),
                    dependents,
                })
            } else {
                Ok(ComponentSupport {
                    component_id: id.clone(),
                    selected: true,
                    state: "blocked".into(),
                    reason: Some(format!(
                        "required component {} is unsupported on {}",
                        component.id,
                        platform.manifest_name()
                    )),
                    dependents,
                })
            }
        })
        .collect()
}

/// The manifest owns the file inventory. No full repository tree is needed
/// during installation; each selected blob still has to pass size and SHA-256.
pub fn select_manifest_files(
    manifest: &RemoteManifest,
    component_ids: &[String],
) -> Result<Vec<SelectedSourceFile>, AppError> {
    let ids = component_ids
        .iter()
        .map(String::as_str)
        .collect::<HashSet<_>>();
    let mut declared = std::collections::BTreeMap::new();
    for component in manifest
        .components
        .iter()
        .filter(|component| ids.contains(component.id.as_str()))
    {
        if component.source.kind == SourceKind::Generated {
            continue;
        }
        for file in &component.expected_files {
            let path = normalize_relative_path(&file.path)
                .map_err(|error| AppError::Source(error.to_string()))?;
            declared.insert(
                path.clone(),
                TreeEntry {
                    path,
                    kind: "blob".into(),
                    size: file.size,
                },
            );
        }
    }
    select_component_files(
        manifest,
        component_ids,
        &declared.into_values().collect::<Vec<_>>(),
    )
}

pub fn select_component_files(
    manifest: &RemoteManifest,
    component_ids: &[String],
    tree: &[TreeEntry],
) -> Result<Vec<SelectedSourceFile>, AppError> {
    let by_id: HashMap<&str, &ComponentDefinition> = manifest
        .components
        .iter()
        .map(|component| (component.id.as_str(), component))
        .collect();
    let mut destinations = HashSet::new();
    let mut selected = Vec::new();
    for component_id in component_ids {
        let component = by_id
            .get(component_id.as_str())
            .ok_or_else(|| AppError::Source(format!("unknown component: {component_id}")))?;
        match component.source.kind {
            SourceKind::Generated => continue,
            SourceKind::File => {
                let source_path = normalize_relative_path(&component.source.path)
                    .map_err(|error| AppError::Source(error.to_string()))?;
                if is_gitkeep_path(&source_path) {
                    continue;
                }
                add_source_file(component, &source_path, &mut destinations, &mut selected)?;
            }
            SourceKind::Tree => {
                let prefix = normalize_relative_path(&component.source.path)
                    .map_err(|error| AppError::Source(error.to_string()))?;
                let mut matched_evidence = HashSet::new();
                for entry in tree.iter().filter(|entry| entry.kind == "blob") {
                    if entry.path != prefix && !entry.path.starts_with(&format!("{prefix}/")) {
                        continue;
                    }
                    let relative = entry
                        .path
                        .strip_prefix(&prefix)
                        .unwrap_or("")
                        .trim_start_matches('/');
                    if !component.source.include.is_empty()
                        && !component
                            .source
                            .include
                            .iter()
                            .any(|pattern| glob_matches(pattern, relative))
                    {
                        continue;
                    }
                    if component
                        .source
                        .exclude
                        .iter()
                        .any(|pattern| glob_matches(pattern, relative))
                    {
                        continue;
                    }
                    if is_gitkeep_path(&entry.path) {
                        continue;
                    }
                    if component
                        .expected_files
                        .iter()
                        .any(|file| file.path == entry.path)
                    {
                        matched_evidence.insert(entry.path.clone());
                    }
                    add_source_file(component, &entry.path, &mut destinations, &mut selected)?;
                }
                for expected in &component.expected_files {
                    let expected_path = normalize_relative_path(&expected.path)
                        .map_err(|error| AppError::Source(error.to_string()))?;
                    if is_gitkeep_path(&expected_path) {
                        continue;
                    }
                    if !matched_evidence.contains(&expected_path) {
                        return Err(AppError::Source(format!(
                            "manifest evidence is not present in the selected source tree: {}",
                            expected.path
                        )));
                    }
                }
            }
        }
    }
    if selected.len() > MAX_SELECTED_FILES {
        return Err(AppError::Source(
            "selected source file count exceeds the aggregate limit".into(),
        ));
    }
    let selected_bytes = selected
        .iter()
        .filter_map(|file| file.expected_size)
        .try_fold(0_u64, |total, size| total.checked_add(size))
        .ok_or_else(|| AppError::Source("selected source size overflows".into()))?;
    if selected_bytes > MAX_SELECTED_BYTES {
        return Err(AppError::Source(
            "selected source size exceeds the aggregate limit".into(),
        ));
    }
    Ok(selected)
}

pub fn is_gitkeep_path(path: &str) -> bool {
    path.rsplit('/').next() == Some(".gitkeep")
}

pub fn add_source_file(
    component: &ComponentDefinition,
    source_path: &str,
    destinations: &mut HashSet<String>,
    selected: &mut Vec<SelectedSourceFile>,
) -> Result<(), AppError> {
    let destination = match component.source.kind {
        SourceKind::File | SourceKind::Generated => component.destination.path.clone(),
        SourceKind::Tree => {
            let prefix = normalize_relative_path(&component.source.path)
                .map_err(|error| AppError::Source(error.to_string()))?;
            let relative = source_path
                .strip_prefix(&prefix)
                .unwrap_or("")
                .trim_start_matches('/');
            let root = component.destination.path.trim_end_matches('/');
            format!("{root}/{relative}")
        }
    };
    let destination = normalize_relative_path(&destination)
        .map_err(|error| AppError::Source(error.to_string()))?;
    let destination_key = canonical_relative_key(&destination)
        .map_err(|error| AppError::Source(error.to_string()))?;
    if !destinations.insert(destination_key) {
        return Err(AppError::Source(format!(
            "duplicate selected destination: {destination}"
        )));
    }
    let expected = component
        .expected_files
        .iter()
        .find(|file| file.path == source_path || file.path == destination);
    if !matches!(component.source.kind, SourceKind::Generated)
        && expected
            .map(|file| file.sha256.is_none() || file.size.is_none())
            .unwrap_or(true)
    {
        return Err(AppError::Source(format!(
            "selected source file lacks manifest evidence: {source_path}"
        )));
    }
    selected.push(SelectedSourceFile {
        component_id: component.id.clone(),
        source_path: source_path.to_string(),
        destination,
        ownership: component.destination.ownership,
        expected_sha256: expected.and_then(|file| file.sha256.clone()),
        expected_size: expected.and_then(|file| file.size),
        executable: false,
        platform: component
            .platforms
            .first()
            .copied()
            .unwrap_or(ManifestPlatform::All),
    });
    Ok(())
}

const GIT_LFS_POINTER_PREFIX: &[u8] = b"version https://git-lfs.github.com/spec/v1";

pub fn verify_download(
    selection: &SelectedSourceFile,
    bytes: &[u8],
    revision: &str,
) -> Result<DownloadedFile, AppError> {
    validate_commit(revision)?;
    let expected_sha256 = selection.expected_sha256.as_deref().ok_or_else(|| {
        AppError::Source(format!(
            "download has no manifest SHA-256 evidence: {}",
            selection.source_path
        ))
    })?;
    validate_sha256(expected_sha256)?;
    let hash = sha256_bytes(bytes);
    if expected_sha256 != hash.as_str() {
        return Err(AppError::Source(format!(
            "checksum mismatch for {}: expected {}, received {}",
            selection.source_path, expected_sha256, hash
        )));
    }
    if let Some(expected_size) = selection.expected_size {
        if expected_size != bytes.len() as u64 {
            return Err(AppError::Source(format!(
                "size mismatch for {}",
                selection.source_path
            )));
        }
    }
    // A committed Git LFS pointer hashes correctly against Git-blob evidence
    // but is a placeholder, not the declared file; reject it before staging.
    if bytes.starts_with(GIT_LFS_POINTER_PREFIX) {
        return Err(AppError::Source(format!(
            "source file is an unresolved Git LFS pointer: {}",
            selection.source_path
        )));
    }
    Ok(DownloadedFile {
        operation_id: String::new(),
        source_path: selection.source_path.clone(),
        destination: selection.destination.clone(),
        source_revision: revision.to_string(),
        manifest_sha256: String::new(),
        sha256: hash,
        size: bytes.len() as u64,
        component_id: selection.component_id.clone(),
        ownership: selection.ownership,
        platform: selection.platform,
        executable: selection.executable,
    })
}

pub fn glob_matches(pattern: &str, path: &str) -> bool {
    let mut expression = String::from("^");
    let chars: Vec<char> = pattern.chars().collect();
    let mut index = 0;
    while index < chars.len() {
        match chars[index] {
            '*' if index + 1 < chars.len() && chars[index + 1] == '*' => {
                expression.push_str(".*");
                index += 2;
            }
            '*' => {
                expression.push_str("[^/]*");
                index += 1;
            }
            '?' => {
                expression.push_str("[^/]");
                index += 1;
            }
            character => {
                expression.push_str(&regex::escape(&character.to_string()));
                index += 1;
            }
        }
    }
    expression.push('$');
    Regex::new(&expression)
        .map(|regex| regex.is_match(path))
        .unwrap_or(false)
}

pub fn cache_key(source: &SourceIdentity, source_path: &str, sha256: &str) -> String {
    format!(
        "{}/{}/{}",
        source.resolved_revision,
        source_path.replace('/', "_"),
        sha256
    )
}

pub fn add_supported_profile_components(
    manifest: &RemoteManifest,
    requested: &mut Vec<String>,
    component_ids: &[String],
    platform: Platform,
) -> Result<(), AppError> {
    for component_id in component_ids {
        if requested.iter().any(|selected| selected == component_id) {
            continue;
        }
        let mut candidate = requested.clone();
        candidate.push(component_id.clone());
        let expanded = expand_components(manifest, &candidate)?;
        let support = resolve_platform_support(manifest, &expanded, platform)?;
        if support.iter().any(|item| item.state == "blocked") {
            continue;
        }
        requested.push(component_id.clone());
    }
    Ok(())
}

pub fn default_profile_component_ids(manifest: &RemoteManifest) -> Result<Vec<String>, AppError> {
    manifest
        .profiles
        .iter()
        .find(|profile| profile.default)
        .map(|profile| profile.components.clone())
        .ok_or_else(|| {
            AppError::Source("the verified source manifest has no default profile".into())
        })
}

/// The components a new installation requests before dependency expansion:
/// the user's choices, the default profile where the platform supports it,
/// the runtime-neutral shared package, every other required runtime-neutral
/// component, and the selected coding clients' packages. Both the desktop
/// plan and the web export start from this list.
pub fn installation_components(
    manifest: &RemoteManifest,
    user_selected: &[String],
    selection: &CodingEnvironmentSelection,
    platform: Platform,
) -> Result<Vec<String>, AppError> {
    let environment_ids = crate::coding_environment::all_environment_component_ids(manifest);
    let mut requested = user_selected
        .iter()
        .filter(|id| !environment_ids.contains(*id))
        .cloned()
        .collect::<Vec<_>>();
    // Resolve non-optional defaults from the current source manifest so a
    // newly published runtime guide or core component is included even if a
    // renderer is stale or omits the newly declared ID.
    let defaults = default_profile_component_ids(manifest)?;
    add_supported_profile_components(manifest, &mut requested, &defaults, platform)?;
    // Runtime-neutral components that are always installed. The manifest
    // remains authoritative: a source revision that does not declare one
    // cannot be made complete by inventing a replacement.
    for shared in [
        "core.agents",
        "core.skills",
        "core.subagents",
        "runtime.agent_sync",
        "mcp.hoi4_agent_tools.bootstrap",
        "mcp.hoi4_agent_tools",
        "docs.mcp_integration",
        "wiki.snapshot",
    ] {
        if manifest
            .components
            .iter()
            .any(|component| component.id == shared)
            && !requested.iter().any(|id| id == shared)
        {
            requested.push(shared.into());
        }
    }
    for component in &manifest.components {
        if !component.optional
            && component.coding_environment.is_none()
            && component
                .platforms
                .iter()
                .any(|declared| declared.supports(platform))
            && !requested.iter().any(|id| id == &component.id)
        {
            requested.push(component.id.clone());
        }
    }
    let active_component_ids = expand_components(manifest, &requested)?
        .into_iter()
        .collect::<HashSet<_>>();
    for id in crate::coding_environment::component_ids_for_platform(
        manifest,
        selection,
        &active_component_ids,
        platform,
    )? {
        if !requested.iter().any(|selected| selected == &id) {
            requested.push(id);
        }
    }
    Ok(requested)
}
