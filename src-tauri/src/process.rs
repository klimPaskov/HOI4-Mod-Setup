use crate::credentials::ScopedSecretEnvironment;
use crate::models::Platform;
use crate::security::{is_link_metadata, redact_secrets, validate_env_name};
use crate::AppError;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessSpec {
    pub executable: PathBuf,
    /// SHA-256 identity captured immediately before the reviewed process is
    /// launched. Manifest-backed callers must provide this; an absent value is
    /// retained only for non-secret test/diagnostic specs.
    #[serde(default)]
    pub executable_sha256: Option<String>,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub cwd: Option<PathBuf>,
    pub platform: Platform,
    #[serde(default)]
    pub environment_names: Vec<String>,
    pub timeout_seconds: u64,
    pub max_output_bytes: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessResult {
    pub status_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub timed_out: bool,
}

impl ProcessSpec {
    pub fn validate(&self, allowlisted_executables: &[PathBuf]) -> Result<(), AppError> {
        if self.executable.as_os_str().is_empty() {
            return Err(AppError::Process("executable is empty".into()));
        }
        if !self.executable.is_absolute() {
            return Err(AppError::Process(
                "process executable must be an absolute reviewed path".into(),
            ));
        }
        if let Some(expected) = self.executable_sha256.as_deref() {
            crate::source::validate_sha256(expected)?;
            if crate::security::path_has_link_component(&self.executable)
                || crate::security::sha256_file(&self.executable)? != expected
            {
                return Err(AppError::Process(
                    "process executable identity changed or contains a link".into(),
                ));
            }
        }
        if self.timeout_seconds == 0 || self.timeout_seconds > 60 * 60 {
            return Err(AppError::Process(
                "process timeout must be between one second and one hour".into(),
            ));
        }
        if self.max_output_bytes == 0 || self.max_output_bytes > 16 * 1024 * 1024 {
            return Err(AppError::Process("process output limit is invalid".into()));
        }
        if self.platform != Platform::current() {
            return Err(AppError::UnsupportedPlatform(format!(
                "process is declared for {:?}",
                self.platform
            )));
        }
        if !allowlisted_executables
            .iter()
            .any(|candidate| same_executable(candidate, &self.executable))
        {
            return Err(AppError::Process(format!(
                "executable is not allowlisted: {}",
                self.executable.display()
            )));
        }
        for name in &self.environment_names {
            validate_env_name(name)?;
        }
        if let Some(cwd) = &self.cwd {
            if !cwd.is_absolute() || !cwd.is_dir() {
                return Err(AppError::Process(
                    "process working directory must be an existing absolute directory".into(),
                ));
            }
        }
        Ok(())
    }

    pub fn preview(&self, secret_names: &[String]) -> String {
        let mut parts = vec![quote_argument(&self.executable.to_string_lossy())];
        parts.extend(self.args.iter().map(|argument| quote_argument(argument)));
        let mut preview = parts.join(" ");
        for name in secret_names {
            preview.push_str(&format!(" {name}=[REDACTED]"));
        }
        redact_secrets(&preview, &[])
    }

    pub fn run(
        &self,
        allowlisted_executables: &[PathBuf],
        environment: Option<&ScopedSecretEnvironment>,
    ) -> Result<ProcessResult, AppError> {
        self.validate(allowlisted_executables)?;
        if let Some(environment) = environment {
            let expected: std::collections::HashSet<&str> =
                self.environment_names.iter().map(String::as_str).collect();
            if environment
                .values()
                .keys()
                .any(|key| !expected.contains(key.as_str()))
            {
                return Err(AppError::Process(
                    "credential environment is broader than the declared process scope".into(),
                ));
            }
        }
        let mut command = Command::new(&self.executable);
        command
            .args(&self.args)
            .env_clear()
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        add_safe_environment(&mut command);
        if let Some(cwd) = &self.cwd {
            command.current_dir(cwd);
        }
        if let Some(environment) = environment {
            command.envs(environment.values());
        }
        let mut child = command
            .spawn()
            .map_err(|error| AppError::Process(format!("spawn failed: {error}")))?;
        let stdout_pipe = child
            .stdout
            .take()
            .ok_or_else(|| AppError::Process("stdout pipe was not created".into()))?;
        let stderr_pipe = child
            .stderr
            .take()
            .ok_or_else(|| AppError::Process("stderr pipe was not created".into()))?;
        let output_limit = self.max_output_bytes;
        let stdout_thread = std::thread::spawn(move || read_bounded(stdout_pipe, output_limit));
        let stderr_thread = std::thread::spawn(move || read_bounded(stderr_pipe, output_limit));
        let started = Instant::now();
        let mut timed_out = false;
        loop {
            if let Some(_status) = child
                .try_wait()
                .map_err(|error| AppError::Process(error.to_string()))?
            {
                break;
            }
            if started.elapsed() >= Duration::from_secs(self.timeout_seconds) {
                timed_out = true;
                terminate_process_tree(&mut child);
                break;
            }
            std::thread::sleep(Duration::from_millis(25));
        }
        let status = child
            .wait()
            .map_err(|error| AppError::Process(format!("collect output failed: {error}")))?;
        let stdout_bytes = stdout_thread
            .join()
            .map_err(|_| AppError::Process("stdout reader failed".into()))??;
        let stderr_bytes = stderr_thread
            .join()
            .map_err(|_| AppError::Process("stderr reader failed".into()))??;
        let stdout = String::from_utf8_lossy(&stdout_bytes).to_string();
        let stderr = String::from_utf8_lossy(&stderr_bytes).to_string();
        let known: Vec<String> = environment
            .map(|environment| environment.values().values().cloned().collect())
            .unwrap_or_default();
        Ok(ProcessResult {
            status_code: status.code(),
            stdout: redact_secrets(&stdout, &known),
            stderr: redact_secrets(&stderr, &known),
            timed_out,
        })
    }

    /// Start a reviewed interactive application without giving the renderer a
    /// shell or an arbitrary executable route. The executable and arguments
    /// are still validated through the same allowlist as captured processes;
    /// the child receives no credential environment and its standard streams
    /// are detached from the installer.
    pub fn spawn_detached(&self, allowlisted_executables: &[PathBuf]) -> Result<(), AppError> {
        self.validate(allowlisted_executables)?;
        let mut command = Command::new(&self.executable);
        command
            .args(&self.args)
            .env_clear()
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        // Preserve only non-secret process plumbing needed by a desktop
        // launcher. In particular, do not inherit MESHY_API_KEY or any other
        // ambient credential-shaped variable.
        add_safe_environment(&mut command);
        if let Some(cwd) = &self.cwd {
            command.current_dir(cwd);
        }
        command
            .spawn()
            .map(|_| ())
            .map_err(|error| AppError::Process(format!("spawn failed: {error}")))
    }
}

/// Terminate a reviewed child and any descendants without invoking a shell.
/// Windows wrappers can spawn Node/Python descendants that would otherwise
/// survive a timeout with the scoped environment still active.
pub(crate) fn terminate_process_tree(child: &mut Child) {
    #[cfg(target_os = "windows")]
    {
        let Some(system_root) = std::env::var_os("SystemRoot") else {
            let _ = child.kill();
            return;
        };
        let taskkill = PathBuf::from(system_root)
            .join("System32")
            .join("taskkill.exe");
        if taskkill.is_file() && !crate::security::path_has_link_component(&taskkill) {
            let pid = child.id().to_string();
            let failed = Command::new(taskkill)
                .args(["/PID", pid.as_str(), "/T", "/F"])
                .status()
                .map(|status| !status.success())
                .unwrap_or(true);
            if !failed {
                return;
            }
        }
        let _ = child.kill();
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = child.kill();
    }
}

fn add_safe_environment(command: &mut Command) {
    for name in [
        "PATH",
        "SystemRoot",
        "SYSTEMROOT",
        "TEMP",
        "TMP",
        "TMPDIR",
        "HOME",
        "USERPROFILE",
        "APPDATA",
        "LOCALAPPDATA",
        "LANG",
        "LC_ALL",
    ] {
        if let Some(value) = std::env::var_os(name) {
            command.env(name, value);
        }
    }
}

/// Resolve a manifest-declared executable without accepting a linked PATH
/// entry or a linked component anywhere in its resolved path. Callers pass
/// only bare executable names; command arguments remain a separate reviewed
/// concern.
pub fn find_path_executable(names: &[&str]) -> Result<PathBuf, AppError> {
    if names.is_empty()
        || names.iter().any(|name| {
            name.is_empty()
                || name.len() > 256
                || name.contains('\0')
                || name
                    .chars()
                    .any(|character| matches!(character, '/' | '\\' | ':'))
                || matches!(*name, "." | "..")
                || Path::new(name).components().count() != 1
        })
    {
        return Err(AppError::Process(
            "approved tool executable names are invalid".into(),
        ));
    }
    let path = std::env::var_os("PATH")
        .ok_or_else(|| AppError::Process("approved tool PATH is unavailable".into()))?;
    for directory in std::env::split_paths(&path) {
        for name in names {
            let candidate = directory.join(name);
            let metadata = fs::symlink_metadata(&candidate).ok();
            if !metadata.is_some_and(|metadata| metadata.is_file()) {
                continue;
            }
            if crate::security::path_has_link_component(&candidate) {
                continue;
            }
            let canonical = fs::canonicalize(&candidate)
                .map_err(|error| AppError::Process(format!("resolve tool executable: {error}")))?;
            if crate::security::path_has_link_component(&canonical) {
                continue;
            }
            return Ok(canonical);
        }
    }
    Err(AppError::Process(format!(
        "manifest-declared tool was not found on PATH: {}",
        names.join(", ")
    )))
}

/// Resolve the platform's system browser through a fixed OS-owned executable
/// path. Login URLs are validated by the Codex boundary before this function
/// is used; this helper never accepts a renderer-supplied executable or PATH
/// shim.
pub fn system_browser_executable() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        let system_root = std::env::var_os("SystemRoot")?;
        reviewed_system_executable(PathBuf::from(system_root).join("explorer.exe"))
    }
    #[cfg(target_os = "macos")]
    {
        reviewed_system_executable(PathBuf::from("/usr/bin/open"))
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        None
    }
}

