//! Running the installed hoi4-imagegen CLI: the version check after an install,
//! the optional ChatGPT sign-in step, and the provider doctor.
//!
//! Every run uses an argument array and an absolute path to the installed
//! binary, whose files must still hash to what Setup recorded at install time.
//! The ChatGPT sign-in belongs to the binary: it opens the browser itself and
//! keeps its tokens in the operating-system keychain, so Setup never sees or
//! stores them. Provider API keys live in Setup's credential store and reach
//! only the one process that needs them, through a scoped environment.

use crate::credentials::{
    image_generation_credential_reference, CredentialStore, ScopedSecretEnvironment,
};
use crate::models::{ImageGenerationSelection, Platform};
use crate::process::{ProcessResult, ProcessSpec};
use crate::security::redact_secrets;
use crate::AppError;
use hoi4_setup_core::release_binary::{self as route, PROVIDER_ID};
use serde::Serialize;
use std::path::PathBuf;

/// Whether a variable name may carry a key to hoi4-imagegen. The setup
/// package names each backend's key variable, and a new backend needs no app
/// release, but only an upper-case name that ends in `_KEY` or `_TOKEN` is
/// ever passed, so a package can never name PATH, a loader variable or a
/// credential that belongs to another tool.
pub fn credential_variable_allowed(name: &str) -> bool {
    let shaped = (3..=48).contains(&name.len())
        && name.starts_with(|c: char| c.is_ascii_uppercase())
        && name
            .bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_');
    shaped
        && (name.ends_with("_KEY") || name.ends_with("_TOKEN"))
        && !["ANTHROPIC_", "MESHY_", "CLAUDE_", "CODEX_"]
            .iter()
            .any(|prefix| name.starts_with(prefix))
}

/// A credential field id of the hoi4-imagegen route, such as `fal_key`. The
/// variable it becomes is the same name in upper case (`FAL_KEY`).
pub fn credential_field_allowed(field: &str) -> bool {
    field.starts_with(|c: char| c.is_ascii_lowercase())
        && field
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
        && credential_variable_allowed(&field.to_ascii_uppercase())
}

/// The variable a credential field becomes.
pub fn credential_variable(field: &str) -> String {
    field.to_ascii_uppercase()
}

const VERSION_TIMEOUT_SECONDS: u64 = 30;
const STATUS_TIMEOUT_SECONDS: u64 = 60;
const LOGIN_TIMEOUT_SECONDS: u64 = 10 * 60;
const DOCTOR_TIMEOUT_SECONDS: u64 = 120;
const MAX_OUTPUT_BYTES: usize = 64 * 1024;

/// Runs a prepared process. The desktop host spawns it; tests record it.
pub trait CliRunner {
    fn run(
        &self,
        spec: &ProcessSpec,
        environment: Option<&ScopedSecretEnvironment>,
    ) -> Result<ProcessResult, AppError>;
}

pub struct OsCliRunner;

impl CliRunner for OsCliRunner {
    fn run(
        &self,
        spec: &ProcessSpec,
        environment: Option<&ScopedSecretEnvironment>,
    ) -> Result<ProcessResult, AppError> {
        spec.run(std::slice::from_ref(&spec.executable), environment)
    }
}

/// The hoi4-imagegen release Setup installed in a project.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstalledImagegen {
    pub root: PathBuf,
    pub version: String,
    pub tree_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SignInStatus {
    pub signed_in: bool,
    /// Bounded, redacted text the CLI printed.
    pub detail: String,
}

impl InstalledImagegen {
    /// The install the lock recorded, or `None` when hoi4-imagegen was never
    /// installed here.
    pub fn from_selection(
        root: &std::path::Path,
        selection: Option<&ImageGenerationSelection>,
    ) -> Option<Self> {
        let selection = selection?;
        Some(Self {
            root: root.to_path_buf(),
            version: selection.installed_release_version.clone()?,
            tree_sha256: selection.installed_release_tree_sha256.clone()?,
        })
    }

