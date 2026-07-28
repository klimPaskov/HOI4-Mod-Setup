//! Build the optional flat source folder intended for a ChatGPT Chat project.
//!
//! This is a generated view of the verified setup inputs. It never removes the
//! normal `.agents/skills` or `.codex/agents` trees and every output remains a
//! normal transaction operation with conflict and rollback evidence.

use crate::models::{GeneratedArtifact, PreparedFile};
use crate::security::{
    canonical_relative_key, is_link_metadata, normalize_relative_path, redact_secrets, safe_join,
};
use crate::AppError;
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, OpenOptions};
use std::io::Read;
use std::path::Path;

#[cfg(unix)]
use std::ffi::CString;
#[cfg(windows)]
use std::ffi::OsString;
#[cfg(unix)]
use std::os::fd::{AsRawFd, FromRawFd};
#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;
#[cfg(windows)]
use std::os::windows::ffi::OsStringExt;
#[cfg(windows)]
use std::os::windows::fs::OpenOptionsExt;
#[cfg(windows)]
use std::os::windows::io::AsRawHandle;

#[cfg(windows)]
use windows_sys::Win32::Storage::FileSystem::{GetFinalPathNameByHandleW, FILE_NAME_NORMALIZED};

pub const FLAT_DESTINATION_ROOT: &str = "chatgpt_project_sources";
const MAX_FLAT_FILES: usize = 512;
const MAX_FLAT_FILE_BYTES: u64 = 8 * 1024 * 1024;
const MAX_FLAT_TOTAL_BYTES: u64 = 64 * 1024 * 1024;

