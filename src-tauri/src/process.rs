use crate::credentials::ScopedSecretEnvironment;
use crate::models::Platform;
#[cfg(any(target_os = "windows", target_os = "macos"))]
use crate::security::is_link_metadata;
use crate::security::{redact_secrets, validate_env_name};
use crate::AppError;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{Read, Write};
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
    #[serde(default)]
    pub stdout_truncated: bool,
    #[serde(default)]
    pub stderr_truncated: bool,
}

#[derive(Default)]
struct ProcessRunProfile<'a> {
    isolated_git_read_only: bool,
    stdin_bytes: Option<&'a [u8]>,
    disable_local_git_config: bool,
    bound_directory: Option<&'a std::fs::File>,
    should_stop: Option<&'a mut dyn FnMut() -> bool>,
}

#[cfg(target_os = "windows")]
struct ChildProcessContainment {
    job: windows_sys::Win32::Foundation::HANDLE,
}

#[cfg(target_os = "windows")]
impl ChildProcessContainment {
    fn attach(child: &Child) -> Result<Self, AppError> {
        use std::mem::{size_of, zeroed};
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::System::JobObjects::{
            AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
            SetInformationJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
            JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        };

        // SAFETY: the job receives no name, the zeroed information structure
        // is initialized before use, and the child process handle remains
        // valid for the duration of this call.
        unsafe {
            let job = CreateJobObjectW(std::ptr::null(), std::ptr::null());
            if job.is_null() {
                return Err(AppError::Process(
                    "Windows process containment could not create a Job Object".into(),
                ));
            }
            let mut information: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = zeroed();
            information.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            if SetInformationJobObject(
                job,
                JobObjectExtendedLimitInformation,
                &information as *const _ as *const _,
                size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            ) == 0
                || AssignProcessToJobObject(job, child.as_raw_handle() as _) == 0
            {
                windows_sys::Win32::Foundation::CloseHandle(job);
                return Err(AppError::Process(
                    "Windows process containment could not bind the reviewed child".into(),
                ));
            }
            Ok(Self { job })
        }
    }
}

#[cfg(target_os = "windows")]
impl Drop for ChildProcessContainment {
    fn drop(&mut self) {
        // SAFETY: this object exclusively owns the valid job handle. Closing
        // it terminates any surviving descendants because KILL_ON_JOB_CLOSE
        // was set before the child was assigned.
        unsafe {
            windows_sys::Win32::Foundation::CloseHandle(self.job);
        }
    }
}

#[cfg(not(target_os = "windows"))]
struct ChildProcessContainment;

