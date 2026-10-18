//! The optional HOI4 portrait-production workflow: its verified upstream
//! revision, the components each provider installs, and the non-secret
//! settings persisted with a plan and lock. Provider credentials never enter
//! these settings.

use crate::models::PortraitPipelineConfig;
use crate::AppError;
use serde::Deserialize;

pub const PORTRAIT_REPOSITORY: &str = "https://github.com/klimPaskov/comfyui-hoi4-portraits";
pub const PORTRAIT_BRANCH: &str = "codex/portrait-pipeline";
pub const PORTRAIT_COMMIT: &str = "b47222a77f2f6454704530865aa1441fad48bdd3";

/// The components an enabled provider installs, or `None` when disabled.
pub fn portrait_component_ids(provider: &str) -> Option<[&'static str; 4]> {
    let route = match provider {
        "cloud" => "workflow.portraits.cloud",
        "local" => "workflow.portraits.local",
        "runpod" => "workflow.portraits.runpod",
        _ => return None,
    };
    Some([
        "workflow.portraits.core",
        route,
        "workflow.portraits.subagent",
        "workflow.portraits.config",
    ])
}

/// The user's portrait choice before validation.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PortraitChoice {
    #[serde(default)]
    pub provider: String,
    #[serde(default)]
    pub preferred_workflow: Option<String>,
    #[serde(default)]
    pub local_comfyui_root: Option<String>,
    #[serde(default)]
    pub local_server_url: Option<String>,
    #[serde(default)]
    pub runpod_url: Option<String>,
    #[serde(default)]
    pub runpod_workspace: Option<String>,
}

/// An HTTPS address without credentials, query, or fragment.
pub fn is_plain_https_url(value: &str) -> bool {
    let Ok(url) = url::Url::parse(value) else {
        return false;
    };
    url.scheme() == "https"
        && url.username().is_empty()
        && url.password().is_none()
        && url.host_str().is_some()
        && url.query().is_none()
        && url.fragment().is_none()
}

fn bounded(value: Option<&str>, label: &str) -> Result<String, AppError> {
    let value = value.unwrap_or_default().trim();
    if value.len() > 1024 || value.chars().any(char::is_control) {
        return Err(AppError::InvalidInput(format!(
            "{label} is not a valid setting"
        )));
    }
    if crate::text::redact_secrets(value, &[]) != value {
        return Err(AppError::Credential(format!(
            "{label} contains credential-shaped text"
        )));
    }
    Ok(value.to_owned())
}

/// Validate a portrait choice into the settings a plan and lock record.
pub fn portrait_config(
    choice: Option<&PortraitChoice>,
) -> Result<PortraitPipelineConfig, AppError> {
    let default = PortraitChoice::default();
    let choice = choice.unwrap_or(&default);
    let provider = if choice.provider.trim().is_empty() {
        "disabled".to_owned()
    } else {
        choice.provider.trim().to_ascii_lowercase()
    };
    if !matches!(provider.as_str(), "cloud" | "local" | "runpod" | "disabled") {
        return Err(AppError::InvalidInput(
            "portrait provider must be Cloud, Local, RunPod, or Disabled".into(),
        ));
    }
    let enabled = provider != "disabled";
    let preferred_workflow = choice
        .preferred_workflow
        .as_deref()
        .unwrap_or("source")
        .trim()
        .to_ascii_lowercase();
    if !matches!(preferred_workflow.as_str(), "source" | "processing_only") {
        return Err(AppError::InvalidInput(
            "portrait workflow must be source or processing_only".into(),
        ));
    }
    let local_server_url = match choice.local_server_url.as_deref().map(str::trim) {
        Some(value) if !value.is_empty() => value.to_owned(),
        _ => "http://127.0.0.1:8188".into(),
    };
    if provider == "local" && !crate::image_generation::is_loopback_http_url(&local_server_url) {
        return Err(AppError::InvalidInput(
            "a local ComfyUI server must use a loopback address on this computer".into(),
        ));
    }
    let runpod_url = bounded(choice.runpod_url.as_deref(), "The RunPod address")?;
    if provider == "runpod" && !runpod_url.is_empty() && !is_plain_https_url(&runpod_url) {
        return Err(AppError::InvalidInput(
            "RunPod portrait URLs must use HTTPS without credentials or query data".into(),
        ));
    }
    let runpod_workspace =
        match bounded(choice.runpod_workspace.as_deref(), "The RunPod workspace")? {
            value if value.is_empty() => "/workspace/comfyui-hoi4-portraits".into(),
            value => value,
        };
    Ok(PortraitPipelineConfig {
        enabled,
        provider_status: match provider.as_str() {
            "cloud" => "needs_authorization",
            "local" => "needs_hardware",
            "runpod" => "needs_runpod",
            _ => "not_selected",
        }
        .into(),
        workflow_repository: PORTRAIT_REPOSITORY.into(),
        workflow_branch: PORTRAIT_BRANCH.into(),
        workflow_commit: PORTRAIT_COMMIT.into(),
        preferred_workflow,
        local_comfyui_root: bounded(choice.local_comfyui_root.as_deref(), "The ComfyUI folder")?,
        local_server_url,
        runpod_url,
        runpod_workspace,
        mcp_registered: provider == "cloud",
        provider,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn providers_install_their_route_and_validate_addresses() {
        assert_eq!(
            portrait_component_ids("local").unwrap()[1],
            "workflow.portraits.local"
        );
        assert!(portrait_component_ids("disabled").is_none());
        let disabled = portrait_config(None).unwrap();
        assert!(!disabled.enabled && disabled.provider == "disabled");
        let cloud = portrait_config(Some(&PortraitChoice {
            provider: "Cloud".into(),
            ..Default::default()
        }))
        .unwrap();
        assert!(
            cloud.enabled && cloud.mcp_registered && cloud.provider_status == "needs_authorization"
        );
        assert!(portrait_config(Some(&PortraitChoice {
            provider: "local".into(),
            local_server_url: Some("http://192.168.1.5:8188".into()),
            ..Default::default()
        }))
        .is_err());
        assert!(portrait_config(Some(&PortraitChoice {
            provider: "runpod".into(),
            runpod_url: Some("https://user:pass@pod.example".into()),
            ..Default::default()
        }))
        .is_err());
        assert!(portrait_config(Some(&PortraitChoice {
            provider: "midjourney".into(),
            ..Default::default()
        }))
        .is_err());
        assert!(is_plain_https_url("https://abc-8188.proxy.runpod.net"));
        assert!(!is_plain_https_url("http://abc.example"));
    }
}
