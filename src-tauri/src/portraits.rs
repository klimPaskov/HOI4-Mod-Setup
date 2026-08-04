//! Read-only discovery and provider checks for the optional portrait workflow.
//!
//! This module deliberately does not install ComfyUI or read credential
//! values. It only inspects explicitly configured/common local roots, checks
//! the loopback health endpoint, verifies the current workflow files, reports
//! model presence, and performs the allowlisted GPU probe used before local
//! setup is offered.

use crate::models::Platform;
use crate::process::{find_path_executable, ProcessSpec};
use crate::security::{path_has_link_component, sha256_bytes, sha256_file};
use crate::AppError;
use reqwest::blocking::Client;
use reqwest::Url;
use serde::Serialize;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

pub const PORTRAIT_REPOSITORY: &str = "https://github.com/klimPaskov/comfyui-hoi4-portraits";
pub const PORTRAIT_BRANCH: &str = "codex/portrait-pipeline";
pub const PORTRAIT_COMMIT: &str = "92c8118f9ab61a0a658af24bc6868ed7f93cdebd";

const WORKFLOW_FILES: [(&str, &str); 2] = [
    (
        "hoi4_portrait_flux2_klein_9b_source.json",
        "fb1c9d58275034461054aed7d2cf005b91e982745b4653c5581cec318cc40e55",
    ),
    (
        "hoi4_portrait_processing_only.json",
        "3aeec172c0c63e2fe39eb0935f9ad850a44ef3e6b9956515b561c9a27ff51b01",
    ),
];