pub fn build_artifacts(
    prepared: &[PreparedFile],
    generated: &[GeneratedArtifact],
    project_root: &Path,
    additional_files: &[String],
) -> Result<Vec<GeneratedArtifact>, AppError> {
    let mut sources = BTreeMap::<String, Vec<u8>>::new();
    for file in prepared {
        insert_source(
            &mut sources,
            &file.destination.replace('\\', "/"),
            file.bytes.clone(),
        )?;
    }
    for file in generated {
        insert_source(
            &mut sources,
            &file.destination.replace('\\', "/"),
            file.bytes
                .clone()
                .unwrap_or_else(|| file.content.as_bytes().to_vec()),
        )?;
    }
    for required in ["AGENTS.md", "README.md"] {
        if !sources.contains_key(required) {
            let path = safe_join(project_root, required)?;
            if path.is_file() {
                insert_source(
                    &mut sources,
                    required,
                    read_regular_file_no_follow_under_root(project_root, required)?,
                )?;
            }
        }
    }
    collect_existing_agent_sources(&mut sources, project_root)?;

    let mut outputs = BTreeMap::<String, (String, Vec<u8>)>::new();
    let mut skill_count = 0usize;
    let mut subagent_count = 0usize;
    for (source, bytes) in &sources {
        if source == "AGENTS.md" || source == "README.md" {
            std::str::from_utf8(bytes).map_err(|_| {
                AppError::Serialization(format!(
                    "flattened required file is not valid UTF-8: {source}"
                ))
            })?;
            insert_output(&mut outputs, source, bytes.clone())?;
            continue;
        }
        let parts = source.split('/').collect::<Vec<_>>();
        if parts.len() == 4
            && parts[0] == ".agents"
            && parts[1] == "skills"
            && parts[3] == "SKILL.md"
        {
            let skill = parts[parts.len() - 2];
            std::str::from_utf8(bytes).map_err(|_| {
                AppError::Serialization(format!("flattened skill is not valid UTF-8: {source}"))
            })?;
            let destination = format!("{skill}.md");
            insert_output(&mut outputs, &destination, bytes.clone())?;
            skill_count += 1;
        } else if parts.len() == 3
            && parts[0] == ".codex"
            && parts[1] == "agents"
            && parts[2].ends_with(".toml")
        {
            std::str::from_utf8(bytes).map_err(|_| {
                AppError::Serialization(format!("flattened subagent is not valid UTF-8: {source}"))
            })?;
            insert_output(&mut outputs, parts[2], bytes.clone())?;
            subagent_count += 1;
        }
    }
    if !outputs.contains_key("agents.md") || !outputs.contains_key("readme.md") {
        return Err(AppError::InvalidInput(
            "flattened Chat sources require AGENTS.md and README.md".into(),
        ));
    }
    if skill_count == 0 || subagent_count == 0 {
        return Err(AppError::InvalidInput(
            "flattened Chat sources require at least one skill and one subagent".into(),
        ));
    }

    let mut extra_seen = BTreeSet::new();
    for raw in additional_files {
        let relative = normalize_relative_path(raw)?;
        if !extra_seen.insert(relative.clone()) {
            continue;
        }
        if relative.starts_with("chatgpt_project_sources/") {
            return Err(AppError::PathSecurity(
                "flattened additional files cannot read from their own output folder".into(),
            ));
        }
        if is_secret_like_path(&relative) {
            return Err(AppError::Credential(format!(
                "flattened additional file has a secret-shaped path: {relative}"
            )));
        }
        let path = safe_join(project_root, &relative)?;
        let metadata_before = fs::symlink_metadata(&path)?;
        if is_link_metadata(&metadata_before) {
            return Err(AppError::PathSecurity(format!(
                "flattened additional file is a link: {relative}"
            )));
        }
        if metadata_before.len() > MAX_FLAT_FILE_BYTES {
            return Err(AppError::InvalidInput(format!(
                "flattened additional file exceeds the {} MiB limit: {relative}",
                MAX_FLAT_FILE_BYTES / 1024 / 1024
            )));
        }
        let bytes = read_regular_file_no_follow_under_root(project_root, &relative)?;
        let metadata_after = fs::symlink_metadata(&path)?;
        if is_link_metadata(&metadata_after) {
            return Err(AppError::PathSecurity(format!(
                "flattened additional file is a link: {relative}"
            )));
        }
        if bytes.len() as u64 > MAX_FLAT_FILE_BYTES {
            return Err(AppError::InvalidInput(format!(
                "flattened additional file exceeds the {} MiB limit: {relative}",
                MAX_FLAT_FILE_BYTES / 1024 / 1024
            )));
        }
        let text = std::str::from_utf8(&bytes).map_err(|_| {
            AppError::Serialization(format!(
                "flattened additional file is not valid UTF-8: {relative}"
            ))
        })?;
        if redact_secrets(text, &[]) != text {
            return Err(AppError::Credential(format!(
                "flattened additional file contains secret-shaped content: {relative}"
            )));
        }
        insert_output(&mut outputs, &format!("extras/{relative}"), bytes)?;
    }

    if outputs.len() > MAX_FLAT_FILES {
        return Err(AppError::InvalidInput(
            "flattened Chat sources exceed the aggregate file limit".into(),
        ));
    }
    let total_bytes = outputs
        .values()
        .try_fold(0_u64, |total, (_, bytes)| {
            total.checked_add(bytes.len() as u64)
        })
        .ok_or_else(|| AppError::InvalidInput("flattened Chat source size overflows".into()))?;
    if total_bytes > MAX_FLAT_TOTAL_BYTES {
        return Err(AppError::InvalidInput(
            "flattened Chat sources exceed the aggregate size limit".into(),
        ));
    }
    Ok(outputs
        .into_iter()
        .map(|(_, (destination, bytes))| GeneratedArtifact {
            component_id: "codex.chat_flatten".into(),
            destination: format!("{FLAT_DESTINATION_ROOT}/{destination}"),
            expected_sha256: crate::security::sha256_bytes(&bytes),
            content: String::from_utf8(bytes.clone())
                .unwrap_or_else(|_| "[binary source; hash-verified by the core]".into()),
            external: false,
            bytes: (!bytes.is_empty() && String::from_utf8(bytes.clone()).is_err())
                .then_some(bytes),
        })
        .collect())
}

