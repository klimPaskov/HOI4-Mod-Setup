use crate::models::Platform;
use crate::process::{find_path_executable, ProcessResult, ProcessSpec};
use crate::security::{
    atomic_write_json, is_link_metadata, normalize_relative_path, path_has_link_component,
    safe_join,
};
use crate::AppError;
use chrono::{Duration as ChronoDuration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use uuid::Uuid;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OnlineGitAction {
    PushRemote,
    CreatePublicGithub,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitOnlineResult {
    pub action: OnlineGitAction,
    pub branch: String,
    pub remote_name: String,
    pub repository: String,
    #[serde(default)]
    pub repository_url: Option<String>,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitOnlinePlan {
    pub plan_id: Uuid,
    pub action: OnlineGitAction,
    pub branch: String,
    pub remote_name: String,
    pub repository: String,
    pub head_sha: String,
    pub reviewed_at: String,
    pub expires_at: String,
    pub git_executable_sha256: String,
    #[serde(default)]
    pub remote_url: Option<String>,
    #[serde(default)]
    pub gh_executable_sha256: Option<String>,
}

#[derive(Debug, Clone)]
struct PendingGitOnlinePlan {
    plan: GitOnlinePlan,
    root: PathBuf,
}

static PENDING_GIT_ONLINE_PLANS: OnceLock<Mutex<HashMap<Uuid, PendingGitOnlinePlan>>> =
    OnceLock::new();
const MAX_GIT_METADATA_TEXT_BYTES: u64 = 1024 * 1024;
const APP_GIT_USER_NAME: &str = "HOI4 Mod Setup";
const APP_GIT_USER_EMAIL: &str = "hoi4-mod-setup@localhost";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HardenedGitProfile {
    Initialize,
    ReadOnly,
    Mutation,
    Online,
    Rollback,
}

impl HardenedGitProfile {
    fn requires_local_config(self) -> bool {
        self != Self::Initialize
    }

    fn timeout_seconds(self) -> u64 {
        match self {
            Self::ReadOnly => 10,
            Self::Initialize | Self::Mutation | Self::Online | Self::Rollback => 120,
        }
    }

    fn max_output_bytes(self) -> usize {
        match self {
            Self::ReadOnly => 4 * 1024 * 1024,
            Self::Initialize | Self::Mutation | Self::Online | Self::Rollback => 2 * 1024 * 1024,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
struct GitOnlineRecord {
    schema_version: &'static str,
    action: OnlineGitAction,
    recorded_at: String,
    branch: String,
    remote_name: String,
    repository: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    repository_url: Option<String>,
    message: String,
}

pub const MANAGED_GITIGNORE_BLOCK: &str = "# BEGIN HOI4 Mod Setup managed rules\n.tmp/\n.agents/\n.codex/\n.tools/\nparadox_wiki/\nAGENTS.md\n.hoi4-mod-setup/backups/\n.hoi4-mod-setup/cache/\n# END HOI4 Mod Setup managed rules";

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
        validate_remote_name(remote)?;
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

fn validate_remote_name(value: &str) -> Result<(), AppError> {
    if value.is_empty()
        || value.len() > 64
        || value.starts_with('-')
        || value.chars().any(|character| character.is_whitespace())
    {
        return Err(AppError::InvalidInput("invalid Git remote name".into()));
    }
    Ok(())
}

pub fn validate_github_repository(value: &str) -> Result<(), AppError> {
    if value.is_empty() || value.len() > 200 || value.contains('\n') || value.contains('\r') {
        return Err(AppError::InvalidInput(
            "GitHub repository name is invalid".into(),
        ));
    }
    let parts: Vec<&str> = value.split('/').collect();
    if parts.len() > 2 || parts.iter().any(|part| part.is_empty()) {
        return Err(AppError::InvalidInput(
            "GitHub repository name is invalid".into(),
        ));
    }
    if parts.iter().any(|part| {
        part.starts_with('-')
            || part.starts_with('.')
            || part.ends_with('-')
            || part.ends_with('.')
            || *part == "."
            || *part == ".."
            || part.chars().any(|character| {
                !(character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.'))
            })
    }) {
        return Err(AppError::InvalidInput(
            "GitHub repository name is invalid".into(),
        ));
    }
    Ok(())
}

fn pending_online_plans() -> &'static Mutex<HashMap<Uuid, PendingGitOnlinePlan>> {
    PENDING_GIT_ONLINE_PLANS.get_or_init(|| Mutex::new(HashMap::new()))
}

pub fn prepare_online_action(
    root: &Path,
    action: OnlineGitAction,
    remote_name: &str,
    repository: &str,
    branch: &str,
) -> Result<GitOnlinePlan, AppError> {
    let status = read_git_head(root);
    if !status.repository_present {
        return Err(AppError::Transaction(
            "a Git repository is required before an online action".into(),
        ));
    }
    let inspection = inspect_read_only(root);
    if inspection.status_probe != "complete" {
        return Err(AppError::Transaction(
            "Git could not be inspected completely; online actions are unavailable".into(),
        ));
    }
    if inspection.status.detached || inspection.status.branch.as_deref() != Some(branch.trim()) {
        return Err(AppError::Transaction(
            "online actions require the selected named branch".into(),
        ));
    }
    if inspection.status.dirty != Some(false) {
        return Err(AppError::Transaction(
            "commit or save the project changes before an online action".into(),
        ));
    }
    if !inspection.submodules.is_empty() {
        return Err(AppError::Transaction(
            "online actions are unavailable while submodules are present".into(),
        ));
    }
    if !inspection.hooks.is_empty() {
        return Err(AppError::Transaction(
            "online actions are unavailable while Git hooks are present".into(),
        ));
    }
    let branch = branch.trim();
    if !valid_branch_name(branch) {
        return Err(AppError::InvalidInput("invalid Git branch name".into()));
    }
    let remote_name = remote_name.trim();
    validate_remote_name(remote_name)?;
    let head_output = run_git_read_only(root, &["rev-parse", "--verify", "HEAD"])?;
    require_success(head_output.clone(), "Git commit check")?;
    let head_sha = head_output
        .stdout
        .lines()
        .map(str::trim)
        .find(|value| valid_commit_id(value))
        .ok_or_else(|| AppError::Transaction("Git did not return an exact HEAD commit".into()))?
        .to_ascii_lowercase();
    validate_online_git_configuration(root)?;
    let (remote_url, gh_executable_sha256) = match action {
        OnlineGitAction::PushRemote => (Some(configured_push_url(root, remote_name)?), None),
        OnlineGitAction::CreatePublicGithub => {
            validate_github_repository(repository.trim())?;
            if remote_exists(root, remote_name)? {
                return Err(AppError::Transaction(format!(
                    "Git remote {remote_name} already exists; choose another remote name before creating a public repository"
                )));
            }
            let gh = find_github_cli()?;
            (None, Some(crate::security::sha256_file(&gh)?))
        }
    };
    let git = find_git_executable()?;
    let git_executable_sha256 = crate::security::sha256_file(&git)?;
    let reviewed_at = Utc::now();
    let plan = GitOnlinePlan {
        plan_id: Uuid::new_v4(),
        action,
        branch: branch.into(),
        remote_name: remote_name.into(),
        repository: repository.trim().into(),
        head_sha,
        reviewed_at: reviewed_at.to_rfc3339(),
        expires_at: (reviewed_at + ChronoDuration::minutes(10)).to_rfc3339(),
        git_executable_sha256,
        remote_url,
        gh_executable_sha256,
    };
    let mut pending = pending_online_plans()
        .lock()
        .map_err(|_| AppError::Process("online Git review store is unavailable".into()))?;
    if pending.len() >= 32 {
        if let Some(oldest) = pending.keys().next().copied() {
            pending.remove(&oldest);
        }
    }
    pending.insert(
        plan.plan_id,
        PendingGitOnlinePlan {
            plan: plan.clone(),
            root: root.to_path_buf(),
        },
    );
    Ok(plan)
}

pub fn execute_online_action(
    root: &Path,
    plan_id: Uuid,
    confirmed: bool,
) -> Result<GitOnlineResult, AppError> {
    if !confirmed {
        return Err(AppError::InvalidInput(
            "online Git action requires separate approval".into(),
        ));
    }
    let pending = pending_online_plans()
        .lock()
        .map_err(|_| AppError::Process("online Git review store is unavailable".into()))?
        .get(&plan_id)
        .cloned()
        .ok_or_else(|| {
            AppError::Transaction("the online Git review has expired; review it again".into())
        })?;
    if pending.root != root {
        return Err(AppError::PathSecurity(
            "online Git review belongs to a different project root".into(),
        ));
    }
    let expires_at =
        chrono::DateTime::parse_from_rfc3339(&pending.plan.expires_at).map_err(|_| {
            AppError::Transaction("the online Git review has invalid expiry evidence".into())
        })?;
    if Utc::now() >= expires_at.with_timezone(&Utc) {
        pending_online_plans()
            .lock()
            .map_err(|_| AppError::Process("online Git review store is unavailable".into()))?
            .remove(&plan_id);
        return Err(AppError::Transaction(
            "the online Git review has expired; review it again".into(),
        ));
    }
    let git = find_git_executable()?;
    if crate::security::sha256_file(&git)? != pending.plan.git_executable_sha256 {
        return Err(AppError::Process(
            "the Git executable changed after review; prepare the online action again".into(),
        ));
    }
    let current = inspect_read_only(root);
    if current.status_probe != "complete"
        || current.status.dirty != Some(false)
        || current.status.detached
        || current.status.branch.as_deref() != Some(pending.plan.branch.as_str())
    {
        return Err(AppError::Transaction(
            "the project changed after review; prepare the online action again".into(),
        ));
    }
    let head_output = run_git_read_only(root, &["rev-parse", "--verify", "HEAD"])?;
    require_success(head_output.clone(), "Git commit check")?;
    let current_head = head_output
        .stdout
        .lines()
        .map(str::trim)
        .find(|value| valid_commit_id(value))
        .ok_or_else(|| AppError::Transaction("Git did not return an exact HEAD commit".into()))?;
    if !current_head.eq_ignore_ascii_case(&pending.plan.head_sha) {
        return Err(AppError::Transaction(
            "the project HEAD changed after review; prepare the online action again".into(),
        ));
    }
    match pending.plan.action {
        OnlineGitAction::PushRemote => {
            validate_online_git_configuration(root)?;
            if configured_push_url(root, &pending.plan.remote_name)?
                != pending.plan.remote_url.clone().unwrap_or_default()
            {
                return Err(AppError::Transaction(
                    "the configured Git remote changed after review; prepare the online action again".into(),
                ));
            }
        }
        OnlineGitAction::CreatePublicGithub => {
            let gh = find_github_cli()?;
            let hash = crate::security::sha256_file(&gh)?;
            if pending.plan.gh_executable_sha256.as_deref() != Some(hash.as_str()) {
                return Err(AppError::Process(
                    "the GitHub CLI changed after review; prepare the online action again".into(),
                ));
            }
        }
    }
    let result = run_online_action(
        root,
        pending.plan.action,
        &pending.plan.remote_name,
        &pending.plan.repository,
        &pending.plan.branch,
        true,
    )?;
    pending_online_plans()
        .lock()
        .map_err(|_| AppError::Process("online Git review store is unavailable".into()))?
        .remove(&plan_id);
    Ok(result)
}

fn run_online_action(
    root: &Path,
    action: OnlineGitAction,
    remote_name: &str,
    repository: &str,
    branch: &str,
    confirmed: bool,
) -> Result<GitOnlineResult, AppError> {
    if !confirmed {
        return Err(AppError::InvalidInput(
            "online Git action requires separate approval".into(),
        ));
    }
    if !read_git_head(root).repository_present {
        return Err(AppError::Transaction(
            "a Git repository is required before an online action".into(),
        ));
    }
    let branch = branch.trim();
    if !valid_branch_name(branch) {
        return Err(AppError::InvalidInput("invalid Git branch name".into()));
    }
    let remote_name = remote_name.trim();
    validate_remote_name(remote_name)?;

    match action {
        OnlineGitAction::PushRemote => {
            validate_online_git_configuration(root)?;
            let remote_url = configured_push_url(root, remote_name)?;
            let output = run_git_online(root, &["push", "--set-upstream", remote_name, branch])?;
            require_success(output, "Git push")?;
            Ok(GitOnlineResult {
                action,
                branch: branch.into(),
                remote_name: remote_name.into(),
                repository: remote_url.clone(),
                repository_url: github_repository_url(&remote_url),
                message: "Changes pushed to the configured remote.".into(),
            })
        }
        OnlineGitAction::CreatePublicGithub => {
            validate_github_repository(repository.trim())?;
            if remote_exists(root, remote_name)? {
                return Err(AppError::Transaction(format!(
                    "Git remote {remote_name} already exists; choose another remote name before creating a public repository"
                )));
            }
            let head = run_git_read_only(root, &["rev-parse", "--verify", "HEAD"])?;
            require_success(head, "Git commit check")?;
            let gh = find_github_cli()?;
            let auth = run_reviewed_tool(
                gh.clone(),
                vec![
                    "auth".into(),
                    "status".into(),
                    "--hostname".into(),
                    "github.com".into(),
                ],
                root,
                30,
                1024 * 1024,
            )?;
            if auth.status_code != Some(0) || auth.timed_out {
                return Err(AppError::Process(
                    "GitHub sign-in is unavailable. Sign in with GitHub CLI and try again.".into(),
                ));
            }
            let args = vec![
                "repo".into(),
                "create".into(),
                repository.trim().into(),
                "--public".into(),
            ];
            let output = run_reviewed_tool(gh, args, root, 300, 2 * 1024 * 1024)?;
            require_success(output.clone(), "GitHub publication")?;
            let repository_url = github_url_from_output(&output.stdout).or_else(|| {
                repository
                    .trim()
                    .contains('/')
                    .then(|| format!("https://github.com/{}", repository.trim()))
            });
            let remote_url = repository_url
                .as_deref()
                .map(|url| format!("{}.git", url.strip_suffix(".git").unwrap_or(url)))
                .ok_or_else(|| {
                    AppError::Process(
                        "GitHub created the repository but did not return its reviewed URL; configure the local remote manually"
                            .into(),
                    )
                })?;
            validate_remote_url(&remote_url)?;
            run_git(root, &["remote", "add", remote_name, remote_url.as_str()])?;
            Ok(GitOnlineResult {
                action,
                branch: branch.into(),
                remote_name: remote_name.into(),
                repository: repository.trim().into(),
                repository_url,
                message: "Public GitHub repository created. Review it, then approve the separate push action.".into(),
            })
        }
    }
}

fn find_github_cli() -> Result<PathBuf, AppError> {
    if cfg!(target_os = "windows") {
        find_path_executable(&["gh.exe", "gh"])
    } else {
        find_path_executable(&["gh", "gh.exe"])
    }
}

fn run_reviewed_tool(
    executable: PathBuf,
    args: Vec<String>,
    root: &Path,
    timeout_seconds: u64,
    max_output_bytes: usize,
) -> Result<ProcessResult, AppError> {
    let spec = ProcessSpec {
        executable: executable.clone(),
        executable_sha256: Some(crate::security::sha256_file(&executable)?),
        args,
        cwd: Some(root.to_path_buf()),
        platform: Platform::current(),
        environment_names: vec![],
        timeout_seconds,
        max_output_bytes,
    };
    spec.run(&[executable], None)
}

fn require_success(output: ProcessResult, operation: &str) -> Result<ProcessResult, AppError> {
    if output.status_code == Some(0) && !output.timed_out {
        Ok(output)
    } else {
        Err(AppError::Process(format!(
            "{operation} did not complete; check the configured Git credentials and try again"
        )))
    }
}

fn configured_push_url(root: &Path, remote_name: &str) -> Result<String, AppError> {
    let output = run_git_read_only(root, &["remote", "get-url", "--push", remote_name])?;
    if output.status_code != Some(0) || output.timed_out {
        return Err(AppError::Transaction(format!(
            "Git push remote {remote_name} is not configured"
        )));
    }
    let value = output.stdout.trim().to_string();
    validate_remote_url(&value)?;
    Ok(value)
}

fn validate_online_git_configuration(root: &Path) -> Result<(), AppError> {
    for (key, label) in [
        ("core.sshCommand", "core.sshCommand"),
        ("core.gitProxy", "core.gitProxy"),
        ("core.hooksPath", "core.hooksPath"),
    ] {
        let output = run_git_read_only(
            root,
            &["config", "--local", "--no-includes", "--get-all", key],
        )?;
        if output.timed_out {
            return Err(AppError::Process(format!(
                "Git configuration check for {label} timed out"
            )));
        }
        if output.status_code == Some(0) && !output.stdout.trim().is_empty() {
            return Err(AppError::Transaction(format!(
                "online actions are unavailable while Git {label} is configured"
            )));
        }
        if output.status_code != Some(0) && output.status_code != Some(1) {
            return Err(AppError::Process(format!(
                "Git configuration check for {label} failed"
            )));
        }
    }
    let rewrites = run_git_read_only(
        root,
        &[
            "config",
            "--local",
            "--no-includes",
            "--get-regexp",
            r"^url\..*\.insteadof$",
        ],
    )?;
    if rewrites.timed_out {
        return Err(AppError::Process(
            "Git URL rewrite configuration check timed out".into(),
        ));
    }
    if rewrites.status_code == Some(0) && !rewrites.stdout.trim().is_empty() {
        return Err(AppError::Transaction(
            "online actions are unavailable while Git URL rewriting is configured".into(),
        ));
    }
    if rewrites.status_code != Some(0) && rewrites.status_code != Some(1) {
        return Err(AppError::Process(
            "Git URL rewrite configuration check failed".into(),
        ));
    }
    Ok(())
}

fn remote_exists(root: &Path, remote_name: &str) -> Result<bool, AppError> {
    let output = run_git_read_only(root, &["remote", "get-url", remote_name])?;
    if output.timed_out {
        return Err(AppError::Process("Git remote check timed out".into()));
    }
    Ok(output.status_code == Some(0))
}

fn github_repository_url(remote: &str) -> Option<String> {
    let trimmed = remote.strip_suffix(".git").unwrap_or(remote);
    if let Some(rest) = trimmed.strip_prefix("https://github.com/") {
        return valid_github_url(rest).map(|_| trimmed.to_string());
    }
    if let Some(rest) = trimmed.strip_prefix("git@github.com:") {
        return valid_github_url(rest).map(|_| format!("https://github.com/{rest}"));
    }
    None
}

fn github_url_from_output(output: &str) -> Option<String> {
    output
        .split_whitespace()
        .map(|value| {
            value.trim_matches(|character: char| matches!(character, '(' | ')' | '[' | ']' | ','))
        })
        .find_map(|value| {
            value
                .strip_prefix("https://github.com/")
                .and_then(valid_github_url)
                .map(|_| value.to_string())
        })
}

fn valid_github_url(value: &str) -> Option<&str> {
    let parts: Vec<&str> = value.split('/').collect();
    (parts.len() == 2
        && parts.iter().all(|part| {
            !part.is_empty()
                && part.chars().all(|character| {
                    character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.')
                })
        }))
    .then_some(value)
}

pub fn write_online_action_record(
    root: &Path,
    transaction_id: Option<&str>,
    result: &GitOnlineResult,
) -> Result<(), AppError> {
    let metadata_root = safe_join(root, ".hoi4-mod-setup")?;
    fs::create_dir_all(&metadata_root)?;
    let relative = if let Some(transaction_id) = transaction_id {
        let id = Uuid::parse_str(transaction_id)
            .map_err(|_| AppError::InvalidInput("invalid transaction ID".into()))?;
        let transactions = metadata_root.join("transactions");
        fs::create_dir_all(&transactions)?;
        let directory = transactions.join(id.to_string());
        fs::create_dir_all(&directory)?;
        format!(".hoi4-mod-setup/transactions/{id}/online-git.json")
    } else {
        ".hoi4-mod-setup/online-git.json".into()
    };
    let path = safe_join(root, &relative)?;
    let record = GitOnlineRecord {
        schema_version: "1.0.0",
        action: result.action,
        recorded_at: Utc::now().to_rfc3339(),
        branch: result.branch.clone(),
        remote_name: result.remote_name.clone(),
        repository: result.repository.clone(),
        repository_url: result.repository_url.clone(),
        message: result.message.clone(),
    };
    atomic_write_json(&path, &record)
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
    const BEGIN: &str = "# BEGIN HOI4 Mod Setup managed rules";
    const END: &str = "# END HOI4 Mod Setup managed rules";
    if let Some(start) = existing.find(BEGIN) {
        if let Some(relative_end) = existing[start..].find(END) {
            let end = start + relative_end + END.len();
            let mut updated = String::with_capacity(existing.len() + MANAGED_GITIGNORE_BLOCK.len());
            updated.push_str(existing[..start].trim_end());
            if !updated.is_empty() {
                updated.push_str("\n\n");
            }
            updated.push_str(MANAGED_GITIGNORE_BLOCK);
            let suffix = existing[end..].trim_start();
            if !suffix.is_empty() {
                updated.push_str("\n\n");
                updated.push_str(suffix);
            }
            return updated;
        }
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
            run_git_initialize(root, &initial_branch)?;
            result.initialized = true;
            if !managed_paths.is_empty() {
                let mut args = vec!["add", "--"];
                args.extend(managed_paths.iter().map(String::as_str));
                run_git(root, &args)?;
            }
            if setup.initial_commit {
                run_git_initial_commit(root)?;
                result.committed = true;
            }
        }
    }
    if let (Some(name), Some(url)) = (&setup.remote_name, &setup.remote_url) {
        if setup.mode == GitMode::Preserve {
            let existing = run_git_read_only(root, &["remote", "get-url", name])?;
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
    let output = run_hardened_git(root, HardenedGitProfile::Mutation, args, &[])?;
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

fn run_git_read_only(root: &Path, args: &[&str]) -> Result<ProcessResult, AppError> {
    run_hardened_git(root, HardenedGitProfile::ReadOnly, args, &[])
}

fn run_git_initialize(root: &Path, initial_branch: &str) -> Result<(), AppError> {
    let output = run_hardened_git(
        root,
        HardenedGitProfile::Initialize,
        &["init", initial_branch],
        &[],
    )?;
    if output.status_code == Some(0) {
        Ok(())
    } else {
        Err(AppError::Process(format!(
            "git init failed: {}",
            output.stderr.trim()
        )))
    }
}

fn run_git_initial_commit(root: &Path) -> Result<(), AppError> {
    let output = run_hardened_git(
        root,
        HardenedGitProfile::Mutation,
        &["commit", "-m", "Initialize HOI4 Mod Setup project"],
        &[
            ("user.name", APP_GIT_USER_NAME),
            ("user.email", APP_GIT_USER_EMAIL),
            ("user.useConfigOnly", "true"),
        ],
    )?;
    if output.status_code == Some(0) {
        Ok(())
    } else {
        Err(AppError::Process(format!(
            "git commit failed: {}",
            output.stderr.trim()
        )))
    }
}

fn run_git_online(root: &Path, args: &[&str]) -> Result<ProcessResult, AppError> {
    run_hardened_git(root, HardenedGitProfile::Online, args, &[])
}

fn run_git_rollback(root: &Path, args: &[&str]) -> Result<ProcessResult, AppError> {
    run_hardened_git(root, HardenedGitProfile::Rollback, args, &[])
}

fn run_hardened_git(
    root: &Path,
    profile: HardenedGitProfile,
    args: &[&str],
    command_config: &[(&str, &str)],
) -> Result<ProcessResult, AppError> {
    let (spec, executable) =
        prepare_hardened_git_process(root, profile, args, command_config, find_git_executable)?;
    spec.run_git_read_only(&[executable])
}

fn prepare_hardened_git_process<F>(
    root: &Path,
    profile: HardenedGitProfile,
    args: &[&str],
    command_config: &[(&str, &str)],
    resolve_executable: F,
) -> Result<(ProcessSpec, PathBuf), AppError>
where
    F: FnOnce() -> Result<PathBuf, AppError>,
{
    if args.is_empty() || args[0].starts_with('-') {
        return Err(AppError::Process(
            "app-owned Git command is missing its fixed operation".into(),
        ));
    }
    if profile == HardenedGitProfile::Initialize && args[0] != "init" {
        return Err(AppError::Process(
            "the Git initialization profile accepts only git init".into(),
        ));
    }
    if profile != HardenedGitProfile::Initialize && args[0] == "init" {
        return Err(AppError::Process(
            "git init requires the isolated initialization profile".into(),
        ));
    }
    if profile == HardenedGitProfile::Initialize {
        let git_path = root.join(".git");
        match fs::symlink_metadata(&git_path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Ok(_) => {
                return Err(AppError::PathSecurity(
                    "the isolated Git initialization profile requires absent .git metadata".into(),
                ))
            }
            Err(error) => {
                return Err(AppError::PathSecurity(format!(
                    "Git initialization metadata cannot be checked safely: {error}"
                )))
            }
        }
    } else if profile.requires_local_config() {
        validate_local_git_config_for_execution(root)?;
    }

    let executable = resolve_executable()?;
    let spec = ProcessSpec {
        executable: executable.clone(),
        executable_sha256: Some(crate::security::sha256_file(&executable)?),
        args: hardened_git_arguments(profile, args, command_config),
        cwd: Some(root.to_path_buf()),
        platform: Platform::current(),
        environment_names: vec![],
        timeout_seconds: profile.timeout_seconds(),
        max_output_bytes: profile.max_output_bytes(),
    };
    Ok((spec, executable))
}

fn hardened_git_arguments(
    profile: HardenedGitProfile,
    args: &[&str],
    command_config: &[(&str, &str)],
) -> Vec<String> {
    let null_path = git_null_path();
    let mut hardened = vec!["--no-optional-locks".into(), "--no-pager".into()];
    for (key, value) in command_config {
        hardened.push("-c".into());
        hardened.push(format!("{key}={value}"));
    }
    for (key, value) in [
        ("core.hooksPath", null_path),
        ("init.templateDir", null_path),
        ("core.fsmonitor", "false"),
        ("core.untrackedCache", "false"),
        ("core.attributesFile", null_path),
        ("core.excludesFile", null_path),
        ("core.gitProxy", ""),
        ("credential.helper", ""),
        ("diff.external", ""),
        ("commit.gpgSign", "false"),
        ("tag.gpgSign", "false"),
        ("gc.auto", "0"),
        ("maintenance.auto", "false"),
        ("fetch.writeCommitGraph", "false"),
        ("fetch.recurseSubmodules", "false"),
        ("push.recurseSubmodules", "no"),
        ("submodule.recurse", "false"),
        ("protocol.ext.allow", "never"),
        ("protocol.file.allow", "never"),
    ] {
        hardened.push("-c".into());
        hardened.push(format!("{key}={value}"));
    }

    if profile == HardenedGitProfile::Initialize {
        hardened.push("init".into());
        hardened.push(format!("--template={null_path}"));
        hardened.extend(args[1..].iter().map(|value| (*value).to_string()));
    } else {
        hardened.extend(args.iter().map(|value| (*value).to_string()));
    }
    hardened
}

fn git_null_path() -> &'static str {
    if cfg!(target_os = "windows") {
        "NUL"
    } else {
        "/dev/null"
    }
}

fn validate_local_git_config_for_execution(root: &Path) -> Result<(), AppError> {
    let config_path = root.join(".git").join("config");
    let metadata = fs::symlink_metadata(&config_path).map_err(|error| {
        AppError::PathSecurity(format!(
            "Git metadata cannot be inspected safely at {}: {error}",
            config_path.display()
        ))
    })?;
    if !metadata.is_file()
        || is_link_metadata(&metadata)
        || metadata.len() > MAX_GIT_METADATA_TEXT_BYTES
        || path_has_link_component(&config_path)
    {
        return Err(AppError::PathSecurity(
            "Git config is linked, oversized, or not a regular file".into(),
        ));
    }
    let config = fs::read_to_string(&config_path)
        .map_err(|error| AppError::PathSecurity(format!("read Git config safely: {error}")))?;
    match unsafe_git_config_entry(&config) {
        Ok(Some(entry)) => {
            return Err(AppError::PathSecurity(format!(
                "Git config contains an executable or externally resolved setting: {entry}"
            )))
        }
        Ok(None) => {}
        Err(error) => {
            return Err(AppError::PathSecurity(format!(
                "Git config is malformed or unsupported: {error}"
            )))
        }
    }
    Ok(())
}

fn unsafe_git_config_entry(config: &str) -> Result<Option<String>, String> {
    if config.contains('\0') {
        return Err("NUL bytes are not allowed".into());
    }
    let mut section = None::<String>;
    for (index, raw_line) in config.lines().enumerate() {
        let line_number = index + 1;
        if raw_line
            .chars()
            .any(|character| character.is_control() && character != '\t')
        {
            return Err(format!("line {line_number} contains a control character"));
        }
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        if line.starts_with('[') {
            let parsed = parse_git_config_section(line, line_number)?;
            if unsafe_git_config_section(&parsed) {
                return Ok(Some(format!("[{parsed}]")));
            }
            section = Some(parsed);
            continue;
        }
        let current_section = section
            .as_deref()
            .ok_or_else(|| format!("line {line_number} appears before any section"))?;
        let (key, value) = parse_git_config_assignment(line, line_number)?;
        let unsafe_key = match current_section {
            "core" => matches!(
                key.as_str(),
                "alternaterefscommand"
                    | "askpass"
                    | "attributesfile"
                    | "editor"
                    | "excludesfile"
                    | "fsmonitor"
                    | "gitproxy"
                    | "hookspath"
                    | "pager"
                    | "sshcommand"
                    | "worktree"
            ),
            "extensions" => key == "worktreeconfig",
            "interactive" => key == "difffilter",
            "remote" => matches!(
                key.as_str(),
                "mirror" | "proxy" | "receivepack" | "uploadpack" | "vcs"
            ),
            "sequence" => key == "editor",
            "submodule" => key == "update",
            "uploadpack" => key == "packobjectshook",
            "commit" | "tag" => key == "gpgsign",
            "web" => key == "browser",
            _ => false,
        };
        if unsafe_key {
            let value_marker = value
                .as_deref()
                .filter(|value| !value.is_empty())
                .map(|_| "=...")
                .unwrap_or_default();
            return Ok(Some(format!("{current_section}.{key}{value_marker}")));
        }
    }
    Ok(None)
}

fn parse_git_config_section(line: &str, line_number: usize) -> Result<String, String> {
    let mut quoted = false;
    let mut closing = None;
    for (offset, character) in line.char_indices().skip(1) {
        match character {
            '\\' => return Err(format!("line {line_number} uses an escaped section header")),
            '"' => quoted = !quoted,
            ']' if !quoted => {
                closing = Some(offset);
                break;
            }
            _ => {}
        }
    }
    let closing =
        closing.ok_or_else(|| format!("line {line_number} has an unterminated section"))?;
    let trailing = line[closing + 1..].trim();
    if !trailing.is_empty() && !trailing.starts_with('#') && !trailing.starts_with(';') {
        return Err(format!(
            "line {line_number} has content after its section header"
        ));
    }

    let header = line[1..closing].trim();
    if header.is_empty() {
        return Err(format!("line {line_number} has an empty section"));
    }
    let (section, subsection) =
        if let Some(whitespace) = header.find(|character: char| character.is_ascii_whitespace()) {
            let section = &header[..whitespace];
            let subsection = header[whitespace..].trim();
            if subsection.len() < 2
                || !subsection.starts_with('"')
                || !subsection.ends_with('"')
                || subsection[1..subsection.len() - 1].contains('"')
            {
                return Err(format!(
                    "line {line_number} has a malformed quoted subsection"
                ));
            }
            (section, Some(&subsection[1..subsection.len() - 1]))
        } else if let Some((section, subsection)) = header.split_once('.') {
            if subsection.is_empty()
                || !subsection.chars().all(|character| {
                    character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.' | '/')
                })
            {
                return Err(format!(
                    "line {line_number} has a malformed legacy subsection"
                ));
            }
            (section, Some(subsection))
        } else {
            (header, None)
        };
    if !valid_git_config_section_name(section) {
        return Err(format!("line {line_number} has an invalid section name"));
    }
    if subsection.is_some_and(|subsection| {
        subsection
            .chars()
            .any(|character| character.is_control() || character == '\\')
    }) {
        return Err(format!("line {line_number} has an unsafe subsection name"));
    }
    Ok(section.to_ascii_lowercase())
}

fn parse_git_config_assignment(
    line: &str,
    line_number: usize,
) -> Result<(String, Option<String>), String> {
    let key_end = line
        .find(|character: char| character.is_ascii_whitespace() || character == '=')
        .unwrap_or(line.len());
    let key = &line[..key_end];
    if !valid_git_config_key_name(key) {
        return Err(format!("line {line_number} has an invalid variable name"));
    }
    let mut remainder = line[key_end..].trim_start();
    if let Some(value) = remainder.strip_prefix('=') {
        remainder = value.trim_start();
    }
    validate_git_config_value(remainder, line_number)?;
    let value = (!remainder.is_empty()).then(|| remainder.to_string());
    Ok((key.to_ascii_lowercase(), value))
}

fn validate_git_config_value(value: &str, line_number: usize) -> Result<(), String> {
    let mut quoted = false;
    let mut escaped = false;
    for character in value.chars() {
        if escaped {
            if !matches!(character, '\\' | '"' | 'n' | 't' | 'b') {
                return Err(format!(
                    "line {line_number} contains an invalid value escape"
                ));
            }
            escaped = false;
            continue;
        }
        match character {
            '\\' => escaped = true,
            '"' => quoted = !quoted,
            '#' | ';' if !quoted => break,
            character if character.is_control() && character != '\t' => {
                return Err(format!("line {line_number} contains a control character"))
            }
            _ => {}
        }
    }
    if escaped {
        return Err(format!(
            "line {line_number} uses a continuation or incomplete escape"
        ));
    }
    if quoted {
        return Err(format!("line {line_number} has an unterminated quote"));
    }
    Ok(())
}

fn valid_git_config_section_name(value: &str) -> bool {
    !value.is_empty()
        && value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '-')
}

fn valid_git_config_key_name(value: &str) -> bool {
    value
        .chars()
        .next()
        .is_some_and(|character| character.is_ascii_alphabetic())
        && value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '-')
}

fn unsafe_git_config_section(section: &str) -> bool {
    matches!(
        section,
        "alias"
            | "browser"
            | "credential"
            | "diff"
            | "difftool"
            | "filter"
            | "gpg"
            | "gui"
            | "help"
            | "http"
            | "https"
            | "include"
            | "includeif"
            | "instaweb"
            | "man"
            | "merge"
            | "mergetool"
            | "pager"
            | "protocol"
            | "sendemail"
            | "tar"
            | "url"
    )
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
    let hooks = inspect_hook_names(&git.join("hooks"))?;
    if !hooks.is_empty() {
        return Err(AppError::Transaction(
            "Git hooks were added after initialization; refusing to remove Git metadata".into(),
        ));
    }
    let output = run_git_rollback(root, &["status", "--porcelain"])?;
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
    let current = run_git_rollback(root, &["remote", "get-url", name])?;
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
    let output = run_git_rollback(root, &["remote", "remove", name])?;
    if output.status_code == Some(0) {
        Ok(())
    } else {
        Err(AppError::Process(format!(
            "git remote remove failed: {}",
            output.stderr.trim()
        )))
    }
}

pub fn validate_remote_url(value: &str) -> Result<(), AppError> {
    let valid = if value.trim() != value
        || value.is_empty()
        || value.chars().any(|character| {
            character.is_control()
                || character.is_whitespace()
                || matches!(character, '\\' | '"' | '\'')
        })
        || value
            .split('/')
            .any(|segment| segment == "." || segment == "..")
    {
        false
    } else if let Some(stripped) = value.strip_prefix("git@") {
        let Some((host, path)) = stripped.split_once(':') else {
            return Err(AppError::InvalidInput(
                "remote URL must be an explicit HTTPS or SSH URL".into(),
            ));
        };
        !host.is_empty()
            && host.chars().all(|character| {
                character.is_ascii_alphanumeric() || matches!(character, '.' | '-' | '_')
            })
            && valid_remote_path(path)
    } else if value.starts_with("https://") || value.starts_with("ssh://") {
        let Ok(parsed) = reqwest::Url::parse(value) else {
            return Err(AppError::InvalidInput(
                "remote URL must be an explicit HTTPS or SSH URL".into(),
            ));
        };
        let expected_scheme = if value.starts_with("https://") {
            "https"
        } else {
            "ssh"
        };
        parsed.scheme() == expected_scheme
            && parsed.host_str().is_some_and(|host| !host.is_empty())
            && parsed.password().is_none()
            && (parsed.username().is_empty() || parsed.username() == "git")
            && parsed.query().is_none()
            && parsed.fragment().is_none()
            && valid_remote_path(parsed.path())
    } else {
        false
    };
    if valid {
        Ok(())
    } else {
        Err(AppError::InvalidInput(
            "remote URL must be an explicit HTTPS or SSH URL".into(),
        ))
    }
}

fn valid_remote_path(path: &str) -> bool {
    let path = path.trim_start_matches('/');
    !path.is_empty()
        && !path
            .split('/')
            .any(|segment| segment.is_empty() || segment == "." || segment == "..")
        && !path.contains('?')
        && !path.contains('#')
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
    if validate_local_git_config_for_execution(root).is_err() {
        inspection.status_probe = "unsafe_configuration".into();
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
            "--ignore-submodules=all",
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

    match read_submodule_paths(root) {
        Ok(paths) => inspection.submodules = paths,
        Err(_) => complete = false,
    }

    let hooks_safe = match inspect_hook_names(&git_path.join("hooks")) {
        Ok(hooks) => {
            inspection.hooks = hooks;
            true
        }
        Err(_) => {
            complete = false;
            false
        }
    };
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
    inspection.status_probe = if !hooks_safe {
        "unsafe_hooks".into()
    } else if complete {
        "complete".into()
    } else {
        "partial".into()
    };
    inspection
}

fn read_submodule_paths(root: &Path) -> Result<Vec<String>, AppError> {
    let path = root.join(".gitmodules");
    let metadata = match fs::symlink_metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => {
            return Err(AppError::PathSecurity(format!(
                "inspect .gitmodules metadata: {error}"
            )))
        }
    };
    if !metadata.is_file()
        || is_link_metadata(&metadata)
        || metadata.len() > MAX_GIT_METADATA_TEXT_BYTES
        || path_has_link_component(&path)
    {
        return Err(AppError::PathSecurity(
            ".gitmodules is linked, oversized, or not a regular file".into(),
        ));
    }
    let contents = fs::read_to_string(&path)
        .map_err(|error| AppError::PathSecurity(format!("read .gitmodules safely: {error}")))?;
    let mut paths = contents
        .lines()
        .map(str::trim)
        .filter_map(|line| {
            let (key, value) = line.split_once('=')?;
            key.trim()
                .eq_ignore_ascii_case("path")
                .then_some(value.trim())
        })
        .map(normalize_relative_path)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| AppError::PathSecurity(format!("invalid submodule path: {error}")))?;
    paths.sort();
    paths.dedup();
    Ok(paths)
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

fn inspect_hook_names(hooks: &Path) -> Result<Vec<String>, AppError> {
    let metadata = match fs::symlink_metadata(hooks) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => {
            return Err(AppError::PathSecurity(format!(
                "Git hooks cannot be inspected safely: {error}"
            )))
        }
    };
    if !metadata.is_dir() || is_link_metadata(&metadata) || path_has_link_component(hooks) {
        return Err(AppError::PathSecurity(
            "Git hooks path is linked or not a readable directory".into(),
        ));
    }

    let entries = fs::read_dir(hooks).map_err(|error| {
        AppError::PathSecurity(format!(
            "Git hooks directory cannot be read safely: {error}"
        ))
    })?;
    let mut names = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|error| {
            AppError::PathSecurity(format!("Git hook entry cannot be read safely: {error}"))
        })?;
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path).map_err(|error| {
            AppError::PathSecurity(format!("Git hook metadata cannot be read safely: {error}"))
        })?;
        if is_link_metadata(&metadata) || path_has_link_component(&path) {
            return Err(AppError::PathSecurity(
                "Git hook entry contains a link or junction".into(),
            ));
        }
        if !metadata.is_file() {
            continue;
        }
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| AppError::PathSecurity("Git hook name is not valid Unicode".into()))?;
        if !name.is_empty()
            && name.len() <= 255
            && !name.ends_with(".sample")
            && !name.ends_with(".disabled")
        {
            names.push(name);
        }
    }
    names.sort();
    names.dedup();
    Ok(names)
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
        for managed in [
            ".tmp/",
            ".agents/",
            ".codex/",
            ".tools/",
            "paradox_wiki/",
            "AGENTS.md",
        ] {
            assert!(first.lines().any(|line| line == managed));
        }
        assert_eq!(first, second);
    }

    #[test]
    fn gitignore_merge_refreshes_an_older_managed_block() {
        let old = "local-rule\n\n# BEGIN HOI4 Mod Setup managed rules\n.tools/3d_pipeline/vendor/\n# END HOI4 Mod Setup managed rules\n\nother-rule";
        let merged = merge_gitignore(Some(old));
        assert!(merged.starts_with("local-rule\n\n"));
        assert!(merged.ends_with("\n\nother-rule"));
        assert!(merged.lines().any(|line| line == ".tmp/"));
        assert!(!merged.contains(".tools/3d_pipeline/vendor/"));
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
        run_git_initialize(project.path(), "--initial-branch=main").unwrap();
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

    #[test]
    fn read_only_inspection_rejects_executable_git_configuration_before_spawn() {
        let project = tempdir().unwrap();
        fs::create_dir(project.path().join(".git")).unwrap();
        fs::write(
            project.path().join(".git").join("HEAD"),
            "ref: refs/heads/main\n",
        )
        .unwrap();
        fs::write(
            project.path().join(".git").join("config"),
            "[core]\n\trepositoryformatversion = 0\n\tfsmonitor = malicious-monitor\n",
        )
        .unwrap();

        let inspection = inspect_read_only(project.path());

        assert!(inspection.status.repository_present);
        assert_eq!(inspection.status_probe, "unsafe_configuration");
        assert!(inspection.commit.is_none());
    }

    #[test]
    fn local_git_config_rejects_includes_filters_and_transport_helpers() {
        for config in [
            "[include]\npath = ../outside.conf\n",
            "[includeIf \"gitdir:../outside\"]\npath = ../outside.conf\n",
            "[filter \"danger\"]\nprocess = helper\n",
            "[FiLtEr.danger]\nprocess = helper\n",
            "[diff \"danger\"]\ntextconv = helper\n",
            "[merge \"danger\"]\ndriver = helper %O %A %B\n",
            "[credential]\nhelper = helper\n",
            "[url \"https://redirect.invalid/\"]\ninsteadOf = https://github.com/\n",
            "[core]\nhooksPath = hooks\n",
            "[core]\nalternateRefsCommand = helper\n",
            "[remote \"origin\"]\nuploadpack = helper\n",
            "[submodule \"danger\"]\nupdate = !helper\n",
            "[extensions]\nworktreeConfig = true\n",
            "[gpg \"ssh\"]\ndefaultKeyCommand = helper\n",
        ] {
            assert!(
                unsafe_git_config_entry(config).unwrap().is_some(),
                "accepted {config}"
            );
        }
        assert!(unsafe_git_config_entry(
            "# normal local repository configuration\n[core]\nrepositoryformatversion = 0\nfilemode = true\nbare\n[remote \"origin\"]\nurl = https://github.com/example/mod.git\nfetch = +refs/heads/*:refs/remotes/origin/*\n[branch \"main\"]\nremote = origin\nmerge = refs/heads/main\n[user]\nname = \"Test User\"\nemail = test@example.com\n"
        )
        .unwrap()
        .is_none());
    }

    #[test]
    fn local_git_config_rejects_malformed_headers_values_and_continuations() {
        for config in [
            "repositoryformatversion = 0\n",
            "[core\nrepositoryformatversion = 0\n",
            "[core] trailing\nrepositoryformatversion = 0\n",
            "[core \"unterminated]\nrepositoryformatversion = 0\n",
            "[core]\ninvalid.key = true\n",
            "[core]\nrepositoryformatversion = \"unterminated\n",
            "[core]\nrepositoryformatversion = value\\\n",
            "[remote \"bad\\name\"]\nurl = https://example.invalid/repo.git\n",
        ] {
            assert!(
                unsafe_git_config_entry(config).is_err(),
                "accepted malformed config {config}"
            );
        }
    }

    #[test]
    fn hostile_local_config_is_rejected_before_executable_resolution() {
        let project = tempdir().unwrap();
        fs::create_dir(project.path().join(".git")).unwrap();
        fs::write(
            project.path().join(".git").join("config"),
            "[filter.danger]\nprocess = marker-helper\n",
        )
        .unwrap();
        let resolver_called = std::cell::Cell::new(false);

        let error = prepare_hardened_git_process(
            project.path(),
            HardenedGitProfile::ReadOnly,
            &["status"],
            &[],
            || {
                resolver_called.set(true);
                Err(AppError::Process(
                    "executable resolution must not be reached".into(),
                ))
            },
        )
        .unwrap_err();

        assert!(!resolver_called.get());
        assert!(matches!(error, AppError::PathSecurity(_)));
    }

    #[test]
    fn every_app_owned_git_profile_has_explicit_hardening() {
        let project = tempdir().unwrap();
        let initialization_project = tempdir().unwrap();
        fs::create_dir(project.path().join(".git")).unwrap();
        fs::write(
            project.path().join(".git").join("config"),
            "[core]\nrepositoryformatversion = 0\nbare = false\n",
        )
        .unwrap();
        for (profile, command) in [
            (HardenedGitProfile::Initialize, "init"),
            (HardenedGitProfile::ReadOnly, "status"),
            (HardenedGitProfile::Mutation, "add"),
            (HardenedGitProfile::Online, "push"),
            (HardenedGitProfile::Rollback, "status"),
        ] {
            let root = if profile == HardenedGitProfile::Initialize {
                initialization_project.path()
            } else {
                project.path()
            };
            let (spec, _) =
                prepare_hardened_git_process(root, profile, &[command], &[], find_git_executable)
                    .unwrap();
            for setting in [
                format!("core.hooksPath={}", git_null_path()),
                format!("init.templateDir={}", git_null_path()),
                format!("core.attributesFile={}", git_null_path()),
                format!("core.excludesFile={}", git_null_path()),
                "credential.helper=".into(),
                "commit.gpgSign=false".into(),
                "protocol.ext.allow=never".into(),
                "protocol.file.allow=never".into(),
            ] {
                assert!(
                    spec.args.contains(&setting),
                    "{profile:?} omitted {setting}"
                );
            }
            assert_eq!(
                spec.args.first().map(String::as_str),
                Some("--no-optional-locks")
            );
            assert_eq!(spec.args.get(1).map(String::as_str), Some("--no-pager"));
            assert!(spec.environment_names.is_empty());
            if profile == HardenedGitProfile::Initialize {
                assert!(spec
                    .args
                    .contains(&format!("--template={}", git_null_path())));
            }
        }
    }

    #[test]
    fn invalid_hook_directory_blocks_repository_inspection() {
        let project = tempdir().unwrap();
        fs::create_dir(project.path().join(".git")).unwrap();
        fs::write(
            project.path().join(".git").join("HEAD"),
            "ref: refs/heads/main\n",
        )
        .unwrap();
        fs::write(
            project.path().join(".git").join("config"),
            "[core]\nrepositoryformatversion = 0\nbare = false\n",
        )
        .unwrap();
        fs::write(project.path().join(".git").join("hooks"), "not a directory").unwrap();

        let inspection = inspect_read_only(project.path());

        assert_eq!(inspection.status_probe, "unsafe_hooks");
        assert!(prepare_online_action(
            project.path(),
            OnlineGitAction::PushRemote,
            "origin",
            "example/mod",
            "main"
        )
        .is_err());
    }

    #[cfg(any(unix, windows))]
    #[test]
    fn linked_hook_entry_blocks_repository_inspection() {
        let project = tempdir().unwrap();
        run_git_initialize(project.path(), "--initial-branch=main").unwrap();
        let hooks = project.path().join(".git").join("hooks");
        fs::create_dir_all(&hooks).unwrap();
        let target = project.path().join("outside-hook");
        fs::write(&target, "outside").unwrap();
        let linked_hook = hooks.join("pre-push");
        #[cfg(unix)]
        let link_result = std::os::unix::fs::symlink(&target, &linked_hook);
        #[cfg(windows)]
        let link_result = std::os::windows::fs::symlink_file(&target, &linked_hook);
        if let Err(error) = link_result {
            #[cfg(windows)]
            if error.kind() == std::io::ErrorKind::PermissionDenied
                || error.raw_os_error() == Some(1314)
            {
                return;
            }
            panic!("create linked hook fixture: {error}");
        }

        let inspection = inspect_read_only(project.path());

        assert_eq!(inspection.status_probe, "unsafe_hooks");
        assert!(inspect_hook_names(&hooks).is_err());
    }

    #[test]
    fn app_owned_commit_does_not_execute_repository_hook() {
        let project = tempdir().unwrap();
        run_git_initialize(project.path(), "--initial-branch=main").unwrap();
        run_git(
            project.path(),
            &["config", "user.email", "test@example.com"],
        )
        .unwrap();
        run_git(
            project.path(),
            &["config", "user.name", "HOI4 Mod Setup Test"],
        )
        .unwrap();
        fs::write(project.path().join("README.md"), "content\n").unwrap();
        run_git(project.path(), &["add", "--", "README.md"]).unwrap();
        let hooks = project.path().join(".git").join("hooks");
        fs::create_dir_all(&hooks).unwrap();
        let hook = hooks.join("pre-commit");
        fs::write(
            &hook,
            "#!/bin/sh\nprintf 'hook ran' > hook-marker\nexit 1\n",
        )
        .unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&hook, fs::Permissions::from_mode(0o755)).unwrap();
        }

        run_git(project.path(), &["commit", "-m", "isolated commit"]).unwrap();

        assert!(!project.path().join("hook-marker").exists());
    }

    #[test]
    fn app_initial_commit_uses_explicit_identity_without_ambient_config() {
        let project = tempdir().unwrap();
        fs::write(project.path().join("README.md"), "content\n").unwrap();
        let setup = GitSetup {
            mode: GitMode::Initialize,
            branch: "main".into(),
            initial_commit: true,
            remote_name: None,
            remote_url: None,
            push_approved: false,
        };

        let result = apply_git_setup(project.path(), &setup, &["README.md".to_string()]).unwrap();
        let identity = run_git_read_only(
            project.path(),
            &["show", "-s", "--format=%an <%ae>", "HEAD"],
        )
        .unwrap();

        assert!(result.initialized);
        assert!(result.committed);
        assert_eq!(
            identity.stdout.trim(),
            format!("{APP_GIT_USER_NAME} <{APP_GIT_USER_EMAIL}>")
        );
    }

    #[test]
    fn submodule_paths_are_read_without_starting_recursive_git_processes() {
        let project = tempdir().unwrap();
        fs::write(
            project.path().join(".gitmodules"),
            "[submodule \"one\"]\n\tpath = vendor/one\n\turl = https://example.invalid/one\n[submodule \"two\"]\n\tpath = vendor/two\n",
        )
        .unwrap();

        assert_eq!(
            read_submodule_paths(project.path()).unwrap(),
            vec!["vendor/one".to_string(), "vendor/two".to_string()]
        );
    }

    #[test]
    fn remote_urls_are_explicit_and_do_not_allow_rewrites_or_credentials() {
        assert!(validate_remote_url("https://github.com/example/mod.git").is_ok());
        assert!(validate_remote_url("ssh://git@github.com/example/mod.git").is_ok());
        assert!(validate_remote_url("git@github.com:example/mod.git").is_ok());
        for value in [
            "https://user:password@github.com/example/mod.git",
            "https://github.com/example/mod.git?redirect=other",
            "https://github.com/example/../mod.git",
            "file:///tmp/mod.git",
            "https://github.com/example/mod.git\n--upload-pack=bad",
        ] {
            assert!(validate_remote_url(value).is_err(), "accepted {value}");
        }
    }

    #[test]
    fn online_review_requires_separate_approval_and_rechecks_head() {
        let project = tempdir().unwrap();
        run_git_initialize(project.path(), "--initial-branch=main").unwrap();
        run_git(
            project.path(),
            &["config", "user.email", "test@example.com"],
        )
        .unwrap();
        run_git(
            project.path(),
            &["config", "user.name", "HOI4 Mod Setup Test"],
        )
        .unwrap();
        run_git(project.path(), &["branch", "-M", "main"]).unwrap();
        fs::write(project.path().join("README.md"), "one\n").unwrap();
        run_git(project.path(), &["add", "README.md"]).unwrap();
        run_git(project.path(), &["commit", "-m", "initial"]).unwrap();
        run_git(
            project.path(),
            &[
                "remote",
                "add",
                "origin",
                "https://github.com/example/mod.git",
            ],
        )
        .unwrap();

        let plan = prepare_online_action(
            project.path(),
            OnlineGitAction::PushRemote,
            "origin",
            "example/mod",
            "main",
        )
        .unwrap();
        assert_eq!(
            plan.remote_url.as_deref(),
            Some("https://github.com/example/mod.git")
        );
        assert_eq!(plan.git_executable_sha256.len(), 64);
        assert!(
            chrono::DateTime::parse_from_rfc3339(&plan.expires_at).unwrap()
                > chrono::DateTime::parse_from_rfc3339(&plan.reviewed_at).unwrap()
        );
        assert!(execute_online_action(project.path(), plan.plan_id, false).is_err());

        fs::write(project.path().join("README.md"), "two\n").unwrap();
        run_git(project.path(), &["add", "README.md"]).unwrap();
        run_git(project.path(), &["commit", "-m", "second"]).unwrap();
        let error = execute_online_action(project.path(), plan.plan_id, true).unwrap_err();
        assert!(error.to_string().contains("HEAD changed after review"));
    }

    #[test]
    fn online_review_expires_before_any_online_action_runs() {
        let project = tempdir().unwrap();
        run_git_initialize(project.path(), "--initial-branch=main").unwrap();
        run_git(
            project.path(),
            &["config", "user.email", "test@example.com"],
        )
        .unwrap();
        run_git(
            project.path(),
            &["config", "user.name", "HOI4 Mod Setup Test"],
        )
        .unwrap();
        fs::write(project.path().join("README.md"), "one\n").unwrap();
        run_git(project.path(), &["add", "README.md"]).unwrap();
        run_git(project.path(), &["commit", "-m", "initial"]).unwrap();
        run_git(
            project.path(),
            &[
                "remote",
                "add",
                "origin",
                "https://github.com/example/mod.git",
            ],
        )
        .unwrap();

        let plan = prepare_online_action(
            project.path(),
            OnlineGitAction::PushRemote,
            "origin",
            "example/mod",
            "main",
        )
        .unwrap();
        pending_online_plans()
            .lock()
            .unwrap()
            .get_mut(&plan.plan_id)
            .unwrap()
            .plan
            .expires_at = (Utc::now() - ChronoDuration::seconds(1)).to_rfc3339();

        let error = execute_online_action(project.path(), plan.plan_id, true).unwrap_err();
        assert!(error.to_string().contains("expired"));
        assert!(!pending_online_plans()
            .lock()
            .unwrap()
            .contains_key(&plan.plan_id));
    }
}