#[cfg(not(target_os = "windows"))]
impl ChildProcessContainment {
    fn attach(_child: &Child) -> Result<Self, AppError> {
        Ok(Self)
    }
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
        self.run_with_profile(
            allowlisted_executables,
            environment,
            ProcessRunProfile::default(),
        )
    }

    /// Run a Git metadata probe with system/global configuration, interactive
    /// credential helpers, optional locks, and platform attribute files
    /// disabled. The Git module separately validates the repository-local
    /// config before invoking this profile.
    pub(crate) fn run_git_read_only(
        &self,
        allowlisted_executables: &[PathBuf],
    ) -> Result<ProcessResult, AppError> {
        let executable_name = self
            .executable
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default();
        if !matches!(
            executable_name.to_ascii_lowercase().as_str(),
            "git" | "git.exe"
        ) || !self.environment_names.is_empty()
        {
            return Err(AppError::Process(
                "the isolated Git profile accepts only a reviewed Git executable without credentials"
                    .into(),
            ));
        }
        self.run_with_profile(
            allowlisted_executables,
            None,
            ProcessRunProfile {
                isolated_git_read_only: true,
                ..ProcessRunProfile::default()
            },
        )
    }

    /// Run an isolated Git probe from the exact directory represented by a
    /// retained handle. Unix changes directory with `fchdir` in the child
    /// immediately before exec; Windows relies on the handle denying delete
    /// sharing while the canonical working directory is used.
    pub(crate) fn run_git_read_only_bound(
        &self,
        allowlisted_executables: &[PathBuf],
        directory: &std::fs::File,
    ) -> Result<ProcessResult, AppError> {
        let executable_name = self
            .executable
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default();
        if !matches!(
            executable_name.to_ascii_lowercase().as_str(),
            "git" | "git.exe"
        ) || !self.environment_names.is_empty()
        {
            return Err(AppError::Process(
                "the isolated Git profile accepts only a reviewed Git executable without credentials"
                    .into(),
            ));
        }
        self.run_with_profile(
            allowlisted_executables,
            None,
            ProcessRunProfile {
                isolated_git_read_only: true,
                bound_directory: Some(directory),
                ..ProcessRunProfile::default()
            },
        )
    }

    pub(crate) fn run_git_read_only_bound_with_check(
        &self,
        allowlisted_executables: &[PathBuf],
        directory: &std::fs::File,
        should_stop: &mut dyn FnMut() -> bool,
    ) -> Result<ProcessResult, AppError> {
        let executable_name = self
            .executable
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default();
        if !matches!(
            executable_name.to_ascii_lowercase().as_str(),
            "git" | "git.exe"
        ) || !self.environment_names.is_empty()
        {
            return Err(AppError::Process(
                "the isolated Git profile accepts only a reviewed Git executable without credentials"
                    .into(),
            ));
        }
        self.run_with_profile(
            allowlisted_executables,
            None,
            ProcessRunProfile {
                isolated_git_read_only: true,
                bound_directory: Some(directory),
                should_stop: Some(should_stop),
                ..ProcessRunProfile::default()
            },
        )
    }

    /// Run a reviewed Git plumbing command from a retained repository handle
    /// with bounded bytes supplied through standard input. This keeps Git from
    /// reopening either the project path or a user-controlled managed file.
    pub(crate) fn run_git_read_only_with_stdin_bound(
        &self,
        allowlisted_executables: &[PathBuf],
        stdin_bytes: &[u8],
        directory: &std::fs::File,
    ) -> Result<ProcessResult, AppError> {
        if stdin_bytes.len() > 32 * 1024 * 1024 {
            return Err(AppError::Process(
                "Git standard input exceeds the 32 MiB process limit".into(),
            ));
        }
        let executable_name = self
            .executable
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default();
        if !matches!(
            executable_name.to_ascii_lowercase().as_str(),
            "git" | "git.exe"
        ) || !self.environment_names.is_empty()
        {
            return Err(AppError::Process(
                "the isolated Git input profile accepts only a reviewed Git executable without credentials"
                    .into(),
            ));
        }
        self.run_with_profile(
            allowlisted_executables,
            None,
            ProcessRunProfile {
                isolated_git_read_only: true,
                stdin_bytes: Some(stdin_bytes),
                bound_directory: Some(directory),
                ..ProcessRunProfile::default()
            },
        )
    }

    /// Run a reviewed Git command with repository-local configuration disabled.
    /// Callers must provide every setting and destination that affects the
    /// reviewed operation on the command line.
    pub(crate) fn run_git_without_local_config(
        &self,
        allowlisted_executables: &[PathBuf],
    ) -> Result<ProcessResult, AppError> {
        let executable_name = self
            .executable
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default();
        if !matches!(
            executable_name.to_ascii_lowercase().as_str(),
            "git" | "git.exe"
        ) || !self.environment_names.is_empty()
        {
            return Err(AppError::Process(
                "the config-free Git profile accepts only a reviewed Git executable without credentials"
                    .into(),
            ));
        }
        self.run_with_profile(
            allowlisted_executables,
            None,
            ProcessRunProfile {
                isolated_git_read_only: true,
                disable_local_git_config: true,
                ..ProcessRunProfile::default()
            },
        )
    }

    fn run_with_profile(
        &self,
        allowlisted_executables: &[PathBuf],
        environment: Option<&ScopedSecretEnvironment>,
        mut profile: ProcessRunProfile<'_>,
    ) -> Result<ProcessResult, AppError> {
        self.validate(allowlisted_executables)?;
        if profile
            .should_stop
            .as_mut()
            .is_some_and(|check| (**check)())
        {
            return Err(AppError::Process("reviewed process was cancelled".into()));
        }
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
            .stdin(if profile.stdin_bytes.is_some() {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        add_safe_environment(&mut command);
        if profile.isolated_git_read_only {
            add_isolated_git_environment(&mut command);
        }
        if profile.disable_local_git_config {
            command.env("GIT_CONFIG", null_git_config_path());
        }
        if let Some(cwd) = &self.cwd {
            command.current_dir(cwd);
        }
        #[cfg(unix)]
        if let Some(directory) = profile.bound_directory {
            use std::os::fd::AsRawFd;
            use std::os::unix::process::CommandExt;

            let directory_fd = directory.as_raw_fd();
            // SAFETY: `fchdir` is async-signal-safe and the retained file stays
            // alive until `spawn` returns. The closure performs no allocation.
            unsafe {
                command.pre_exec(move || {
                    if libc::fchdir(directory_fd) == 0 {
                        Ok(())
                    } else {
                        Err(std::io::Error::last_os_error())
                    }
                });
            }
        }
        #[cfg(not(unix))]
        let _ = profile.bound_directory;
        if let Some(environment) = environment {
            command.envs(environment.values());
        }
        configure_child_no_console_window(&mut command);
        configure_child_process_group(&mut command);
        let mut child = command
            .spawn()
            .map_err(|error| AppError::Process(format!("spawn failed: {error}")))?;
        let _containment = match ChildProcessContainment::attach(&child) {
            Ok(containment) => containment,
            Err(error) => {
                terminate_process_tree(&mut child);
                let _ = child.wait();
                return Err(error);
            }
        };
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
        if let Some(stdin_bytes) = profile.stdin_bytes {
            let mut stdin = child
                .stdin
                .take()
                .ok_or_else(|| AppError::Process("stdin pipe was not created".into()))?;
            if let Err(error) = stdin.write_all(stdin_bytes) {
                terminate_process_tree(&mut child);
                let _ = child.wait();
                let _ = stdout_thread.join();
                let _ = stderr_thread.join();
                return Err(AppError::Process(format!(
                    "write to reviewed process failed: {error}"
                )));
            }
        }
        let started = Instant::now();
        let mut timed_out = false;
        let mut cancelled = false;
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
            if profile
                .should_stop
                .as_mut()
                .is_some_and(|check| (**check)())
            {
                cancelled = true;
                terminate_process_tree(&mut child);
                break;
            }
            std::thread::sleep(Duration::from_millis(25));
        }
        let status = child
            .wait()
            .map_err(|error| AppError::Process(format!("collect output failed: {error}")))?;
        let (stdout_bytes, stdout_truncated) = stdout_thread
            .join()
            .map_err(|_| AppError::Process("stdout reader failed".into()))??;
        let (stderr_bytes, stderr_truncated) = stderr_thread
            .join()
            .map_err(|_| AppError::Process("stderr reader failed".into()))??;
        if cancelled {
            return Err(AppError::Process("reviewed process was cancelled".into()));
        }
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
            stdout_truncated,
            stderr_truncated,
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
        configure_child_no_console_window(&mut command);
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
        let Some(system_root) = windows_directory() else {
            let _ = child.kill();
            return;
        };
        let taskkill = system_root.join("System32").join("taskkill.exe");
        if taskkill.is_file() && !crate::security::path_has_link_component(&taskkill) {
            let pid = child.id().to_string();
            let mut command = Command::new(taskkill);
            command.args(["/PID", pid.as_str(), "/T", "/F"]);
            configure_child_no_console_window(&mut command);
            let failed = command
                .status()
                .map(|status| !status.success())
                .unwrap_or(true);
            if !failed {
                return;
            }
        }
        let _ = child.kill();
    }
    #[cfg(unix)]
    {
        let process_group = -(child.id() as i32);
        let terminated = unsafe { libc::kill(process_group, libc::SIGKILL) } == 0;
        if !terminated {
            let _ = child.kill();
        }
    }
}

