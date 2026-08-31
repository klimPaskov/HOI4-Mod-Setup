//! Coding-client selection and native package mapping.
//!
//! Coding environments are deliberately independent from the setup assistant
//! AI provider and from workflow profiles.  The source manifest declares the
//! component closure for each environment; this module only validates the
//! user's small, non-secret selection and resolves those declarations.

use crate::models::{CodingEnvironmentSelection, RemoteManifest};
use crate::AppError;
use serde_json::Value;
use std::collections::HashSet;

pub const CODEX: &str = "codex";
pub const CLAUDE_CODE: &str = "claude_code";
pub const CURSOR: &str = "cursor";
pub const QODER: &str = "qoder";
pub const OPENCODE: &str = "opencode";

pub const SUPPORTED: [&str; 5] = [CODEX, CLAUDE_CODE, CURSOR, QODER, OPENCODE];

pub fn default_selection() -> CodingEnvironmentSelection {
    CodingEnvironmentSelection {
        primary: CODEX.into(),
        additional: Vec::new(),
    }
}

pub fn is_supported(value: &str) -> bool {
    SUPPORTED.contains(&value)
}

pub fn validate_selection(selection: &CodingEnvironmentSelection) -> Result<(), AppError> {
    if !is_supported(selection.primary.as_str()) {
        return Err(AppError::InvalidInput(format!(
            "unsupported coding environment: {}",
            selection.primary
        )));
    }
    let mut seen = HashSet::new();
    for environment in &selection.additional {
        if !is_supported(environment) {
            return Err(AppError::InvalidInput(format!(
                "unsupported additional coding environment: {environment}"
            )));
        }
        if environment == &selection.primary {
            return Err(AppError::InvalidInput(
                "the primary coding environment cannot also be additional".into(),
            ));
        }
        if !seen.insert(environment.as_str()) {
            return Err(AppError::InvalidInput(format!(
                "additional coding environment is duplicated: {environment}"
            )));
        }
    }
    Ok(())
}

/// Read the renderer's camelCase state while accepting the snake_case shape
/// used by persisted locks and older integrations. Missing fields are a
/// compatibility default, never an implicit second primary.
pub fn selection_from_value(value: &Value) -> Result<CodingEnvironmentSelection, AppError> {
    let nested = value.get("coding_environments");
    let primary = value
        .get("primaryCodingEnvironment")
        .or_else(|| value.get("primary_coding_environment"))
        .or_else(|| nested.and_then(|value| value.get("primary")))
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(CODEX)
        .to_string();
    let additional = value
        .get("additionalCodingEnvironments")
        .or_else(|| value.get("additional_coding_environments"))
        .or_else(|| nested.and_then(|value| value.get("additional")))
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(Value::as_str)
                .map(ToOwned::to_owned)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let selection = CodingEnvironmentSelection {
        primary,
        additional,
    };
    validate_selection(&selection)?;
    Ok(selection)
}

/// Resolve all manifest components belonging to the selected environments.
/// A package is considered incomplete when the manifest does not declare any
/// component for a requested environment; this prevents a misleading partial
/// install when an older source revision predates a client package.
pub fn component_ids(
    manifest: &RemoteManifest,
    selection: &CodingEnvironmentSelection,
    active_component_ids: &HashSet<String>,
) -> Result<Vec<String>, AppError> {
    validate_selection(selection)?;
    let optional_ids = manifest
        .components
        .iter()
        .filter(|component| component.optional)
        .map(|component| component.id.as_str())
        .collect::<HashSet<_>>();
    let mut result = Vec::new();
    let mut requested = vec![selection.primary.as_str()];
    requested.extend(selection.additional.iter().map(String::as_str));
    for environment in requested {
        let ids = manifest
            .components
            .iter()
            .filter(|component| component.coding_environment.as_deref() == Some(environment))
            .filter(|component| {
                if !component.optional {
                    return true;
                }
                let activation_dependencies = component
                    .dependencies
                    .iter()
                    .filter(|dependency| optional_ids.contains(dependency.as_str()))
                    .collect::<Vec<_>>();
                !activation_dependencies.is_empty()
                    && activation_dependencies
                        .iter()
                        .all(|dependency| active_component_ids.contains(dependency.as_str()))
            })
            .map(|component| component.id.clone())
            .collect::<Vec<_>>();
        if ids.is_empty() {
            return Err(AppError::Source(format!(
                "the verified source manifest does not declare a complete {environment} coding-environment package"
            )));
        }
        result.extend(ids);
    }
    result.sort();
    result.dedup();
    Ok(result)
}

pub fn all_environment_component_ids(manifest: &RemoteManifest) -> HashSet<String> {
    manifest
        .components
        .iter()
        .filter(|component| component.coding_environment.is_some())
        .map(|component| component.id.clone())
        .collect()
}

pub fn selected_environment_ids(selection: &CodingEnvironmentSelection) -> Vec<String> {
    let mut result = vec![selection.primary.clone()];
    result.extend(selection.additional.clone());
    result
}

