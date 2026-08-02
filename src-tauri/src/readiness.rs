use crate::descriptors::{parse_descriptor, validate_thumbnail_png};
use crate::git::read_git_head;
use crate::models::*;
use crate::security::{
    is_link_metadata, path_has_link_component, safe_join, sha256_file,
    validate_external_destination,
};
use crate::AppError;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::fs;
use std::path::{Path, PathBuf};

#[cfg(test)]
pub(crate) fn manifest_wiki_pages() -> Vec<String> {
    serde_json::from_slice::<RemoteManifest>(include_bytes!(
        "../../docs/source-manifest/hoi4-mod-setup.manifest.json"
    ))
    .map(|manifest| manifest.wiki.required_pages)
    .unwrap_or_default()
}

fn mcp_component_is_selected(state: &str) -> bool {
    !matches!(state, "not_selected" | "removed" | "unsupported_platform")
}

fn component_dependency_is_satisfied(component: &LockComponent) -> bool {
    component.validation.as_deref() == Some("pass")
        || (component.id == "mcp.hoi4_agent_tools"
            && matches!(
                component.validation.as_deref(),
                Some("planned_unavailable" | "unsupported_platform")
            ))
}

const AGENT_TREE_MAX_FILES: usize = 20_000;
const AGENT_TREE_MAX_DEPTH: usize = 16;

fn bounded_agent_files(root: &Path, suffix: &str) -> Option<Vec<PathBuf>> {
    let mut pending = vec![(root.to_path_buf(), 0usize)];
    let mut files = Vec::new();
    while let Some((directory, depth)) = pending.pop() {
        if depth > AGENT_TREE_MAX_DEPTH
            || path_has_link_component(&directory)
            || !fs::symlink_metadata(&directory).ok()?.is_dir()
        {
            return None;
        }
        for entry in fs::read_dir(&directory).ok()? {
            let entry = entry.ok()?;
            let path = entry.path();
            let metadata = fs::symlink_metadata(&path).ok()?;
            if is_link_metadata(&metadata) || path_has_link_component(&path) {
                return None;
            }
            if metadata.is_dir() {
                pending.push((path, depth + 1));
            } else if metadata.is_file()
                && path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.ends_with(suffix))
            {
                files.push(path);
                if files.len() > AGENT_TREE_MAX_FILES {
                    return None;
                }
            }
        }
    }
    Some(files)
}

pub(crate) fn valid_agents_file(project_root: &Path) -> bool {
    let path = project_root.join("AGENTS.md");
    fs::symlink_metadata(&path)
        .ok()
        .is_some_and(|metadata| metadata.is_file() && !path_has_link_component(&path))
        && fs::read_to_string(path).ok().is_some_and(|text| {
            !text.trim().is_empty() && !text.contains("{{PROJECT_") && !text.contains("<PROJECT_")
        })
}

pub(crate) fn valid_skill_tree(project_root: &Path) -> bool {
    let root = project_root.join(".agents/skills");
    let Some(files) = bounded_agent_files(&root, "SKILL.md") else {
        return false;
    };
    !files.is_empty()
        && files.iter().all(|path| {
            fs::read_to_string(path).ok().is_some_and(|text| {
                text.starts_with("---\n")
                    && text.contains("\nname:")
                    && text.contains("\ndescription:")
            })
        })
}

pub(crate) fn valid_subagent_tree(project_root: &Path) -> bool {
    let root = project_root.join(".codex/agents");
    let Some(files) = bounded_agent_files(&root, ".toml") else {
        return false;
    };
    !files.is_empty()
        && files.iter().all(|path| {
            let Ok(text) = fs::read_to_string(path) else {
                return false;
            };
            let Ok(value) = text.parse::<toml::Value>() else {
                return false;
            };
            match value.get("fork_context").and_then(toml::Value::as_bool) {
                Some(value) => !value,
                None => text.contains("fork_context=false"),
            }
        })
}

pub(crate) fn flattened_artifact_status(
    project_root: &Path,
    artifacts: &[GeneratedArtifact],
) -> String {
    if artifacts.is_empty()
        || artifacts.iter().any(|artifact| {
            !artifact
                .destination
                .replace('\\', "/")
                .starts_with("chatgpt_project_sources/")
                || safe_join(project_root, &artifact.destination)
                    .ok()
                    .and_then(|path| fs::symlink_metadata(path).ok())
                    .is_none_or(|metadata| is_link_metadata(&metadata) || !metadata.is_file())
                || safe_join(project_root, &artifact.destination)
                    .ok()
                    .and_then(|path| sha256_file(&path).ok())
                    .is_none_or(|hash| hash != artifact.expected_sha256)
        })
    {
        return "block".into();
    }
    let has_agents = artifacts
        .iter()
        .any(|artifact| artifact.destination == "chatgpt_project_sources/AGENTS.md");
    let has_readme = artifacts
        .iter()
        .any(|artifact| artifact.destination == "chatgpt_project_sources/README.md");
    if has_agents && has_readme {
        "pass".into()
    } else {
        "block".into()
    }
}

pub(crate) fn flattened_lock_status(project_root: &Path, lock: &InstallationLock) -> String {
    if !lock.flatten_chat_sources {
        return "not_selected".into();
    }
    let artifacts = lock
        .files
        .iter()
        .filter(|file| file.component_id == "codex.chat_flatten" && !file.external)
        .map(|file| GeneratedArtifact {
            component_id: file.component_id.clone(),
            destination: file.path.clone(),
            content: file.generated_content.clone().unwrap_or_default(),
            expected_sha256: file.installed_sha256.clone(),
            external: false,
            bytes: file.generated_bytes.clone(),
        })
        .collect::<Vec<_>>();
    flattened_artifact_status(project_root, &artifacts)
}

fn locked_file_destination(
    project_root: &Path,
    file: &LockedFile,
) -> Result<std::path::PathBuf, AppError> {
    if file.external {
        validate_external_destination(&file.path)
    } else {
        safe_join(project_root, &file.path)
    }
}

#[cfg(unix)]
fn locked_executable_state_matches(path: &Path, expected: bool) -> bool {
    use std::os::unix::fs::PermissionsExt;

    fs::symlink_metadata(path)
        .ok()
        .filter(|metadata| metadata.is_file() && !is_link_metadata(metadata))
        .is_some_and(|metadata| (metadata.permissions().mode() & 0o111 != 0) == expected)
}

