//! HOI4 descriptor parsing and deterministic rendering.

use crate::models::GeneratedArtifact;
use crate::models::ProjectIdentity;
use crate::paths::user_facing_path;
use crate::security::{sha256_bytes, validate_external_destination};
use crate::AppError;
pub use hoi4_setup_core::descriptors::*;
use std::path::Path;

pub fn render_launcher_descriptor(
    identity: &ProjectIdentity,
    project_root: &Path,
) -> Result<String, AppError> {
    let canonical_root = crate::paths::validate_project_root_or_destination(project_root)?.0;
    let raw_path = canonical_root
        .to_str()
        .ok_or_else(|| AppError::InvalidInput("project path is not valid Unicode".into()))?;
    if raw_path.contains('\0') || raw_path.contains('\n') || raw_path.contains('\r') {
        return Err(AppError::InvalidInput(
            "project path contains a descriptor-breaking control character".into(),
        ));
    }
    launcher_descriptor_text(identity, &launcher_path_text(&canonical_root))
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
///
/// The comparison is text only; the declared path is never opened. Surrounding
/// spaces and trailing separators are ignored, and a relative value such as
/// `mod/atlantis` is read against `user_directory` (the HOI4 user folder that
/// holds the `mod` folder), as the launcher does. `..`, stream, and verbatim
/// forms never match.
pub(crate) fn launcher_path_matches_project_root(
    declared_path: &str,
    project_root: &Path,
    user_directory: Option<&Path>,
) -> Result<bool, AppError> {
    let canonical_root = crate::paths::validate_project_root_or_destination(project_root)?.0;
    let windows = cfg!(target_os = "windows");
    let normalize = |text: &str| {
        let text = if windows {
            text.replace('\\', "/")
        } else {
            text.to_owned()
        };
        let trimmed = text.trim();
        let without_trailing = trimmed.trim_end_matches('/');
        if without_trailing.is_empty() {
            trimmed.to_owned()
        } else {
            without_trailing.to_owned()
        }
    };
    let same = |left: &str, right: &str| {
        if windows {
            left.to_lowercase() == right.to_lowercase()
        } else {
            left == right
        }
    };
    let declared = normalize(declared_path);
    if same(&declared, &normalize(&launcher_path_text(&canonical_root))) {
        return Ok(true);
    }
    let absolute =
        declared.starts_with('/') || (windows && declared.as_bytes().get(1) == Some(&b':'));
    if absolute {
        return Ok(false);
    }
    let Some(user_directory) = user_directory else {
        return Ok(false);
    };
    let Ok(relative_root) = canonical_root.strip_prefix(user_directory) else {
        return Ok(false);
    };
    let relative_root = relative_root
        .components()
        .map(|component| component.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("/");
    let declared = declared.strip_prefix("./").unwrap_or(&declared);
    Ok(!relative_root.is_empty() && same(declared, &relative_root))
}

/// Whether a user's launcher descriptor registers the project: its parsed or
/// raw `path=` value matches the root, reading a relative value against the
/// HOI4 user folder (the parent of the folder that holds the project). Every
/// read-side check uses this so discovery, approval, scan evidence, and
/// readiness agree.
pub(crate) fn launcher_registers_project_root(
    bytes: &[u8],
    declared: &str,
    project_root: &Path,
) -> bool {
    let canonical = crate::paths::validate_project_root_or_destination(project_root)
        .ok()
        .map(|root| root.0);
    let user_directory = canonical
        .as_deref()
        .and_then(Path::parent)
        .and_then(Path::parent);
    let matches = |value: &str| {
        launcher_path_matches_project_root(value, project_root, user_directory).unwrap_or(false)
    };
    if matches(declared) {
        return true;
    }
    raw_quoted_field(bytes, "path").is_some_and(|raw| matches(&raw))
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
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
    fn real_world_descriptors_with_multiline_blocks_and_repeated_paths_parse() {
        let descriptor = concat!(
            "\u{feff}name=\"Chaos Redux\"\n",
            "replace_path=\"gfx/loadingscreens\"\n",
            "replace_path=\"history/units\"\n",
            "tags={\n",
            "\t\"Alternative History\"\n",
            "\t\"Events\" # main\n",
            "\n",
            "\t\"Gameplay\"\n",
            "}\n",
            "# a full-line comment\n",
            "picture=\"thumbnail.png\"\n",
            "version=\"0.1\" # current\n",
            "supported_version=\"1.19.*\"\n",
            "dependencies={\n",
            "\t\"Base {mod}\"\n",
            "}\n",
        );
        let parsed = parse_descriptor(descriptor.as_bytes()).unwrap();
        assert_eq!(parsed.fields["name"], "Chaos Redux");
        assert_eq!(
            parsed.fields["tags"],
            "{ \"Alternative History\" \"Events\" \"Gameplay\" }"
        );
        assert_eq!(
            parsed.fields["replace_path"],
            "gfx/loadingscreens\nhistory/units"
        );
        assert_eq!(parsed.fields["version"], "0.1");
        assert_eq!(parsed.fields["dependencies"], "{ \"Base {mod}\" }");
    }

    #[test]
    fn unterminated_or_duplicated_descriptor_fields_are_rejected() {
        assert!(parse_descriptor(b"name=\"A\"\ntags={\n\t\"Events\"\n").is_err());
        assert!(parse_descriptor(b"name=\"A\"\nname=\"B\"\n").is_err());
        let endless = format!("name=\"A\"\ntags={{\n{}", "\"Events\"\n".repeat(600));
        assert!(parse_descriptor(endless.as_bytes()).is_err());
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

        assert!(launcher_path_matches_project_root(&declared, &project_root, None).unwrap());
        assert!(!launcher_path_matches_project_root(
            &format!("{declared}_other"),
            &project_root,
            None
        )
        .unwrap());
        // Spaces and a trailing separator do not change the folder.
        for equivalent in [format!("{declared}/"), format!(" {declared} ")] {
            assert!(
                launcher_path_matches_project_root(&equivalent, &project_root, None).unwrap(),
                "{equivalent}"
            );
        }
        let sibling = parent.path().join("atlantis_sibling");
        let sibling_declared = user_facing_path(
            &crate::paths::validate_project_root_or_destination(&sibling)
                .unwrap()
                .0,
        )
        .replace('\\', "/");
        assert!(
            !launcher_path_matches_project_root(&sibling_declared, &project_root, None).unwrap()
        );
        for mismatched in [
            format!("{declared}/../atlantis_rising"),
            format!("{declared}:stream"),
            format!("//?/{declared}"),
        ] {
            assert!(!launcher_path_matches_project_root(&mismatched, &project_root, None).unwrap());
        }
        let different_case = declared.to_ascii_uppercase();
        assert_eq!(
            launcher_path_matches_project_root(&different_case, &project_root, None).unwrap(),
            cfg!(target_os = "windows")
        );
        let mixed_separators = declared.replace('/', "\\");
        assert_eq!(
            launcher_path_matches_project_root(&mixed_separators, &project_root, None).unwrap(),
            cfg!(target_os = "windows")
        );
    }

    #[test]
    fn relative_launcher_paths_resolve_against_the_hoi4_user_folder() {
        let user = tempfile::tempdir().unwrap();
        let user = crate::paths::validate_project_root_or_destination(user.path())
            .unwrap()
            .0;
        std::fs::create_dir_all(user.join("mod").join("atlantis")).unwrap();
        let project_root = user.join("mod").join("atlantis");
        for declared in ["mod/atlantis", "./mod/atlantis/", " mod/atlantis "] {
            assert!(
                launcher_path_matches_project_root(declared, &project_root, Some(&user)).unwrap(),
                "{declared}"
            );
            assert!(!launcher_path_matches_project_root(declared, &project_root, None).unwrap());
        }
        for declared in ["mod/other", "atlantis", "mod/../mod/atlantis", ""] {
            assert!(
                !launcher_path_matches_project_root(declared, &project_root, Some(&user)).unwrap(),
                "{declared}"
            );
        }
        assert_eq!(
            raw_quoted_field(b"name=\"A\"\npath=\"C:\\Users\\a\\mod\\x\"\n", "path").as_deref(),
            Some("C:\\Users\\a\\mod\\x")
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
        assert!(launcher_path_matches_project_root(declared, &project_root, None).unwrap());
        assert!(!launcher_path_matches_project_root(
            &declared.replace('\\', "/"),
            &project_root,
            None
        )
        .unwrap());
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

        assert!(launcher_path_matches_project_root("/unused", &linked, None).is_err());
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
