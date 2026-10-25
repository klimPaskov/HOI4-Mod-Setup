//! The package's own "set this project up" command, run once after an
//! installation, update, or repair has been applied.
//!
//! The installed package ships `python -B .tools/check.py setup`: it installs
//! the Python packages the checks need, records the game folder in a gitignored
//! settings file, makes sure the ignore entries exist, runs the synchronizers
//! that regenerate the agent projections, and prints a readiness report. The
//! app runs it as one reviewed action and never reimplements those steps.
//!
//! The command regenerates a few files the lock already tracks (the agent map
//! and projections, `.gitignore`, the quality baseline). The finishing step
//! therefore records their new hashes in the lock, and refuses a change to any
//! other tracked file.

use crate::models::*;
use crate::security::{atomic_write_json, sha256_file};
use crate::AppError;
use std::collections::BTreeMap;
use std::path::Path;

/// The manifest component that installs the check tooling.
pub const COMPONENT_ID: &str = "runtime.agent_sync";
/// The entry point the reviewed action runs.
pub const SCRIPT: &str = ".tools/check.py";
pub const ACTION_ID: &str = "external.agent_setup.run";
const LOCK_PATH: &str = ".hoi4-mod-setup/install.lock.json";

/// Paths the setup command may regenerate. Everything else the lock tracks
/// must be byte-identical after it ran.
const REWRITABLE_EXACT: &[&str] = &[
    ".gitignore",
    "AGENTS.md",
    ".claude/agent-map.md",
    ".tools/quality_baseline.json",
];
const REWRITABLE_PREFIXES: &[&str] = &[".codex/", ".cursor/", ".opencode/", ".qoder/", ".agents/"];

pub fn rewritable(path: &str) -> bool {
    let path = path.replace('\\', "/");
    REWRITABLE_EXACT.contains(&path.as_str())
        || REWRITABLE_PREFIXES
            .iter()
            .any(|prefix| path.starts_with(prefix))
}

