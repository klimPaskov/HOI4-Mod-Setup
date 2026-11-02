//! The reviewed hoi4-imagegen actions of an installation plan and the runner
//! that executes them after the project files are applied.
//!
//! Planning turns the verified source manifest into one install action (the
//! declared URL, size and SHA-256 of this platform's archive) and, when a
//! version is being replaced or removed, one cleanup action for the recorded
//! tree. The runner re-validates both from the plan alone: the renderer can
//! never supply or change them.

use crate::imagegen::{CliRunner, InstalledImagegen};
use crate::models::{
    ExternalAction, ImageGenerationSelection, InstallationPlan, Platform, RemoteManifest,
};
use crate::release_install::{install_release, remove_release, ReleaseDownloader, RemoveResult};
use crate::transaction::PostInstallActionOutcome;
use crate::AppError;
use hoi4_setup_core::release_binary::{
    self as route, ReleaseBinaryRoute, ReleaseTarget, COMPONENT_ID, PROVIDER_ID,
};
use serde_json::Value;
use std::path::Path;

pub const INSTALL_ACTION_ID: &str = "external.imagegen.hoi4_imagegen.install";
pub const CLEANUP_ACTION_ID: &str = "external.imagegen.hoi4_imagegen.cleanup";
const INSTALL_SOURCE: &str = "app_managed_download";
const CLEANUP_SOURCE: &str = "app_managed_cleanup";

fn base_action(id: &str, source: &str) -> ExternalAction {
    ExternalAction {
        id: id.into(),
        component_id: COMPONENT_ID.into(),
        platform: Platform::current(),
        command_source: source.into(),
        executable: Some("HOI4 Mod Setup".into()),
        arguments: Vec::new(),
        working_directory: Some("<project_root>".into()),
        environment_names: Vec::new(),
        network_access: "none".into(),
        expected_writes: Vec::new(),
        privilege: "current_user".into(),
        rollback_boundary: String::new(),
        display_command: None,
        risk: "medium".into(),
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
        required_tool_names: Vec::new(),
    }
}

/// What planning does for the hoi4-imagegen provider.
#[derive(Debug, Clone)]
pub enum InstallPlanning {
    /// Another provider, or none, is selected.
    NotSelected,
    /// No release is built for this computer.
    Unsupported(String),
    Install(Box<ExternalAction>),
}

/// The reviewed install action for the selected release-binary provider. The
/// URL, size and SHA-256 come from the verified source manifest.
pub fn plan_install(
    manifest: &RemoteManifest,
    selection: &ImageGenerationSelection,
) -> Result<InstallPlanning, AppError> {
    let selected = std::iter::once(&selection.primary_provider)
        .chain(selection.additional_providers.iter())
        .any(|id| id == PROVIDER_ID);
    if !selected {
        return Ok(InstallPlanning::NotSelected);
    }
    let provider = manifest
        .image_generation
        .as_ref()
        .and_then(|catalog| catalog.get("providers"))
        .and_then(Value::as_array)
        .and_then(|rows| {
            rows.iter()
                .find(|row| row.get("id").and_then(Value::as_str) == Some(PROVIDER_ID))
        })
        .ok_or_else(|| {
            AppError::Source("the source manifest has no hoi4-imagegen provider".into())
        })?;
    let release = ReleaseBinaryRoute::from_provider(provider)?;
    let target = match release.target_for(Platform::current(), std::env::consts::ARCH) {
        Ok(target) => target,
        Err(AppError::UnsupportedPlatform(message)) => {
            return Ok(InstallPlanning::Unsupported(message))
        }
        Err(error) => return Err(error),
    };
    let mut action = base_action(INSTALL_ACTION_ID, INSTALL_SOURCE);
    action.arguments = vec![
        target.url.clone(),
        target.triple.clone(),
        target.archive.extension().into(),
        target.mcp_entry.clone(),
        target.cli_entry.clone(),
    ];
    action.network_access = format!(
        "https://github.com/{}/releases/download (one release archive, size and SHA-256 declared by the source manifest)",
        route::RELEASE_REPOSITORY
    );
    action.expected_writes = vec![format!("{}/**", release.version_dir())];
    action.rollback_boundary = "Installs the version beside any other, never over it. An update removes the previous version only after the new one passes its version check, and only while its files still match what Setup recorded. Rolling back the project files does not reinstall a removed version: run Repair.".into();
    action.display_command = Some(format!(
        "Download hoi4-imagegen v{} for {} from GitHub, verify its SHA-256, and install it in {}",
        release.version,
        target.triple,
        release.version_dir()
    ));
    action.verified_executable_sha256 = Some(target.sha256.clone());
    action.verified_executable_size = Some(target.size);
    action.verified_package_name = Some("hoi4-imagegen".into());
    action.verified_package_version = Some(release.version.clone());
    action.verified_runtime_entry =
        Some(release.mcp_relative(Platform::current() == Platform::Windows));
    Ok(InstallPlanning::Install(Box::new(action)))
}