/// Compatibility helper for lock reconciliation. Future source manifests may
/// add more environment component IDs; current IDs are kept here so a
/// deselected package cannot be resurrected by a legacy lock append.
pub fn is_known_environment_component_id(id: &str) -> bool {
    matches!(
        id,
        "codex.config"
            | "mcp.hoi4_agent_tools"
            | "core.claude.instructions"
            | "runtime.claude"
            | "runtime.claude.mcp"
            | "runtime.cursor"
            | "runtime.cursor.mcp"
            | "runtime.qoder"
            | "runtime.qoder.mcp"
            | "runtime.opencode"
            | "runtime.opencode.config"
            | "runtime.opencode.mcp"
            | "runtime.claude.portrait_agent"
            | "runtime.claude.super_event_agents"
            | "runtime.cursor.portrait_agent"
            | "runtime.cursor.super_event_agents"
            | "runtime.qoder.portrait_agent"
            | "runtime.qoder.super_event_agents"
            | "runtime.opencode.portrait_agent"
            | "runtime.opencode.super_event_agents"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn codex_is_the_default_primary_environment() {
        assert_eq!(default_selection().primary, CODEX);
        assert!(default_selection().additional.is_empty());
    }

    #[test]
    fn primary_cannot_be_repeated_as_an_additional_environment() {
        let error = validate_selection(&CodingEnvironmentSelection {
            primary: CURSOR.into(),
            additional: vec![CURSOR.into()],
        })
        .unwrap_err();
        assert!(error.to_string().contains("cannot also be additional"));
    }

    #[test]
    fn persisted_nested_state_is_read_and_validated() {
        let selection = selection_from_value(&json!({
            "coding_environments": {
                "primary": "claude_code",
                "additional": ["cursor", "opencode"]
            }
        }))
        .unwrap();
        assert_eq!(selection.primary, CLAUDE_CODE);
        assert_eq!(selection.additional, vec![CURSOR, OPENCODE]);
    }

    fn published_manifest() -> RemoteManifest {
        serde_json::from_slice(include_bytes!(
            "../../docs/source-manifest/hoi4-mod-setup.manifest.json"
        ))
        .expect("published manifest fixture parses")
    }

    #[test]
    fn every_primary_and_additional_environment_combination_has_a_complete_package() {
        let manifest = published_manifest();
        for (primary_index, primary) in SUPPORTED.iter().enumerate() {
            let additional_candidates = SUPPORTED
                .iter()
                .enumerate()
                .filter_map(|(index, environment)| (index != primary_index).then_some(*environment))
                .collect::<Vec<_>>();
            for mask in 0..(1usize << additional_candidates.len()) {
                let additional = additional_candidates
                    .iter()
                    .enumerate()
                    .filter_map(|(index, environment)| {
                        (mask & (1usize << index) != 0).then_some((*environment).to_string())
                    })
                    .collect::<Vec<_>>();
                let selection = CodingEnvironmentSelection {
                    primary: (*primary).into(),
                    additional,
                };
                let active = ["mcp.hoi4_agent_tools".to_string()]
                    .into_iter()
                    .collect::<HashSet<_>>();
                let ids = component_ids(&manifest, &selection, &active)
                    .expect("every published environment combination is declared");
                for environment in selected_environment_ids(&selection) {
                    assert!(ids.iter().any(|id| {
                        manifest.components.iter().any(|component| {
                            component.id == id.as_str()
                                && component.coding_environment.as_deref()
                                    == Some(environment.as_str())
                        })
                    }));
                }
                assert_eq!(ids.len(), ids.iter().collect::<HashSet<_>>().len());
            }
        }
    }

    #[test]
    fn optional_environment_projections_require_their_workflow_dependency() {
        let manifest = published_manifest();
        let selection = CodingEnvironmentSelection {
            primary: CURSOR.into(),
            additional: vec![],
        };
        let shared = ["mcp.hoi4_agent_tools".to_string()]
            .into_iter()
            .collect::<HashSet<_>>();
        let core = component_ids(&manifest, &selection, &shared).unwrap();
        assert!(core.iter().any(|id| id == "runtime.cursor.mcp"));
        assert!(!core.iter().any(|id| id == "runtime.cursor.portrait_agent"));
        assert!(!core
            .iter()
            .any(|id| id == "runtime.cursor.super_event_agents"));

        let active = [
            "mcp.hoi4_agent_tools".to_string(),
            "workflow.portraits.subagent".to_string(),
            "workflow.super_events.subagents".to_string(),
        ]
        .into_iter()
        .collect::<HashSet<_>>();
        let optional = component_ids(&manifest, &selection, &active).unwrap();
        assert!(optional
            .iter()
            .any(|id| id == "runtime.cursor.portrait_agent"));
        assert!(optional
            .iter()
            .any(|id| id == "runtime.cursor.super_event_agents"));
    }
}