#[cfg(not(unix))]
fn locked_executable_state_matches(_path: &Path, _expected: bool) -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadinessInput {
    pub project_id: String,
    pub project_root: String,
    #[serde(default)]
    pub selected_components: Vec<String>,
    #[serde(default)]
    pub source_verified: bool,
    #[serde(default)]
    pub descriptors_valid: bool,
    #[serde(default)]
    pub launcher_valid: bool,
    #[serde(default)]
    pub thumbnail_valid: bool,
    #[serde(default)]
    pub structure_valid: bool,
    #[serde(default)]
    pub agents_valid: bool,
    #[serde(default)]
    pub skills_valid: bool,
    #[serde(default)]
    pub subagents_valid: bool,
    #[serde(default)]
    pub codex_valid: bool,
    #[serde(default)]
    pub codex_authenticated: bool,
    #[serde(default)]
    pub codex_analysis_status: String,
    #[serde(default)]
    pub codex_confirmed_field_count: u32,
    #[serde(default = "crate::models::default_ai_provider")]
    pub ai_provider: String,
    #[serde(default = "crate::models::default_ai_model")]
    pub ai_model: String,
    #[serde(default)]
    pub ai_authenticated: bool,
    #[serde(default)]
    pub ai_analysis_status: String,
    #[serde(default)]
    pub ai_confirmed_field_count: u32,
    #[serde(default)]
    pub flatten_status: String,
    #[serde(default)]
    pub mcp_status: String,
    #[serde(default)]
    pub mcp_blocking: bool,
    #[serde(default)]
    pub wiki_status: String,
    #[serde(default)]
    pub wiki_required_pages: Vec<String>,
    #[serde(default)]
    pub wiki_broken_links: Vec<String>,
    #[serde(default)]
    pub git_status: String,
    #[serde(default)]
    pub environment_status: String,
    #[serde(default)]
    pub hashes_valid: bool,
    #[serde(default)]
    pub conflict_status: String,
    #[serde(default)]
    pub dependency_status: String,
    #[serde(default)]
    pub workflow_3d_state: String,
    #[serde(default)]
    pub workflow_super_events_state: String,
    /// Provenance is copied from the exact locked manifest. Readiness does
    /// not substitute a newer bundled manifest for an installed revision.
    #[serde(default = "crate::models::default_unknown_status")]
    pub source_license_status: String,
    #[serde(default = "crate::models::default_unknown_status")]
    pub wiki_source_status: String,
    #[serde(default = "crate::models::default_unknown_status")]
    pub wiki_license_status: String,
    #[serde(default)]
    pub notes: Vec<String>,
}

impl Default for ReadinessInput {
    fn default() -> Self {
        Self {
            project_id: "project".into(),
            project_root: ".".into(),
            selected_components: vec![],
            source_verified: false,
            descriptors_valid: false,
            launcher_valid: false,
            thumbnail_valid: false,
            structure_valid: false,
            agents_valid: false,
            skills_valid: false,
            subagents_valid: false,
            codex_valid: false,
            codex_authenticated: false,
            codex_analysis_status: "blocked".into(),
            codex_confirmed_field_count: 0,
            ai_provider: crate::models::default_ai_provider(),
            ai_model: crate::models::default_ai_model(),
            ai_authenticated: false,
            ai_analysis_status: "blocked".into(),
            ai_confirmed_field_count: 0,
            flatten_status: "not_selected".into(),
            mcp_status: "not_selected".into(),
            mcp_blocking: false,
            wiki_status: "not_selected".into(),
            wiki_required_pages: vec![],
            wiki_broken_links: vec![],
            git_status: "not_selected".into(),
            environment_status: "not_selected".into(),
            hashes_valid: false,
            conflict_status: "pass".into(),
            dependency_status: "pass".into(),
            workflow_3d_state: "not_selected".into(),
            workflow_super_events_state: "not_selected".into(),
            source_license_status: "unknown".into(),
            wiki_source_status: "unknown".into(),
            wiki_license_status: "unknown".into(),
            notes: vec![],
        }
    }
}