#[cfg(unix)]
pub(crate) fn configure_child_process_group(command: &mut Command) {
    use std::os::unix::process::CommandExt;
    unsafe {
        command.pre_exec(|| {
            if libc::setpgid(0, 0) == 0 {
                Ok(())
            } else {
                Err(std::io::Error::last_os_error())
            }
        });
    }
}

/// Keep supervised console tools inside the desktop application. Windows
/// otherwise gives a console-subsystem child its own visible terminal when
/// the app itself was launched from a shortcut or the Start menu.
pub(crate) fn configure_child_no_console_window(command: &mut Command) {
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;

        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }

    #[cfg(not(target_os = "windows"))]
    let _ = command;
}

#[cfg(not(unix))]
pub(crate) fn configure_child_process_group(_command: &mut Command) {}

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
        "SystemDrive",
        "HOMEDRIVE",
        "HOMEPATH",
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

fn add_isolated_git_environment(command: &mut Command) {
    let null_config = null_git_config_path();
    command
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", null_config)
        .env("GIT_ATTR_NOSYSTEM", "1")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_ASKPASS", "")
        .env("GCM_INTERACTIVE", "Never")
        .env("GCM_GUI_PROMPT", "0")
        .env("GIT_NO_REPLACE_OBJECTS", "1");
}