/// Removes one installed version if its files still match `tree_sha256`.
pub fn cleanup_action(version: &str, tree_sha256: &str) -> ExternalAction {
    let mut action = base_action(CLEANUP_ACTION_ID, CLEANUP_SOURCE);
    action.arguments = vec![version.into(), tree_sha256.into()];
    action.expected_writes = vec![format!("{}/**", route::version_dir(version))];
    action.rollback_boundary =
        "Removes only the exact hoi4-imagegen files that Setup recorded. A changed folder is kept."
            .into();
    action.display_command = Some(format!(
        "Remove hoi4-imagegen {version} from this project if its files match the hashes Setup saved."
    ));
    action
}

fn outcome(state: &str, evidence: impl Into<String>) -> PostInstallActionOutcome {
    PostInstallActionOutcome {
        component_id: COMPONENT_ID.into(),
        state: state.into(),
        evidence: evidence.into(),
    }
}

fn changed() -> AppError {
    AppError::Transaction("the reviewed hoi4-imagegen action is incomplete or changed".into())
}

fn reviewed_install(action: &ExternalAction) -> Result<(String, ReleaseTarget), AppError> {
    let version = action
        .verified_package_version
        .clone()
        .ok_or_else(changed)?;
    let [url, triple, archive, mcp, cli] = action.arguments.as_slice() else {
        return Err(changed());
    };
    if action.command_source != INSTALL_SOURCE
        || action.component_id != COMPONENT_ID
        || !action.environment_names.is_empty()
        || action.contains_secret
        || action.verified_package_name.as_deref() != Some("hoi4-imagegen")
        || action.platform != Platform::current()
    {
        return Err(changed());
    }
    let declared = serde_json::json!({
        "url": url,
        "sha256": action.verified_executable_sha256.clone().ok_or_else(changed)?,
        "size": action.verified_executable_size.ok_or_else(changed)?,
        "archive": archive,
        "entries": {"mcp": mcp, "cli": cli},
    });
    let target = route::parse_target(&version, triple, &declared)?;
    if route::host_triple(Platform::current(), std::env::consts::ARCH)
        != Some(target.triple.as_str())
    {
        return Err(AppError::UnsupportedPlatform(
            "the reviewed hoi4-imagegen archive is for another platform".into(),
        ));
    }
    Ok((version, target))
}

fn reviewed_cleanup(action: &ExternalAction) -> Result<(String, String), AppError> {
    let [version, tree] = action.arguments.as_slice() else {
        return Err(changed());
    };
    if action.command_source != CLEANUP_SOURCE
        || action.component_id != COMPONENT_ID
        || !action.environment_names.is_empty()
        || action.network_access != "none"
        || action.expected_writes != [format!("{}/**", route::version_dir(version))]
    {
        return Err(changed());
    }
    hoi4_setup_core::manifest::validate_sha256(tree)?;
    Ok((version.clone(), tree.clone()))
}