/// The reviewed action. `game_dir` is the Hearts of Iron IV folder the app
/// already knows; without it the command is run without `--save-game-dir` and
/// its own `doctor` report says what is missing.
pub fn action(game_dir: Option<&str>) -> ExternalAction {
    let mut arguments = vec![SCRIPT.to_string(), "setup".to_string()];
    if let Some(dir) = game_dir.filter(|dir| !dir.trim().is_empty()) {
        arguments.push("--save-game-dir".into());
        // A canonicalized Windows path carries a verbatim `\\?\` prefix; the
        // package's tools expect the plain folder.
        let plain = dir.replace('\\', "/");
        arguments.push(plain.strip_prefix("//?/").unwrap_or(&plain).to_string());
    }
    ExternalAction {
        id: ACTION_ID.into(),
        component_id: COMPONENT_ID.into(),
        platform: Platform::current(),
        command_source: "repository_script".into(),
        executable: Some("Python 3.9 or newer (python.org)".into()),
        arguments,
        working_directory: Some("<project_root>".into()),
        environment_names: Vec::new(),
        network_access: "Downloads Pillow, numpy, scipy and openpyxl from PyPI with pip when they are missing.".into(),
        expected_writes: vec![
            ".tools/local_settings.json".into(),
            ".tools/.venv/**".into(),
            ".gitignore".into(),
            ".claude/agent-map.md".into(),
            ".tools/quality_baseline.json".into(),
            "agent projection files for the selected coding clients".into(),
            "Python packages in the Python's own package folder".into(),
        ],
        privilege: "current_user".into(),
        rollback_boundary: "The files this command regenerates are tracked in the installation lock after it runs. Installed Python packages are outside the project and are reported, not removed.".into(),
        display_command: Some("Prepare the development tools: install their Python packages, record the game folder, update the agent files, and check the project.".into()),
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

/// Whether a plan leaves the check tooling installed: it carries the entry
/// point, does not delete it, and is not a removal.
pub fn wants_setup(operations: &[PlanOperation], maintenance_mode: Option<&str>) -> bool {
    maintenance_mode != Some("remove")
        && operations.iter().any(|operation| {
            !operation.external
                && operation.destination.eq_ignore_ascii_case(SCRIPT)
                && operation.action != OperationAction::DeleteManaged
        })
}

/// Add the reviewed action to a plan that installs the tooling.
pub fn add_to_plan(plan: &mut InstallationPlan, game_dir: Option<&str>) {
    plan.external_actions
        .retain(|existing| existing.id != ACTION_ID);
    if wants_setup(&plan.operations, plan.maintenance_mode.as_deref()) {
        plan.external_actions.push(action(game_dir));
        plan.optional_workflows
            .insert(COMPONENT_ID.into(), "selected_pending".into());
    }
}

/// The plan's reviewed action, checked against the one this version defines.
pub fn reviewed_action(plan: &InstallationPlan) -> Result<Option<&ExternalAction>, AppError> {
    let mut found = plan
        .external_actions
        .iter()
        .filter(|candidate| candidate.id == ACTION_ID);
    let Some(action) = found.next() else {
        return Ok(None);
    };
    if found.next().is_some() {
        return Err(AppError::Transaction(
            "the reviewed plan contains more than one development tools action".into(),
        ));
    }
    let game_dir = action
        .arguments
        .iter()
        .position(|argument| argument == "--save-game-dir")
        .and_then(|index| action.arguments.get(index + 1))
        .map(String::as_str);
    let expected = self::action(game_dir);
    if action.component_id != expected.component_id
        || action.command_source != expected.command_source
        || action.platform != expected.platform
        || action.arguments != expected.arguments
        || action.privilege != "current_user"
        || action.contains_secret
        || !action.environment_names.is_empty()
    {
        return Err(AppError::Transaction(
            "the reviewed development tools action is incomplete or changed".into(),
        ));
    }
    Ok(Some(action))
}

/// Check the installed entry point against the hash the reviewed plan holds for
/// it, immediately before it runs.
pub fn verify_script(root: &Path, plan: &InstallationPlan) -> Result<(), AppError> {
    let operation = plan
        .operations
        .iter()
        .find(|operation| !operation.external && operation.destination.eq_ignore_ascii_case(SCRIPT))
        .ok_or_else(|| {
            AppError::Transaction("the reviewed plan has no entry for the check tooling".into())
        })?;
    let expected = operation
        .result_sha256
        .as_deref()
        .or(operation.source_sha256.as_deref())
        .ok_or_else(|| {
            AppError::Transaction("the check tooling has no checksum evidence".into())
        })?;
    let bytes = crate::flatten::read_regular_file_no_follow_under_root(root, SCRIPT)?;
    if crate::security::sha256_bytes(&bytes) != expected {
        return Err(AppError::PathSecurity(
            "the check tooling changed before it ran".into(),
        ));
    }
    Ok(())
}

/// The hash of each tracked file the command is allowed to regenerate.
pub fn snapshot(
    root: &Path,
    lock: &InstallationLock,
) -> Result<BTreeMap<String, Option<String>>, AppError> {
    let mut hashes = BTreeMap::new();
    for file in lock.files.iter().filter(|file| !file.external) {
        let path = crate::security::safe_join(root, &file.path)?;
        let hash = if path.is_file() {
            Some(sha256_file(&path)?)
        } else {
            None
        };
        hashes.insert(file.path.clone(), hash);
    }
    Ok(hashes)
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Changes {
    /// Tracked, allowed files whose bytes changed: path, new hash, new size.
    pub regenerated: Vec<(String, String, u64)>,
    /// Tracked files the command was not allowed to change, but did.
    pub unexpected: Vec<String>,
}

/// Compare the tracked files before and after the command ran.
pub fn changes_since(
    root: &Path,
    before: &BTreeMap<String, Option<String>>,
) -> Result<Changes, AppError> {
    let mut changes = Changes::default();
    for (relative, previous) in before {
        let path = crate::security::safe_join(root, relative)?;
        let current = if path.is_file() {
            Some(sha256_file(&path)?)
        } else {
            None
        };
        if &current == previous {
            continue;
        }
        match current {
            Some(hash) if rewritable(relative) => {
                let size = std::fs::metadata(&path)?.len();
                changes.regenerated.push((relative.clone(), hash, size));
            }
            _ => changes.unexpected.push(relative.clone()),
        }
    }
    Ok(changes)
}

/// Record the outcome in the lock: the new hashes of regenerated files, and the
/// state of the step for the readiness report.
pub fn amend_lock(
    root: &Path,
    changes: &Changes,
    state: &str,
    reason: Option<String>,
) -> Result<(), AppError> {
    let value: serde_json::Value = serde_json::from_slice(&std::fs::read(root.join(LOCK_PATH))?)?;
    let mut lock = crate::migrations::migrate_lock(value)?;
    for (relative, hash, size) in &changes.regenerated {
        if let Some(file) = lock
            .files
            .iter_mut()
            .find(|file| !file.external && &file.path == relative)
        {
            file.installed_sha256 = hash.clone();
            file.installed_size = Some(*size);
        }
    }
    lock.optional_workflows.insert(
        COMPONENT_ID.into(),
        OptionalWorkflowLock {
            state: state.into(),
            reason,
            credential_reference: None,
        },
    );
    atomic_write_json(&root.join(LOCK_PATH), &lock)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::security::sha256_bytes;

    fn lock_with(files: &[(&str, &[u8])]) -> InstallationLock {
        let mut lock: InstallationLock = serde_json::from_str(include_str!(
            "../../docs/examples/installation-lock.example.json"
        ))
        .unwrap();
        let template = lock.files[0].clone();
        lock.files = files
            .iter()
            .map(|(path, bytes)| {
                let mut file = template.clone();
                file.path = (*path).into();
                file.external = false;
                file.installed_sha256 = sha256_bytes(bytes);
                file.installed_size = Some(bytes.len() as u64);
                file
            })
            .collect();
        lock
    }

    #[test]
    fn only_generated_projections_may_be_rewritten() {
        for path in [
            ".gitignore",
            "AGENTS.md",
            ".claude/agent-map.md",
            ".tools/quality_baseline.json",
            ".codex/agents/x.toml",
            ".agents/skills/x/SKILL.md",
        ] {
            assert!(rewritable(path), "{path}");
        }
        for path in [
            "CLAUDE.md",
            ".claude/skills/x/SKILL.md",
            ".claude/agents/x.md",
            "descriptor.mod",
            ".tools/check.py",
        ] {
            assert!(!rewritable(path), "{path}");
        }
    }

    #[test]
    fn the_action_carries_the_game_folder_and_is_validated() {
        let mut plan: InstallationPlan = serde_json::from_str(include_str!(
            "../../docs/examples/installation-plan.example.json"
        ))
        .unwrap();
        // The canonicalized Windows form carries a verbatim prefix.
        plan.external_actions = vec![action(Some("\\\\?\\C:\\Games\\HOI4"))];
        let reviewed = reviewed_action(&plan).unwrap().unwrap();
        assert_eq!(
            reviewed.arguments,
            [
                ".tools/check.py",
                "setup",
                "--save-game-dir",
                "C:/Games/HOI4"
            ]
        );
        plan.external_actions[0].arguments.push("--extra".into());
        assert!(reviewed_action(&plan).is_err());
        plan.external_actions = vec![action(None), action(None)];
        assert!(reviewed_action(&plan).is_err());
        plan.external_actions.clear();
        assert!(reviewed_action(&plan).unwrap().is_none());
    }

    #[test]
    fn a_removal_or_a_plan_without_the_tooling_gets_no_action() {
        let mut plan: InstallationPlan = serde_json::from_str(include_str!(
            "../../docs/examples/installation-plan.example.json"
        ))
        .unwrap();
        let mut operation = plan.operations[0].clone();
        operation.destination = SCRIPT.into();
        operation.external = false;
        operation.action = OperationAction::Replace;
        plan.operations = vec![operation];
        plan.external_actions.clear();
        add_to_plan(&mut plan, Some("C:/g"));
        assert_eq!(plan.external_actions.len(), 1);
        assert_eq!(
            plan.optional_workflows
                .get(COMPONENT_ID)
                .map(String::as_str),
            Some("selected_pending")
        );
        // Running it again adds nothing twice.
        add_to_plan(&mut plan, Some("C:/g"));
        assert_eq!(plan.external_actions.len(), 1);
        plan.maintenance_mode = Some("remove".into());
        add_to_plan(&mut plan, Some("C:/g"));
        assert!(plan.external_actions.is_empty());
        plan.maintenance_mode = None;
        plan.operations[0].destination = "other.py".into();
        add_to_plan(&mut plan, Some("C:/g"));
        assert!(plan.external_actions.is_empty());
    }

    #[test]
    fn regenerated_files_are_recorded_and_other_changes_are_refused() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(root.path().join(".claude")).unwrap();
        std::fs::create_dir_all(root.path().join(".hoi4-mod-setup")).unwrap();
        std::fs::write(root.path().join(".claude/agent-map.md"), b"map v1").unwrap();
        std::fs::write(root.path().join("CLAUDE.md"), b"rules").unwrap();
        let lock = lock_with(&[(".claude/agent-map.md", b"map v1"), ("CLAUDE.md", b"rules")]);
        atomic_write_json(&root.path().join(LOCK_PATH), &lock).unwrap();
        let before = snapshot(root.path(), &lock).unwrap();

        std::fs::write(root.path().join(".claude/agent-map.md"), b"map v2").unwrap();
        let changes = changes_since(root.path(), &before).unwrap();
        assert_eq!(changes.regenerated.len(), 1);
        assert!(changes.unexpected.is_empty());
        amend_lock(root.path(), &changes, "ready", None).unwrap();
        let amended = crate::migrations::migrate_lock(
            serde_json::from_slice(&std::fs::read(root.path().join(LOCK_PATH)).unwrap()).unwrap(),
        )
        .unwrap();
        let map = amended
            .files
            .iter()
            .find(|file| file.path == ".claude/agent-map.md")
            .unwrap();
        assert_eq!(map.installed_sha256, sha256_bytes(b"map v2"));
        assert_eq!(map.installed_size, Some(6));
        assert_eq!(amended.optional_workflows[COMPONENT_ID].state, "ready");

        // The canonical instructions must never change.
        std::fs::write(root.path().join("CLAUDE.md"), b"edited").unwrap();
        let changes = changes_since(root.path(), &before).unwrap();
        assert_eq!(changes.unexpected, ["CLAUDE.md"]);
    }
}
