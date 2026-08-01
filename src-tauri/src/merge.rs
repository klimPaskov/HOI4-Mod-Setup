use crate::models::{LocalState, PlanConflict};
use crate::security::sha256_bytes;
use crate::AppError;
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileKind {
    Text,
    Toml,
    Json,
    Binary,
    Symlink,
    Directory,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MergeClassification {
    Create,
    ReplaceUnmodified,
    KeepLocal,
    AlreadyCurrent,
    Conflict,
    UserOwnedConflict,
}

#[derive(Debug, Clone)]
pub struct ConflictDecision {
    pub path: String,
    pub kind: FileKind,
    pub classification: MergeClassification,
    pub options: Vec<String>,
    pub selected: Option<String>,
    pub base_sha256: Option<String>,
    pub local_sha256: Option<String>,
    pub incoming_sha256: Option<String>,
}

pub fn classify(
    base: Option<&[u8]>,
    local: Option<&[u8]>,
    incoming: Option<&[u8]>,
) -> MergeClassification {
    match (base, local, incoming) {
        (None, None, Some(_)) => MergeClassification::Create,
        (Some(base), Some(local), Some(_incoming)) if *local == *base => {
            MergeClassification::ReplaceUnmodified
        }
        (Some(base), Some(_local), Some(incoming)) if incoming == base => {
            MergeClassification::KeepLocal
        }
        (Some(_), Some(local), Some(incoming)) if *local == *incoming => {
            MergeClassification::AlreadyCurrent
        }
        (None, Some(_), Some(_)) => MergeClassification::UserOwnedConflict,
        (Some(_), None, Some(_)) => MergeClassification::Conflict,
        (_, _, None) => MergeClassification::Conflict,
        _ => MergeClassification::Conflict,
    }
}

pub fn allowed_choices(kind: FileKind, classification: MergeClassification) -> Vec<String> {
    if matches!(
        classification,
        MergeClassification::Create
            | MergeClassification::ReplaceUnmodified
            | MergeClassification::KeepLocal
            | MergeClassification::AlreadyCurrent
    ) {
        return Vec::new();
    }
    let mut choices = vec!["keep", "replace", "rename", "skip"];
    if matches!(kind, FileKind::Text | FileKind::Toml | FileKind::Json)
        && classification != MergeClassification::UserOwnedConflict
    {
        choices.insert(2, "merge");
    }
    choices.into_iter().map(str::to_string).collect()
}

pub fn build_conflict(
    path: &str,
    kind: FileKind,
    base: Option<&[u8]>,
    local: Option<&[u8]>,
    incoming: Option<&[u8]>,
) -> ConflictDecision {
    let classification = classify(base, local, incoming);
    let mut options = allowed_choices(kind, classification);
    if kind == FileKind::Json && !json_conflict_supports_structured_merge(base, local, incoming) {
        options.retain(|choice| choice != "merge");
    }
    ConflictDecision {
        path: path.into(),
        kind,
        classification,
        options,
        selected: None,
        base_sha256: base.map(sha256_bytes),
        local_sha256: local.map(sha256_bytes),
        incoming_sha256: incoming.map(sha256_bytes),
    }
}

fn json_conflict_supports_structured_merge(
    base: Option<&[u8]>,
    local: Option<&[u8]>,
    incoming: Option<&[u8]>,
) -> bool {
    [base, local, incoming]
        .into_iter()
        .flatten()
        .map(serde_json::from_slice::<Value>)
        .all(|value| value.is_ok_and(|value| !json_contains_array(&value)))
}

fn json_contains_array(value: &Value) -> bool {
    match value {
        Value::Array(_) => true,
        Value::Object(object) => object.values().any(json_contains_array),
        _ => false,
    }
}

pub fn to_plan_conflict(decision: &ConflictDecision, selected: Option<String>) -> PlanConflict {
    PlanConflict {
        id: format!("conflict.{}", &sha256_bytes(decision.path.as_bytes())[..16]),
        path: decision.path.clone(),
        options: decision.options.clone(),
        selected,
        apply_to_identical: false,
    }
}

pub fn three_way_merge(base: &str, local: &str, incoming: &str) -> Result<String, AppError> {
    if *local == *base {
        return Ok(incoming.to_string());
    }
    if *incoming == *base || *local == *incoming {
        return Ok(local.to_string());
    }
    let base_lines: Vec<&str> = base.lines().collect();
    let local_lines: Vec<&str> = local.lines().collect();
    let incoming_lines: Vec<&str> = incoming.lines().collect();
    if base_lines.len() == local_lines.len() && base_lines.len() == incoming_lines.len() {
        let mut merged = Vec::with_capacity(base_lines.len());
        for index in 0..base_lines.len() {
            let base_line = base_lines[index];
            let local_line = local_lines[index];
            let incoming_line = incoming_lines[index];
            if local_line == base_line {
                merged.push(incoming_line);
            } else if incoming_line == base_line || local_line == incoming_line {
                merged.push(local_line);
            } else {
                return Err(AppError::Merge(format!(
                    "overlapping edit at line {}",
                    index + 1
                )));
            }
        }
        return Ok(merged.join("\n"));
    }
    Err(AppError::Merge(
        "non-overlapping line merge could not be proven safe".into(),
    ))
}

pub fn structured_toml_merge(base: &str, local: &str, incoming: &str) -> Result<String, AppError> {
    let base_value: toml::Value = base
        .parse()
        .map_err(|error| AppError::Merge(format!("base TOML: {error}")))?;
    let mut local_value: toml::Value = local
        .parse()
        .map_err(|error| AppError::Merge(format!("local TOML: {error}")))?;
    let incoming_value: toml::Value = incoming
        .parse()
        .map_err(|error| AppError::Merge(format!("incoming TOML: {error}")))?;
    merge_toml_value(&base_value, &mut local_value, &incoming_value, "")?;
    Ok(local_value.to_string())
}

fn merge_toml_value(
    base: &toml::Value,
    local: &mut toml::Value,
    incoming: &toml::Value,
    path: &str,
) -> Result<(), AppError> {
    match (base, local, incoming) {
        (toml::Value::Table(base), toml::Value::Table(local), toml::Value::Table(incoming)) => {
            for (key, incoming_value) in incoming {
                let child_path = if path.is_empty() {
                    key.clone()
                } else {
                    format!("{path}.{key}")
                };
                match (base.get(key), local.get(key)) {
                    (None, None) => {
                        local.insert(key.clone(), incoming_value.clone());
                    }
                    (Some(base_value), Some(local_value)) if local_value == base_value => {
                        local.insert(key.clone(), incoming_value.clone());
                    }
                    (Some(base_value), Some(local_value))
                        if incoming_value == base_value || incoming_value == local_value => {}
                    (Some(base_value), Some(local_value)) => {
                        let mut local_clone = local_value.clone();
                        merge_toml_value(
                            base_value,
                            &mut local_clone,
                            incoming_value,
                            &child_path,
                        )?;
                        local.insert(key.clone(), local_clone);
                    }
                    (Some(_), None) => {
                        return Err(AppError::Merge(format!("local TOML removed {child_path}")))
                    }
                    (None, Some(_)) => {
                        return Err(AppError::Merge(format!(
                            "incoming TOML adds conflicting {child_path}"
                        )))
                    }
                }
            }
            Ok(())
        }
        (_, local, incoming) if *local == *incoming => Ok(()),
        (base, local, incoming) if *local == *base => {
            *local = incoming.clone();
            Ok(())
        }
        (base, _, incoming) if incoming == base => Ok(()),
        _ => Err(AppError::Merge(format!("conflicting TOML value at {path}"))),
    }
}

pub fn structured_json_merge(base: &str, local: &str, incoming: &str) -> Result<String, AppError> {
    let base_value: Value = serde_json::from_str(base)
        .map_err(|error| AppError::Merge(format!("base JSON: {error}")))?;
    let mut local_value: Value = serde_json::from_str(local)
        .map_err(|error| AppError::Merge(format!("local JSON: {error}")))?;
    let incoming_value: Value = serde_json::from_str(incoming)
        .map_err(|error| AppError::Merge(format!("incoming JSON: {error}")))?;
    merge_json_value(&base_value, &mut local_value, &incoming_value, "$")?;
    serde_json::to_string_pretty(&local_value).map_err(|error| AppError::Merge(error.to_string()))
}

fn merge_json_value(
    base: &Value,
    local: &mut Value,
    incoming: &Value,
    path: &str,
) -> Result<(), AppError> {
    if *local == *incoming {
        return Ok(());
    }
    if *local == *base {
        *local = incoming.clone();
        return Ok(());
    }
    if incoming == base {
        return Ok(());
    }
    match (base, local, incoming) {
        (Value::Object(base), Value::Object(local), Value::Object(incoming)) => {
            for (key, incoming_value) in incoming {
                let child = format!("{path}.{key}");
                match (base.get(key), local.get_mut(key)) {
                    (None, None) => {
                        local.insert(key.clone(), incoming_value.clone());
                    }
                    (Some(base_value), Some(local_value)) => {
                        merge_json_value(base_value, local_value, incoming_value, &child)?
                    }
                    (Some(_), None) => {
                        return Err(AppError::Merge(format!("local JSON removed {child}")))
                    }
                    (None, Some(_)) => {
                        return Err(AppError::Merge(format!(
                            "incoming JSON adds conflicting {child}"
                        )))
                    }
                }
            }
            Ok(())
        }
        (Value::Array(_), Value::Array(_), Value::Array(_)) => Err(AppError::Merge(format!(
            "conflicting JSON array at {path} requires a declared identity key"
        ))),
        _ => Err(AppError::Merge(format!("conflicting JSON value at {path}"))),
    }
}

pub fn validate_merged_result(kind: FileKind, path: &str, bytes: &[u8]) -> Result<(), AppError> {
    if bytes.windows(7).any(|window| window == b"<<<<<<<")
        || bytes.windows(7).any(|window| window == b">>>>>>>")
        || bytes.windows(7).any(|window| window == b"=======")
    {
        return Err(AppError::Merge(format!(
            "unresolved merge markers in {path}"
        )));
    }
    match kind {
        FileKind::Toml => {
            let text =
                std::str::from_utf8(bytes).map_err(|error| AppError::Merge(error.to_string()))?;
            text.parse::<toml::Value>()
                .map_err(|error| AppError::Merge(format!("invalid TOML result: {error}")))?;
        }
        FileKind::Json => {
            serde_json::from_slice::<Value>(bytes)
                .map_err(|error| AppError::Merge(format!("invalid JSON result: {error}")))?;
        }
        FileKind::Text => {
            let text =
                std::str::from_utf8(bytes).map_err(|error| AppError::Merge(error.to_string()))?;
            if text.contains("{{PROJECT_") || text.contains("<PROJECT_") {
                return Err(AppError::Merge(format!(
                    "unresolved template token in {path}"
                )));
            }
        }
        FileKind::Binary | FileKind::Symlink | FileKind::Directory => {}
    }
    Ok(())
}

pub fn local_state_from_bytes(base: Option<&[u8]>, local: Option<&[u8]>) -> LocalState {
    match (base, local) {
        (None, None) => LocalState::Absent,
        (Some(base), Some(local)) if base == local => LocalState::Unmodified,
        (Some(_), Some(_)) => LocalState::Modified,
        (None, Some(_)) => LocalState::Modified,
        (Some(_), None) => LocalState::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modified_text_offers_merge_but_binary_does_not() {
        let decision = build_conflict(
            "file.txt",
            FileKind::Text,
            Some(b"base"),
            Some(b"local"),
            Some(b"incoming"),
        );
        assert!(decision.options.contains(&"merge".into()));
        let binary = build_conflict(
            "file.bin",
            FileKind::Binary,
            Some(b"base"),
            Some(b"local"),
            Some(b"incoming"),
        );
        assert!(!binary.options.contains(&"merge".into()));
    }

    #[test]
    fn simple_three_way_merge_is_safe() {
        let merged = three_way_merge("a\nb\n", "a\nlocal\n", "a\nincoming\n");
        assert!(merged.is_err());
        assert_eq!(
            three_way_merge("a\nb\n", "a\nb\n", "a\nc\n").unwrap(),
            "a\nc\n"
        );
    }

    #[test]
    fn structured_toml_merge_keeps_local_and_incoming_disjoint_keys() {
        let result = structured_toml_merge(
            "[server]\na = 1\n",
            "[server]\na = 1\nb = 2\n",
            "[server]\na = 1\nc = 3\n",
        )
        .unwrap();
        assert!(result.contains("b = 2"));
        assert!(result.contains("c = 3"));
    }

    #[test]
    fn json_array_conflicts_do_not_offer_or_apply_positional_merge() {
        let base = br#"{"items":[{"id":"a","value":1},{"id":"b","value":1}]}"#;
        let local = br#"{"items":[{"id":"b","value":2},{"id":"a","value":1}]}"#;
        let incoming = br#"{"items":[{"id":"a","value":3},{"id":"b","value":1}]}"#;
        let decision = build_conflict(
            "settings.json",
            FileKind::Json,
            Some(base),
            Some(local),
            Some(incoming),
        );
        assert!(!decision.options.contains(&"merge".into()));
        assert!(structured_json_merge(
            std::str::from_utf8(base).unwrap(),
            std::str::from_utf8(local).unwrap(),
            std::str::from_utf8(incoming).unwrap(),
        )
        .unwrap_err()
        .to_string()
        .contains("requires a declared identity key"));
    }

    #[test]
    fn json_objects_without_arrays_still_offer_structured_merge() {
        let decision = build_conflict(
            "settings.json",
            FileKind::Json,
            Some(br#"{"base":true,"local":false,"incoming":false}"#),
            Some(br#"{"base":true,"local":true,"incoming":false}"#),
            Some(br#"{"base":true,"local":false,"incoming":true}"#),
        );
        assert!(decision.options.contains(&"merge".into()));
    }
}
