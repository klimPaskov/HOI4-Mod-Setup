//! The web edition's result: compare the reviewed incoming files with the
//! folder the user dropped, apply the user's keep or replace decisions, and
//! write a ZIP with the installation lock. Nothing is written into the
//! dropped folder; it stays the backup.

use crate::adapters::{EntryKind, ProjectFs};
use crate::models::*;
use crate::text::{normalize_relative_path, sha256_bytes, validate_destination_set};
use crate::zip::{write_zip, ZipEntry};
use crate::AppError;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Largest single project file compared or copied.
pub const MAX_EXPORT_FILE_BYTES: u64 = 64 * 1024 * 1024;

/// One reviewed file the setup wants in the project.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IncomingFile {
    pub path: String,
    pub component_id: String,
    pub bytes: Vec<u8>,
    pub ownership: Ownership,
    /// Manifest source path, or `generated:<path>` for generated files.
    pub source_path: String,
    pub executable: bool,
}

impl IncomingFile {
    fn generated(&self) -> bool {
        self.source_path.starts_with("generated:")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExportAction {
    /// The folder has no file here; the setup adds it.
    Create,
    /// The folder already has exactly these bytes.
    Unchanged,
    /// The folder has different bytes; the user keeps or replaces them.
    Conflict,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExportEntry {
    pub path: String,
    pub component_id: String,
    pub action: ExportAction,
    pub incoming_sha256: String,
    pub local_sha256: Option<String>,
    pub size: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Decision {
    Keep,
    Replace,
}

/// What the ZIP holds for an existing mod.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExportLayout {
    /// Only new and replaced files, to extract over the mod folder.
    Overlay,
    /// The whole mod folder with the changes applied.
    FullCopy,
}

fn exists(fs: &dyn ProjectFs, path: &str) -> Result<bool, AppError> {
    let parent = path.rsplit_once('/').map_or("", |(parent, _)| parent);
    if !parent.is_empty() && !exists_directory(fs, parent)? {
        return Ok(false);
    }
    Ok(fs
        .list(parent)?
        .iter()
        .any(|entry| entry.path == path && entry.kind == EntryKind::File))
}

fn exists_directory(fs: &dyn ProjectFs, path: &str) -> Result<bool, AppError> {
    let parent = path.rsplit_once('/').map_or("", |(parent, _)| parent);
    if !parent.is_empty() && !exists_directory(fs, parent)? {
        return Ok(false);
    }
    Ok(fs
        .list(parent)?
        .iter()
        .any(|entry| entry.path == path && entry.kind == EntryKind::Directory))
}

/// Compare every incoming file with the dropped folder (`None` for a new mod).
pub fn plan_export(
    incoming: &[IncomingFile],
    local: Option<&dyn ProjectFs>,
) -> Result<Vec<ExportEntry>, AppError> {
    let paths = incoming
        .iter()
        .map(|file| normalize_relative_path(&file.path))
        .collect::<Result<Vec<_>, _>>()?;
    validate_destination_set(&paths)?;
    let mut entries = Vec::with_capacity(incoming.len());
    for (file, path) in incoming.iter().zip(paths) {
        let incoming_sha256 = sha256_bytes(&file.bytes);
        let local_sha256 = match local {
            Some(fs) if exists(fs, &path)? => {
                Some(sha256_bytes(&fs.read(&path, MAX_EXPORT_FILE_BYTES)?))
            }
            _ => None,
        };
        let action = match &local_sha256 {
            None => ExportAction::Create,
            Some(local) if *local == incoming_sha256 => ExportAction::Unchanged,
            Some(_) => ExportAction::Conflict,
        };
        entries.push(ExportEntry {
            path,
            component_id: file.component_id.clone(),
            action,
            incoming_sha256,
            local_sha256,
            size: file.bytes.len() as u64,
        });
    }
    entries.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(entries)
}

/// The non-file parts of the installation lock written into the ZIP.
#[derive(Debug, Clone)]
pub struct LockInputs {
    pub project_id: String,
    pub script_prefix: Option<String>,
    pub primary_namespace: Option<String>,
    pub installed_at: String,
    pub source: LockSourceIdentity,
    pub ai_provider: String,
    pub ai_model: String,
    pub ai_reasoning_effort: String,
    /// The hosted route address when a hosted provider made suggestions.
    pub ai_endpoint: Option<String>,
    pub ai_optimization_profile: String,
    pub primary_coding_environment: String,
    pub additional_coding_environments: Vec<String>,
    pub wiki_required_pages: Vec<String>,
    pub wiki_metadata: WikiInstallMetadata,
    pub components: Vec<LockComponent>,
    pub optional_workflows: BTreeMap<String, OptionalWorkflowLock>,
}

pub struct ExportRequest<'a> {
    pub incoming: &'a [IncomingFile],
    pub entries: &'a [ExportEntry],
    pub decisions: &'a BTreeMap<String, Decision>,
    pub local: Option<&'a dyn ProjectFs>,
    pub layout: ExportLayout,
    /// Folder name for a full copy or a new mod, such as the project ID.
    pub root_folder: &'a str,
    /// Extra files placed at the archive root, such as the launcher
    /// descriptor `<id>.mod` for a new mod.
    pub root_files: Vec<ZipEntry>,
    pub lock: LockInputs,
}

fn locked_file(
    file: &IncomingFile,
    revision: &str,
    installed: &[u8],
    preserved_local: bool,
) -> LockedFile {
    let generated = file.generated() && !preserved_local;
    // Text is recorded once; only binary output keeps bytes.
    let generated_text = generated
        .then(|| String::from_utf8(installed.to_vec()).ok())
        .flatten();
    LockedFile {
        path: file.path.clone(),
        location_scope: Some("project".into()),
        component_id: file.component_id.clone(),
        source_path: file.source_path.clone(),
        source_revision: revision.to_owned(),
        source_sha256: sha256_bytes(&file.bytes),
        source_size: Some(file.bytes.len() as u64),
        base_sha256: None,
        installed_sha256: sha256_bytes(installed),
        installed_size: Some(installed.len() as u64),
        ownership: file.ownership,
        preserved_local,
        external: false,
        generated_content: generated_text.clone(),
        generated_bytes: (generated && generated_text.is_none()).then(|| installed.to_vec()),
        executable: file.executable,
        platform: Some(ManifestPlatform::All),
        installer_created: !preserved_local && !file.generated(),
    }
}

fn changes_summary(entries: &[ExportEntry], decisions: &BTreeMap<String, Decision>) -> String {
    let mut created = Vec::new();
    let mut replaced = Vec::new();
    let mut kept = Vec::new();
    for entry in entries {
        match (entry.action, decisions.get(&entry.path)) {
            (ExportAction::Create, _) => created.push(entry.path.as_str()),
            (ExportAction::Conflict, Some(Decision::Replace)) => replaced.push(entry.path.as_str()),
            (ExportAction::Conflict, _) => kept.push(entry.path.as_str()),
            (ExportAction::Unchanged, _) => {}
        }
    }
    let list = |paths: &[&str]| {
        if paths.is_empty() {
            "- None\n".to_owned()
        } else {
            paths.iter().map(|path| format!("- `{path}`\n")).collect()
        }
    };
    format!(
        "# Changes from HOI4 Mod Setup\n\nExtract this archive over your mod folder. Files not listed here are untouched.\n\n## Added\n\n{}\n## Replaced\n\n{}\n## Kept as you had them\n\n{}",
        list(&created),
        list(&replaced),
        list(&kept)
    )
}

/// Build the export ZIP. Every conflict needs a decision.
pub fn build_export(request: ExportRequest<'_>) -> Result<Vec<u8>, AppError> {
    let by_path = request
        .incoming
        .iter()
        .map(|file| Ok((normalize_relative_path(&file.path)?, file)))
        .collect::<Result<BTreeMap<_, _>, AppError>>()?;
    let revision = request.lock.source.revision.clone();
    let mut project_files: Vec<ZipEntry> = Vec::new();
    let mut locked = Vec::new();
    for entry in request.entries {
        let file = by_path.get(&entry.path).ok_or_else(|| {
            AppError::InvalidInput(format!("the plan lists an unknown file: {}", entry.path))
        })?;
        match entry.action {
            ExportAction::Create => {
                project_files.push(ZipEntry {
                    path: entry.path.clone(),
                    bytes: file.bytes.clone(),
                });
                locked.push(locked_file(file, &revision, &file.bytes, false));
            }
            ExportAction::Unchanged => {
                if request.layout == ExportLayout::FullCopy || request.local.is_none() {
                    project_files.push(ZipEntry {
                        path: entry.path.clone(),
                        bytes: file.bytes.clone(),
                    });
                }
                locked.push(locked_file(file, &revision, &file.bytes, false));
            }
            ExportAction::Conflict => match request.decisions.get(&entry.path) {
                Some(Decision::Replace) => {
                    project_files.push(ZipEntry {
                        path: entry.path.clone(),
                        bytes: file.bytes.clone(),
                    });
                    locked.push(locked_file(file, &revision, &file.bytes, false));
                }
                Some(Decision::Keep) => {
                    let local = request
                        .local
                        .ok_or_else(|| {
                            AppError::InvalidInput("a kept file needs the dropped folder".into())
                        })?
                        .read(&entry.path, MAX_EXPORT_FILE_BYTES)?;
                    if request.layout == ExportLayout::FullCopy {
                        project_files.push(ZipEntry {
                            path: entry.path.clone(),
                            bytes: local.clone(),
                        });
                    }
                    locked.push(locked_file(file, &revision, &local, true));
                }
                None => {
                    return Err(AppError::InvalidInput(format!(
                        "decide whether to keep or replace {}",
                        entry.path
                    )))
                }
            },
        }
    }
    if let (Some(fs), ExportLayout::FullCopy) = (request.local, request.layout) {
        let mut stack = vec![String::new()];
        while let Some(directory) = stack.pop() {
            for item in fs.list(&directory)? {
                match item.kind {
                    EntryKind::Directory => stack.push(item.path),
                    EntryKind::File if !by_path.contains_key(&item.path) => {
                        project_files.push(ZipEntry {
                            bytes: fs.read(&item.path, MAX_EXPORT_FILE_BYTES)?,
                            path: item.path,
                        });
                    }
                    _ => {}
                }
            }
        }
    }
    locked.sort_by(|left, right| left.path.cmp(&right.path));
    let inputs = request.lock;
    let lock = InstallationLock {
        schema_version: "1.0.0".into(),
        project_id: inputs.project_id,
        script_prefix: inputs.script_prefix,
        primary_namespace: inputs.primary_namespace,
        installed_at: inputs.installed_at,
        updated_at: None,
        source: inputs.source,
        ai_provider: inputs.ai_provider,
        ai_model: inputs.ai_model,
        ai_reasoning_effort: inputs.ai_reasoning_effort,
        ai_endpoint: inputs.ai_endpoint,
        ai_optimization_profile: inputs.ai_optimization_profile,
        primary_coding_environment: inputs.primary_coding_environment,
        additional_coding_environments: inputs.additional_coding_environments,
        flatten_chat_sources: false,
        codex_analysis: None,
        wiki_required_pages: inputs.wiki_required_pages,
        wiki_metadata: Some(inputs.wiki_metadata),
        components: inputs.components,
        files: locked,
        merge_choices: request
            .decisions
            .iter()
            .map(|(path, decision)| MergeChoice {
                path: path.clone(),
                choice: match decision {
                    Decision::Keep => "keep",
                    Decision::Replace => "replace",
                }
                .into(),
                result_sha256: None,
            })
            .collect(),
        optional_workflows: inputs.optional_workflows,
        portrait_pipeline: None,
        image_generation: None,
        live_testing: None,
        local_modifications: Vec::new(),
        rollback_records: Vec::new(),
    };
    project_files.push(ZipEntry {
        path: ".hoi4-mod-setup/install.lock.json".into(),
        bytes: crate::text::persistable_json_bytes(&lock)?,
    });
    if request.local.is_some() && request.layout == ExportLayout::Overlay {
        project_files.push(ZipEntry {
            path: "HOI4_MOD_SETUP_CHANGES.md".into(),
            bytes: changes_summary(request.entries, request.decisions).into_bytes(),
        });
    }
    let prefix = if request.local.is_some() && request.layout == ExportLayout::Overlay {
        String::new()
    } else {
        format!("{}/", normalize_relative_path(request.root_folder)?)
    };
    let mut archive_entries = request.root_files;
    archive_entries.extend(project_files.into_iter().map(|file| ZipEntry {
        path: format!("{prefix}{}", file.path),
        bytes: file.bytes,
    }));
    archive_entries.sort_by(|left, right| left.path.cmp(&right.path));
    write_zip(&archive_entries)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory_fs::MemoryFs;
    use crate::zip::read_zip;

    fn incoming() -> Vec<IncomingFile> {
        vec![
            IncomingFile {
                path: "AGENTS.md".into(),
                component_id: "core.agents".into(),
                bytes: b"# Agents\n".to_vec(),
                ownership: Ownership::Merged,
                source_path: "templates/AGENTS.md".into(),
                executable: false,
            },
            IncomingFile {
                path: ".claude/skills/demo/SKILL.md".into(),
                component_id: "core.skills".into(),
                bytes: b"skill v2".to_vec(),
                ownership: Ownership::Managed,
                source_path: ".claude/skills/demo/SKILL.md".into(),
                executable: false,
            },
            IncomingFile {
                path: "descriptor.mod".into(),
                component_id: "project.descriptor".into(),
                bytes: b"name=\"Demo\"\n".to_vec(),
                ownership: Ownership::Generated,
                source_path: "generated:descriptor.mod".into(),
                executable: false,
            },
        ]
    }

    fn lock_inputs() -> LockInputs {
        LockInputs {
            project_id: "demo".into(),
            script_prefix: Some("demo".into()),
            primary_namespace: Some("demo".into()),
            installed_at: "2026-10-18T00:00:00Z".into(),
            source: LockSourceIdentity {
                repository: "klimPaskov/Agentic-HOI4-Modding".into(),
                mode: SourceMode::PinnedCommit,
                revision: "a".repeat(40),
                requested_ref: Some("a".repeat(40)),
                release: None,
                manifest_sha256: "b".repeat(64),
                manifest_origin: "remote".into(),
            },
            ai_provider: "none".into(),
            ai_model: "deterministic".into(),
            ai_reasoning_effort: "none".into(),
            ai_endpoint: None,
            ai_optimization_profile: "Deterministic setup".into(),
            primary_coding_environment: "claude_code".into(),
            additional_coding_environments: Vec::new(),
            wiki_required_pages: Vec::new(),
            wiki_metadata: WikiInstallMetadata {
                snapshot_marker: None,
                required_media_policy: "none".into(),
                source_status: "verified".into(),
                license_status: "verified".into(),
                repository_license_status: "verified".into(),
                notes: Vec::new(),
            },
            components: vec![LockComponent {
                id: "core.agents".into(),
                version: None,
                state: "installed".into(),
                source_revision: Some("a".repeat(40)),
                validation: Some("not_run".into()),
            }],
            optional_workflows: BTreeMap::new(),
        }
    }

    fn dropped() -> MemoryFs {
        let mut fs = MemoryFs::new();
        fs.add_file("descriptor.mod", b"name=\"Demo\"\n".to_vec())
            .unwrap();
        fs.add_file("AGENTS.md", b"# My own rules\n".to_vec())
            .unwrap();
        fs.add_file("events/demo.txt", b"add_namespace = demo\n".to_vec())
            .unwrap();
        fs
    }

    #[test]
    fn existing_mods_are_compared_and_exported_as_an_overlay() {
        let fs = dropped();
        let files = incoming();
        let entries = plan_export(&files, Some(&fs)).unwrap();
        let action = |path: &str| {
            entries
                .iter()
                .find(|entry| entry.path == path)
                .unwrap()
                .action
        };
        assert_eq!(action("descriptor.mod"), ExportAction::Unchanged);
        assert_eq!(action("AGENTS.md"), ExportAction::Conflict);
        assert_eq!(action(".claude/skills/demo/SKILL.md"), ExportAction::Create);

        let undecided = build_export(ExportRequest {
            incoming: &files,
            entries: &entries,
            decisions: &BTreeMap::new(),
            local: Some(&fs),
            layout: ExportLayout::Overlay,
            root_folder: "demo",
            root_files: Vec::new(),
            lock: lock_inputs(),
        });
        assert!(undecided.is_err(), "every conflict needs a decision");

        let decisions = BTreeMap::from([("AGENTS.md".to_owned(), Decision::Keep)]);
        let archive = build_export(ExportRequest {
            incoming: &files,
            entries: &entries,
            decisions: &decisions,
            local: Some(&fs),
            layout: ExportLayout::Overlay,
            root_folder: "demo",
            root_files: Vec::new(),
            lock: lock_inputs(),
        })
        .unwrap();
        let paths = read_zip(&archive)
            .unwrap()
            .into_iter()
            .map(|entry| entry.path)
            .collect::<Vec<_>>();
        assert_eq!(
            paths,
            [
                ".claude/skills/demo/SKILL.md",
                ".hoi4-mod-setup/install.lock.json",
                "HOI4_MOD_SETUP_CHANGES.md"
            ],
            "an overlay holds only new files, the lock, and the change list"
        );
    }

    #[test]
    fn a_full_copy_keeps_every_local_file_and_the_lock_validates() {
        let fs = dropped();
        let files = incoming();
        let entries = plan_export(&files, Some(&fs)).unwrap();
        let decisions = BTreeMap::from([("AGENTS.md".to_owned(), Decision::Keep)]);
        let archive = build_export(ExportRequest {
            incoming: &files,
            entries: &entries,
            decisions: &decisions,
            local: Some(&fs),
            layout: ExportLayout::FullCopy,
            root_folder: "demo",
            root_files: vec![ZipEntry {
                path: "demo.mod".into(),
                bytes: b"name=\"Demo\"\npath=\"mod/demo\"\n".to_vec(),
            }],
            lock: lock_inputs(),
        })
        .unwrap();
        let entries_in_zip = read_zip(&archive).unwrap();
        let file = |path: &str| {
            entries_in_zip
                .iter()
                .find(|entry| entry.path == path)
                .unwrap_or_else(|| panic!("{path} missing"))
                .bytes
                .clone()
        };
        assert_eq!(
            file("demo/AGENTS.md"),
            b"# My own rules\n",
            "the kept file stays as it was"
        );
        assert_eq!(file("demo/events/demo.txt"), b"add_namespace = demo\n");
        assert_eq!(file("demo.mod"), b"name=\"Demo\"\npath=\"mod/demo\"\n");
        let lock: serde_json::Value =
            serde_json::from_slice(&file("demo/.hoi4-mod-setup/install.lock.json")).unwrap();
        let validator = crate::schemas::validator(include_str!(
            "../../../docs/schemas/installation-lock.schema.json"
        ))
        .unwrap();
        if let Err(error) = validator.validate(&lock) {
            panic!(
                "lock does not match the schema at {}: {error}",
                error.instance_path()
            );
        }
        let agents = lock["files"]
            .as_array()
            .unwrap()
            .iter()
            .find(|file| file["path"] == "AGENTS.md")
            .unwrap();
        assert_eq!(agents["preserved_local"], true);
        assert_eq!(
            agents["installed_sha256"],
            sha256_bytes(b"# My own rules\n")
        );
    }

    #[test]
    fn a_new_mod_is_one_folder_with_its_launcher_file() {
        let files = incoming();
        let entries = plan_export(&files, None).unwrap();
        assert!(entries
            .iter()
            .all(|entry| entry.action == ExportAction::Create));
        let archive = build_export(ExportRequest {
            incoming: &files,
            entries: &entries,
            decisions: &BTreeMap::new(),
            local: None,
            layout: ExportLayout::FullCopy,
            root_folder: "demo",
            root_files: vec![ZipEntry {
                path: "demo.mod".into(),
                bytes: b"name=\"Demo\"\npath=\"mod/demo\"\n".to_vec(),
            }],
            lock: lock_inputs(),
        })
        .unwrap();
        let paths = read_zip(&archive)
            .unwrap()
            .into_iter()
            .map(|entry| entry.path)
            .collect::<Vec<_>>();
        assert!(paths
            .iter()
            .all(|path| path == "demo.mod" || path.starts_with("demo/")));
        assert!(paths.contains(&"demo/descriptor.mod".to_owned()));
    }
}