/// Run the reviewed hoi4-imagegen actions of a plan: install the new version
/// and check it, then remove the version it replaces.
pub fn run_reviewed_actions(
    root: &Path,
    plan: &InstallationPlan,
    downloader: &dyn ReleaseDownloader,
    runner: &dyn CliRunner,
) -> Result<PostInstallActionOutcome, AppError> {
    let of = |id: &str| {
        plan.external_actions
            .iter()
            .filter(|action| action.id == id && action.component_id == COMPONENT_ID)
            .collect::<Vec<_>>()
    };
    let (installs, cleanups) = (of(INSTALL_ACTION_ID), of(CLEANUP_ACTION_ID));
    let removing = plan.maintenance_mode.as_deref() == Some("remove");
    if installs.len() > 1
        || cleanups.len() > 1
        || (installs.is_empty() && cleanups.is_empty())
        || (removing && !installs.is_empty())
    {
        return Err(AppError::Transaction(
            "the reviewed hoi4-imagegen actions are missing or duplicated".into(),
        ));
    }
    let mut parts = Vec::new();
    let mut state = "not_selected";
    let mut installed_ok = true;
    let mut installed_version = None;
    if let Some(action) = installs.first() {
        let (version, target) = reviewed_install(action)?;
        let known = plan
            .image_generation
            .as_ref()
            .filter(|selection| {
                selection.installed_release_version.as_deref() == Some(version.as_str())
            })
            .and_then(|selection| selection.installed_release_tree_sha256.clone());
        let receipt = install_release(root, &version, &target, known.as_deref(), downloader)?;
        parts.push(format!(
            "{}{}",
            route::EVIDENCE_VERSION_PREFIX,
            receipt.version
        ));
        parts.push(format!(
            "{}{}",
            route::EVIDENCE_TREE_PREFIX,
            receipt.tree_sha256
        ));
        let installed = InstalledImagegen {
            root: root.to_path_buf(),
            version: receipt.version.clone(),
            tree_sha256: receipt.tree_sha256.clone(),
        };
        installed_version = Some(receipt.version);
        match installed.verify_version(runner) {
            Ok(()) => state = "ready",
            Err(error) => {
                state = "incomplete";
                installed_ok = false;
                parts.push(format!("version-check-failed:{error}"));
            }
        }
    }
    if let Some(action) = cleanups.first() {
        let (old_version, tree) = reviewed_cleanup(action)?;
        if installed_ok && installed_version.as_deref() != Some(old_version.as_str()) {
            match remove_release(root, &old_version, &tree)? {
                RemoveResult::Removed | RemoveResult::AlreadyAbsent => {
                    if installs.is_empty() {
                        parts.push(route::EVIDENCE_REMOVED.into());
                    }
                }
                RemoveResult::ModifiedKept => {
                    parts.push(format!("old-version-kept:{old_version}"));
                    if installs.is_empty() {
                        state = "incomplete";
                    }
                }
            }
        }
    }
    Ok(outcome(state, parts.join(";")))
}

/// What the transaction records from an action outcome: the installed
/// version and tree hash, `None` fields after a removal, or `Ok(None)` when
/// the outcome carries no change.
#[allow(clippy::type_complexity)]
pub fn receipt_from_evidence(
    evidence: &str,
) -> Result<Option<(Option<String>, Option<String>)>, AppError> {
    let parts = evidence.split(';').collect::<Vec<_>>();
    if parts.contains(&route::EVIDENCE_REMOVED) {
        return Ok(Some((None, None)));
    }
    let find = |prefix: &str| parts.iter().find_map(|part| part.strip_prefix(prefix));
    match (
        find(route::EVIDENCE_VERSION_PREFIX),
        find(route::EVIDENCE_TREE_PREFIX),
    ) {
        (Some(version), Some(tree)) => {
            hoi4_setup_core::manifest::validate_sha256(tree).map_err(|_| {
                AppError::Transaction("hoi4-imagegen returned an invalid install hash".into())
            })?;
            let digits = version.split('.').collect::<Vec<_>>();
            if digits.len() != 3
                || digits
                    .iter()
                    .any(|part| part.is_empty() || !part.bytes().all(|byte| byte.is_ascii_digit()))
            {
                return Err(AppError::Transaction(
                    "hoi4-imagegen returned an invalid version".into(),
                ));
            }
            Ok(Some((Some(version.to_owned()), Some(tree.to_owned()))))
        }
        _ => Ok(None),
    }
}
