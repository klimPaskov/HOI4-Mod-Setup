//! HOI4 descriptor parsing and deterministic rendering.

use crate::models::GeneratedArtifact;
use crate::models::ProjectIdentity;
use crate::security::{sha256_bytes, validate_external_destination};
use crate::AppError;
use regex::Regex;
use std::collections::BTreeMap;
use std::io::Cursor;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Descriptor {
    pub fields: BTreeMap<String, String>,
}

pub fn validate_project_id(value: &str) -> Result<(), AppError> {
    let pattern = Regex::new(r"^[a-z][a-z0-9_]{1,63}$").expect("static project id regex");
    if pattern.is_match(value) {
        Ok(())
    } else {
        Err(AppError::InvalidInput(
            "project ID must start with a lowercase letter and contain only lowercase letters, digits, and underscores".into(),
        ))
    }
}

pub fn validate_field(value: &str, label: &str) -> Result<(), AppError> {
    if value.trim().is_empty()
        || value.contains('\0')
        || value.contains('\n')
        || value.contains('\r')
    {
        return Err(AppError::InvalidInput(format!(
            "{label} is empty or contains a newline"
        )));
    }
    Ok(())
}

pub fn parse_descriptor(bytes: &[u8]) -> Result<Descriptor, AppError> {
    let text = String::from_utf8(bytes.to_vec())
        .map_err(|_| AppError::InvalidInput("descriptor is not UTF-8".into()))?;
    let mut fields = BTreeMap::new();
    for (line_number, line) in text.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let (key, value) = parse_assignment(trimmed).ok_or_else(|| {
            AppError::InvalidInput(format!("descriptor line {} is malformed", line_number + 1))
        })?;
        if fields.insert(key.clone(), value).is_some() {
            return Err(AppError::InvalidInput(format!(
                "descriptor field is duplicated: {key}"
            )));
        }
    }
    if fields.is_empty() {
        return Err(AppError::InvalidInput(
            "descriptor contains no recognized assignments".into(),
        ));
    }
    Ok(Descriptor { fields })
}

fn parse_assignment(line: &str) -> Option<(String, String)> {
    let (raw_key, raw_value) = line.split_once('=')?;
    let key = raw_key.trim();
    if key.is_empty()
        || !key.chars().enumerate().all(|(index, character)| {
            if index == 0 {
                character.is_ascii_alphabetic() || character == '_'
            } else {
                character.is_ascii_alphanumeric() || character == '_'
            }
        })
    {
        return None;
    }
    let raw_value = raw_value.trim();
    if raw_value.starts_with('"') {
        let mut escaped = false;
        let mut closing = None;
        for (index, character) in raw_value.char_indices().skip(1) {
            if escaped {
                escaped = false;
                continue;
            }
            if character == '\\' {
                escaped = true;
            } else if character == '"' {
                closing = Some(index);
                break;
            }
        }
        let closing = closing?;
        if !raw_value[closing + 1..].trim().is_empty() {
            return None;
        }
        return Some((key.to_string(), unquote(&raw_value[1..closing])));
    }
    if raw_value.starts_with('{') {
        if !raw_value.ends_with('}') || raw_value.len() < 2 {
            return None;
        }
        return Some((key.to_string(), raw_value.to_string()));
    }
    if raw_value.is_empty()
        || raw_value.chars().any(|character| character.is_whitespace())
        || raw_value.contains('#')
    {
        return None;
    }
    Some((key.to_string(), raw_value.to_string()))
}

pub fn render_descriptor_mod(identity: &ProjectIdentity) -> Result<String, AppError> {
    validate_project_id(&identity.project_id)?;
    validate_field(&identity.display_name, "mod name")?;
    validate_field(&identity.version, "version")?;
    validate_field(&identity.supported_game_version, "supported game version")?;
    let mut descriptor = format!(
        "name=\"{}\"\nversion=\"{}\"\nsupported_version=\"{}\"\npicture=\"thumbnail.png\"\n",
        quote(&identity.display_name),
        quote(&identity.version),
        quote(&identity.supported_game_version),
    );
    // Script prefixes and namespaces are project conventions, not valid HOI4
    // descriptor fields. They are retained in installation metadata and
    // adapted project guidance instead of being written to either .mod file.
    if !identity.descriptor_tags.is_empty() {
        let mut tags = identity.descriptor_tags.clone();
        tags.sort();
        tags.dedup();
        if tags.len() != identity.descriptor_tags.len() || tags.len() > 32 {
            return Err(AppError::InvalidInput(
                "descriptor tags must be unique and bounded".into(),
            ));
        }
        let mut rendered = String::from("tags={");
        for tag in tags {
            validate_field(&tag, "descriptor tag")?;
            if tag.chars().count() > 64 {
                return Err(AppError::InvalidInput("descriptor tag is too long".into()));
            }
            rendered.push_str(&format!(" \"{}\"", quote(&tag)));
        }
        rendered.push_str(" }\n");
        descriptor.push_str(&rendered);
    }
    Ok(descriptor)
}