pub fn evaluate(input: &ReadinessInput) -> ReadinessReport {
    let mut checks = Vec::new();
    let ai_provider = if input.ai_provider.trim().is_empty() {
        "codex"
    } else {
        input.ai_provider.trim()
    };
    let ai_model = if input.ai_model.trim().is_empty() {
        "default"
    } else {
        input.ai_model.trim()
    };
    let launcher_required = input.selected_components.iter().any(|component| {
        matches!(
            component.as_str(),
            "project.launcher_scaffold"
                | "project.descriptor"
                | "project.launcher_descriptor"
                | "project.thumbnail"
        )
    });
    let thumbnail_required = launcher_required;
    add_bool_check(
        &mut checks,
        "source.revision",
        "Source revision",
        "dependency",
        input.source_verified,
        true,
        "The manifest and selected files share one verified exact revision.",
        "source",
    );
    let license_status = if input.source_license_status.trim().is_empty() {
        "unknown"
    } else {
        input.source_license_status.trim()
    };
    let wiki_source_status = if input.wiki_source_status.trim().is_empty() {
        "unknown"
    } else {
        input.wiki_source_status.trim()
    };
    let wiki_license_status = if input.wiki_license_status.trim().is_empty() {
        "unknown"
    } else {
        input.wiki_license_status.trim()
    };
    add_status_check(
        &mut checks,
        "source.license_metadata",
        "Source license metadata",
        "dependency",
        license_status,
        false,
        "License evidence is shown from the manifest and is never inferred.",
        "manifest.repository.license_evidence",
    );
    add_status_check(
        &mut checks,
        "wiki.provenance",
        "Wiki source provenance",
        "wiki",
        wiki_source_status,
        false,
        "Wiki provenance is shown from the manifest and is never invented.",
        "manifest.wiki.provenance",
    );
    add_status_check(
        &mut checks,
        "wiki.license_metadata",
        "Wiki license metadata",
        "wiki",
        wiki_license_status,
        false,
        "Wiki license evidence is shown from the exact locked manifest and is never inferred.",
        "manifest.wiki.provenance.license_status",
    );
    add_bool_check(
        &mut checks,
        "descriptor.project",
        "Project and launcher descriptors",
        "descriptor",
        input.descriptors_valid,
        true,
        "Both descriptor routes parse and point to the reviewed project.",
        "descriptor.mod",
    );
    add_bool_check(
        &mut checks,
        "launcher.registration",
        "Launcher registration",
        "launcher",
        !launcher_required || input.launcher_valid,
        true,
        "The external launcher descriptor resolves to the reviewed project root.",
        "HOI4 user mod directory",
    );
    add_bool_check(
        &mut checks,
        "thumbnail.integrity",
        "Thumbnail integrity",
        "thumbnail",
        !thumbnail_required || input.thumbnail_valid,
        true,
        "thumbnail.png is a valid decoded PNG and is hash-tracked.",
        "thumbnail.png",
    );
    add_bool_check(
        &mut checks,
        "structure.core",
        "Project structure",
        "structure",
        input.structure_valid,
        true,
        "The selected structure profile is present.",
        "project root",
    );
    add_bool_check(
        &mut checks,
        "codex.agents",
        "Project instructions",
        "codex",
        input.agents_valid,
        true,
        "AGENTS.md is present and contains no unresolved template tokens.",
        "AGENTS.md",
    );
    add_bool_check(
        &mut checks,
        "skills.core",
        "Workflow skills",
        "skill",
        input.skills_valid,
        true,
        "Selected skills are present and valid.",
        ".agents/skills/",
    );
    add_bool_check(
        &mut checks,
        "subagents.core",
        "Bounded subagents",
        "subagent",
        input.subagents_valid,
        true,
        "Selected subagent definitions parse and require fork_context=false.",
        ".codex/agents/",
    );
    let integration_label = if ai_provider == "codex" {
        "Codex configuration".to_string()
    } else {
        format!("{ai_provider} project integration")
    };
    let integration_message = if ai_provider == "codex" {
        "Project Codex configuration parses and preserves unrelated entries.".to_string()
    } else {
        format!("The selected {ai_provider} project profile is recorded without a Codex opener.")
    };
    add_bool_check(
        &mut checks,
        "codex.config",
        &integration_label,
        "ai",
        ai_provider != "codex" || input.codex_valid,
        true,
        &integration_message,
        ".codex/config.toml",
    );
    let authenticated = if ai_provider == "codex" {
        input.codex_authenticated
    } else {
        input.ai_authenticated
    };
    let analysis_status = if ai_provider == "codex" {
        input.codex_analysis_status.as_str()
    } else {
        input.ai_analysis_status.as_str()
    };
    let confirmed_field_count = if ai_provider == "codex" {
        input.codex_confirmed_field_count
    } else {
        input.ai_confirmed_field_count
    };
    let analysis_passed = analysis_status == "confirmed" && confirmed_field_count > 0;
    let auth_check_id = if ai_provider == "codex" {
        "codex.authenticated"
    } else {
        "ai.authenticated"
    };
    let auth_label = if ai_provider == "codex" {
        "ChatGPT Codex authentication".to_string()
    } else {
        format!("{ai_provider} connection")
    };
    let auth_message = if ai_provider == "codex" {
        "The official Codex App Server reported an authenticated ChatGPT account during setup."
            .to_string()
    } else {
        format!("The {ai_provider} provider connection is available for semantic planning.")
    };
    let auth_path = if ai_provider == "codex" {
        "codex app-server account/read"
    } else {
        "OS credential vault or local endpoint"
    };
    add_bool_check(
        &mut checks,
        auth_check_id,
        &auth_label,
        "ai",
        authenticated,
        true,
        &auth_message,
        auth_path,
    );
    let analysis_check_id = if ai_provider == "codex" {
        "analysis.chatgpt"
    } else {
        "analysis.ai"
    };
    let analysis_label = if ai_provider == "codex" {
        "ChatGPT Codex analysis".to_string()
    } else {
        format!("{ai_provider} semantic analysis")
    };
    let analysis_message = if ai_provider == "codex" {
        "Required schema-constrained Codex proposals were confirmed before planning.".to_string()
    } else {
        format!(
            "Required schema-constrained {ai_provider} proposals were confirmed before planning."
        )
    };
    add_bool_check(
        &mut checks,
        analysis_check_id,
        &analysis_label,
        "analysis",
        analysis_passed,
        true,
        &analysis_message,
        ".hoi4-mod-setup/install.lock.json",
    );
    add_status_check(
        &mut checks,
        "mcp.hoi4",
        "HOI4 MCP route",
        "mcp",
        &input.mcp_status,
        input.mcp_blocking,
        "MCP state comes from the manifest-declared route; unsupported commands are not run.",
        ".codex/config.toml",
    );
    add_status_check(
        &mut checks,
        "codex.chat_flatten",
        "ChatGPT Chat sources",
        "ai",
        &input.flatten_status,
        false,
        "The optional flattened source folder is checked against its installed hashes.",
        "chatgpt_project_sources/",
    );
    add_wiki_check(&mut checks, input);
    add_status_check(
        &mut checks,
        "git.project",
        "Git setup",
        "git",
        &input.git_status,
        false,
        "Git is reported as selected, preserved, or intentionally skipped.",
        ".git",
    );
    add_status_check(
        &mut checks,
        "environment.required",
        "Required environment",
        "environment",
        &input.environment_status,
        true,
        "Secrets are available through the OS credential vault without entering the project.",
        "OS credential vault",
    );
    add_bool_check(
        &mut checks,
        "hashes.managed",
        "Managed file hashes",
        "hash",
        input.hashes_valid,
        true,
        "Installed files match the recorded source and result hashes.",
        ".hoi4-mod-setup/install.lock.json",
    );
    add_status_check(
        &mut checks,
        "conflicts.resolved",
        "Conflicts",
        "conflict",
        &input.conflict_status,
        true,
        "Every modified destination has a reviewed valid action.",
        "installation plan",
    );
    add_status_check(
        &mut checks,
        "dependencies.core",
        "Dependencies",
        "dependency",
        &input.dependency_status,
        true,
        "Selected component dependencies are complete and supported.",
        "manifest",
    );
    add_workflow_3d_check(&mut checks, &input.workflow_3d_state);
    add_super_events_check(&mut checks, &input.workflow_super_events_state);

    let mut summary = ReadinessSummary::default();
    let mut blocking_ids = Vec::new();
    for check in &checks {
        match check.status.as_str() {
            "pass" => summary.pass += 1,
            "warn" | "unsupported_platform" => summary.warn += 1,
            "block" => {
                summary.block += 1;
                if check.blocking {
                    blocking_ids.push(check.id.clone());
                }
            }
            "not_selected" => summary.not_selected += 1,
            "planned_unavailable" => summary.planned_unavailable += 1,
            _ => summary.warn += 1,
        }
    }
    let enabled = blocking_ids.is_empty();
    let mut notes = input.notes.clone();
    notes.push("Optional workflow status does not block core AI readiness.".into());
    let persisted_analysis_status = if analysis_passed {
        "confirmed"
    } else {
        "block"
    };
    let blocking_check_ids = blocking_ids.clone();
    ReadinessReport {
        schema_version: "1.0.0".into(),
        report_id: uuid::Uuid::new_v4(),
        project_id: input.project_id.clone(),
        generated_at: Utc::now().to_rfc3339(),
        codex: ReadinessCodexSummary {
            provider: ai_provider.into(),
            model: ai_model.into(),
            integration: if ai_provider == "codex" {
                "codex_app_server".into()
            } else {
                "provider_api".into()
            },
            auth_mode: if ai_provider == "codex" {
                "chatgpt".into()
            } else if ai_provider == "local" {
                "local_endpoint".into()
            } else {
                "api_key".into()
            },
            authenticated_during_setup: authenticated,
            analysis_status: persisted_analysis_status.into(),
            confirmed_field_count,
            no_account_metadata_persisted: true,
            blocking_check_ids: blocking_check_ids.clone(),
        },
        checks,
        summary,
        core_ready: enabled,
        open_in_codex: OpenInCodex {
            enabled: ai_provider == "codex" && enabled,
            blocking_check_ids,
            command_preview: (ai_provider == "codex" && enabled)
                .then(|| format!("codex --cd \"{}\"", input.project_root)),
        },
        notes,
    }
}

