use crate::models::Platform;
use crate::process::{ProcessResult, ProcessSpec};
use crate::security::{is_link_metadata, normalize_relative_path, path_has_link_component};
use crate::AppError;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GitMode {
    Initialize,
    Preserve,
    Skip,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitSetup {
    pub mode: GitMode,
    pub branch: String,
    pub initial_commit: bool,
    #[serde(default)]
    pub remote_name: Option<String>,
    #[serde(default)]
    pub remote_url: Option<String>,
    pub push_approved: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitPlan {
    pub actions: Vec<String>,
    pub branch: Option<String>,
    pub initial_commit_preview: Vec<String>,
    pub remote: Option<String>,
    pub push: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GitStatus {
    pub repository_present: bool,
    pub branch: Option<String>,
    pub detached: bool,
    pub dirty: Option<bool>,
    pub remotes: Vec<String>,
    pub tracked_secret_like_paths: Vec<String>,
}

/// Bounded, read-only repository evidence used by the existing-project scan.
/// This is deliberately separate from `GitStatus`, which is also used by the
/// installation planner and only needs the state required to choose a Git
/// setup mode.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GitInspection {
    pub status: GitStatus,
    pub commit: Option<String>,
    pub staged_files: u32,
    pub unstaged_files: u32,
    pub untracked_files: u32,
    pub submodules: Vec<String>,
    pub hooks: Vec<String>,
    pub ignore_files: Vec<String>,
    pub status_probe: String,
    pub tracked_path_scan_complete: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GitApplyResult {
    pub initialized: bool,
    pub committed: bool,
    pub remote_configured: bool,
}

pub const MANAGED_GITIGNORE_BLOCK: &str = "# BEGIN HOI4 Mod Setup managed rules\n.hoi4-mod-setup/backups/\n.hoi4-mod-setup/cache/\n.tools/3d_pipeline/vendor/\n# END HOI4 Mod Setup managed rules";

pub fn validate_git_setup(setup: &GitSetup) -> Result<(), AppError> {
    if setup.mode == GitMode::Initialize && !valid_branch_name(&setup.branch) {
        return Err(AppError::InvalidInput(
            "invalid initial Git branch name".into(),
        ));
    }
    if setup.push_approved {
        return Err(AppError::InvalidInput(
            "push requires a separate publish action".into(),
        ));
    }
    if let Some(remote) = &setup.remote_name {
        if remote.is_empty()
            || remote.len() > 64
            || remote.starts_with('-')
            || remote.chars().any(|character| character.is_whitespace())
        {
            return Err(AppError::InvalidInput("invalid Git remote name".into()));
        }
    }
    if let Some(url) = &setup.remote_url {
        validate_remote_url(url)?;
        if setup.remote_name.is_none() {
            return Err(AppError::InvalidInput(
                "remote URL requires a remote name".into(),
            ));
        }
    }
    Ok(())
}

pub fn plan_git(
    setup: &GitSetup,
    existing: &GitStatus,
    approved_files: &[String],
) -> Result<GitPlan, AppError> {
    validate_git_setup(setup)?;
    let mut actions = Vec::new();
    let branch = match setup.mode {
        GitMode::Initialize => {
            if existing.repository_present {
                return Err(AppError::InvalidInput(
                    "initialize selected but a Git repository already controls the project".into(),
                ));
            }
            actions.push("create .git metadata during apply".into());
            actions.push("merge .gitignore managed rules".into());
            Some(setup.branch.clone())
        }
        GitMode::Preserve => {
            if !existing.repository_present {
                return Err(AppError::InvalidInput(
                    "preserve selected but no Git repository was detected".into(),
                ));
            }
            actions.push("preserve history, branches, remotes, hooks, and worktrees".into());
            existing.branch.clone()
        }
        GitMode::Skip => None,
    };
    if setup.initial_commit && setup.mode == GitMode::Initialize {
        actions.push("create optional initial commit after file validation".into());
    }
    if let (Some(name), Some(url)) = (&setup.remote_name, &setup.remote_url) {
        actions.push(format!("configure local remote {name} -> {url}"));
    }
    Ok(GitPlan {
        actions,
        branch,
        initial_commit_preview: approved_files.to_vec(),
        remote: setup.remote_url.clone(),
        push: false,
    })
}

pub fn merge_gitignore(existing: Option<&str>) -> String {
    let existing = existing.unwrap_or("").trim_end();
    if existing.contains("# BEGIN HOI4 Mod Setup managed rules") {
        return existing.to_string();
    }
    if existing.is_empty() {
        MANAGED_GITIGNORE_BLOCK.to_string()
    } else {
        format!("{existing}\n\n{MANAGED_GITIGNORE_BLOCK}")
    }
}

pub fn valid_branch_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 200
        && !value.starts_with('-')
        && !value.ends_with('.')
        && !value.ends_with('/')
        && !value.contains("..")
        && !value.contains("@{")
        && !value
            .chars()
            .any(|character| character.is_whitespace() || "~^:?*[\\".contains(character))
}

pub fn apply_git_setup(
    root: &Path,
    setup: &GitSetup,
    managed_paths: &[String],
) -> Result<GitApplyResult, AppError> {
    validate_git_setup(setup)?;
    let mut result = GitApplyResult::default();
    match setup.mode {
        GitMode::Skip | GitMode::Preserve => {}
        GitMode::Initialize => {
            let git_path = root.join(".git");
            let git_metadata = match std::fs::symlink_metadata(&git_path) {
                Ok(metadata) => Some(metadata),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
                Err(error) => return Err(AppError::Transaction(error.to_string())),
            };
            if let Some(metadata) = git_metadata {
                if is_link_metadata(&metadata) || path_has_link_component(&git_path) {
                    return Err(AppError::PathSecurity(
                        "cannot initialize Git through a .git link or junction".into(),
                    ));
                }
                return Err(AppError::Transaction(
                    "cannot initialize Git over an existing repository".into(),
                ));
            }
            let initial_branch = format!("--initial-branch={}", setup.branch);
            run_git(root, &["init", &initial_branch])?;
            result.initialized = true;
            if !managed_paths.is_empty() {
                let mut args = vec!["add", "--"];
                args.extend(managed_paths.iter().map(String::as_str));
                run_git(root, &args)?;
            }
            if setup.initial_commit {
                run_git(root, &["commit", "-m", "Initialize HOI4 Mod Setup project"])?;
                result.committed = true;
            }
        }
    }
    if let (Some(name), Some(url)) = (&setup.remote_name, &setup.remote_url) {
        if setup.mode == GitMode::Preserve {
            let existing = run_git_capture(root, &["remote", "get-url", name])?;
            if existing.status_code == Some(0) {
                if existing.stdout.trim() != url {
                    return Err(AppError::Transaction(format!(
                        "Git remote {name} already exists with a different URL; refusing to replace it"
                    )));
                }
                // The requested remote is already present and identical. It
                // belongs to the existing repository, so rollback must not
                // remove it.
                return Ok(result);
            }
        }
        run_git(root, &["remote", "add", name, url])?;
        result.remote_configured = true;
    }
    Ok(result)
}

fn run_git(root: &Path, args: &[&str]) -> Result<(), AppError> {
    let output = run_git_capture(root, args)?;
    if output.status_code == Some(0) {
        Ok(())
    } else {
        Err(AppError::Process(format!(
            "git {} failed: {}",
            args.join(" "),
            output.stderr.trim()
        )))
    }
}

fn run_git_capture(root: &Path, args: &[&str]) -> Result<ProcessResult, AppError> {
    let executable = find_git_executable()?;
    let spec = ProcessSpec {
        executable: executable.clone(),
        args: args.iter().map(|value| (*value).to_string()).collect(),
        cwd: Some(root.to_path_buf()),
        platform: Platform::current(),
        environment_names: vec![],
        timeout_seconds: 120,
        max_output_bytes: 2 * 1024 * 1024,
    };
    spec.run(&[executable], None)
}

fn run_git_read_only(root: &Path, args: &[&str]) -> Result<ProcessResult, AppError> {
    let executable = find_git_executable()?;
    let spec = ProcessSpec {
        executable: executable.clone(),
        args: args.iter().map(|value| (*value).to_string()).collect(),
        cwd: Some(root.to_path_buf()),
        platform: Platform::current(),
        environment_names: vec![],
        timeout_seconds: 10,
        max_output_bytes: 4 * 1024 * 1024,
    };
    spec.run(&[executable], None)
}

fn find_git_executable() -> Result<PathBuf, AppError> {
    let names = if cfg!(target_os = "windows") {
        ["git.exe", "git"]
    } else {
        ["git", "git.exe"]
    };
    std::env::var_os("PATH")
        .into_iter()
        .flat_map(|path| std::env::split_paths(&path).collect::<Vec<_>>())
        .flat_map(|entry| names.iter().map(move |name| entry.join(name)))
        .find_map(|candidate| {
            let metadata = std::fs::symlink_metadata(&candidate).ok()?;
            if !metadata.is_file()
                || is_link_metadata(&metadata)
                || path_has_link_component(&candidate)
            {
                return None;
            }
            let canonical = std::fs::canonicalize(candidate).ok()?;
            (!path_has_link_component(&canonical)).then_some(canonical)
        })
        .ok_or_else(|| {
            AppError::Process("Git executable was not found on the approved PATH".into())
        })
}

pub fn rollback_initialized_git(root: &Path) -> Result<(), AppError> {
    let git = root.join(".git");
    let metadata = std::fs::symlink_metadata(&git);
    if metadata.as_ref().is_ok_and(is_link_metadata) || path_has_link_component(&git) {
        return Err(AppError::PathSecurity(
            "refusing to remove linked .git metadata".into(),
        ));
    }
    if !metadata.is_ok_and(|metadata| metadata.is_dir()) {
        return Ok(());
    }
    let output = run_git_capture(root, &["status", "--porcelain"])?;
    if output.status_code != Some(0) {
        return Err(AppError::Process(
            "cannot verify Git state before rollback".into(),
        ));
    }
    if !output.stdout.trim().is_empty() {
        return Err(AppError::Transaction(
            "Git contains user changes; refusing to remove initialized metadata".into(),
        ));
    }
    std::fs::remove_dir_all(&git)?;
    Ok(())
}

pub fn rollback_added_remote(root: &Path, name: &str, expected_url: &str) -> Result<(), AppError> {
    if name.is_empty()
        || name.len() > 64
        || name.starts_with('-')
        || name.chars().any(|character| character.is_whitespace())
    {
        return Err(AppError::PathSecurity(
            "journal contains an invalid Git remote name".into(),
        ));
    }
    validate_remote_url(expected_url)?;
    let current = run_git_capture(root, &["remote", "get-url", name])?;
    if current.status_code != Some(0) {
        // If the user already removed the transaction-added remote, there is
        // nothing left for rollback to do. Other Git failures remain visible.
        return Ok(());
    }
    if current.stdout.trim() != expected_url {
        return Err(AppError::Transaction(
            "Git remote changed after setup; refusing to remove the user-updated remote".into(),
        ));
    }
    run_git(root, &["remote", "remove", name])
}

pub fn validate_remote_url(value: &str) -> Result<(), AppError> {
    let valid =
        (value.starts_with("https://") || value.starts_with("ssh://") || value.starts_with("git@"))
            && !value.contains('\n')
            && !value.contains('\r')
            && !value.contains(' ')
            && !(value.starts_with("https://") && value.contains('@'))
            && !(value.starts_with("ssh://") && value.contains('@'))
            && !value.contains("..\\")
            && !value.contains("../");
    if valid {
        Ok(())
    } else {
        Err(AppError::InvalidInput(
            "remote URL must be an explicit HTTPS or SSH URL".into(),
        ))
    }
}

pub fn read_git_head(root: &Path) -> GitStatus {
    let git = root.join(".git");
    let git_metadata = std::fs::symlink_metadata(&git).ok();
    let linked =
        git_metadata.as_ref().is_some_and(is_link_metadata) || path_has_link_component(&git);
    let repository_present = !linked
        && git_metadata
            .as_ref()
            .is_some_and(|metadata| metadata.is_dir() || metadata.is_file());
    let head = if repository_present
        && git_metadata
            .as_ref()
            .is_some_and(|metadata| metadata.is_dir())
        && !path_has_link_component(&git.join("HEAD"))
    {
        std::fs::read_to_string(git.join("HEAD"))
            .ok()
            .map(|value| value.trim().to_string())
    } else {
        None
    };
    let branch = head
        .as_deref()
        .and_then(|value| value.strip_prefix("ref: refs/heads/"))
        .map(str::to_string);
    GitStatus {
        repository_present,
        branch,
        detached: head
            .as_deref()
            .is_some_and(|value| !value.starts_with("ref:")),
        dirty: None,
        remotes: vec![],
        tracked_secret_like_paths: vec![],
    }
}

/// Inspect the repository without changing the worktree or following a
/// linked worktree's external gitdir. Git commands are fixed, bounded, and
/// passed through the same executable allowlist as installation operations.
pub fn inspect_read_only(root: &Path) -> GitInspection {
    let base = read_git_head(root);
    let mut inspection = GitInspection {
        status: base.clone(),
        status_probe: if !base.repository_present {
            "not_a_repository".into()
        } else {
            "unavailable".into()
        },
        ..GitInspection::default()
    };
    if !base.repository_present {
        return inspection;
    }

    let git_path = root.join(".git");
    let Some(git_metadata) = fs::symlink_metadata(&git_path).ok() else {
        return inspection;
    };
    if !git_metadata.is_dir()
        || is_link_metadata(&git_metadata)
        || path_has_link_component(&git_path)
    {
        inspection.status_probe = "linked_worktree_not_followed".into();
        return inspection;
    }

    let mut complete = true;
    match run_git_read_only(
        root,
        &[
            "status",
            "--porcelain=v1",
            "--branch",
            "--untracked-files=normal",
            "--no-optional-locks",
        ],
    ) {
        Ok(result) if result.status_code == Some(0) && !result.timed_out => {
            parse_status_output(&result.stdout, &mut inspection);
        }
        _ => complete = false,
    }

    match run_git_read_only(root, &["rev-parse", "--verify", "HEAD"]) {
        Ok(result) if result.status_code == Some(0) && !result.timed_out => {
            inspection.commit = result
                .stdout
                .lines()
                .map(str::trim)
                .find(|value| valid_commit_id(value))
                .map(str::to_string);
        }
        _ => complete = false,
    }

    match run_git_read_only(root, &["remote"]) {
        Ok(result) if result.status_code == Some(0) && !result.timed_out => {
            inspection.status.remotes = result
                .stdout
                .lines()
                .map(str::trim)
                .filter(|value| !value.is_empty() && value.len() <= 128)
                .filter(|value| value.chars().all(|character| !character.is_control()))
                .map(str::to_string)
                .collect();
            inspection.status.remotes.sort();
            inspection.status.remotes.dedup();
        }
        _ => complete = false,
    }

    match run_git_read_only(root, &["submodule", "status", "--recursive"]) {
        Ok(result) if result.status_code == Some(0) && !result.timed_out => {
            inspection.submodules = parse_submodule_paths(&result.stdout);
        }
        _ => complete = false,
    }

    inspection.hooks = safe_hook_names(&git_path.join("hooks"));
    inspection.ignore_files = safe_ignore_files(root, &git_path);

    let mut tracked_scan_complete = false;
    match run_git_read_only(root, &["ls-files", "--cached", "-z", "--"]) {
        Ok(result) if result.status_code == Some(0) && !result.timed_out => {
            tracked_scan_complete = result.stdout.len() < 4 * 1024 * 1024;
            inspection.status.tracked_secret_like_paths = result
                .stdout
                .split('\0')
                .filter(|path| !path.is_empty() && crate::git::secret_like_path(path))
                .filter_map(|path| normalize_relative_path(path).ok())
                .take(100)
                .collect();
            inspection.status.tracked_secret_like_paths.sort();
            inspection.status.tracked_secret_like_paths.dedup();
        }
        _ => complete = false,
    }
    inspection.tracked_path_scan_complete = tracked_scan_complete;
    if !tracked_scan_complete {
        complete = false;
    }
    inspection.status_probe = if complete {
        "complete".into()
    } else {
        "partial".into()
    };
    inspection
}

fn parse_status_output(output: &str, inspection: &mut GitInspection) {
    for line in output.lines() {
        if let Some(header) = line.strip_prefix("## ") {
            let header = header.trim();
            if header == "HEAD (no branch)" {
                inspection.status.branch = None;
                inspection.status.detached = true;
            } else {
                let branch = header
                    .strip_prefix("No commits yet on ")
                    .unwrap_or(header)
                    .split("...")
                    .next()
                    .unwrap_or_default()
                    .trim();
                if !branch.is_empty() && branch != "(no branch)" {
                    inspection.status.branch = Some(branch.to_string());
                    inspection.status.detached = false;
                }
            }
            continue;
        }
        let bytes = line.as_bytes();
        if bytes.len() < 3 {
            continue;
        }
        let staged = bytes[0] != b' ' && bytes[0] != b'?';
        let unstaged = bytes[1] != b' ' && bytes[1] != b'?';
        if bytes[0] == b'?' && bytes[1] == b'?' {
            inspection.untracked_files = inspection.untracked_files.saturating_add(1);
        } else {
            if staged {
                inspection.staged_files = inspection.staged_files.saturating_add(1);
            }
            if unstaged {
                inspection.unstaged_files = inspection.unstaged_files.saturating_add(1);
            }
        }
    }
    inspection.status.dirty = Some(
        inspection.staged_files > 0
            || inspection.unstaged_files > 0
            || inspection.untracked_files > 0,
    );
}

fn valid_commit_id(value: &str) -> bool {
    matches!(value.len(), 40 | 64) && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn parse_submodule_paths(output: &str) -> Vec<String> {
    let mut paths = output
        .lines()
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            let marker = fields.next()?;
            if marker.is_empty() {
                return None;
            }
            normalize_relative_path(fields.next()?).ok()
        })
        .collect::<Vec<_>>();
    paths.sort();
    paths.dedup();
    paths
}

fn safe_hook_names(hooks: &Path) -> Vec<String> {
    if path_has_link_component(hooks) {
        return Vec::new();
    }
    let Ok(entries) = fs::read_dir(hooks) else {
        return Vec::new();
    };
    let mut names = entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let metadata = fs::symlink_metadata(entry.path()).ok()?;
            (metadata.is_file() && !is_link_metadata(&metadata))
                .then(|| entry.file_name().to_string_lossy().to_string())
        })
        .filter(|name| !name.is_empty() && name.len() <= 255)
        .collect::<Vec<_>>();
    names.sort();
    names.dedup();
    names
}