const MODEL_FILES: [(&str, &str, u64); 6] = [
    (
        "diffusion_models",
        "flux-2-klein-base-9b-fp8.safetensors",
        9_567_278_472,
    ),
    (
        "text_encoders",
        "qwen_3_8b_fp8mixed.safetensors",
        8_664_848_742,
    ),
    ("vae", "flux2-vae.safetensors", 336_211_292),
    (
        "loras",
        "hoi4_portraits_flux2_klein_9b_lora_000002500.safetensors",
        331_379_656,
    ),
    ("upscale_models", "RealESRGAN_x2plus.pth", 67_061_725),
    ("background_removal", "birefnet.safetensors", 444_473_596),
];

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalPortraitDiscovery {
    pub status: String,
    pub configured_root: Option<String>,
    pub detected_root: Option<String>,
    pub server_url: String,
    pub server_status: String,
    pub hardware_status: String,
    pub gpu_name: Option<String>,
    pub vram_gb: Option<f64>,
    pub workflow_status: String,
    pub model_status: String,
    pub huggingface_access_hint: bool,
    pub message: String,
    pub canonical_repository: String,
    pub canonical_commit: String,
    pub install_command: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalPortraitInstallResult {
    pub status: String,
    pub installed: Vec<String>,
    pub skipped: Vec<String>,
    pub conflicts: Vec<String>,
    pub message: String,
    pub canonical_commit: String,
}

#[derive(Debug)]
struct LocalRoot {
    configured: Option<String>,
    detected: Option<PathBuf>,
    error: Option<String>,
}

pub fn discover_local(configured_root: Option<&str>, server_url: &str) -> LocalPortraitDiscovery {
    let configured_display = configured_root
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let root = find_local_root(configured_display.as_deref());
    let (server_status, server_message) = check_server(server_url);
    let (hardware_status, gpu_name, vram_gb, hardware_message) = check_hardware();
    let workflow_status = root
        .detected
        .as_deref()
        .map(workflow_status)
        .unwrap_or_else(|| "missing".into());
    let huggingface_access_hint = huggingface_access_hint();
    let model_status = root
        .detected
        .as_deref()
        .map(|path| model_status(path, huggingface_access_hint))
        .unwrap_or_else(|| "missing".into());

    let status = if root.error.is_some() {
        "unreachable"
    } else if hardware_status != "ready" {
        "needs_hardware"
    } else if server_status != "ready" {
        "unreachable"
    } else if root.detected.is_none() || workflow_status != "ready" {
        "needs_workflow_install"
    } else if model_status == "needs_huggingface_access" {
        "needs_huggingface_access"
    } else if model_status != "ready" {
        "needs_models"
    } else {
        "ready"
    };

    let mut messages = Vec::new();
    if let Some(error) = root.error {
        messages.push(error);
    }
    messages.push(server_message);
    messages.push(hardware_message);
    if workflow_status != "ready" {
        messages.push(
            "The current pinned portrait workflows are not installed in this ComfyUI root.".into(),
        );
    }
    if model_status == "needs_huggingface_access" {
        messages.push("The gated FLUX.2 base model is missing; accept its Hugging Face agreement and authenticate before downloading models.".into());
    } else if model_status != "ready" {
        messages.push(
            "One or more pinned portrait model files are missing or have the wrong size.".into(),
        );
    }
    if messages.is_empty() {
        messages.push("Local ComfyUI passed the pinned portrait workflow checks.".into());
    }

    LocalPortraitDiscovery {
        status: status.into(),
        configured_root: root.configured,
        detected_root: root
            .detected
            .as_deref()
            .and_then(|path| fs::canonicalize(path).ok())
            .map(|path| path.display().to_string()),
        server_url: server_url.trim().to_string(),
        server_status,
        hardware_status,
        gpu_name,
        vram_gb,
        workflow_status,
        model_status,
        huggingface_access_hint,
        message: messages.join(" "),
        canonical_repository: PORTRAIT_REPOSITORY.into(),
        canonical_commit: PORTRAIT_COMMIT.into(),
        install_command: "python scripts/install_workflows.py --comfyui-root <COMFYUI_ROOT>".into(),
    }
}

pub fn install_current_workflows(root_value: &str) -> Result<LocalPortraitInstallResult, AppError> {
    let root = PathBuf::from(root_value.trim());
    if !root.is_absolute() {
        return Err(AppError::PathSecurity(
            "the ComfyUI root must be an absolute path".into(),
        ));
    }
    if path_has_link_component(&root) || !is_comfyui_root(&root) {
        return Err(AppError::PathSecurity(
            "the selected ComfyUI root is not a verified checkout or contains a link".into(),
        ));
    }
    let client = Client::builder()
        .timeout(Duration::from_secs(30))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|error| {
            AppError::Source(format!("portrait workflow client could not start: {error}"))
        })?;
    let mut downloaded = Vec::new();
    for (filename, expected) in WORKFLOW_FILES {
        let url = format!(
            "https://raw.githubusercontent.com/klimPaskov/comfyui-hoi4-portraits/{PORTRAIT_COMMIT}/workflows/{filename}"
        );
        let response = client.get(&url).send().map_err(|error| {
            AppError::Source(format!("download portrait workflow {filename}: {error}"))
        })?;
        if !response.status().is_success() {
            return Err(AppError::Source(format!(
                "portrait workflow {filename} returned HTTP {}",
                response.status()
            )));
        }
        let bytes = response
            .bytes()
            .map_err(|error| {
                AppError::Source(format!("read portrait workflow {filename}: {error}"))
            })?
            .to_vec();
        if bytes.len() > 8 * 1024 * 1024 || sha256_bytes(&bytes) != expected {
            return Err(AppError::Source(format!(
                "portrait workflow {filename} failed the pinned checksum"
            )));
        }
        downloaded.push((filename, expected, bytes));
    }

    let destination_root = root
        .join("user")
        .join("default")
        .join("workflows")
        .join("hoi4_portraits");
    if path_has_link_component(&destination_root) {
        return Err(AppError::PathSecurity(
            "the portrait workflow destination contains a link or junction".into(),
        ));
    }
    let mut conflicts = Vec::new();
    let mut skipped = Vec::new();
    for (filename, expected, _) in &downloaded {
        let destination = destination_root.join(filename);
        if destination.is_file() {
            if sha256_file(&destination).is_ok_and(|hash| hash == *expected) {
                skipped.push((*filename).into());
            } else {
                conflicts.push((*filename).into());
            }
        }
    }
    if !conflicts.is_empty() {
        return Ok(LocalPortraitInstallResult {
            status: "conflict".into(),
            installed: Vec::new(),
            skipped,
            conflicts: conflicts.clone(),
            message: format!(
                "Existing workflow files differ from the pinned revision; nothing was overwritten: {}.",
                conflicts.join(", ")
            ),
            canonical_commit: PORTRAIT_COMMIT.into(),
        });
    }

    fs::create_dir_all(&destination_root).map_err(|error| {
        AppError::Transaction(format!("create portrait workflow destination: {error}"))
    })?;
    let staging = destination_root
        .parent()
        .ok_or_else(|| {
            AppError::PathSecurity("portrait workflow destination has no parent".into())
        })?
        .join(format!(".hoi4-portrait-staging-{}", uuid::Uuid::new_v4()));
    if path_has_link_component(&staging) {
        return Err(AppError::PathSecurity(
            "portrait workflow staging path contains a link or junction".into(),
        ));
    }
    fs::create_dir(&staging).map_err(|error| {
        AppError::Transaction(format!("create portrait workflow staging: {error}"))
    })?;
    let mut installed = Vec::new();
    let result = (|| -> Result<(), AppError> {
        for (filename, expected, bytes) in &downloaded {
            if skipped.iter().any(|item| item == filename) {
                continue;
            }
            let staged = staging.join(filename);
            fs::write(&staged, bytes).map_err(|error| {
                AppError::Transaction(format!("stage portrait workflow {filename}: {error}"))
            })?;
            if !sha256_file(&staged).is_ok_and(|hash| hash == *expected) {
                return Err(AppError::Transaction(format!(
                    "staged portrait workflow could not be verified: {filename}"
                )));
            }
        }
        for (filename, _, _) in &downloaded {
            if skipped.iter().any(|item| item == filename) {
                continue;
            }
            fs::rename(staging.join(filename), destination_root.join(filename)).map_err(
                |error| {
                    AppError::Transaction(format!("apply portrait workflow {filename}: {error}"))
                },
            )?;
            installed.push((*filename).into());
        }
        Ok(())
    })();
    let _ = fs::remove_dir(&staging);
    result?;
    Ok(LocalPortraitInstallResult {
        status: "ready".into(),
        installed,
        skipped,
        conflicts: Vec::new(),
        message: "The pinned portrait UI workflows were installed. Model downloads remain a separate Hugging Face-gated setup step.".into(),
        canonical_commit: PORTRAIT_COMMIT.into(),
    })
}