#[allow(clippy::too_many_arguments)]
fn add_bool_check(
    checks: &mut Vec<ReadinessCheck>,
    id: &str,
    label: &str,
    category: &str,
    passed: bool,
    blocking: bool,
    message: &str,
    path: &str,
) {
    checks.push(ReadinessCheck {
        id: id.into(),
        category: category.into(),
        label: label.into(),
        status: if passed { "pass" } else { "block" }.into(),
        blocking,
        message: Some(if passed {
            message.to_string()
        } else {
            format!("{message} Check is not satisfied.")
        }),
        evidence: vec![ReadinessEvidence {
            kind: "state".into(),
            value: json!(passed),
            path: Some(path.into()),
        }],
    });
}

#[allow(clippy::too_many_arguments)]
fn add_status_check(
    checks: &mut Vec<ReadinessCheck>,
    id: &str,
    label: &str,
    category: &str,
    status: &str,
    blocking: bool,
    message: &str,
    path: &str,
) {
    let normalized = match status {
        "pass" | "ready" | "healthy" | "installed" | "supported" => "pass",
        "not_selected" => "not_selected",
        "planned_unavailable" => "planned_unavailable",
        "unsupported_platform" => "unsupported_platform",
        "verified" => "pass",
        "repository_only" | "not_found" | "unknown" => "warn",
        "warn" | "incomplete" | "degraded" => "warn",
        _ => "block",
    };
    checks.push(ReadinessCheck {
        id: id.into(),
        category: category.into(),
        label: label.into(),
        status: normalized.into(),
        blocking: blocking && normalized == "block",
        message: Some(message.into()),
        evidence: vec![ReadinessEvidence {
            kind: "state".into(),
            value: json!(status),
            path: Some(path.into()),
        }],
    });
}

const MAX_WIKI_FILES: usize = 100_000;
const MAX_WIKI_BYTES: u64 = 512 * 1024 * 1024;

/// Check local Markdown and wiki-link targets without following external URLs
/// or links outside the installed offline wiki. The result is bounded so a
/// malformed snapshot cannot turn readiness into an unbounded scan.
pub(crate) fn wiki_link_integrity(project_root: &Path) -> Vec<String> {
    let wiki_root = project_root.join("paradox_wiki");
    if !wiki_root.is_dir() || path_has_link_component(&wiki_root) {
        return vec!["paradox_wiki/".into()];
    }
    let mut files = Vec::new();
    let mut broken = Vec::new();
    let mut bytes_read = 0_u64;
    collect_wiki_files(&wiki_root, &mut files, &mut broken, &mut bytes_read, 0);
    let mut targets = Vec::new();
    for file in files {
        if file
            .extension()
            .and_then(|extension| extension.to_str())
            .is_none_or(|extension| !extension.eq_ignore_ascii_case("md"))
        {
            continue;
        }
        let bytes = match fs::read(&file) {
            Ok(bytes) => bytes,
            Err(_) => {
                broken.push(relative_wiki_path(&wiki_root, &file));
                continue;
            }
        };
        let text = String::from_utf8_lossy(&bytes);
        for target in markdown_targets(&text) {
            if let Some(target) = normalize_wiki_target(&target) {
                targets.push((relative_wiki_path(&wiki_root, &file), target));
            }
        }
    }
    for (source, target) in targets {
        let candidate = match safe_join(&wiki_root, &target) {
            Ok(path) => path,
            Err(_) => {
                broken.push(format!("{source} -> {target}"));
                continue;
            }
        };
        let exists = candidate.is_file()
            || (Path::new(&target).extension().is_none()
                && safe_join(&wiki_root, &format!("{target}.md"))
                    .map(|path| path.is_file())
                    .unwrap_or(false));
        if !exists {
            broken.push(format!("{source} -> {target}"));
        }
    }
    broken.sort();
    broken.dedup();
    broken.truncate(100);
    broken
}