fn collect_existing_agent_sources(
    sources: &mut BTreeMap<String, Vec<u8>>,
    project_root: &Path,
) -> Result<(), AppError> {
    let skills_root = safe_join(project_root, ".agents/skills")?;
    if skills_root.exists() {
        let metadata = fs::symlink_metadata(&skills_root)?;
        if is_link_metadata(&metadata) || !metadata.is_dir() {
            return Err(AppError::PathSecurity(
                "flattened skill root is not a normal directory".into(),
            ));
        }
        for entry in fs::read_dir(&skills_root)? {
            let entry = entry?;
            let skill_dir = entry.path();
            let skill_metadata = fs::symlink_metadata(&skill_dir)?;
            if is_link_metadata(&skill_metadata) {
                return Err(AppError::PathSecurity(format!(
                    "flattened skill directory is a link: {}",
                    skill_dir.display()
                )));
            }
            if !skill_metadata.is_dir() {
                continue;
            }
            let skill_path = skill_dir.join("SKILL.md");
            if !skill_path.exists() {
                continue;
            }
            let relative = format!(
                ".agents/skills/{}/SKILL.md",
                entry.file_name().to_string_lossy()
            );
            if !sources.contains_key(&relative) {
                insert_source(
                    sources,
                    &relative,
                    read_regular_file_no_follow_under_root(project_root, &relative)?,
                )?;
            }
        }
    }

    let agents_root = safe_join(project_root, ".codex/agents")?;
    if agents_root.exists() {
        let metadata = fs::symlink_metadata(&agents_root)?;
        if is_link_metadata(&metadata) || !metadata.is_dir() {
            return Err(AppError::PathSecurity(
                "flattened subagent root is not a normal directory".into(),
            ));
        }
        for entry in fs::read_dir(&agents_root)? {
            let entry = entry?;
            let path = entry.path();
            let metadata = fs::symlink_metadata(&path)?;
            if is_link_metadata(&metadata) {
                return Err(AppError::PathSecurity(format!(
                    "flattened subagent file is a link: {}",
                    path.display()
                )));
            }
            if !metadata.is_file()
                || path.extension().and_then(|value| value.to_str()) != Some("toml")
            {
                continue;
            }
            let relative = format!(".codex/agents/{}", entry.file_name().to_string_lossy());
            if !sources.contains_key(&relative) {
                insert_source(
                    sources,
                    &relative,
                    read_regular_file_no_follow_under_root(project_root, &relative)?,
                )?;
            }
        }
    }
    Ok(())
}

pub(crate) fn read_regular_file_no_follow_under_root(
    root: &Path,
    relative: &str,
) -> Result<Vec<u8>, AppError> {
    let normalized = normalize_relative_path(relative)?;
    let path = safe_join(root, &normalized)?;
    let root_canonical = fs::canonicalize(root).map_err(|error| {
        AppError::PathSecurity(format!("flattened project root is not accessible: {error}"))
    })?;
    let mut file = open_regular_file_under_root(&root_canonical, &normalized).map_err(|error| {
        AppError::InvalidInput(format!(
            "flattened file is not readable: {relative}: {error}"
        ))
    })?;
    verify_open_file_contained(&file, &root_canonical, relative)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() || is_link_metadata(&metadata) {
        return Err(AppError::PathSecurity(format!(
            "flattened file is not a regular file: {relative}"
        )));
    }
    if metadata.len() > MAX_FLAT_FILE_BYTES {
        return Err(AppError::InvalidInput(format!(
            "flattened file exceeds the {} MiB limit: {relative}",
            MAX_FLAT_FILE_BYTES / 1024 / 1024
        )));
    }
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).map_err(|error| {
        AppError::InvalidInput(format!(
            "flattened file could not be read: {relative}: {error}"
        ))
    })?;
    let after = file.metadata()?;
    verify_open_file_contained(&file, &root_canonical, relative)?;
    if !after.is_file()
        || is_link_metadata(&after)
        || after.len() != bytes.len() as u64
        || path_has_link_component_for_flatten(&path)
    {
        return Err(AppError::PathSecurity(format!(
            "flattened file changed or became a link during read: {relative}"
        )));
    }
    Ok(bytes)
}

#[cfg(unix)]
fn open_regular_file_under_root(
    root: &Path,
    relative: &str,
) -> Result<std::fs::File, std::io::Error> {
    let root_file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(root)?;
    let mut directory = root_file;
    let parts = relative.split('/').collect::<Vec<_>>();
    for (index, part) in parts.iter().enumerate() {
        let name = CString::new(part.as_bytes()).map_err(|_| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "path segment contains NUL",
            )
        })?;
        let mut flags = libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_CLOEXEC;
        if index + 1 != parts.len() {
            flags |= libc::O_DIRECTORY;
        }
        let descriptor = unsafe { libc::openat(directory.as_raw_fd(), name.as_ptr(), flags) };
        if descriptor < 0 {
            return Err(std::io::Error::last_os_error());
        }
        let next = unsafe { std::fs::File::from_raw_fd(descriptor) };
        if index + 1 == parts.len() {
            return Ok(next);
        }
        directory = next;
    }
    Err(std::io::Error::new(
        std::io::ErrorKind::InvalidInput,
        "relative path has no file name",
    ))
}