fn find_local_root(configured_root: Option<&str>) -> LocalRoot {
    if let Some(value) = configured_root {
        let path = PathBuf::from(value);
        if !path.is_absolute() {
            return LocalRoot {
                configured: Some(value.into()),
                detected: None,
                error: Some("The configured ComfyUI root must be an absolute path.".into()),
            };
        }
        if path_has_link_component(&path) {
            return LocalRoot {
                configured: Some(value.into()),
                detected: None,
                error: Some("The configured ComfyUI root contains a link or junction and was not inspected.".into()),
            };
        }
        if is_comfyui_root(&path) {
            return LocalRoot {
                configured: Some(value.into()),
                detected: Some(path),
                error: None,
            };
        }
        return LocalRoot {
            configured: Some(value.into()),
            detected: None,
            error: Some(
                "The configured folder is not a ComfyUI checkout containing main.py and comfy/."
                    .into(),
            ),
        };
    }

    for candidate in candidate_roots() {
        if !path_has_link_component(&candidate) && is_comfyui_root(&candidate) {
            return LocalRoot {
                configured: None,
                detected: Some(candidate),
                error: None,
            };
        }
    }
    LocalRoot {
        configured: None,
        detected: None,
        error: None,
    }
}

fn candidate_roots() -> Vec<PathBuf> {
    #[cfg(any(target_os = "windows", target_os = "macos"))]
    let mut candidates = Vec::new();
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let candidates = Vec::new();
    #[cfg(target_os = "windows")]
    {
        if let Some(local_app_data) = env::var_os("LOCALAPPDATA") {
            candidates.push(PathBuf::from(local_app_data).join("ComfyUI"));
        }
        if let Some(user_profile) = env::var_os("USERPROFILE") {
            candidates.push(PathBuf::from(user_profile).join("ComfyUI"));
        }
        candidates.push(PathBuf::from(r"C:\ComfyUI"));
    }
    #[cfg(target_os = "macos")]
    {
        if let Some(home) = env::var_os("HOME") {
            let home = PathBuf::from(home);
            candidates.push(home.join("ComfyUI"));
            candidates.push(home.join("Applications").join("ComfyUI"));
        }
    }
    candidates
}

fn is_comfyui_root(root: &Path) -> bool {
    root.is_dir() && root.join("main.py").is_file() && root.join("comfy").is_dir()
}

fn workflow_status(root: &Path) -> String {
    let workflow_root = root
        .join("user")
        .join("default")
        .join("workflows")
        .join("hoi4_portraits");
    if WORKFLOW_FILES.iter().all(|(filename, expected)| {
        let path = workflow_root.join(filename);
        path.is_file() && sha256_file(&path).is_ok_and(|hash| hash == *expected)
    }) {
        "ready".into()
    } else {
        "missing".into()
    }
}

fn model_status(root: &Path, huggingface_access: bool) -> String {
    let complete = MODEL_FILES.iter().all(|(directory, filename, size)| {
        root.join("models")
            .join(directory)
            .join(filename)
            .metadata()
            .is_ok_and(|metadata| metadata.is_file() && metadata.len() == *size)
    });
    if complete {
        "ready".into()
    } else if !root
        .join("models")
        .join("diffusion_models")
        .join(MODEL_FILES[0].1)
        .is_file()
        && !huggingface_access
    {
        "needs_huggingface_access".into()
    } else {
        "missing".into()
    }
}