fn collect_wiki_files(
    directory: &Path,
    files: &mut Vec<PathBuf>,
    broken: &mut Vec<String>,
    bytes_read: &mut u64,
    depth: usize,
) {
    if depth > 32 || files.len() >= MAX_WIKI_FILES || *bytes_read >= MAX_WIKI_BYTES {
        broken.push("wiki scan limit reached".into());
        return;
    }
    let entries = match fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(_) => {
            broken.push(relative_wiki_path(directory, directory));
            return;
        }
    };
    for entry in entries.flatten() {
        if files.len() >= MAX_WIKI_FILES || *bytes_read >= MAX_WIKI_BYTES {
            broken.push("wiki scan limit reached".into());
            return;
        }
        let path = entry.path();
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(_) => continue,
        };
        if is_link_metadata(&metadata) {
            broken.push(path.display().to_string());
        } else if metadata.is_dir() {
            collect_wiki_files(&path, files, broken, bytes_read, depth + 1);
        } else if metadata.is_file()
            && path
                .extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| extension.eq_ignore_ascii_case("md"))
        {
            *bytes_read = bytes_read.saturating_add(metadata.len());
            files.push(path);
        }
    }
}

fn relative_wiki_path(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

fn markdown_targets(text: &str) -> Vec<String> {
    let mut targets = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find("](") {
        let after = &rest[start + 2..];
        let Some(end) = after.find(')') else { break };
        let raw_target = after[..end].trim();
        let target = if let Some(inner) = raw_target.strip_prefix('<') {
            inner.find('>').map(|close| inner[..close].to_string())
        } else {
            raw_target.split_whitespace().next().map(str::to_string)
        };
        if let Some(target) = target {
            targets.push(target);
        }
        rest = &after[end + 1..];
    }
    targets
}

fn normalize_wiki_target(raw: &str) -> Option<String> {
    let target = raw.trim();
    if target.is_empty()
        || target.starts_with('#')
        || target.starts_with("http://")
        || target.starts_with("https://")
        || target.starts_with("mailto:")
        || target.starts_with("tel:")
        || target.starts_with("data:")
    {
        return None;
    }
    let target = target.split(['#', '?']).next().unwrap_or_default();
    if target.is_empty() {
        return None;
    }
    let mut normalized = String::with_capacity(target.len());
    let mut characters = target.chars().peekable();
    while let Some(character) = characters.next() {
        if character == '\\' {
            if let Some(next) = characters.peek().copied() {
                if matches!(
                    next,
                    '\\' | '`'
                        | '*'
                        | '_'
                        | '{'
                        | '}'
                        | '['
                        | ']'
                        | '('
                        | ')'
                        | '#'
                        | '+'
                        | '-'
                        | '.'
                        | '!'
                ) {
                    normalized.push(next);
                    characters.next();
                    continue;
                }
            }
            normalized.push('/');
        } else {
            normalized.push(character);
        }
    }
    Some(normalized.trim_start_matches('/').to_string())
}

fn add_wiki_check(checks: &mut Vec<ReadinessCheck>, input: &ReadinessInput) {
    if input.wiki_status == "not_selected" {
        add_status_check(
            checks,
            "wiki.snapshot",
            "Offline wiki",
            "wiki",
            "not_selected",
            true,
            "The offline wiki is not selected.",
            "paradox_wiki/",
        );
        return;
    }
    let root = Path::new(&input.project_root);
    let mut missing = Vec::new();
    for page in &input.wiki_required_pages {
        if safe_join(root, &format!("paradox_wiki/{page}"))
            .map(|path| !path.is_file())
            .unwrap_or(true)
        {
            missing.push(page.clone());
        }
    }
    let status = if !missing.is_empty()
        || !input.wiki_broken_links.is_empty()
        || input.wiki_status == "block"
    {
        "block"
    } else {
        input.wiki_status.as_str()
    };
    let message = if !missing.is_empty() {
        format!("{} required wiki pages are missing.", missing.len())
    } else if !input.wiki_broken_links.is_empty() {
        format!(
            "{} offline wiki links are broken or unsafe.",
            input.wiki_broken_links.len()
        )
    } else {
        "Offline wiki integrity and declared coverage were checked.".into()
    };
    checks.push(ReadinessCheck {
        id: "wiki.coverage".into(),
        category: "wiki".into(),
        label: "Offline wiki coverage".into(),
        status: status.into(),
        blocking: true,
        message: Some(message),
        evidence: vec![ReadinessEvidence {
            kind: "required_pages".into(),
            value: json!({"required": input.wiki_required_pages.len(), "missing": missing, "broken_links": input.wiki_broken_links}),
            path: Some("paradox_wiki/".into()),
        }],
    });
}

fn add_workflow_3d_check(checks: &mut Vec<ReadinessCheck>, status: &str) {
    let normalized = match status {
        "ready" | "healthy" | "installed" => "pass",
        "not_selected" => "not_selected",
        "unsupported_platform" => "unsupported_platform",
        "planned_unavailable" => "planned_unavailable",
        _ => "warn",
    };
    checks.push(ReadinessCheck {
        id: "workflow.3d".into(),
        category: "workflow".into(),
        label: "3D model workflow".into(),
        status: normalized.into(),
        blocking: false,
        message: Some(match normalized {
            "pass" => "Credential, dependencies, and selected 3D health checks passed.".into(),
            "unsupported_platform" => "Not available on this computer.".into(),
            "not_selected" => "The 3D workflow was not selected.".into(),
            _ => "The optional 3D workflow is incomplete; core setup remains usable.".into(),
        }),
        evidence: vec![ReadinessEvidence {
            kind: "workflow_state".into(),
            value: json!(status),
            path: None,
        }],
    });
}

fn add_super_events_check(checks: &mut Vec<ReadinessCheck>, status: &str) {
    let normalized = match status {
        "ready" | "healthy" | "installed" => "pass",
        "not_selected" | "" => "not_selected",
        "unsupported_platform" => "unsupported_platform",
        _ => "warn",
    };
    checks.push(ReadinessCheck {
        id: "workflow.super_events".into(),
        category: "workflow".into(),
        label: "Super Events workflow".into(),
        status: normalized.into(),
        blocking: false,
        message: Some(match normalized {
            "pass" => {
                "The selected Super Events skill is installed and covered by managed hash checks."
                    .into()
            }
            "not_selected" => "The Super Events workflow was not selected.".into(),
            _ => {
                "The optional Super Events workflow needs review; core setup remains usable.".into()
            }
        }),
        evidence: vec![ReadinessEvidence {
            kind: "workflow_state".into(),
            value: json!(status),
            path: Some(".agents/skills/hoi4-super-events/".into()),
        }],
    });
}

pub fn core_ready(report: &ReadinessReport) -> bool {
    report.core_ready
}

pub fn project_input(project_root: &Path, project_id: &str) -> Result<ReadinessInput, AppError> {
    let lock_path = project_root.join(".hoi4-mod-setup/install.lock.json");
    let lock = fs::read(&lock_path).ok().and_then(|bytes| {
        let value = serde_json::from_slice::<serde_json::Value>(&bytes).ok()?;
        crate::migrations::migrate_lock(value).ok()
    });
    let hashes_valid = lock.as_ref().is_some_and(|lock| {
        crate::source::validate_commit(&lock.source.revision).is_ok()
            && crate::source::validate_sha256(&lock.source.manifest_sha256).is_ok()
            && matches!(
                lock.source.manifest_origin.as_str(),
                "remote" | "bundled_revision_bootstrap"
            )
            && lock.files.iter().all(|file| {
                let Some(path) = locked_file_destination(project_root, file).ok() else {
                    return false;
                };
                let hash_valid = sha256_file(&path)
                    .ok()
                    .is_some_and(|hash| hash == file.installed_sha256);
                let size_valid = file
                    .installed_size
                    .map(|expected| {
                        fs::metadata(&path)
                            .map(|meta| meta.len() == expected)
                            .unwrap_or(false)
                    })
                    .unwrap_or(true);
                hash_valid
                    && size_valid
                    && locked_executable_state_matches(&path, file.executable)
                    && file.source_revision == lock.source.revision
                    && crate::source::validate_sha256(&file.source_sha256).is_ok()
                    && crate::source::validate_sha256(&file.installed_sha256).is_ok()
            })
    });
    let project_descriptor_valid = fs::read(project_root.join("descriptor.mod"))
        .ok()
        .and_then(|bytes| parse_descriptor(&bytes).ok())
        .is_some_and(|descriptor| {
            descriptor.fields.contains_key("name")
                && descriptor.fields.contains_key("supported_version")
                && descriptor
                    .fields
                    .get("picture")
                    .is_some_and(|value| value == "thumbnail.png")
        });
    let launcher_descriptors_valid = lock.as_ref().is_some_and(|lock| {
        let launcher_files = lock
            .files
            .iter()
            .filter(|file| file.external && file.component_id.starts_with("project.launcher"))
            .collect::<Vec<_>>();
        !launcher_files.is_empty()
            && launcher_files.iter().all(|file| {
                locked_file_destination(project_root, file)
                    .ok()
                    .and_then(|path| fs::read(path).ok())
                    .is_some_and(|bytes| launcher_descriptor_matches_project(project_root, &bytes))
            })
    });
    let thumbnail_valid = lock.as_ref().is_some_and(|lock| {
        let Some(file) = lock
            .files
            .iter()
            .find(|file| !file.external && file.path == "thumbnail.png")
        else {
            return false;
        };
        fs::read(project_root.join("thumbnail.png"))
            .ok()
            .is_some_and(|bytes| {
                validate_thumbnail_png(&bytes).is_ok()
                    && sha256_file(&project_root.join("thumbnail.png"))
                        .ok()
                        .is_some_and(|hash| hash == file.installed_sha256)
            })
    });
    let descriptor_valid = project_descriptor_valid;
    let agents_valid = valid_agents_file(project_root);
    let codex_text = fs::read_to_string(project_root.join(".codex/config.toml")).ok();
    let codex_valid = codex_text
        .as_deref()
        .is_some_and(|text| text.parse::<toml::Value>().is_ok());
    let on_windows = Platform::current() == Platform::Windows;
    let mcp_unsupported = lock.as_ref().is_some_and(|lock| {
        lock.components.iter().any(|component| {
            component.id == "mcp.hoi4_agent_tools" && component.state == "unsupported_platform"
        })
    });
    let mcp_selected = lock.as_ref().is_some_and(|lock| {
        lock.components.iter().any(|component| {
            component.id == "mcp.hoi4_agent_tools" && mcp_component_is_selected(&component.state)
        })
    });
    let mcp_planned_unavailable = lock.as_ref().is_some_and(|lock| {
        lock.components.iter().any(|component| {
            component.id == "mcp.hoi4_agent_tools"
                && component.validation.as_deref() == Some("planned_unavailable")
        })
    });
    let mcp_declared = codex_text.as_deref().is_some_and(|text| {
        text.contains("[mcp_servers.hoi4_agent_tools]")
            && text.contains("command = \"hoi4-agent-tools.cmd\"")
    });
    let mcp_status = if mcp_unsupported {
        "unsupported_platform".into()
    } else if !mcp_selected {
        "not_selected".into()
    } else if !mcp_declared {
        "block".into()
    } else if on_windows && mcp_planned_unavailable {
        "planned_unavailable".into()
    } else if on_windows {
        "health_not_run".into()
    } else {
        "unsupported_platform".into()
    };
    let wiki_selected = lock.as_ref().is_some_and(|lock| {
        lock.components.iter().any(|component| {
            component.id == "wiki.snapshot" && component.state != "unsupported_platform"
        })
    });
    let wiki_pages = lock
        .as_ref()
        .map(|lock| lock.wiki_required_pages.clone())
        .unwrap_or_default();
    let wiki_broken_links = if project_root.join("paradox_wiki").is_dir() {
        wiki_link_integrity(project_root)
    } else {
        vec!["paradox_wiki/".into()]
    };
    let wiki_metadata_valid = !wiki_selected
        || lock.as_ref().is_some_and(|lock| {
            !lock.wiki_required_pages.is_empty() && lock.wiki_metadata.is_some()
        });
    let source_license_status = lock
        .as_ref()
        .and_then(|lock| lock.wiki_metadata.as_ref())
        .map(|metadata| metadata.repository_license_status.clone())
        .unwrap_or_else(|| "unknown".into());
    let wiki_source_status = lock
        .as_ref()
        .and_then(|lock| lock.wiki_metadata.as_ref())
        .map(|metadata| metadata.source_status.clone())
        .unwrap_or_else(|| "unknown".into());
    let wiki_license_status = lock
        .as_ref()
        .and_then(|lock| lock.wiki_metadata.as_ref())
        .map(|metadata| metadata.license_status.clone())
        .unwrap_or_else(|| "unknown".into());
    let wiki_status = if !wiki_selected {
        "not_selected".into()
    } else if project_root.join("paradox_wiki").is_dir()
        && wiki_broken_links.is_empty()
        && wiki_metadata_valid
        && wiki_pages.iter().all(|page| {
            safe_join(project_root, &format!("paradox_wiki/{page}"))
                .map(|path| path.is_file())
                .unwrap_or(false)
        })
    {
        "pass".into()
    } else {
        "block".into()
    };
    let git_status = if read_git_head(project_root).repository_present {
        "pass".into()
    } else {
        "not_selected".into()
    };
    let workflow_3d_state = lock
        .as_ref()
        .and_then(|lock| lock.optional_workflows.get("workflow.3d"))
        .map(|workflow| workflow.state.clone())
        .unwrap_or_else(|| "not_selected".into());
    let workflow_super_events_state = lock
        .as_ref()
        .and_then(|lock| lock.optional_workflows.get("workflow.super_events"))
        .map(|workflow| workflow.state.clone())
        .unwrap_or_else(|| "not_selected".into());
    let codex_analysis = lock.as_ref().and_then(|lock| lock.codex_analysis.as_ref());
    let ai_provider = lock
        .as_ref()
        .map(|lock| lock.ai_provider.clone())
        .filter(|provider| !provider.trim().is_empty())
        .unwrap_or_else(|| "codex".into());
    let ai_model = lock
        .as_ref()
        .map(|lock| lock.ai_model.clone())
        .filter(|model| !model.trim().is_empty())
        .unwrap_or_else(|| "default".into());
    let ai_authenticated = codex_analysis
        .is_some_and(|record| crate::codex::validate_confirmed_record(record).is_ok());
    let ai_analysis_status = if ai_authenticated {
        "confirmed"
    } else {
        "blocked"
    };
    let codex_confirmed_field_count = codex_analysis
        .map(|record| record.confirmed_fields.len() as u32)
        .unwrap_or(0);
    let dependency_status = if lock.as_ref().is_some_and(|lock| {
        lock.components
            .iter()
            .all(component_dependency_is_satisfied)
    }) {
        "pass"
    } else {
        "block"
    };
    let mcp_blocking = on_windows && mcp_selected && mcp_status != "planned_unavailable";
    Ok(ReadinessInput {
        project_id: project_id.into(),
        project_root: project_root.display().to_string(),
        selected_components: lock
            .as_ref()
            .map(|lock| lock.components.iter().map(|component| component.id.clone()).collect())
            .unwrap_or_default(),
        source_verified: lock.is_some() && hashes_valid,
        descriptors_valid: descriptor_valid,
        launcher_valid: launcher_descriptors_valid,
        thumbnail_valid,
        structure_valid: project_root.is_dir(),
        agents_valid,
        skills_valid: valid_skill_tree(project_root),
        subagents_valid: valid_subagent_tree(project_root),
        codex_valid,
        codex_authenticated: ai_provider == "codex" && ai_authenticated,
        codex_analysis_status: if ai_provider == "codex" {
            ai_analysis_status.into()
        } else {
            "blocked".into()
        },
        codex_confirmed_field_count,
        ai_provider,
        ai_model,
        ai_authenticated,
        ai_analysis_status: ai_analysis_status.into(),
        ai_confirmed_field_count: codex_confirmed_field_count,
        flatten_status: lock
            .as_ref()
            .map(|lock| flattened_lock_status(project_root, lock))
            .unwrap_or_else(|| "not_selected".into()),
        mcp_status,
        mcp_blocking,
        wiki_status,
        wiki_required_pages: wiki_pages,
        git_status,
        // Meshy is an optional workflow credential. Its incomplete state is
        // reported by the dedicated 3D check and must not block core setup.
        environment_status: "pass".into(),
        wiki_broken_links,
        hashes_valid,
        conflict_status: if lock.as_ref().is_some_and(|lock| lock.local_modifications.is_empty()) {
            "pass".into()
        } else {
            "block".into()
        },
        dependency_status: dependency_status.into(),
        workflow_3d_state,
        workflow_super_events_state,
        source_license_status,
        wiki_source_status,
        wiki_license_status,
        notes: vec![
            "Readiness uses the required wiki page list recorded with the installed manifest revision.".into(),
            "MCP structural preflight is not a health result; the Tauri readiness command performs a bounded initialize and read-only tools/list probe before reporting pass.".into(),
        ],
    })
}

fn project_descriptor_name(project_root: &Path) -> Option<String> {
    fs::read(project_root.join("descriptor.mod"))
        .ok()
        .and_then(|bytes| parse_descriptor(&bytes).ok())
        .and_then(|descriptor| descriptor.fields.get("name").cloned())
}

pub(crate) fn launcher_descriptor_matches_project(project_root: &Path, bytes: &[u8]) -> bool {
    let Some(project_name) = project_descriptor_name(project_root) else {
        return false;
    };
    parse_descriptor(bytes).ok().is_some_and(|descriptor| {
        descriptor.fields.get("name") == Some(&project_name)
            && descriptor
                .fields
                .get("path")
                .is_some_and(|path| path_matches_project(path, project_root))
    })
}

fn path_matches_project(value: &str, project_root: &Path) -> bool {
    let expected = crate::paths::validate_project_root(project_root)
        .ok()
        .map(|path| path.to_string_lossy().replace('\\', "/"));
    let value = value.replace('\\', "/");
    expected.is_some_and(|expected| {
        if cfg!(target_os = "windows") {
            value.eq_ignore_ascii_case(&expected)
        } else {
            value == expected
        }
    })
}

pub fn check_project_files(
    project_root: &Path,
    project_id: &str,
) -> Result<ReadinessReport, AppError> {
    Ok(evaluate(&project_input(project_root, project_id)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn optional_incomplete_workflow_does_not_block_codex() {
        let input = ReadinessInput {
            source_verified: true,
            descriptors_valid: true,
            launcher_valid: true,
            thumbnail_valid: true,
            structure_valid: true,
            agents_valid: true,
            skills_valid: true,
            subagents_valid: true,
            codex_valid: true,
            codex_authenticated: true,
            codex_analysis_status: "confirmed".into(),
            codex_confirmed_field_count: 1,
            mcp_status: "pass".into(),
            wiki_status: "pass".into(),
            git_status: "not_selected".into(),
            environment_status: "pass".into(),
            hashes_valid: true,
            conflict_status: "pass".into(),
            dependency_status: "pass".into(),
            workflow_3d_state: "incomplete".into(),
            workflow_super_events_state: "ready".into(),
            wiki_required_pages: vec![],
            ..Default::default()
        };
        let report = evaluate(&input);
        assert!(report.open_in_codex.enabled);
        assert!(report
            .checks
            .iter()
            .any(|check| check.id == "workflow.3d" && check.status == "warn"));
        assert!(report
            .checks
            .iter()
            .any(|check| check.id == "workflow.super_events" && check.status == "pass"));
        assert!(!report
            .checks
            .iter()
            .any(|check| check.id.contains("lora") || check.label.contains("ComfyUI")));
    }

    #[test]
    fn deselected_or_unsupported_mcp_is_non_blocking() {
        assert!(!mcp_component_is_selected("not_selected"));
        assert!(!mcp_component_is_selected("unsupported_platform"));
        assert!(mcp_component_is_selected("installed"));
        assert!(component_dependency_is_satisfied(&LockComponent {
            id: "mcp.hoi4_agent_tools".into(),
            version: None,
            state: "installed".into(),
            source_revision: None,
            validation: Some("planned_unavailable".into()),
        }));
        assert!(!component_dependency_is_satisfied(&LockComponent {
            id: "core.skills".into(),
            version: None,
            state: "installed".into(),
            source_revision: None,
            validation: Some("planned_unavailable".into()),
        }));
    }

    #[test]
    fn readiness_validates_agent_files_instead_of_only_directories() {
        let project = tempfile::tempdir().unwrap();
        fs::write(project.path().join("AGENTS.md"), "# instructions").unwrap();
        let skill = project.path().join(".agents/skills/example/SKILL.md");
        fs::create_dir_all(skill.parent().unwrap()).unwrap();
        fs::write(
            &skill,
            "---\nname: example\ndescription: Example skill.\n---\n# Skill\n",
        )
        .unwrap();
        let subagent = project.path().join(".codex/agents/example.toml");
        fs::create_dir_all(subagent.parent().unwrap()).unwrap();
        fs::write(
            &subagent,
            "name = \"example\"\ndescription = \"Example\"\ndeveloper_instructions = \"fork_context=false\"\n",
        )
        .unwrap();

        assert!(valid_agents_file(project.path()));
        assert!(valid_skill_tree(project.path()));
        assert!(valid_subagent_tree(project.path()));

        fs::write(&skill, "# malformed").unwrap();
        assert!(!valid_skill_tree(project.path()));

        fs::write(
            &subagent,
            "name = \"example\"\ndescription = \"Example\"\nfork_context = true\ndeveloper_instructions = \"fork_context=false\"\n",
        )
        .unwrap();
        assert!(!valid_subagent_tree(project.path()));
    }

    #[test]
    fn failed_core_check_disables_open_in_codex() {
        let report = evaluate(&ReadinessInput::default());
        assert!(!report.open_in_codex.enabled);
        assert!(!report.open_in_codex.blocking_check_ids.is_empty());
    }

    #[test]
    fn broken_offline_wiki_link_blocks_core_readiness() {
        let report = evaluate(&ReadinessInput {
            source_verified: true,
            descriptors_valid: true,
            launcher_valid: true,
            thumbnail_valid: true,
            structure_valid: true,
            agents_valid: true,
            skills_valid: true,
            subagents_valid: true,
            codex_valid: true,
            codex_authenticated: true,
            codex_analysis_status: "confirmed".into(),
            codex_confirmed_field_count: 1,
            mcp_status: "pass".into(),
            wiki_status: "pass".into(),
            wiki_broken_links: vec!["index.md -> missing.md".into()],
            git_status: "not_selected".into(),
            environment_status: "pass".into(),
            hashes_valid: true,
            conflict_status: "pass".into(),
            dependency_status: "pass".into(),
            ..Default::default()
        });
        assert_eq!(
            report
                .checks
                .iter()
                .find(|check| check.id == "wiki.coverage")
                .map(|check| check.status.as_str()),
            Some("block")
        );
        assert!(!report.open_in_codex.enabled);
    }

    #[test]
    fn wiki_link_integrity_accepts_local_pages_and_ignores_external_links() {
        let project = tempfile::tempdir().unwrap();
        let wiki = project.path().join("paradox_wiki");
        fs::create_dir_all(&wiki).unwrap();
        fs::write(
            wiki.join("index.md"),
            "[local](<Page Two.md>) [missing](missing.md) [external](https://example.test)",
        )
        .unwrap();
        fs::write(wiki.join("Page Two.md"), "# page").unwrap();
        let broken = wiki_link_integrity(project.path());
        assert_eq!(broken, vec!["index.md -> missing.md"]);
    }

    #[test]
    fn wiki_link_integrity_handles_markdown_escapes_citations_and_binary_media() {
        let project = tempfile::tempdir().unwrap();
        let wiki = project.path().join("paradox_wiki");
        fs::create_dir_all(wiki.join("media")).unwrap();
        fs::write(
            wiki.join("index.md"),
            "[objects](loc\\_objects.md) [[1]](#cite-note-1) [[a]](#cnote-a)",
        )
        .unwrap();
        fs::write(wiki.join("loc_objects.md"), "# objects").unwrap();
        fs::write(
            wiki.join("media").join("image.png"),
            b"\x89PNG\r\n\x1a\n](missing.md)",
        )
        .unwrap();

        assert!(wiki_link_integrity(project.path()).is_empty());
    }
}