#[cfg(windows)]
fn open_regular_file_under_root(
    _root: &Path,
    relative: &str,
) -> Result<std::fs::File, std::io::Error> {
    let path = safe_join(_root, relative).map_err(|error| {
        std::io::Error::new(std::io::ErrorKind::PermissionDenied, error.to_string())
    })?;
    let mut options = OpenOptions::new();
    options.read(true);
    options.custom_flags(0x0020_0000);
    options.open(path)
}

fn verify_open_file_contained(
    file: &std::fs::File,
    root: &Path,
    display_path: &str,
) -> Result<(), AppError> {
    #[cfg(windows)]
    {
        let final_path = windows_final_path(file)?;
        let root = root
            .to_string_lossy()
            .trim_end_matches(['\\', '/'])
            .to_ascii_lowercase();
        let final_path = final_path
            .to_string_lossy()
            .trim_end_matches(['\\', '/'])
            .to_ascii_lowercase();
        if final_path != root
            && !final_path.starts_with(&(root.clone() + "\\"))
            && !final_path.starts_with(&(root + "/"))
        {
            return Err(AppError::PathSecurity(format!(
                "flattened file handle escaped its root: {display_path}"
            )));
        }
    }
    #[cfg(not(windows))]
    {
        let _ = (file, root, display_path);
    }
    Ok(())
}

#[cfg(windows)]
fn windows_final_path(file: &std::fs::File) -> Result<OsString, AppError> {
    let handle = file.as_raw_handle();
    let mut buffer = vec![0_u16; 512];
    loop {
        let length = unsafe {
            GetFinalPathNameByHandleW(
                handle,
                buffer.as_mut_ptr(),
                buffer.len() as u32,
                FILE_NAME_NORMALIZED,
            )
        };
        if length == 0 {
            return Err(AppError::PathSecurity(format!(
                "cannot resolve flattened file handle: {}",
                std::io::Error::last_os_error()
            )));
        }
        if (length as usize) < buffer.len() {
            return Ok(OsString::from_wide(&buffer[..length as usize]));
        }
        if buffer.len() >= 32 * 1024 {
            return Err(AppError::PathSecurity(
                "flattened file handle path is oversized".into(),
            ));
        }
        buffer.resize(buffer.len() * 2, 0);
    }
}

fn path_has_link_component_for_flatten(path: &Path) -> bool {
    crate::security::path_has_link_component(path)
}

fn insert_output(
    outputs: &mut BTreeMap<String, (String, Vec<u8>)>,
    destination: &str,
    bytes: Vec<u8>,
) -> Result<(), AppError> {
    if bytes.len() as u64 > MAX_FLAT_FILE_BYTES {
        return Err(AppError::InvalidInput(format!(
            "flattened Chat source exceeds the {} MiB per-file limit: {destination}",
            MAX_FLAT_FILE_BYTES / 1024 / 1024
        )));
    }
    let text = String::from_utf8_lossy(&bytes);
    if redact_secrets(&text, &[]) != text {
        return Err(AppError::Credential(format!(
            "flattened Chat source contains secret-shaped content: {destination}"
        )));
    }
    let key = canonical_relative_key(destination)?;
    if let Some((existing_destination, _)) = outputs.get(&key) {
        return Err(AppError::Transaction(format!(
            "flattened Chat source name collision: {destination} conflicts with {existing_destination}"
        )));
    }
    outputs.insert(key, (destination.into(), bytes));
    Ok(())
}