fn reviewed_system_executable(path: PathBuf) -> Option<PathBuf> {
    let metadata = fs::symlink_metadata(&path).ok()?;
    if is_link_metadata(&metadata) || !metadata.is_file() {
        return None;
    }
    let canonical = fs::canonicalize(path).ok()?;
    if crate::security::path_has_link_component(&canonical) {
        return None;
    }
    Some(canonical)
}

fn same_executable(left: &PathBuf, right: &PathBuf) -> bool {
    std::fs::canonicalize(left).ok() == std::fs::canonicalize(right).ok()
}

fn read_bounded<R: Read>(reader: R, limit: usize) -> Result<Vec<u8>, AppError> {
    let mut bytes = Vec::new();
    reader
        .take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| AppError::Process(format!("read process output failed: {error}")))?;
    if bytes.len() > limit {
        bytes.truncate(limit);
    }
    Ok(bytes)
}

fn quote_argument(value: &str) -> String {
    if value
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || "_./:-".contains(character))
    {
        value.to_string()
    } else {
        format!("\"{}\"", value.replace('"', "\\\""))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::credentials::{save_meshy_key, MemoryCredentialStore, ScopedSecretEnvironment};

    #[test]
    fn process_preview_contains_names_not_secret_values() {
        let spec = ProcessSpec {
            executable: PathBuf::from("tool.exe"),
            executable_sha256: None,
            args: vec!["--mode".into(), "health check".into()],
            cwd: None,
            platform: Platform::current(),
            environment_names: vec!["MESHY_API_KEY".into()],
            timeout_seconds: 30,
            max_output_bytes: 1024,
        };
        let preview = spec.preview(&["MESHY_API_KEY".into()]);
        assert!(preview.contains("MESHY_API_KEY"));
        assert!(!preview.contains("msy_"));
        assert!(preview.contains("health check"));
    }

    #[test]
    fn unallowlisted_process_is_rejected() {
        let spec = ProcessSpec {
            executable: PathBuf::from("not-allowed.exe"),
            executable_sha256: None,
            args: vec![],
            cwd: None,
            platform: Platform::current(),
            environment_names: vec![],
            timeout_seconds: 1,
            max_output_bytes: 100,
        };
        assert!(spec.validate(&[]).is_err());
    }

    #[cfg(any(unix, windows))]
    #[test]
    fn executable_identity_mismatch_is_rejected_before_spawn() {
        let executable = if cfg!(windows) {
            std::fs::canonicalize(std::env::var_os("ComSpec").unwrap()).unwrap()
        } else {
            std::fs::canonicalize("/bin/sh").unwrap()
        };
        let spec = ProcessSpec {
            executable: executable.clone(),
            executable_sha256: Some("0".repeat(64)),
            args: vec!["-c".into(), "exit 0".into()],
            cwd: None,
            platform: Platform::current(),
            environment_names: vec![],
            timeout_seconds: 5,
            max_output_bytes: 1024,
        };
        let error = spec
            .validate(std::slice::from_ref(&executable))
            .unwrap_err();
        assert!(error.to_string().contains("identity changed"));
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn approved_process_receives_only_the_scoped_meshy_environment() {
        let store = MemoryCredentialStore::default();
        let reference = save_meshy_key(&store, "mesh_key_process_test").unwrap();
        let environment =
            ScopedSecretEnvironment::from_credential(&store, &reference, "MESHY_API_KEY").unwrap();
        let executable = std::fs::canonicalize(std::env::var_os("ComSpec").unwrap()).unwrap();
        let spec = ProcessSpec {
            executable: executable.clone(),
            executable_sha256: Some(crate::security::sha256_file(&executable).unwrap()),
            args: vec!["/D".into(), "/C".into(), "echo %MESHY_API_KEY%".into()],
            cwd: None,
            platform: Platform::current(),
            environment_names: vec!["MESHY_API_KEY".into()],
            timeout_seconds: 5,
            max_output_bytes: 4096,
        };
        let result = spec.run(&[executable], Some(&environment)).unwrap();
        assert_eq!(result.status_code, Some(0));
        assert!(result.stdout.contains("[REDACTED]"));
        assert!(!result.stdout.contains("mesh_key_process_test"));
    }

    #[cfg(unix)]
    #[test]
    fn approved_process_receives_only_the_scoped_meshy_environment() {
        let store = MemoryCredentialStore::default();
        let reference = save_meshy_key(&store, "mesh_key_process_test").unwrap();
        let environment =
            ScopedSecretEnvironment::from_credential(&store, &reference, "MESHY_API_KEY").unwrap();
        let executable = std::fs::canonicalize("/bin/sh").unwrap();
        let spec = ProcessSpec {
            executable: executable.clone(),
            executable_sha256: Some(crate::security::sha256_file(&executable).unwrap()),
            args: vec!["-c".into(), "printf '%s' \"$MESHY_API_KEY\"".into()],
            cwd: None,
            platform: Platform::current(),
            environment_names: vec!["MESHY_API_KEY".into()],
            timeout_seconds: 5,
            max_output_bytes: 4096,
        };
        let result = spec.run(&[executable], Some(&environment)).unwrap();
        assert_eq!(result.status_code, Some(0));
        assert_eq!(result.stdout, "[REDACTED]");
        assert!(!result.stdout.contains("mesh_key_process_test"));
    }
}