    fn cli_name() -> &'static str {
        if cfg!(windows) {
            "hoi4-imagegen.exe"
        } else {
            "hoi4-imagegen"
        }
    }

    fn spec(
        &self,
        arguments: &[&str],
        timeout_seconds: u64,
        environment_names: Vec<String>,
    ) -> Result<ProcessSpec, AppError> {
        // The installed files must be exactly what Setup recorded.
        let Some((_, tree)) = crate::release_install::installed_tree(&self.root, &self.version)?
        else {
            return Err(AppError::Process(
                "hoi4-imagegen is not installed in this project".into(),
            ));
        };
        if tree != self.tree_sha256 {
            return Err(AppError::Process(
                "the installed hoi4-imagegen files changed since Setup installed them. Run Update or Repair".into(),
            ));
        }
        let executable = self
            .root
            .join(route::version_dir(&self.version))
            .join(Self::cli_name());
        let digest = crate::security::sha256_file(&executable)?;
        Ok(ProcessSpec {
            executable,
            executable_sha256: Some(digest),
            args: arguments
                .iter()
                .map(|argument| (*argument).to_owned())
                .collect(),
            cwd: Some(self.root.clone()),
            platform: Platform::current(),
            environment_names,
            timeout_seconds,
            max_output_bytes: MAX_OUTPUT_BYTES,
        })
    }

    fn text(result: &ProcessResult) -> String {
        let combined = format!("{}\n{}", result.stdout.trim(), result.stderr.trim());
        redact_secrets(combined.trim(), &[])
            .chars()
            .take(2000)
            .collect()
    }

    /// Run `hoi4-imagegen --version` and require the manifest's version.
    pub fn verify_version(&self, runner: &dyn CliRunner) -> Result<(), AppError> {
        let spec = self.spec(&["--version"], VERSION_TIMEOUT_SECONDS, Vec::new())?;
        let result = runner.run(&spec, None)?;
        if result.timed_out || result.status_code != Some(0) {
            return Err(AppError::Process(
                "hoi4-imagegen --version did not succeed".into(),
            ));
        }
        match crate::release_install::reported_version(&result.stdout) {
            Some(reported) if reported == self.version => Ok(()),
            Some(reported) => Err(AppError::Process(format!(
                "hoi4-imagegen reports version {reported} but the manifest declares {}",
                self.version
            ))),
            None => Err(AppError::Process(
                "hoi4-imagegen did not report a version".into(),
            )),
        }
    }

    /// `hoi4-imagegen login-status`. Exit code zero means signed in.
    pub fn sign_in_status(&self, runner: &dyn CliRunner) -> Result<SignInStatus, AppError> {
        let spec = self.spec(&["login-status"], STATUS_TIMEOUT_SECONDS, Vec::new())?;
        let result = runner.run(&spec, None)?;
        Ok(SignInStatus {
            signed_in: !result.timed_out && result.status_code == Some(0),
            detail: Self::text(&result),
        })
    }

    /// The risk note `hoi4-imagegen login-risk-note` prints. The UI shows it
    /// and asks the user to confirm before [`Self::sign_in`] is called.
    pub fn risk_note(&self, runner: &dyn CliRunner) -> Result<String, AppError> {
        let spec = self.spec(&["login-risk-note"], STATUS_TIMEOUT_SECONDS, Vec::new())?;
        let result = runner.run(&spec, None)?;
        let note = result.stdout.trim();
        if result.timed_out || result.status_code != Some(0) || note.is_empty() {
            return Err(AppError::Process(
                "hoi4-imagegen could not show its sign-in risk note".into(),
            ));
        }
        Ok(redact_secrets(note, &[]).chars().take(8000).collect())
    }

    /// Launch `hoi4-imagegen login --accept-risk`, then report `login-status`.
    /// Call it only after the user confirmed the risk note in the UI. The
    /// binary opens the browser, waits for the redirect and stores the tokens;
    /// Setup sees neither.
    pub fn sign_in(&self, runner: &dyn CliRunner) -> Result<SignInStatus, AppError> {
        let spec = self.spec(
            &["login", "--accept-risk"],
            LOGIN_TIMEOUT_SECONDS,
            Vec::new(),
        )?;
        let result = runner.run(&spec, None)?;
        let mut status = self.sign_in_status(runner)?;
        if !status.signed_in && !result.timed_out {
            let printed = Self::text(&result);
            if !printed.is_empty() {
                status.detail = printed;
            }
        }
        if result.timed_out {
            status.detail =
                "Sign-in did not finish. Choose Sign in with ChatGPT to try again.".into();
        }
        Ok(status)
    }

    /// `hoi4-imagegen doctor` with the saved provider keys in its environment.
    pub fn doctor(
        &self,
        runner: &dyn CliRunner,
        environment: Option<&ScopedSecretEnvironment>,
    ) -> Result<String, AppError> {
        let names = environment
            .map(ScopedSecretEnvironment::variable_names)
            .unwrap_or_default();
        let spec = self.spec(&["doctor"], DOCTOR_TIMEOUT_SECONDS, names)?;
        let result = runner.run(&spec, environment)?;
        let text = Self::text(&result);
        Ok(match environment {
            Some(environment) => environment.redact(&text),
            None => text,
        })
    }
}

/// The selected provider keys as a scoped environment for one process, or
/// `None` when no key is saved. `fields` is (field id, environment variable
/// name); only fields the selection holds a key for are read.
pub fn scoped_environment<S: CredentialStore>(
    store: &S,
    selection: &ImageGenerationSelection,
    fields: &[(String, String)],
) -> Result<Option<ScopedSecretEnvironment>, AppError> {
    let mut entries = Vec::new();
    for (field, environment_name) in fields {
        let key = format!("{PROVIDER_ID}.{field}");
        let Some(recorded) = selection.credential_references.get(&key) else {
            continue;
        };
        let reference = image_generation_credential_reference(PROVIDER_ID, field)?;
        if &reference.reference != recorded {
            return Err(AppError::Credential(
                "image credential reference does not match its provider field".into(),
            ));
        }
        entries.push((reference, environment_name.clone()));
    }
    if entries.is_empty() {
        return Ok(None);
    }
    ScopedSecretEnvironment::from_image_credentials(store, &entries, credential_variable_allowed)
        .map(Some)
}