fn safe_ignore_files(root: &Path, git_path: &Path) -> Vec<String> {
    let mut files = Vec::new();
    for (relative, path) in [
        (".gitignore", root.join(".gitignore")),
        (".git/info/exclude", git_path.join("info").join("exclude")),
    ] {
        let metadata = fs::symlink_metadata(path).ok();
        if metadata.is_some_and(|metadata| metadata.is_file() && !is_link_metadata(&metadata)) {
            files.push(relative.to_string());
        }
    }
    files
}

pub fn secret_like_path(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    lower.ends_with(".env")
        || lower.contains("credentials")
        || lower.contains("secret")
        || lower.ends_with(".pem")
        || lower.ends_with(".p12")
        || lower.ends_with(".pfx")
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn gitignore_merge_is_idempotent_and_preserves_local_rules() {
        let first = merge_gitignore(Some("# local\n*.tmp"));
        let second = merge_gitignore(Some(&first));
        assert!(first.contains("*.tmp"));
        assert_eq!(first, second);
    }

    #[test]
    fn push_is_never_part_of_setup() {
        let setup = GitSetup {
            mode: GitMode::Initialize,
            branch: "main".into(),
            initial_commit: true,
            remote_name: Some("origin".into()),
            remote_url: Some("https://github.com/example/mod.git".into()),
            push_approved: false,
        };
        let plan = plan_git(&setup, &GitStatus::default(), &["AGENTS.md".into()]).unwrap();
        assert!(!plan.push);
        assert!(plan
            .actions
            .iter()
            .all(|action| !action.to_ascii_lowercase().contains("push")));
    }

    #[test]
    fn preserve_matching_remote_is_a_noop_and_conflicts_fail_closed() {
        let project = tempdir().unwrap();
        run_git(project.path(), &["init"]).unwrap();
        let original = "https://github.com/example/mod.git";
        run_git(project.path(), &["remote", "add", "origin", original]).unwrap();
        let matching = GitSetup {
            mode: GitMode::Preserve,
            branch: "main".into(),
            initial_commit: false,
            remote_name: Some("origin".into()),
            remote_url: Some(original.into()),
            push_approved: false,
        };
        let result = apply_git_setup(project.path(), &matching, &[]).unwrap();
        assert!(!result.remote_configured);

        let conflicting = GitSetup {
            remote_url: Some("https://github.com/other/mod.git".into()),
            ..matching
        };
        let error = apply_git_setup(project.path(), &conflicting, &[]).unwrap_err();
        assert!(error.to_string().contains("different URL"));
    }

    #[test]
    fn read_only_status_parser_reports_branch_and_worktree_buckets() {
        let mut inspection = GitInspection::default();
        parse_status_output(
            "## feature/example...origin/feature/example [ahead 1]\nM  staged.txt\n M changed.txt\n?? new.txt\n",
            &mut inspection,
        );
        assert_eq!(inspection.status.branch.as_deref(), Some("feature/example"));
        assert!(!inspection.status.detached);
        assert_eq!(inspection.staged_files, 1);
        assert_eq!(inspection.unstaged_files, 1);
        assert_eq!(inspection.untracked_files, 1);
        assert_eq!(inspection.status.dirty, Some(true));
    }

    #[test]
    fn read_only_status_parser_reports_detached_head() {
        let mut inspection = GitInspection::default();
        parse_status_output("## HEAD (no branch)\n", &mut inspection);
        assert!(inspection.status.detached);
        assert!(inspection.status.branch.is_none());
        assert_eq!(inspection.status.dirty, Some(false));
    }
}