fn null_git_config_path() -> &'static str {
    if cfg!(target_os = "windows") {
        "NUL"
    } else {
        "/dev/null"
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

/// Establish publisher identity independently of a PATH lookup before a
/// discovered executable receives account-storage paths or vault secrets.
/// The candidate path is passed as a dedicated, process-scoped environment
/// value; it is never interpolated into the fixed verification command.
pub fn validate_executable_publisher(
    executable: &Path,
    expected_publisher: &str,
) -> Result<(), AppError> {
    if !executable.is_absolute()
        || expected_publisher.is_empty()
        || expected_publisher.len() > 128
        || expected_publisher
            .chars()
            .any(|character| character.is_control())
    {
        return Err(AppError::Process(
            "executable publisher review input is invalid".into(),
        ));
    }
    if crate::security::path_has_link_component(executable) {
        return Err(AppError::Process(
            "executable publisher review rejected a linked path".into(),
        ));
    }

    #[cfg(target_os = "windows")]
    {
        let system_root = windows_directory()
            .ok_or_else(|| AppError::Process("Windows system root is unavailable".into()))?;
        let verifier = system_root
            .join("System32")
            .join("WindowsPowerShell")
            .join("v1.0")
            .join("powershell.exe");
        let verifier = reviewed_system_executable(verifier)
            .ok_or_else(|| AppError::Process("Windows signature verifier is unavailable".into()))?;
        let before_sha256 = crate::security::sha256_file(executable)?;
        let script = "$candidate = [Environment]::GetEnvironmentVariable('HOI4_SETUP_EXECUTABLE'); $signature = Get-AuthenticodeSignature -LiteralPath $candidate; if ($signature.Status -ne 'Valid' -or $null -eq $signature.SignerCertificate) { exit 3 }; [Console]::Out.Write($signature.SignerCertificate.GetNameInfo([System.Security.Cryptography.X509Certificates.X509NameType]::SimpleName, $false))";
        let mut command = Command::new(verifier);
        command
            .args([
                "-NoLogo",
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                script,
            ])
            .env_clear()
            .env(
                "SystemRoot",
                windows_directory().ok_or_else(|| {
                    AppError::Process("Windows system root is unavailable".into())
                })?,
            )
            .env("HOI4_SETUP_EXECUTABLE", executable);
        configure_child_no_console_window(&mut command);
        let output = command.output().map_err(|error| {
            AppError::Process(format!("Windows publisher verification failed: {error}"))
        })?;
        if !output.status.success() || output.stdout.len() > 4096 {
            return Err(AppError::Process(
                "executable does not have a valid reviewed Windows signature".into(),
            ));
        }
        let publisher = String::from_utf8_lossy(&output.stdout);
        let reviewed_windows_publisher = match expected_publisher {
            // Exact current Authenticode identity verified from the official
            // Codex desktop installation; the logical product name is never
            // matched as a substring.
            "OpenAI" => "OpenAI OpCo, LLC",
            value => value,
        };
        if !publisher
            .trim()
            .eq_ignore_ascii_case(reviewed_windows_publisher)
            || crate::security::sha256_file(executable)? != before_sha256
        {
            return Err(AppError::Process(
                "executable publisher does not match the reviewed product".into(),
            ));
        }
        Ok(())
    }

    #[cfg(target_os = "macos")]
    {
        let verifier = reviewed_system_executable(PathBuf::from("/usr/bin/codesign"))
            .ok_or_else(|| AppError::Process("macOS signature verifier is unavailable".into()))?;
        let mut verification_command = Command::new(&verifier);
        verification_command
            .args(["--verify", "--strict", "--verbose=2"])
            .arg(executable)
            .env_clear();
        configure_child_no_console_window(&mut verification_command);
        let verification = verification_command.output().map_err(|error| {
            AppError::Process(format!("macOS publisher verification failed: {error}"))
        })?;
        if !verification.status.success() {
            return Err(AppError::Process(
                "executable does not have a valid reviewed macOS signature".into(),
            ));
        }
        let mut details_command = Command::new(verifier);
        details_command
            .args(["-dv", "--verbose=4"])
            .arg(executable)
            .env_clear();
        configure_child_no_console_window(&mut details_command);
        let details = details_command.output().map_err(|error| {
            AppError::Process(format!("macOS publisher inspection failed: {error}"))
        })?;
        let detail = format!(
            "{}\n{}",
            String::from_utf8_lossy(&details.stdout),
            String::from_utf8_lossy(&details.stderr)
        );
        if !details.status.success()
            || detail.len() > 16 * 1024
            || !detail.contains(expected_publisher)
        {
            return Err(AppError::Process(
                "executable publisher does not match the reviewed product".into(),
            ));
        }
        Ok(())
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        let _ = (executable, expected_publisher);
        Err(AppError::UnsupportedPlatform(
            "executable publisher verification is supported only on Windows and macOS".into(),
        ))
    }
}

/// Relay stdio to one already-private, hash-bound credential-bearing runtime.
/// This is intentionally narrower than ProcessSpec: it exists for an MCP
/// server whose JSONL stream belongs to Codex rather than the desktop UI.
pub(crate) fn run_private_credential_stdio_proxy(
    executable: &Path,
    expected_sha256: &str,
    args: &[String],
    environment_name: &str,
    environment_value: &str,
) -> Result<i32, AppError> {
    if !executable.is_absolute()
        || environment_name != "MESHY_API_KEY"
        || environment_value.trim().is_empty()
        || args.is_empty()
        || args.iter().any(|value| value.contains('\0'))
        || crate::security::sha256_file(executable)? != expected_sha256
    {
        return Err(AppError::Process(
            "the private credential-bearing runtime identity is invalid".into(),
        ));
    }
    let mut command = Command::new(executable);
    command
        .args(args)
        .env_clear()
        .env(environment_name, environment_value)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());
    add_safe_environment(&mut command);
    configure_child_no_console_window(&mut command);
    configure_child_process_group(&mut command);
    if crate::security::sha256_file(executable)? != expected_sha256 {
        return Err(AppError::Process(
            "the private credential-bearing runtime changed before spawn".into(),
        ));
    }
    let mut child = command
        .spawn()
        .map_err(|error| AppError::Process(format!("private runtime spawn failed: {error}")))?;
    let _containment = match ChildProcessContainment::attach(&child) {
        Ok(containment) => containment,
        Err(error) => {
            terminate_process_tree(&mut child);
            let _ = child.wait();
            return Err(error);
        }
    };
    let status = child.wait()?;
    Ok(status.code().unwrap_or(1))
}

