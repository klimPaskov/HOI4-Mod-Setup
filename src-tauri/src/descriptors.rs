//! HOI4 descriptor parsing and deterministic rendering.

use crate::models::GeneratedArtifact;
use crate::models::ProjectIdentity;
use crate::paths::user_facing_path;
use crate::security::{sha256_bytes, validate_external_destination};
use crate::AppError;
use regex::Regex;
use std::collections::BTreeMap;
use std::io::Cursor;
use std::path::Path;

pub const HOI4_DESCRIPTOR_TAGS: &[&str] = &[
    "Alternative History",
    "Balance",
    "Events",
    "Fixes",
    "Gameplay",
    "Graphics",
    "Historical",
    "Ideologies",
    "Map",
    "Military",
    "National Focuses",
    "Sound",
    "Technologies",
    "Translation",
    "Utilities",
];

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

pub fn validate_descriptor_tags(tags: &[String]) -> Result<(), AppError> {
    let unique = tags.iter().collect::<std::collections::BTreeSet<_>>();
    if tags.is_empty()
        || tags.len() > HOI4_DESCRIPTOR_TAGS.len()
        || unique.len() != tags.len()
        || tags
            .iter()
            .any(|tag| !HOI4_DESCRIPTOR_TAGS.contains(&tag.as_str()))
    {
        return Err(AppError::InvalidInput(
            "descriptor tags must use the supported Hearts of Iron IV categories".into(),
        ));
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
        validate_descriptor_tags(&identity.descriptor_tags)?;
        let mut tags = identity.descriptor_tags.clone();
        tags.sort();
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

/// A deterministic 600x600 black PNG keeps a fresh project launcher-ready
/// without pretending to be user artwork. The file is replaceable and its
/// exact bytes are tracked as generated content in the transaction lock.
pub fn placeholder_thumbnail_png() -> Result<Vec<u8>, AppError> {
    const SIDE: u32 = 600;
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, SIDE, SIDE);
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.set_compression(png::Compression::Best);
        let mut writer = encoder.write_header().map_err(|error| {
            AppError::InvalidInput(format!("thumbnail header could not be encoded: {error}"))
        })?;
        let pixels = vec![0_u8; (SIDE * SIDE * 3) as usize];
        writer.write_image_data(&pixels).map_err(|error| {
            AppError::InvalidInput(format!("thumbnail pixels could not be encoded: {error}"))
        })?;
    }
    Ok(bytes)
}

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
    let canonical_root = crate::paths::validate_project_root_or_destination(project_root)?.0;
    let raw_path = canonical_root
        .to_str()
        .ok_or_else(|| AppError::InvalidInput("project path is not valid Unicode".into()))?;
    if raw_path.contains('\0') || raw_path.contains('\n') || raw_path.contains('\r') {
        return Err(AppError::InvalidInput(
            "project path contains a descriptor-breaking control character".into(),
        ));
    }
    let path = launcher_path_text(&canonical_root);
    Ok(format!("{descriptor}path=\"{}\"\n", quote(&path)))
}

fn launcher_path_text(project_root: &Path) -> String {
    let path = user_facing_path(project_root);
    if cfg!(target_os = "windows") {
        path.replace('\\', "/")
    } else {
        path
    }
}