/// A fixed 1x1 RGBA PNG keeps a fresh project launcher-ready without
/// pretending to be user artwork. The file is deliberately replaceable and
/// its bytes are tracked as generated content in the transaction lock.
pub const PLACEHOLDER_THUMBNAIL_PNG: &[u8] = &[
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1f, 0x15, 0xc4,
    0x89, 0x00, 0x00, 0x00, 0x0b, 0x49, 0x44, 0x41, 0x54, 0x78, 0xda, 0x63, 0x60, 0x00, 0x02, 0x00,
    0x00, 0x05, 0x00, 0x01, 0xe9, 0xfa, 0xdc, 0xd8, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4e, 0x44,
    0xae, 0x42, 0x60, 0x82,
];

pub fn validate_thumbnail_png(bytes: &[u8]) -> Result<(u32, u32), AppError> {
    if bytes.len() > 16 * 1024 * 1024 {
        return Err(AppError::InvalidInput(
            "thumbnail exceeds the bounded PNG size".into(),
        ));
    }
    let decoder = png::Decoder::new(Cursor::new(bytes));
    let mut reader = decoder.read_info().map_err(|error| {
        AppError::InvalidInput(format!("thumbnail is not a valid PNG: {error}"))
    })?;
    let (width, height) = {
        let info = reader.info();
        (info.width, info.height)
    };
    if width == 0 || height == 0 || width > 4096 || height > 4096 {
        return Err(AppError::InvalidInput(
            "thumbnail dimensions are outside the supported range".into(),
        ));
    }
    let output_size = reader.output_buffer_size();
    let mut output = vec![0_u8; output_size];
    reader.next_frame(&mut output).map_err(|error| {
        AppError::InvalidInput(format!("thumbnail could not be decoded: {error}"))
    })?;
    Ok((width, height))
}

pub fn render_launcher_descriptor(
    identity: &ProjectIdentity,
    project_root: &Path,
) -> Result<String, AppError> {
    let descriptor = render_descriptor_mod(identity)?;
    let path = project_root
        .to_str()
        .ok_or_else(|| AppError::InvalidInput("project path is not valid Unicode".into()))?;
    if path.contains('\0') || path.contains('\n') || path.contains('\r') {
        return Err(AppError::InvalidInput(
            "project path contains a descriptor-breaking control character".into(),
        ));
    }
    Ok(format!(
        "{descriptor}path=\"{}\"\n",
        quote(&path.replace('\\', "/"))
    ))
}

pub fn validate_launcher_destination(identity: &ProjectIdentity) -> Result<(), AppError> {
    let Some(path) = identity.launcher_descriptor_path.as_ref() else {
        return Ok(());
    };
    let path = validate_external_destination(&path.to_string_lossy())?;
    let expected = format!("{}.mod", identity.project_id);
    let actual = path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| {
            AppError::InvalidInput("launcher descriptor filename is not Unicode".into())
        })?;
    let matches = if cfg!(target_os = "windows") {
        actual.eq_ignore_ascii_case(&expected)
    } else {
        actual == expected
    };
    if !matches {
        return Err(AppError::InvalidInput(format!(
            "launcher descriptor must be named {expected}"
        )));
    }
    Ok(())
}

