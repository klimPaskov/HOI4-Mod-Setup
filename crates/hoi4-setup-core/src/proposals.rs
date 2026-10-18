//! Values the files decide, so the setup assistant does not have to guess
//! them: descriptor tags and name, the dominant event namespace, the
//! localisation convention, the existing content folders, and the
//! documentation style. They are shown as Detected; an assistant may only
//! fill the gaps, and a deterministic-only setup uses them directly.

use crate::descriptors::HOI4_DESCRIPTOR_TAGS;
use crate::models::ScanFinding;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// One value read from the project's own files.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DetectedValue {
    /// The semantic proposal key it decides, such as `descriptor_tags`.
    pub key: String,
    pub value: Value,
    /// Which files or findings decided it, in plain words.
    pub reason: String,
}

fn finding<'a>(findings: &'a [ScanFinding], id: &str) -> Option<&'a Value> {
    findings
        .iter()
        .find(|finding| finding.id == id)
        .map(|finding| &finding.value)
}

fn valid_identifier(value: &str) -> bool {
    let mut characters = value.chars();
    characters
        .next()
        .is_some_and(|first| first.is_ascii_lowercase() || first == '_')
        && characters.all(|character| {
            character.is_ascii_lowercase() || character.is_ascii_digit() || character == '_'
        })
        && value.len() <= 64
}

/// The official tags in a descriptor `tags = { "A" "B" }` value.
pub fn descriptor_tags(raw: &str) -> Vec<String> {
    let mut tags = Vec::new();
    for quoted in raw.split('"').skip(1).step_by(2) {
        if let Some(official) = HOI4_DESCRIPTOR_TAGS
            .iter()
            .find(|tag| tag.eq_ignore_ascii_case(quoted.trim()))
        {
            if !tags.iter().any(|known: &String| known == official) {
                tags.push((*official).to_owned());
            }
        }
    }
    tags
}

fn first_values(value: &Value) -> Vec<(String, u64)> {
    value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|item| {
            Some((
                item.get("value")?.as_str()?.to_owned(),
                item.get("sample_files")?.as_u64()?,
            ))
        })
        .collect()
}

/// Folders a profile category stands for, as a starter-folder path.
fn category_folder(category: &str) -> Option<String> {
    let (root, rest) = category.split_once('/')?;
    match (root, rest) {
        ("common" | "history", "root") => Some(root.to_owned()),
        ("common" | "history", sub) => Some(format!("{root}/{sub}")),
        ("events" | "decisions" | "interface" | "map", _) => Some(root.to_owned()),
        ("localisation", _) => Some("localisation".into()),
        _ => None,
    }
}

/// Every value the scan findings decide.
pub fn detect(findings: &[ScanFinding]) -> Vec<DetectedValue> {
    let mut detected = Vec::new();
    if let Some(name) = finding(findings, "descriptor.name").and_then(Value::as_str) {
        if !name.trim().is_empty() {
            detected.push(DetectedValue {
                key: "display_name".into(),
                value: json!(name.trim()),
                reason: "The name field of descriptor.mod.".into(),
            });
        }
    }
    if let Some(tags) = finding(findings, "descriptor.metadata")
        .and_then(|metadata| metadata.get("tags"))
        .and_then(Value::as_str)
        .map(descriptor_tags)
        .filter(|tags| !tags.is_empty())
    {
        detected.push(DetectedValue {
            key: "descriptor_tags".into(),
            value: json!(tags),
            reason: "The tags field of descriptor.mod.".into(),
        });
    }
    if let Some(identifiers) = finding(findings, "profile.identifiers") {
        let namespaces = first_values(&identifiers["namespace_declarations"]);
        if let Some((namespace, files)) = namespaces
            .iter()
            .find(|(namespace, _)| valid_identifier(namespace))
        {
            let reason = format!(
                "The most common event namespace in the sampled files ({files} file{}).",
                if *files == 1 { "" } else { "s" }
            );
            detected.push(DetectedValue {
                key: "primary_namespace".into(),
                value: json!(namespace),
                reason: reason.clone(),
            });
            detected.push(DetectedValue {
                key: "script_prefix".into(),
                value: json!(namespace),
                reason,
            });
        }
    }
    if let Some(localisation) = finding(findings, "profile.localisation") {
        let languages = first_values(&localisation["language_headers"])
            .into_iter()
            .map(|(language, _)| language)
            .take(8)
            .collect::<Vec<_>>();
        let count = |key: &str| localisation[key].as_u64().unwrap_or(0);
        if !languages.is_empty() {
            let encoding = if count("utf8_bom") > 0 && count("utf8_without_bom") == 0 {
                "UTF-8 with BOM"
            } else if count("utf8_bom") > 0 {
                "mostly UTF-8 with BOM"
            } else {
                "UTF-8 without BOM"
            };
            let keys = if count("versioned_quoted_keys") >= count("unversioned_quoted_keys") {
                "versioned keys such as key:0"
            } else {
                "unversioned keys"
            };
            let mut text = format!("{}; {encoding}; {keys}", languages.join(", "));
            text.truncate(256);
            detected.push(DetectedValue {
                key: "localisation_convention".into(),
                value: json!(text),
                reason: "Language headers and encodings of the sampled localisation files.".into(),
            });
        }
    }
    if let Some(structure) = finding(findings, "profile.structure") {
        let mut folders = Vec::new();
        for category in structure["content_categories"]
            .as_array()
            .into_iter()
            .flatten()
        {
            if let Some(folder) = category["category"].as_str().and_then(category_folder) {
                if !folders.contains(&folder) && folders.len() < 32 {
                    folders.push(folder);
                }
            }
        }
        if !folders.is_empty() {
            folders.sort();
            detected.push(DetectedValue {
                key: "folder_profile".into(),
                value: json!(folders),
                reason: "Content folders that already hold game files.".into(),
            });
        }
    }
    if let Some(documentation) = finding(findings, "profile.documentation") {
        let documents = documentation["documents"]
            .as_array()
            .map(Vec::len)
            .unwrap_or(0);
        if documents > 0 {
            let in_docs = documentation["documents"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|document| document["path"].as_str())
                .any(|path| path.starts_with("docs/") || path.starts_with("documentation/"));
            detected.push(DetectedValue {
                key: "documentation_convention".into(),
                value: json!(if in_docs {
                    "Markdown documentation in docs/"
                } else {
                    "A README at the project root"
                }),
                reason: "The documentation files found in the project.".into(),
            });
        }
    }
    detected
}