pub(crate) fn validate_loopback_url(raw: &str) -> Result<Url, String> {
    let url =
        Url::parse(raw.trim()).map_err(|_| "The local ComfyUI URL is invalid.".to_string())?;
    if url.scheme() != "http"
        || url.username() != ""
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || !matches!(url.host_str(), Some("127.0.0.1" | "localhost" | "::1"))
    {
        return Err("Local ComfyUI checks accept only an HTTP loopback URL.".into());
    }
    Ok(url)
}

fn check_server(raw: &str) -> (String, String) {
    let url = match validate_loopback_url(raw) {
        Ok(url) => url,
        Err(error) => return ("invalid".into(), error),
    };
    let endpoint = match url.join("system_stats") {
        Ok(endpoint) => endpoint,
        Err(_) => {
            return (
                "invalid".into(),
                "The local ComfyUI health URL could not be formed.".into(),
            )
        }
    };
    let client = match Client::builder().timeout(Duration::from_secs(3)).build() {
        Ok(client) => client,
        Err(_) => {
            return (
                "unreachable".into(),
                "The local ComfyUI HTTP client could not start.".into(),
            )
        }
    };
    match client.get(endpoint).send() {
        Ok(response) if response.status().is_success() => (
            "ready".into(),
            "The local ComfyUI health endpoint responded.".into(),
        ),
        Ok(response) => (
            "unreachable".into(),
            format!(
                "The local ComfyUI health endpoint returned HTTP {}.",
                response.status()
            ),
        ),
        Err(_) => (
            "unreachable".into(),
            "The local ComfyUI health endpoint could not be reached.".into(),
        ),
    }
}

fn check_hardware() -> (String, Option<String>, Option<f64>, String) {
    let executable = match find_path_executable(&["nvidia-smi.exe", "nvidia-smi"]) {
        Ok(executable) => executable,
        Err(_) => return ("needs_hardware".into(), None, None, "No verified NVIDIA GPU probe was available; local FLUX.2 portrait setup is not offered on this computer.".into()),
    };
    let spec = ProcessSpec {
        executable: executable.clone(),
        executable_sha256: None,
        args: vec![
            "--query-gpu=name,memory.total".into(),
            "--format=csv,noheader,nounits".into(),
        ],
        cwd: None,
        platform: Platform::current(),
        environment_names: Vec::new(),
        timeout_seconds: 5,
        max_output_bytes: 4096,
    };
    let result = match spec.run(&[executable], None) {
        Ok(result) if result.status_code == Some(0) && !result.timed_out => result,
        _ => {
            return (
                "needs_hardware".into(),
                None,
                None,
                "The verified NVIDIA GPU probe did not report a usable adapter.".into(),
            )
        }
    };
    let line = result.stdout.lines().find(|line| !line.trim().is_empty());
    let (name, memory_mb) = line
        .and_then(|line| {
            let mut parts = line.splitn(2, ',');
            let name = parts.next()?.trim().to_string();
            let memory = parts.next()?.trim().parse::<f64>().ok()?;
            Some((name, memory))
        })
        .unwrap_or_else(|| ("NVIDIA GPU".into(), 0.0));
    let vram_gb = memory_mb / 1024.0;
    if vram_gb < 18.0 {
        return (
            "needs_hardware".into(),
            Some(name),
            Some(vram_gb),
            format!("The detected GPU has {vram_gb:.1} GiB VRAM; the current local workflow needs about 18 GiB or more for a practical run."),
        );
    }
    (
        "ready".into(),
        Some(name),
        Some(vram_gb),
        format!("The detected GPU has {vram_gb:.1} GiB VRAM and passed the local workflow hardware check."),
    )
}

fn huggingface_access_hint() -> bool {
    if env::var_os("HF_TOKEN").is_some_and(|value| !value.is_empty()) {
        return true;
    }
    let home = env::var_os("HF_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("USERPROFILE").map(PathBuf::from))
        .or_else(|| env::var_os("HOME").map(PathBuf::from));
    home.map(|path| {
        path.join(".cache")
            .join("huggingface")
            .join("token")
            .is_file()
    })
    .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn loopback_validation_rejects_remote_and_credentials() {
        assert!(validate_loopback_url("https://127.0.0.1:8188").is_err());
        assert!(validate_loopback_url("http://example.com:8188").is_err());
        assert!(validate_loopback_url("http://user:secret@127.0.0.1:8188").is_err());
        assert!(validate_loopback_url("http://127.0.0.1:8188").is_ok());
    }

    #[test]
    fn workflow_status_requires_all_current_hashes() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        fs::create_dir_all(root.join("user/default/workflows/hoi4_portraits")).unwrap();
        for (filename, _) in WORKFLOW_FILES {
            fs::write(
                root.join("user/default/workflows/hoi4_portraits")
                    .join(filename),
                b"not-current",
            )
            .unwrap();
        }
        assert_eq!(workflow_status(root), "missing");
    }
}