fn insert_source(
    sources: &mut BTreeMap<String, Vec<u8>>,
    destination: &str,
    bytes: Vec<u8>,
) -> Result<(), AppError> {
    if bytes.len() as u64 > MAX_FLAT_FILE_BYTES {
        return Err(AppError::InvalidInput(format!(
            "flattened source exceeds the {} MiB per-file limit: {destination}",
            MAX_FLAT_FILE_BYTES / 1024 / 1024
        )));
    }
    if let Some(existing) = sources.get(destination) {
        if existing == &bytes {
            return Ok(());
        }
        return Err(AppError::Transaction(format!(
            "flattened source input collision: {destination}"
        )));
    }
    if sources.len() >= MAX_FLAT_FILES {
        return Err(AppError::InvalidInput(
            "flattened Chat sources exceed the aggregate file limit".into(),
        ));
    }
    let current_total = sources.values().try_fold(0_u64, |total, existing| {
        total.checked_add(existing.len() as u64)
    });
    let next_total = current_total
        .and_then(|total| total.checked_add(bytes.len() as u64))
        .ok_or_else(|| AppError::InvalidInput("flattened Chat source size overflows".into()))?;
    if next_total > MAX_FLAT_TOTAL_BYTES {
        return Err(AppError::InvalidInput(
            "flattened Chat sources exceed the aggregate size limit".into(),
        ));
    }
    sources.insert(destination.into(), bytes);
    Ok(())
}