/// Resolve the platform's system browser through a fixed OS-owned executable
/// path. Login URLs are validated by the Codex boundary before this function
/// is used; this helper never accepts a renderer-supplied executable or PATH
/// shim.
pub fn system_browser_executable() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        reviewed_system_executable(windows_directory()?.join("explorer.exe"))
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

#[cfg(target_os = "windows")]
fn windows_directory() -> Option<PathBuf> {
    use std::os::windows::ffi::OsStringExt;

    let mut buffer = vec![0u16; 32_768];
    let length = unsafe {
        windows_sys::Win32::System::SystemInformation::GetWindowsDirectoryW(
            buffer.as_mut_ptr(),
            buffer.len() as u32,
        )
    } as usize;
    if length == 0 || length >= buffer.len() {
        return None;
    }
    buffer.truncate(length);
    Some(PathBuf::from(std::ffi::OsString::from_wide(&buffer)))
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
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

fn read_bounded<R: Read>(reader: R, limit: usize) -> Result<(Vec<u8>, bool), AppError> {
    let mut bytes = Vec::new();
    reader
        .take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| AppError::Process(format!("read process output failed: {error}")))?;
    let truncated = bytes.len() > limit;
    if truncated {
        bytes.truncate(limit);
    }
    Ok((bytes, truncated))
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

    #[test]
    fn bounded_process_output_reports_truncation() {
        let (bytes, truncated) = read_bounded(std::io::Cursor::new(b"12345"), 4).unwrap();

        assert_eq!(bytes, b"1234");
        assert!(truncated);
    }

    #[cfg(any(unix, windows))]
    #[test]
    fn reviewed_process_cancellation_interrupts_the_child_promptly() {
        let (executable, args) = if cfg!(windows) {
            (
                std::fs::canonicalize(std::env::var_os("ComSpec").unwrap()).unwrap(),
                vec!["/D".into(), "/C".into(), "ping 127.0.0.1 -n 30 >NUL".into()],
            )
        } else {
            (
                std::fs::canonicalize("/bin/sh").unwrap(),
                vec!["-c".into(), "sleep 30".into()],
            )
        };
        let spec = ProcessSpec {
            executable: executable.clone(),
            executable_sha256: Some(crate::security::sha256_file(&executable).unwrap()),
            args,
            cwd: None,
            platform: Platform::current(),
            environment_names: vec![],
            timeout_seconds: 30,
            max_output_bytes: 1024,
        };
        let started = Instant::now();
        let mut should_stop = || started.elapsed() >= Duration::from_millis(100);

        let error = spec
            .run_with_profile(
                &[executable],
                None,
                ProcessRunProfile {
                    should_stop: Some(&mut should_stop),
                    ..ProcessRunProfile::default()
                },
            )
            .unwrap_err();

        assert!(error.to_string().contains("cancelled"));
        assert!(started.elapsed() < Duration::from_secs(3));
    }

    #[test]
    fn read_only_git_environment_disables_ambient_configuration_and_prompts() {
        let mut command = Command::new("git");
        add_isolated_git_environment(&mut command);
        let value = |name: &str| {
            command
                .get_envs()
                .find(|(key, _)| *key == std::ffi::OsStr::new(name))
                .and_then(|(_, value)| value)
                .map(|value| value.to_string_lossy().into_owned())
        };

        assert_eq!(value("GIT_CONFIG_NOSYSTEM").as_deref(), Some("1"));
        assert_eq!(value("GIT_OPTIONAL_LOCKS").as_deref(), Some("0"));
        assert_eq!(value("GIT_TERMINAL_PROMPT").as_deref(), Some("0"));
        assert_eq!(value("GCM_INTERACTIVE").as_deref(), Some("Never"));
        assert_eq!(
            value("GIT_CONFIG_GLOBAL").as_deref(),
            Some(if cfg!(target_os = "windows") {
                "NUL"
            } else {
                "/dev/null"
            })
        );
    }

    #[test]
    fn config_free_git_environment_disables_repository_local_configuration() {
        let mut command = Command::new("git");
        command.env("GIT_CONFIG", null_git_config_path());
        let local = command
            .get_envs()
            .find(|(key, _)| *key == std::ffi::OsStr::new("GIT_CONFIG"))
            .and_then(|(_, value)| value)
            .map(|value| value.to_string_lossy().into_owned());

        assert_eq!(local.as_deref(), Some(null_git_config_path()));
    }

    #[test]
    fn config_free_git_child_cannot_observe_a_repository_local_sentinel() {
        let project = tempfile::tempdir().unwrap();
        let git = find_path_executable(&["git.exe", "git"]).unwrap();
        let init = ProcessSpec {
            executable: git.clone(),
            executable_sha256: Some(crate::security::sha256_file(&git).unwrap()),
            args: vec!["init".into()],
            cwd: Some(project.path().to_path_buf()),
            platform: Platform::current(),
            environment_names: vec![],
            timeout_seconds: 30,
            max_output_bytes: 1024 * 1024,
        };
        assert_eq!(
            init.run_git_read_only(&[git.clone()]).unwrap().status_code,
            Some(0)
        );
        fs::write(
            project.path().join(".git/config"),
            "[core]\nrepositoryformatversion = 0\n[hoi4setup]\nsentinel = visible-only-locally\n",
        )
        .unwrap();
        let probe = ProcessSpec {
            executable: git.clone(),
            executable_sha256: Some(crate::security::sha256_file(&git).unwrap()),
            args: vec!["config".into(), "--get".into(), "hoi4setup.sentinel".into()],
            cwd: Some(project.path().to_path_buf()),
            platform: Platform::current(),
            environment_names: vec![],
            timeout_seconds: 30,
            max_output_bytes: 1024 * 1024,
        };

        let output = probe.run_git_without_local_config(&[git]).unwrap();

        assert_ne!(output.status_code, Some(0));
        assert!(!output.stdout.contains("visible-only-locally"));
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
    fn unsigned_lookalike_cannot_self_authorize_as_a_reviewed_publisher() {
        let current_test_binary = std::env::current_exe().unwrap();
        let error = validate_executable_publisher(&current_test_binary, "OpenAI")
            .unwrap_err()
            .to_string();
        assert!(error.contains("signature") || error.contains("publisher"));
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

    #[cfg(target_os = "windows")]
    #[test]
    fn closing_the_job_object_terminates_a_reviewed_child() {
        let executable = std::fs::canonicalize(std::env::var_os("ComSpec").unwrap()).unwrap();
        let mut command = Command::new(executable);
        command
            .args(["/D", "/C", "ping 127.0.0.1 -n 60 >NUL"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        configure_child_no_console_window(&mut command);
        let mut child = command.spawn().unwrap();
        let containment = ChildProcessContainment::attach(&child).unwrap();

        drop(containment);

        let stopped = (0..100).any(|_| match child.try_wait() {
            Ok(Some(_)) => true,
            Ok(None) => {
                std::thread::sleep(Duration::from_millis(10));
                false
            }
            Err(_) => false,
        });
        if !stopped {
            let _ = child.kill();
            let _ = child.wait();
        }
        assert!(stopped, "the Job Object left the reviewed child running");
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

    #[cfg(unix)]
    #[test]
    fn supervised_process_group_termination_reaches_descendants() {
        let directory = tempfile::tempdir().unwrap();
        let pid_file = directory.path().join("descendant.pid");
        let mut command = Command::new("/bin/sh");
        command
            .arg("-c")
            .arg("sleep 60 & printf '%s' \"$!\" > \"$1\"; wait")
            .arg("hoi4-mod-setup-process-test")
            .arg(&pid_file)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        configure_child_process_group(&mut command);
        let mut child = command.spawn().unwrap();
        let process_group = -(child.id() as i32);

        let descendant = (0..100)
            .find_map(|_| {
                let parsed = std::fs::read_to_string(&pid_file)
                    .ok()
                    .and_then(|value| value.trim().parse::<i32>().ok());
                if parsed.is_none() {
                    std::thread::sleep(Duration::from_millis(10));
                }
                parsed
            })
            .expect("shell should record its descendant PID");
        assert_eq!(unsafe { libc::kill(descendant, 0) }, 0);

        terminate_process_tree(&mut child);
        let _ = child.wait();
        let group_gone = (0..100).any(|_| {
            let result = unsafe { libc::kill(process_group, 0) };
            if result == -1 && std::io::Error::last_os_error().raw_os_error() == Some(libc::ESRCH) {
                true
            } else {
                std::thread::sleep(Duration::from_millis(10));
                false
            }
        });
        assert!(
            group_gone,
            "terminated process group still has live members"
        );
    }
}