/// Compare the parsed launcher `path=` value with the same canonical project
/// root representation used by the renderer. Windows filesystem APIs retain
/// a verbatim `\\?\` prefix internally, but that implementation detail is not
/// valid user-facing launcher content.
pub(crate) fn launcher_path_matches_project_root(
    declared_path: &str,
    project_root: &Path,
) -> Result<bool, AppError> {
    let canonical_root = crate::paths::validate_project_root_or_destination(project_root)?.0;
    let expected = launcher_path_text(&canonical_root);
    Ok(if cfg!(target_os = "windows") {
        declared_path
            .replace('\\', "/")
            .eq_ignore_ascii_case(&expected)
    } else {
        declared_path == expected
    })
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
    let thumbnail = placeholder_thumbnail_png()?;
    artifacts.push(GeneratedArtifact {
        component_id: "project.thumbnail".into(),
        destination: "thumbnail.png".into(),
        content: "Deterministic 600x600 black PNG placeholder; replaceable after install.".into(),
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
        let parent = tempfile::tempdir().unwrap();
        let project_root = parent.path().join("cold_war_curtain");
        std::fs::create_dir(&project_root).unwrap();
        let canonical_root = crate::paths::validate_project_root(&project_root).unwrap();
        let expected = launcher_path_text(&canonical_root);

        let text = render_launcher_descriptor(&identity, &project_root).unwrap();
        assert!(text.contains(&format!("path=\"{expected}\"")));
        assert!(!text.contains("script_prefix="));
        assert!(!text.contains("namespace="));
    }

    #[test]
    fn launcher_path_comparison_uses_the_exact_user_facing_canonical_root() {
        let parent = tempfile::tempdir().unwrap();
        let project_root = parent.path().join("atlantis_rising");
        let canonical = crate::paths::validate_project_root_or_destination(&project_root)
            .unwrap()
            .0;
        let declared = user_facing_path(&canonical).replace('\\', "/");

        assert!(launcher_path_matches_project_root(&declared, &project_root).unwrap());
        assert!(
            !launcher_path_matches_project_root(&format!("{declared}_other"), &project_root)
                .unwrap()
        );
        let sibling = parent.path().join("atlantis_sibling");
        let sibling_declared = user_facing_path(
            &crate::paths::validate_project_root_or_destination(&sibling)
                .unwrap()
                .0,
        )
        .replace('\\', "/");
        assert!(!launcher_path_matches_project_root(&sibling_declared, &project_root).unwrap());
        for mismatched in [
            format!("{declared}/"),
            format!("{declared}/../atlantis_rising"),
            format!("{declared}:stream"),
            format!("//?/{declared}"),
        ] {
            assert!(!launcher_path_matches_project_root(&mismatched, &project_root).unwrap());
        }
        let different_case = declared.to_ascii_uppercase();
        assert_eq!(
            launcher_path_matches_project_root(&different_case, &project_root).unwrap(),
            cfg!(target_os = "windows")
        );
        let mixed_separators = declared.replace('/', "\\");
        assert_eq!(
            launcher_path_matches_project_root(&mixed_separators, &project_root).unwrap(),
            cfg!(target_os = "windows")
        );
    }

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn launcher_paths_preserve_literal_backslashes_on_unix() {
        let mut identity = identity();
        let parent = tempfile::tempdir().unwrap();
        let project_root = parent.path().join("atlantis\\rising");
        std::fs::create_dir(&project_root).unwrap();
        identity.project_root = project_root.clone();

        let rendered = render_launcher_descriptor(&identity, &project_root).unwrap();
        let parsed = parse_descriptor(rendered.as_bytes()).unwrap();
        let declared = parsed.fields.get("path").unwrap();

        assert!(declared.contains("atlantis\\rising"));
        assert!(launcher_path_matches_project_root(declared, &project_root).unwrap());
        assert!(
            !launcher_path_matches_project_root(&declared.replace('\\', "/"), &project_root)
                .unwrap()
        );
    }

    #[cfg(unix)]
    #[test]
    fn launcher_path_comparison_rejects_a_linked_project_root() {
        use std::os::unix::fs::symlink;

        let parent = tempfile::tempdir().unwrap();
        let target = parent.path().join("target");
        let linked = parent.path().join("linked");
        std::fs::create_dir(&target).unwrap();
        symlink(&target, &linked).unwrap();

        assert!(launcher_path_matches_project_root("/unused", &linked).is_err());
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn launcher_descriptor_removes_windows_verbatim_path_prefix() {
        let parent = tempfile::tempdir().unwrap();
        let project_root = parent.path().join("cold_war_curtain");
        std::fs::create_dir(&project_root).unwrap();
        let identity = identity();
        let text = render_launcher_descriptor(&identity, &project_root).unwrap();

        assert!(text.contains("/cold_war_curtain\""));
        assert!(!text.contains("/?/"));
        assert!(!text.contains("//?/"));
    }

    #[test]
    fn generated_thumbnail_is_a_600_square_with_only_black_pixels() {
        let bytes = placeholder_thumbnail_png().unwrap();
        assert_eq!(validate_thumbnail_png(&bytes).unwrap(), (600, 600));
        let decoder = png::Decoder::new(Cursor::new(bytes));
        let mut reader = decoder.read_info().unwrap();
        let mut pixels = vec![0_u8; reader.output_buffer_size()];
        let frame = reader.next_frame(&mut pixels).unwrap();
        assert_eq!(frame.color_type, png::ColorType::Rgb);
        assert!(pixels[..frame.buffer_size()]
            .iter()
            .all(|pixel| *pixel == 0));
    }

    #[test]
    fn tags_are_parseable_by_readiness_validation() {
        let mut identity = identity();
        identity.descriptor_tags = vec!["Gameplay".into(), "Graphics".into()];
        let rendered = render_descriptor_mod(&identity).unwrap();
        let parsed = parse_descriptor(rendered.as_bytes()).unwrap();
        assert_eq!(
            parsed.fields.get("tags"),
            Some(&r#"{ "Gameplay" "Graphics" }"#.into())
        );
    }

    #[test]
    fn unknown_descriptor_tags_are_rejected() {
        let mut identity = identity();
        identity.descriptor_tags = vec!["Total Conversion".into()];
        assert!(render_descriptor_mod(&identity).is_err());
    }

    #[test]
    fn launcher_filename_must_match_project_id() {
        let directory = tempfile::tempdir().unwrap();
        let mut identity = identity();
        identity.launcher_descriptor_path = Some(directory.path().join("different.mod"));
        assert!(validate_launcher_destination(&identity).is_err());
    }
}