pub fn generated_artifacts(identity: &ProjectIdentity) -> Result<Vec<GeneratedArtifact>, AppError> {
    let descriptor = render_descriptor_mod(identity)?;
    validate_launcher_destination(identity)?;
    let mut artifacts = vec![GeneratedArtifact {
        component_id: "project.descriptor".into(),
        destination: "descriptor.mod".into(),
        expected_sha256: sha256_bytes(descriptor.as_bytes()),
        content: descriptor,
        external: false,
        bytes: None,
    }];
    if let Some(path) = &identity.launcher_descriptor_path {
        let launcher = render_launcher_descriptor(identity, &identity.project_root)?;
        artifacts.push(GeneratedArtifact {
            component_id: "project.launcher_descriptor".into(),
            destination: path.display().to_string(),
            expected_sha256: sha256_bytes(launcher.as_bytes()),
            content: launcher,
            external: true,
            bytes: None,
        });
    }
    let thumbnail = PLACEHOLDER_THUMBNAIL_PNG.to_vec();
    artifacts.push(GeneratedArtifact {
        component_id: "project.thumbnail".into(),
        destination: "thumbnail.png".into(),
        content: "Deterministic 1x1 RGBA PNG placeholder; replaceable after install.".into(),
        expected_sha256: sha256_bytes(&thumbnail),
        external: false,
        bytes: Some(thumbnail),
    });
    Ok(artifacts)
}

fn quote(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

fn unquote(value: &str) -> String {
    let mut result = String::with_capacity(value.len());
    let mut escaped = false;
    for character in value.chars() {
        if escaped {
            result.push(character);
            escaped = false;
        } else if character == '\\' {
            escaped = true;
        } else {
            result.push(character);
        }
    }
    if escaped {
        result.push('\\');
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn identity() -> ProjectIdentity {
        ProjectIdentity {
            display_name: "Cold War Curtain".into(),
            project_id: "cold_war_curtain".into(),
            author: "".into(),
            version: "0.1.0".into(),
            supported_game_version: "1.17.*".into(),
            project_root: PathBuf::from("C:/mods/cold_war_curtain"),
            default_branch: "main".into(),
            script_prefix: None,
            primary_namespace: None,
            descriptor_tags: Vec::new(),
            launcher_descriptor_path: Some(PathBuf::from(
                "C:/Users/test/Documents/Paradox Interactive/Hearts of Iron IV/mod/cold_war_curtain.mod",
            )),
        }
    }

    #[test]
    fn descriptors_round_trip_and_escape_values() {
        let mut identity = identity();
        identity.script_prefix = Some("cwsea".into());
        identity.primary_namespace = Some("cwsea".into());
        let rendered = render_descriptor_mod(&identity).unwrap();
        let parsed = parse_descriptor(rendered.as_bytes()).unwrap();
        assert_eq!(parsed.fields.get("name").unwrap(), "Cold War Curtain");
        assert_eq!(parsed.fields.get("supported_version").unwrap(), "1.17.*");
        assert!(!parsed.fields.contains_key("script_prefix"));
        assert!(!parsed.fields.contains_key("namespace"));
        assert!(!rendered.contains("script_prefix="));
        assert!(!rendered.contains("namespace="));
    }

    #[test]
    fn unsafe_project_ids_are_rejected() {
        assert!(validate_project_id("../mod").is_err());
        assert!(validate_project_id("ColdWar").is_err());
        assert!(validate_project_id("cold_war_curtain").is_ok());
    }

    #[test]
    fn launcher_descriptor_contains_the_selected_project_path() {
        let mut identity = identity();
        identity.script_prefix = Some("cwsea".into());
        identity.primary_namespace = Some("cwsea".into());
        let text =
            render_launcher_descriptor(&identity, Path::new("C:/mods/cold_war_curtain")).unwrap();
        assert!(text.contains("path=\"C:/mods/cold_war_curtain\""));
        assert!(!text.contains("script_prefix="));
        assert!(!text.contains("namespace="));
    }

    #[test]
    fn bundled_thumbnail_decodes_as_a_small_png() {
        assert_eq!(
            validate_thumbnail_png(PLACEHOLDER_THUMBNAIL_PNG).unwrap(),
            (1, 1)
        );
    }

    #[test]
    fn tags_are_parseable_by_readiness_validation() {
        let mut identity = identity();
        identity.descriptor_tags = vec!["Gameplay".into(), "Total Conversion".into()];
        let rendered = render_descriptor_mod(&identity).unwrap();
        let parsed = parse_descriptor(rendered.as_bytes()).unwrap();
        assert_eq!(
            parsed.fields.get("tags"),
            Some(&r#"{ "Gameplay" "Total Conversion" }"#.into())
        );
    }

    #[test]
    fn launcher_filename_must_match_project_id() {
        let directory = tempfile::tempdir().unwrap();
        let mut identity = identity();
        identity.launcher_descriptor_path = Some(directory.path().join("different.mod"));
        assert!(validate_launcher_destination(&identity).is_err());
    }
}