fn is_secret_like_path(path: &str) -> bool {
    path.split('/')
        .map(|segment| segment.to_ascii_lowercase())
        .any(|segment| {
            segment == ".env"
                || segment.starts_with(".env.")
                || segment.ends_with(".pem")
                || segment.ends_with(".key")
                || matches!(
                    segment.as_str(),
                    "id_rsa" | "id_ed25519" | "authorized_keys"
                )
                || segment.contains("credential")
                || segment.contains("secret")
                || segment.contains("token")
                || segment.contains("password")
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn prepared(destination: &str, content: &str) -> PreparedFile {
        PreparedFile {
            operation_id: destination.into(),
            destination: destination.into(),
            bytes: content.as_bytes().to_vec(),
            expected_sha256: crate::security::sha256_bytes(content.as_bytes()),
        }
    }

    #[test]
    fn skills_are_flattened_and_required_files_are_preserved() {
        let project = tempfile::tempdir().unwrap();
        let output = build_artifacts(
            &[
                prepared(".agents/skills/hoi4-events/SKILL.md", "events"),
                prepared(".codex/agents/hoi4setup_worker.toml", "fork_context=false"),
            ],
            &[
                GeneratedArtifact {
                    component_id: "core.agents".into(),
                    destination: "AGENTS.md".into(),
                    content: "agents".into(),
                    expected_sha256: crate::security::sha256_bytes(b"agents"),
                    external: false,
                    bytes: None,
                },
                GeneratedArtifact {
                    component_id: "project.readme".into(),
                    destination: "README.md".into(),
                    content: "readme".into(),
                    expected_sha256: crate::security::sha256_bytes(b"readme"),
                    external: false,
                    bytes: None,
                },
            ],
            project.path(),
            &[],
        )
        .unwrap();
        let destinations = output
            .iter()
            .map(|artifact| artifact.destination.as_str())
            .collect::<BTreeSet<_>>();
        assert!(destinations.contains("chatgpt_project_sources/hoi4-events.md"));
        assert!(destinations.contains("chatgpt_project_sources/hoi4setup_worker.toml"));
        assert!(destinations.contains("chatgpt_project_sources/AGENTS.md"));
        assert!(destinations.contains("chatgpt_project_sources/README.md"));
    }

    #[test]
    fn missing_required_flatten_inputs_and_collisions_fail_closed() {
        let project = tempfile::tempdir().unwrap();
        assert!(build_artifacts(
            &[prepared(".agents/skills/one/SKILL.md", "one")],
            &[],
            project.path(),
            &[],
        )
        .is_err());
        assert!(build_artifacts(
            &[
                prepared(".agents/skills/one/SKILL.md", "one"),
                prepared(".agents/skills/two/SKILL.md", "two"),
                prepared(".codex/agents/one.toml", "a"),
                prepared(".codex/agents/two.toml", "b"),
            ],
            &[
                GeneratedArtifact {
                    component_id: "core.agents".into(),
                    destination: "AGENTS.md".into(),
                    content: "agents".into(),
                    expected_sha256: crate::security::sha256_bytes(b"agents"),
                    external: false,
                    bytes: None,
                },
                GeneratedArtifact {
                    component_id: "project.readme".into(),
                    destination: "README.md".into(),
                    content: "readme".into(),
                    expected_sha256: crate::security::sha256_bytes(b"readme"),
                    external: false,
                    bytes: None,
                },
            ],
            project.path(),
            &["../secret.txt".into()],
        )
        .is_err());
    }

    #[test]
    fn case_collisions_and_secret_extras_fail_closed() {
        let project = tempfile::tempdir().unwrap();
        let generated = vec![
            GeneratedArtifact {
                component_id: "core.agents".into(),
                destination: "AGENTS.md".into(),
                content: "agents".into(),
                expected_sha256: crate::security::sha256_bytes(b"agents"),
                external: false,
                bytes: None,
            },
            GeneratedArtifact {
                component_id: "project.readme".into(),
                destination: "README.md".into(),
                content: "readme".into(),
                expected_sha256: crate::security::sha256_bytes(b"readme"),
                external: false,
                bytes: None,
            },
        ];
        let inputs = [
            prepared(".agents/skills/one/SKILL.md", "one"),
            prepared(".agents/skills/two/SKILL.md", "two"),
            prepared(".agents/skills/ONE/SKILL.md", "one"),
            prepared(".codex/agents/one.toml", "agent"),
        ];
        assert!(build_artifacts(&inputs, &generated, project.path(), &[]).is_err());

        fs::write(project.path().join("notes.txt"), "api_key=secret-value").unwrap();
        assert!(build_artifacts(
            &inputs[..2]
                .iter()
                .chain(std::iter::once(&inputs[3]))
                .cloned()
                .collect::<Vec<_>>(),
            &generated,
            project.path(),
            &["notes.txt".into()],
        )
        .is_err());
        fs::write(project.path().join("binary.bin"), [0_u8, 0xff, 0x00]).unwrap();
        assert!(build_artifacts(
            &inputs[..2]
                .iter()
                .chain(std::iter::once(&inputs[3]))
                .cloned()
                .collect::<Vec<_>>(),
            &generated,
            project.path(),
            &["binary.bin".into()],
        )
        .is_err());
        fs::write(project.path().join(".env"), "MESHY_API_KEY=secret-value").unwrap();
        assert!(build_artifacts(
            &inputs[..2]
                .iter()
                .chain(std::iter::once(&inputs[3]))
                .cloned()
                .collect::<Vec<_>>(),
            &generated,
            project.path(),
            &[".env".into()],
        )
        .is_err());
        fs::write(
            project.path().join(".env.local"),
            "MESHY_API_KEY=secret-value",
        )
        .unwrap();
        assert!(build_artifacts(
            &inputs[..2]
                .iter()
                .chain(std::iter::once(&inputs[3]))
                .cloned()
                .collect::<Vec<_>>(),
            &generated,
            project.path(),
            &[".env.local".into()],
        )
        .is_err());
    }

    #[test]
    fn regular_reader_binds_file_access_to_the_project_root() {
        let project = tempfile::tempdir().unwrap();
        fs::create_dir_all(project.path().join("nested")).unwrap();
        fs::write(project.path().join("nested/file.txt"), "inside").unwrap();

        let bytes =
            read_regular_file_no_follow_under_root(project.path(), "nested/file.txt").unwrap();

        assert_eq!(bytes, b"inside");
    }

    #[cfg(unix)]
    #[test]
    fn linked_skill_ancestors_are_rejected_before_flattening() {
        use std::os::unix::fs::symlink;

        let project = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        fs::create_dir_all(outside.path().join("one")).unwrap();
        fs::write(outside.path().join("one/SKILL.md"), "outside").unwrap();
        fs::create_dir_all(project.path().join(".agents")).unwrap();
        symlink(outside.path(), project.path().join(".agents/skills")).unwrap();

        let error = build_artifacts(
            &[
                prepared(".agents/skills/one/SKILL.md", "one"),
                prepared(".codex/agents/worker.toml", "fork_context=false"),
            ],
            &[
                GeneratedArtifact {
                    component_id: "core.agents".into(),
                    destination: "AGENTS.md".into(),
                    content: "agents".into(),
                    expected_sha256: crate::security::sha256_bytes(b"agents"),
                    external: false,
                    bytes: None,
                },
                GeneratedArtifact {
                    component_id: "project.readme".into(),
                    destination: "README.md".into(),
                    content: "readme".into(),
                    expected_sha256: crate::security::sha256_bytes(b"readme"),
                    external: false,
                    bytes: None,
                },
            ],
            project.path(),
            &[],
        )
        .unwrap_err();
        assert!(error.to_string().contains("link"));
    }
}