/// A lowercase identifier derived from a display name, for a new mod set up
/// without an assistant: ASCII letters and digits joined by underscores.
pub fn identifier_from_name(name: &str) -> String {
    let mut identifier = String::new();
    for character in name.chars() {
        if character.is_ascii_alphanumeric() {
            identifier.push(character.to_ascii_lowercase());
        } else if !identifier.ends_with('_') && !identifier.is_empty() {
            identifier.push('_');
        }
    }
    let mut identifier = identifier.trim_matches('_').to_owned();
    if identifier
        .chars()
        .next()
        .is_none_or(|first| !first.is_ascii_lowercase())
    {
        identifier.insert_str(0, "mod_");
    }
    identifier.truncate(63);
    identifier.trim_end_matches('_').to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory_fs::MemoryFs;
    use crate::models::ScanEvidence;

    fn plain_finding(id: &str, value: Value) -> ScanFinding {
        ScanFinding {
            id: id.into(),
            category: "descriptor".into(),
            key: id.into(),
            value,
            model_excerpt: None,
            status: "accepted".into(),
            origin: "deterministic".into(),
            user_value: None,
            confidence: 1.0,
            blocking: false,
            proposed_action: None,
            decision_state: "pending".into(),
            evidence: Vec::<ScanEvidence>::new(),
            recommendation: None,
        }
    }

    #[test]
    fn values_the_files_decide_are_detected() {
        let mut fs = MemoryFs::new();
        fs.add_file("README.md", b"# Demo\nA mod.\n".to_vec())
            .unwrap();
        fs.add_file(
            "events/demo.txt",
            b"add_namespace = demo_events\ncountry_event = { id = GER_demo.1 }\n".to_vec(),
        )
        .unwrap();
        fs.add_file(
            "common/national_focus/demo.txt",
            b"focus_tree = { id = demo_focus }\n".to_vec(),
        )
        .unwrap();
        fs.add_file(
            "localisation/english/demo_l_english.yml",
            b"\xef\xbb\xbfl_english:\n demo.1.t:0 \"Title\"\n".to_vec(),
        )
        .unwrap();
        let mut findings = crate::profile::profile(&fs, &|| false, &mut |_, _| {})
            .unwrap()
            .findings;
        findings.push(plain_finding("descriptor.name", json!("Demo Mod")));
        findings.push(plain_finding(
            "descriptor.metadata",
            json!({"tags": "{ \"gameplay\" \"Total Conversion\" \"National Focuses\" }"}),
        ));
        let detected = detect(&findings);
        let value = |key: &str| {
            detected
                .iter()
                .find(|value| value.key == key)
                .unwrap_or_else(|| panic!("{key} not detected"))
                .value
                .clone()
        };
        assert_eq!(value("display_name"), json!("Demo Mod"));
        assert_eq!(
            value("descriptor_tags"),
            json!(["Gameplay", "National Focuses"])
        );
        assert_eq!(value("primary_namespace"), json!("demo_events"));
        assert_eq!(value("script_prefix"), json!("demo_events"));
        assert_eq!(
            value("localisation_convention"),
            json!("english; UTF-8 with BOM; versioned keys such as key:0")
        );
        assert_eq!(
            value("folder_profile"),
            json!(["common/national_focus", "events", "localisation"])
        );
        assert_eq!(
            value("documentation_convention"),
            json!("A README at the project root")
        );
    }

    #[test]
    fn nothing_is_invented_for_an_empty_project() {
        let fs = MemoryFs::new();
        let findings = crate::profile::profile(&fs, &|| false, &mut |_, _| {})
            .unwrap()
            .findings;
        assert!(detect(&findings).is_empty());
    }

    #[test]
    fn identifiers_from_names_are_valid_project_ids() {
        assert_eq!(
            identifier_from_name("The Great War: Redux!"),
            "the_great_war_redux"
        );
        assert_eq!(identifier_from_name("1914"), "mod_1914");
        assert_eq!(identifier_from_name("   "), "mod");
        assert!(
            crate::descriptors::validate_project_id(&identifier_from_name("Ünïcode Ëmpire"))
                .is_ok()
        );
    }
}
