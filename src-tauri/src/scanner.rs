use crate::models::*;
use crate::security::{is_link_metadata, path_has_link_component, sha256_bytes};
use crate::AppError;
use chrono::Utc;
use regex::Regex;
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use uuid::Uuid;

const MAX_FILES: usize = 20_000;
const MAX_DEPTH: usize = 16;
const MAX_DIRECTORIES: usize = 10_000;
const MAX_TOTAL_BYTES: u64 = 64 * 1024 * 1024;
const MAX_TEXT_BYTES: u64 = 2 * 1024 * 1024;
const MAX_LAUNCHER_CANDIDATES: usize = 512;
const MAX_LAUNCHER_DESCRIPTOR_BYTES: u64 = 256 * 1024;

const IGNORED_SCAN_DIRECTORIES: &[&str] = &[
    ".git",
    ".hoi4-mod-setup",
    ".tools",
    ".tmp",
    "paradox_wiki",
    ".venv",
    "venv",
    "env",
    "__pycache__",
    ".pytest_cache",
    ".mypy_cache",
    ".ruff_cache",
    ".tox",
    ".nox",
    ".idea",
    ".vscode",
    ".vs",
    ".cache",
    "cache",
    "coverage",
    "htmlcov",
    "node_modules",
    "target",
    "dist",
    "build",
    "out",
];

const IGNORED_SCAN_FILES: &[&str] = &[".ds_store", "thumbs.db", "desktop.ini"];

#[derive(Debug, Clone)]
pub struct ScanOptions {
    pub max_files: usize,
    pub max_depth: usize,
    pub approved_external_descriptor: Option<PathBuf>,
    pub cancel_after_files: Option<usize>,
    pub cancel_flag: Option<Arc<AtomicBool>>,
}

impl Default for ScanOptions {
    fn default() -> Self {
        Self {
            max_files: MAX_FILES,
            max_depth: MAX_DEPTH,
            approved_external_descriptor: None,
            cancel_after_files: None,
            cancel_flag: None,
        }
    }
}

pub fn discover_launcher_descriptor(root: &Path) -> Result<Option<PathBuf>, AppError> {
    let root = fs::canonicalize(root)
        .map_err(|error| AppError::Scan(format!("cannot resolve project root: {error}")))?;
    let parent = root
        .parent()
        .ok_or_else(|| AppError::Scan("project root has no parent directory".into()))?;
    if path_has_link_component(parent) {
        return Err(AppError::Scan(
            "project parent contains a symlink or junction".into(),
        ));
    }
    let expected_name = root
        .file_name()
        .map(|name| format!("{}.mod", name.to_string_lossy()).to_ascii_lowercase());
    let mut matches = Vec::new();
    let mut inspected = 0usize;
    for entry in fs::read_dir(parent)
        .map_err(|error| AppError::Scan(format!("cannot inspect project parent: {error}")))?
    {
        let entry = entry?;
        let path = entry.path();
        let is_mod = path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("mod"));
        if !is_mod {
            continue;
        }
        inspected += 1;
        if inspected > MAX_LAUNCHER_CANDIDATES {
            break;
        }
        let metadata = fs::symlink_metadata(&path)?;
        if is_link_metadata(&metadata)
            || !metadata.is_file()
            || metadata.len() > MAX_LAUNCHER_DESCRIPTOR_BYTES
        {
            continue;
        }
        let bytes = fs::read(&path)?;
        let Ok(descriptor) = crate::descriptors::parse_descriptor(&bytes) else {
            continue;
        };
        let Some(declared) = descriptor.fields.get("path") else {
            continue;
        };
        let declared_path = PathBuf::from(declared);
        if !declared_path.is_absolute() || path_has_link_component(&declared_path) {
            continue;
        }
        let Ok(declared_root) = fs::canonicalize(&declared_path) else {
            continue;
        };
        let same_root = if cfg!(target_os = "windows") {
            declared_root
                .to_string_lossy()
                .eq_ignore_ascii_case(&root.to_string_lossy())
        } else {
            declared_root == root
        };
        if same_root {
            let exact_name = expected_name.as_deref().is_some_and(|expected| {
                path.file_name()
                    .map(|name| name.to_string_lossy().to_ascii_lowercase() == expected)
                    .unwrap_or(false)
            });
            matches.push((exact_name, path));
        }
    }
    matches.sort_by(|left, right| right.0.cmp(&left.0).then_with(|| left.1.cmp(&right.1)));
    if matches.len() > 1 && !matches[0].0 {
        return Ok(None);
    }
    Ok(matches.into_iter().next().map(|(_, path)| path))
}

#[derive(Debug, Clone, Serialize)]
pub struct ScanProgress {
    pub stage: String,
    pub current_path: String,
    pub files_scanned: u32,
    pub directories_scanned: u32,
    pub bytes_read: u64,
}

#[derive(Debug, Clone)]
struct FileObservation {
    relative: String,
    bytes: Vec<u8>,
    is_symlink: bool,
}

pub fn scan_project(root: &Path, options: &ScanOptions) -> Result<ScanResult, AppError> {
    scan_project_with_progress(root, options, |_| {})
}

pub fn scan_project_with_progress<F>(
    root: &Path,
    options: &ScanOptions,
    mut progress: F,
) -> Result<ScanResult, AppError>
where
    F: FnMut(ScanProgress),
{
    let started_at = Utc::now().to_rfc3339();
    let root_metadata = fs::symlink_metadata(root)
        .map_err(|error| AppError::Scan(format!("cannot inspect project root: {error}")))?;
    if is_link_metadata(&root_metadata) {
        return Err(AppError::Scan(
            "project root is a symlink or junction; choose its resolved directory explicitly"
                .into(),
        ));
    }
    let root = fs::canonicalize(root)
        .map_err(|error| AppError::Scan(format!("cannot open project root: {error}")))?;
    if !root.is_dir() {
        return Err(AppError::Scan("project root is not a directory".into()));
    }

    let mut observations = Vec::new();
    let mut link_conflicts = Vec::new();
    let mut total_bytes = 0_u64;
    let mut directories = 0_usize;
    emit_progress(
        &mut progress,
        "discovering_files",
        &root,
        &root,
        &observations,
        directories,
        total_bytes,
    );
    collect_files(
        &root,
        &root,
        0,
        options,
        &mut observations,
        &mut link_conflicts,
        &mut total_bytes,
        &mut directories,
        &mut progress,
    )?;
    observations.sort_by(|left, right| left.relative.cmp(&right.relative));

    let mut findings = Vec::new();
    let mut conflicts = link_conflicts;
    if scan_stopped(options, observations.len()) {
        mark_cancelled(
            &root,
            &mut conflicts,
            &mut progress,
            &observations,
            directories,
            total_bytes,
        );
    } else {
        emit_progress(
            &mut progress,
            "detecting_descriptors",
            Path::new("descriptor.mod"),
            &root,
            &observations,
            directories,
            total_bytes,
        );
        detect_descriptors(
            &root,
            observations
                .iter()
                .find(|file| file.relative == "descriptor.mod"),
            options.approved_external_descriptor.as_deref(),
            &mut findings,
            &mut conflicts,
        );
        emit_progress(
            &mut progress,
            "detecting_thumbnail",
            Path::new("thumbnail.png"),
            &root,
            &observations,
            directories,
            total_bytes,
        );
        detect_thumbnail(
            observations
                .iter()
                .find(|file| file.relative == "thumbnail.png"),
            &mut findings,
            &mut conflicts,
        );
        emit_progress(
            &mut progress,
            "detecting_structure",
            Path::new("<project tree>"),
            &root,
            &observations,
            directories,
            total_bytes,
        );
        detect_structure(&observations, &mut findings);
        emit_progress(
            &mut progress,
            "detecting_git",
            Path::new(".git"),
            &root,
            &observations,
            directories,
            total_bytes,
        );
        detect_git(&root, &observations, &mut findings, &mut conflicts);
        emit_progress(
            &mut progress,
            "detecting_identifiers",
            Path::new("events/ and common/"),
            &root,
            &observations,
            directories,
            total_bytes,
        );
        detect_identifiers(&observations, &mut findings);
        emit_progress(
            &mut progress,
            "detecting_localisation",
            Path::new("localisation/"),
            &root,
            &observations,
            directories,
            total_bytes,
        );
        detect_localisation(&observations, &mut findings, &mut conflicts);
        emit_progress(
            &mut progress,
            "detecting_documentation",
            Path::new("README.md and docs/"),
            &root,
            &observations,
            directories,
            total_bytes,
        );
        detect_documentation(&observations, &mut findings);
        emit_progress(
            &mut progress,
            "detecting_agentic_files",
            Path::new(".agents/ and .codex/"),
            &root,
            &observations,
            directories,
            total_bytes,
        );
        detect_agentic_files(&observations, &mut findings, &mut conflicts);
        emit_progress(
            &mut progress,
            "detecting_paths",
            Path::new("absolute paths"),
            &root,
            &observations,
            directories,
            total_bytes,
        );
        detect_absolute_paths(&observations, &mut findings);
        emit_progress(
            &mut progress,
            "detecting_components",
            Path::new("managed component files"),
            &root,
            &observations,
            directories,
            total_bytes,
        );
        detect_existing_components(&root, &observations, &mut conflicts);
        detect_managed_installation(&root, &mut findings, &mut conflicts);

        if let Some(external) = options.approved_external_descriptor.as_deref() {
            let exists = fs::symlink_metadata(external)
                .ok()
                .is_some_and(|metadata| metadata.is_file() && !is_link_metadata(&metadata));
            if !exists {
                findings.push(finding(
                    "descriptor.launcher.missing",
                    "descriptor",
                    "launcher_descriptor",
                    json!(null),
                    "needs_review",
                    evidence(
                        "approved_external_descriptor",
                        &external.display().to_string(),
                        0.5,
                        Some("Approved launcher descriptor was not found."),
                    ),
                    Some("Choose a launcher descriptor destination during review."),
                ));
            }
        }
    }

    findings.sort_by(|left, right| left.id.cmp(&right.id));
    conflicts.sort_by(|left, right| {
        left.id
            .cmp(&right.id)
            .then_with(|| left.path.cmp(&right.path))
    });
    conflicts.dedup_by(|left, right| left.id == right.id && left.path == right.path);
    let summary = summarize(&findings, &conflicts);
    let limits_hit = conflicts
        .iter()
        .filter(|conflict| {
            conflict.id.starts_with("scan.") && !conflict.id.starts_with("scan.sensitive.")
        })
        .map(|conflict| conflict.id.clone())
        .collect::<Vec<_>>();
    let cancelled = limits_hit.iter().any(|id| id == "scan.cancelled");
    emit_progress(
        &mut progress,
        if cancelled { "cancelled" } else { "complete" },
        &root,
        &root,
        &observations,
        directories,
        total_bytes,
    );
    Ok(ScanResult {
        schema_version: "1.0.0".into(),
        scan_id: Uuid::new_v4(),
        project_root: root.display().to_string(),
        mode: "existing".into(),
        platform: Some(Platform::current()),
        started_at,
        completed_at: Some(Utc::now().to_rfc3339()),
        read_only: true,
        scanner_version: Some("0.1.0".into()),
        partial: !limits_hit.is_empty(),
        cancelled,
        semantic_analysis: crate::models::SemanticAnalysisSummary {
            requested: true,
            required: true,
            status: "authentication_required".into(),
            engine: "codex_app_server".into(),
            auth_mode: "chatgpt".into(),
            transport: "stdio_jsonl".into(),
            model: None,
            input_manifest: Vec::new(),
            output_schema_id: "codex-analysis/1.0.0".into(),
            analysis_id: None,
            response_sha256: None,
            suggestions: Vec::new(),
        },
        limits_hit,
        files_scanned: observations.len() as u32,
        directories_scanned: directories as u32,
        bytes_read: total_bytes,
        findings,
        conflicts,
        summary,
    })
}

#[allow(clippy::too_many_arguments)]
fn collect_files(
    root: &Path,
    current: &Path,
    depth: usize,
    options: &ScanOptions,
    observations: &mut Vec<FileObservation>,
    link_conflicts: &mut Vec<ScanConflict>,
    total_bytes: &mut u64,
    directories: &mut usize,
    progress: &mut impl FnMut(ScanProgress),
) -> Result<(), AppError> {
    if scan_stopped(options, observations.len()) {
        mark_cancelled(
            root,
            link_conflicts,
            progress,
            observations,
            *directories,
            *total_bytes,
        );
        return Ok(());
    }
    if depth > options.max_depth.min(MAX_DEPTH) {
        link_conflicts.push(ScanConflict {
            id: format!(
                "scan.depth.{}",
                &sha256_bytes(relative_path(root, current).as_bytes())[..12]
            ),
            path: relative_path(root, current),
            kind: "other".into(),
            severity: "warn".into(),
            details: Some("Maximum scan depth reached; remaining files were not inspected.".into()),
        });
        return Ok(());
    }
    *directories += 1;
    if *directories > MAX_DIRECTORIES {
        link_conflicts.push(ScanConflict {
            id: "scan.directory_limit".into(),
            path: relative_path(root, current),
            kind: "other".into(),
            severity: "warn".into(),
            details: Some(
                "Maximum directory budget reached; remaining entries were not inspected.".into(),
            ),
        });
        return Ok(());
    }
    let entries = match fs::read_dir(current) {
        Ok(entries) => entries,
        Err(error) => {
            link_conflicts.push(ScanConflict {
                id: format!(
                    "scan.unreadable.{}",
                    &sha256_bytes(relative_path(root, current).as_bytes())[..12]
                ),
                path: relative_path(root, current),
                kind: "other".into(),
                severity: "warn".into(),
                details: Some(format!("Directory could not be read: {error}")),
            });
            return Ok(());
        }
    };
    for entry in entries {
        if observations.len() >= options.max_files.min(MAX_FILES) {
            link_conflicts.push(ScanConflict {
                id: "scan.file_limit".into(),
                path: relative_path(root, current),
                kind: "other".into(),
                severity: "warn".into(),
                details: Some(
                    "Scan file limit reached; remaining files were not inspected.".into(),
                ),
            });
            return Ok(());
        }
        if scan_stopped(options, observations.len()) {
            mark_cancelled(
                root,
                link_conflicts,
                progress,
                observations,
                *directories,
                *total_bytes,
            );
            return Ok(());
        }
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                link_conflicts.push(ScanConflict {
                    id: format!(
                        "scan.entry_error.{}",
                        &sha256_bytes(relative_path(root, current).as_bytes())[..12]
                    ),
                    path: relative_path(root, current),
                    kind: "other".into(),
                    severity: "warn".into(),
                    details: Some(format!("Directory entry could not be inspected: {error}")),
                });
                continue;
            }
        };
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        if is_unrelated_scan_directory(&name) || is_unrelated_scan_file(&name) {
            // Managed tooling and the offline wiki are intentionally outside
            // the semantic scan. Inspect only their entry metadata so a link
            // is never followed silently.
            if is_unrelated_scan_directory(&name)
                && fs::symlink_metadata(&path)
                    .ok()
                    .is_some_and(|metadata| is_link_metadata(&metadata))
            {
                link_conflicts.push(ScanConflict {
                    id: format!(
                        "scan.link.{}",
                        &sha256_bytes(relative_path(root, &path).as_bytes())[..12]
                    ),
                    path: relative_path(root, &path),
                    kind: "other".into(),
                    severity: "block".into(),
                    details: Some("Symlink or junction was not followed during scan.".into()),
                });
            }
            continue;
        }
        let relative = relative_path(root, &path);
        if sensitive_scan_path(&relative) {
            link_conflicts.push(ScanConflict {
                id: format!(
                    "scan.sensitive.{}",
                    &sha256_bytes(relative.as_bytes())[..12]
                ),
                path: relative,
                kind: "secret_excluded".into(),
                severity: "warn".into(),
                details: Some(
                    "Credential-shaped project content was excluded from the read-only scan."
                        .into(),
                ),
            });
            continue;
        }
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) => {
                link_conflicts.push(ScanConflict {
                    id: format!(
                        "scan.metadata_error.{}",
                        &sha256_bytes(relative.as_bytes())[..12]
                    ),
                    path: relative,
                    kind: "other".into(),
                    severity: "warn".into(),
                    details: Some(format!("File metadata could not be inspected: {error}")),
                });
                continue;
            }
        };
        if is_link_metadata(&metadata) {
            link_conflicts.push(ScanConflict {
                id: format!("scan.link.{}", &sha256_bytes(relative.as_bytes())[..12]),
                path: relative.clone(),
                kind: "other".into(),
                severity: "block".into(),
                details: Some("Symlink or junction was not followed during scan.".into()),
            });
            observations.push(FileObservation {
                relative,
                bytes: Vec::new(),
                is_symlink: true,
            });
        } else if metadata.is_dir() {
            collect_files(
                root,
                &path,
                depth + 1,
                options,
                observations,
                link_conflicts,
                total_bytes,
                directories,
                progress,
            )?;
        } else if metadata.is_file() {
            let bytes = if metadata.len() > MAX_TEXT_BYTES
                || total_bytes.saturating_add(metadata.len()) > MAX_TOTAL_BYTES
            {
                link_conflicts.push(ScanConflict {
                    id: format!(
                        "scan.bytes_limit.{}",
                        &sha256_bytes(relative.as_bytes())[..12]
                    ),
                    path: relative.clone(),
                    kind: "other".into(),
                    severity: "warn".into(),
                    details: Some(
                        "File was not read because the scan byte budget was reached.".into(),
                    ),
                });
                Vec::new()
            } else {
                match fs::read(&path) {
                    Ok(bytes) => bytes,
                    Err(error) => {
                        link_conflicts.push(ScanConflict {
                            id: format!(
                                "scan.read_error.{}",
                                &sha256_bytes(relative.as_bytes())[..12]
                            ),
                            path: relative.clone(),
                            kind: "other".into(),
                            severity: "warn".into(),
                            details: Some(format!("File could not be read: {error}")),
                        });
                        Vec::new()
                    }
                }
            };
            if !bytes.is_empty() {
                *total_bytes = total_bytes.saturating_add(bytes.len() as u64);
            }
            observations.push(FileObservation {
                relative,
                bytes,
                is_symlink: false,
            });
            emit_progress(
                progress,
                "discovering_files",
                &path,
                root,
                observations,
                *directories,
                *total_bytes,
            );
        }
    }
    Ok(())
}

fn scan_stopped(options: &ScanOptions, files_scanned: usize) -> bool {
    options
        .cancel_flag
        .as_ref()
        .is_some_and(|flag| flag.load(Ordering::SeqCst))
        || options
            .cancel_after_files
            .is_some_and(|limit| files_scanned >= limit)
}

fn mark_cancelled(
    root: &Path,
    conflicts: &mut Vec<ScanConflict>,
    progress: &mut impl FnMut(ScanProgress),
    observations: &[FileObservation],
    directories: usize,
    bytes_read: u64,
) {
    if !conflicts
        .iter()
        .any(|conflict| conflict.id == "scan.cancelled")
    {
        conflicts.push(ScanConflict {
            id: "scan.cancelled".into(),
            path: relative_path(root, root),
            kind: "other".into(),
            severity: "info".into(),
            details: Some("Scan cancelled by the caller before project mutation.".into()),
        });
    }
    emit_progress(
        progress,
        "cancelled",
        root,
        root,
        observations,
        directories,
        bytes_read,
    );
}

fn emit_progress(
    progress: &mut impl FnMut(ScanProgress),
    stage: &str,
    current: &Path,
    root: &Path,
    observations: &[FileObservation],
    directories: usize,
    bytes_read: u64,
) {
    progress(ScanProgress {
        stage: stage.into(),
        current_path: if current == root {
            ".".into()
        } else {
            relative_path(root, current)
        },
        files_scanned: observations.len() as u32,
        directories_scanned: directories as u32,
        bytes_read,
    });
}

fn sensitive_scan_path(relative: &str) -> bool {
    let normalized = relative.replace('\\', "/").to_ascii_lowercase();
    let segments = normalized.split('/').collect::<Vec<_>>();
    if segments.iter().any(|segment| {
        matches!(
            *segment,
            ".git"
                | "auth"
                | "credential"
                | "credentials"
                | "secret"
                | "secrets"
                | "token"
                | "tokens"
        )
    }) {
        return true;
    }
    let name = segments.last().copied().unwrap_or_default();
    name == ".env"
        || name.starts_with(".env.")
        || name.ends_with(".pem")
        || name.ends_with(".key")
        || name.ends_with(".p12")
        || name.ends_with(".pfx")
}

fn is_unrelated_scan_directory(name: &str) -> bool {
    IGNORED_SCAN_DIRECTORIES
        .iter()
        .any(|candidate| name.eq_ignore_ascii_case(candidate))
}

fn is_unrelated_scan_file(name: &str) -> bool {
    let normalized = name.to_ascii_lowercase();
    IGNORED_SCAN_FILES
        .iter()
        .any(|candidate| normalized == *candidate)
        || [".pyc", ".pyo", ".log", ".tmp", ".bak", ".swp"]
            .iter()
            .any(|suffix| normalized.ends_with(suffix))
}

fn detect_descriptors(
    root: &Path,
    descriptor: Option<&FileObservation>,
    approved_external: Option<&Path>,
    findings: &mut Vec<ScanFinding>,
    conflicts: &mut Vec<ScanConflict>,
) {
    if let Some(descriptor) = descriptor.filter(|file| !file.is_symlink) {
        let parsed = crate::descriptors::parse_descriptor(&descriptor.bytes);
        if let Ok(parsed) = parsed {
            if let Some(name) = parsed.fields.get("name") {
                findings.push(finding(
                    "descriptor.name",
                    "descriptor",
                    "mod_name",
                    json!(name),
                    "accepted",
                    evidence(
                        "descriptor_parser",
                        "descriptor.mod",
                        1.0,
                        Some("Matched the project descriptor name field."),
                    ),
                    None,
                ));
            } else {
                conflicts.push(ScanConflict {
                    id: "conflict.descriptor.malformed".into(),
                    path: "descriptor.mod".into(),
                    kind: "descriptor_mismatch".into(),
                    severity: "block".into(),
                    details: Some("descriptor.mod has no name field.".into()),
                });
            }
        } else {
            conflicts.push(ScanConflict {
                id: "conflict.descriptor.malformed".into(),
                path: "descriptor.mod".into(),
                kind: "descriptor_mismatch".into(),
                severity: "block".into(),
                details: Some(
                    "descriptor.mod exists but its name field could not be parsed.".into(),
                ),
            });
        }
    } else {
        findings.push(finding(
            "descriptor.project.missing",
            "descriptor",
            "project_descriptor",
            json!(false),
            "needs_review",
            evidence(
                "descriptor_discovery",
                "descriptor.mod",
                1.0,
                Some("Project descriptor was not found."),
            ),
            Some("Generate descriptor.mod during the reviewed installation."),
        ));
    }

    if let Some(external) = approved_external {
        let external_value = external.display().to_string();
        let metadata = fs::symlink_metadata(external).ok();
        let is_link = metadata.as_ref().is_some_and(is_link_metadata);
        let exists = metadata.as_ref().is_some_and(|metadata| metadata.is_file());
        findings.push(finding(
            "descriptor.launcher",
            "descriptor",
            "launcher_descriptor",
            json!({"path": external_value, "exists": exists}),
            if exists { "accepted" } else { "needs_review" },
            evidence(
                "approved_external_descriptor",
                &external.display().to_string(),
                if exists { 1.0 } else { 0.5 },
                None,
            ),
            None,
        ));
        if exists && !is_link {
            match fs::read(external).ok().and_then(|bytes| {
                crate::descriptors::parse_descriptor(&bytes).ok()
            }) {
                Some(parsed) if parsed.fields.contains_key("name") && parsed.fields.contains_key("path") => {}
                _ => conflicts.push(ScanConflict {
                    id: "conflict.launcher.malformed".into(),
                    path: external_value.clone(),
                    kind: "descriptor_mismatch".into(),
                    severity: "block".into(),
                    details: Some("Approved launcher descriptor is not a valid descriptor with name and path fields.".into()),
                }),
            }
        }
        if is_link {
            conflicts.push(ScanConflict {
                id: "conflict.launcher.symlink".into(),
                path: external_value.clone(),
                kind: "descriptor_mismatch".into(),
                severity: "block".into(),
                details: Some(
                    "Approved launcher descriptor is a link and was not followed.".into(),
                ),
            });
        }
        if exists
            && !crate::security::is_within(root, external)
            && path_has_link_component(external)
        {
            conflicts.push(ScanConflict {
                id: "conflict.launcher.link".into(),
                path: external.display().to_string(),
                kind: "descriptor_mismatch".into(),
                severity: "block".into(),
                details: Some("Launcher descriptor path contains a link component.".into()),
            });
        }
    }
}

fn detect_structure(observations: &[FileObservation], findings: &mut Vec<ScanFinding>) {
    let mut top_level = std::collections::BTreeSet::new();
    for file in observations {
        if let Some(first) = file.relative.split('/').next() {
            top_level.insert(first.to_string());
        }
    }
    findings.push(finding(
        "structure.top_level",
        "structure",
        "top_level_entries",
        json!(top_level),
        "accepted",
        evidence(
            "bounded_tree_scan",
            ".",
            1.0,
            Some("Top-level project entries observed without following links."),
        ),
        None,
    ));
}

fn detect_thumbnail(
    thumbnail: Option<&FileObservation>,
    findings: &mut Vec<ScanFinding>,
    conflicts: &mut Vec<ScanConflict>,
) {
    let Some(thumbnail) = thumbnail.filter(|file| !file.is_symlink) else {
        findings.push(finding(
            "thumbnail.missing",
            "thumbnail",
            "thumbnail",
            json!(null),
            "needs_review",
            evidence(
                "thumbnail_discovery",
                "thumbnail.png",
                1.0,
                Some("The conventional project thumbnail was not found."),
            ),
            Some("Choose a valid thumbnail or install the managed placeholder during review."),
        ));
        return;
    };
    match crate::descriptors::validate_thumbnail_png(&thumbnail.bytes) {
        Ok((width, height)) => {
            let hash = sha256_bytes(&thumbnail.bytes);
            let managed_placeholder = crate::descriptors::placeholder_thumbnail_png()
                .map(|placeholder| hash == sha256_bytes(&placeholder))
                .unwrap_or(false);
            findings.push(finding(
                "thumbnail.integrity",
                "thumbnail",
                "thumbnail",
                json!({"path": "thumbnail.png", "width": width, "height": height, "sha256": hash, "managed_placeholder": managed_placeholder}),
                "accepted",
                evidence(
                    "png_decoder",
                    "thumbnail.png",
                    1.0,
                    Some(if managed_placeholder {
                        "PNG decodes and matches the managed placeholder hash."
                    } else {
                        "PNG decodes; the file is treated as user artwork."
                    }),
                ),
                None,
            ));
        }
        Err(error) => {
            findings.push(finding(
                "thumbnail.invalid",
                "thumbnail",
                "thumbnail",
                json!({"path": "thumbnail.png"}),
                "blocking",
                evidence(
                    "png_decoder",
                    "thumbnail.png",
                    1.0,
                    Some("The thumbnail could not be decoded as a bounded PNG."),
                ),
                Some("Replace the invalid thumbnail after reviewing the binary conflict."),
            ));
            conflicts.push(ScanConflict {
                id: "conflict.thumbnail.invalid".into(),
                path: "thumbnail.png".into(),
                kind: "binary_mismatch".into(),
                severity: "block".into(),
                details: Some(error.to_string()),
            });
        }
    }
}

fn detect_git(
    root: &Path,
    observations: &[FileObservation],
    findings: &mut Vec<ScanFinding>,
    conflicts: &mut Vec<ScanConflict>,
) {
    let git = root.join(".git");
    let git_metadata = fs::symlink_metadata(&git).ok();
    let git_is_link = git_metadata.as_ref().is_some_and(is_link_metadata);
    let is_git = git_metadata
        .as_ref()
        .is_some_and(|metadata| metadata.is_dir() || metadata.is_file());
    let head_path = git.join("HEAD");
    let head_is_link = fs::symlink_metadata(&head_path)
        .ok()
        .is_some_and(|metadata| is_link_metadata(&metadata));
    let head = if git_is_link {
        None
    } else if git_metadata
        .as_ref()
        .is_some_and(|metadata| metadata.is_dir())
    {
        if head_is_link {
            None
        } else {
            fs::read_to_string(head_path)
                .ok()
                .map(|value| value.trim().to_string())
        }
    } else {
        None
    };
    let inspection = crate::git::inspect_read_only(root);
    let mut ignore_files = inspection.ignore_files.clone();
    ignore_files.extend(
        observations
            .iter()
            .filter(|file| {
                !file.is_symlink
                    && (file.relative == ".gitignore" || file.relative.ends_with("/.gitignore"))
            })
            .map(|file| file.relative.clone()),
    );
    ignore_files.sort();
    ignore_files.dedup();
    let status = &inspection.status;
    let status_value = json!({
        "present": is_git,
        "head": head,
        "branch": status.branch,
        "detached": status.detached,
        "commit": inspection.commit,
        "dirty": status.dirty,
        "staged_files": inspection.staged_files,
        "unstaged_files": inspection.unstaged_files,
        "untracked_files": inspection.untracked_files,
        "remotes": status.remotes,
        "submodules": inspection.submodules,
        "hooks": inspection.hooks,
        "ignore_files": ignore_files,
        "tracked_secret_like_paths": status.tracked_secret_like_paths,
        "tracked_path_scan_complete": inspection.tracked_path_scan_complete,
        "status_probe": inspection.status_probe,
    });
    let finding_status = if !is_git || inspection.status_probe == "complete" {
        "accepted"
    } else {
        "needs_review"
    };
    findings.push(finding(
        "git.repository",
        "git",
        "repository",
        status_value,
        finding_status,
        evidence(
            "git_metadata",
            ".git/HEAD",
            if !is_git {
                0.8
            } else if inspection.status_probe == "complete" {
                1.0
            } else {
                0.7
            },
            Some("Git metadata and bounded read-only status evidence were collected without changing the repository."),
        ),
        (finding_status == "accepted")
            .then_some("Git state is available for review and maintenance planning."),
    ));
    if is_git
        && !matches!(
            inspection.status_probe.as_str(),
            "complete" | "linked_worktree_not_followed"
        )
    {
        conflicts.push(ScanConflict {
            id: "conflict.git.inspection".into(),
            path: ".git".into(),
            kind: "other".into(),
            severity: "warn".into(),
            details: Some(
                "Git metadata exists, but bounded status evidence was incomplete; review before maintenance."
                    .into(),
            ),
        });
    }
    if git_is_link {
        conflicts.push(ScanConflict {
            id: "conflict.git.link".into(),
            path: ".git".into(),
            kind: "other".into(),
            severity: "block".into(),
            details: Some("The .git entry is a link; Git metadata was not followed.".into()),
        });
    } else if head_is_link {
        conflicts.push(ScanConflict {
            id: "conflict.git.head_link".into(),
            path: ".git/HEAD".into(),
            kind: "other".into(),
            severity: "block".into(),
            details: Some("Git HEAD is a link; metadata was not followed.".into()),
        });
    } else if git_metadata
        .as_ref()
        .is_some_and(|metadata| metadata.is_file())
    {
        conflicts.push(ScanConflict {
            id: "conflict.git.worktree".into(),
            path: ".git".into(),
            kind: "other".into(),
            severity: "warn".into(),
            details: Some(
                "The project uses a linked Git worktree; Git actions need explicit review.".into(),
            ),
        });
    }
}

fn detect_identifiers(observations: &[FileObservation], findings: &mut Vec<ScanFinding>) {
    let identifier = Regex::new(r"\b([a-z][a-z0-9]{1,15})_[a-z0-9][a-z0-9_]*\b")
        .expect("static identifier regex");
    let mut counts: HashMap<String, u32> = HashMap::new();
    let mut paths: HashMap<String, String> = HashMap::new();
    for file in observations
        .iter()
        .filter(|file| !file.is_symlink && !file.bytes.is_empty())
    {
        let text = String::from_utf8_lossy(&file.bytes);
        for capture in identifier.captures_iter(&text) {
            let prefix = capture[1].to_string();
            *counts.entry(prefix.clone()).or_default() += 1;
            paths.entry(prefix).or_insert_with(|| file.relative.clone());
        }
    }
    if let Some((prefix, count)) = counts.iter().max_by_key(|(_, count)| *count) {
        let confidence = (*count as f32 / counts.values().sum::<u32>() as f32).clamp(0.5, 0.99);
        let path = paths
            .get(prefix)
            .cloned()
            .unwrap_or_else(|| "project files".into());
        findings.push(finding(
            "namespace.primary",
            "namespace",
            "primary_namespace",
            json!(prefix),
            if confidence >= 0.9 {
                "accepted"
            } else {
                "needs_review"
            },
            evidence(
                "identifier_frequency",
                &path,
                confidence,
                Some(&format!(
                    "{:.0}% of observed identifiers use {prefix}",
                    confidence * 100.0
                )),
            ),
            Some("Confirm the namespace before adapting project instructions."),
        ));
    } else {
        findings.push(finding(
            "namespace.primary.missing",
            "namespace",
            "primary_namespace",
            json!(null),
            "needs_review",
            evidence(
                "identifier_frequency",
                ".",
                0.4,
                Some("No repeated namespaced identifiers were found."),
            ),
            Some("Enter a project namespace during review."),
        ));
    }
}

fn detect_localisation(
    observations: &[FileObservation],
    findings: &mut Vec<ScanFinding>,
    conflicts: &mut Vec<ScanConflict>,
) {
    let localisation: Vec<&FileObservation> = observations
        .iter()
        .filter(|file| {
            let lower = file.relative.to_ascii_lowercase();
            lower.contains("localisation/") || lower.contains("localization/")
        })
        .filter(|file| file.relative.ends_with(".yml") || file.relative.ends_with(".yaml"))
        .collect();
    if localisation.is_empty() {
        findings.push(finding(
            "localisation.missing",
            "localisation",
            "files",
            json!(0),
            "needs_review",
            evidence(
                "localisation_detector",
                "localisation/",
                0.9,
                Some("No localisation files were found under the conventional folders."),
            ),
            Some("Add localisation folders only when the selected mod profile needs them."),
        ));
        return;
    }
    let bom_count = localisation
        .iter()
        .filter(|file| file.bytes.starts_with(&[0xEF, 0xBB, 0xBF]))
        .count();
    let prefixes =
        Regex::new(r"(?m)^\s*([A-Za-z0-9][A-Za-z0-9_.-]*)_").expect("static localisation regex");
    let mut prefix_counts: HashMap<String, u32> = HashMap::new();
    for file in &localisation {
        let text = String::from_utf8_lossy(&file.bytes);
        for capture in prefixes.captures_iter(&text) {
            *prefix_counts.entry(capture[1].to_string()).or_default() += 1;
        }
    }
    let mixed = bom_count > 0 && bom_count < localisation.len();
    if mixed {
        conflicts.push(ScanConflict {
            id: "conflict.localisation.encoding".into(),
            path: "localisation/".into(),
            kind: "encoding_mixed".into(),
            severity: "warn".into(),
            details: Some(format!(
                "{bom_count} of {} localisation files have a UTF-8 BOM.",
                localisation.len()
            )),
        });
    }
    let prefix = prefix_counts
        .into_iter()
        .max_by_key(|(_, count)| *count)
        .map(|(prefix, _)| prefix);
    findings.push(finding(
        "localisation.encoding",
        "localisation",
        "encoding_policy",
        json!({"files": localisation.len(), "utf8_bom": bom_count, "prefix": prefix}),
        if mixed { "needs_review" } else { "accepted" },
        evidence(
            "bom_scanner",
            "localisation/",
            if mixed { 0.85 } else { 1.0 },
            Some("Encoding is reported; files are not normalized during scan."),
        ),
        Some("Preserve existing files and review encoding before content changes."),
    ));
}

fn detect_documentation(observations: &[FileObservation], findings: &mut Vec<ScanFinding>) {
    let docs = observations
        .iter()
        .filter(|file| {
            let lower = file.relative.to_ascii_lowercase();
            lower.starts_with("docs/") || lower == "readme.md" || lower == "agents.md"
        })
        .count();
    findings.push(finding(
        "documentation.inventory",
        "documentation",
        "documentation_files",
        json!(docs),
        "accepted",
        evidence(
            "documentation_detector",
            "docs/",
            0.95,
            Some("Documentation count is bounded to the selected project root."),
        ),
        None,
    ));
}

fn detect_agentic_files(
    observations: &[FileObservation],
    findings: &mut Vec<ScanFinding>,
    conflicts: &mut Vec<ScanConflict>,
) {
    let skill_files: Vec<&FileObservation> = observations
        .iter()
        .filter(|file| {
            file.relative.starts_with(".agents/skills/") && file.relative.ends_with("SKILL.md")
        })
        .collect();
    let malformed_skills = skill_files
        .iter()
        .filter(|file| {
            let text = String::from_utf8_lossy(&file.bytes);
            !(text.starts_with("---\n")
                && text.contains("\nname:")
                && text.contains("\ndescription:"))
        })
        .map(|file| file.relative.clone())
        .collect::<Vec<_>>();
    let subagent_files: Vec<&FileObservation> = observations
        .iter()
        .filter(|file| {
            file.relative.starts_with(".codex/agents/") && file.relative.ends_with(".toml")
        })
        .collect();
    let malformed_subagents = subagent_files
        .iter()
        .filter(|file| {
            let text = String::from_utf8_lossy(&file.bytes);
            let Ok(value) = text.parse::<toml::Value>() else {
                return true;
            };
            match value.get("fork_context").and_then(toml::Value::as_bool) {
                Some(value) => value,
                None => !text.contains("fork_context=false"),
            }
        })
        .map(|file| file.relative.clone())
        .collect::<Vec<_>>();
    let codex_observation = observations
        .iter()
        .find(|file| file.relative == ".codex/config.toml");
    let codex_config = codex_observation.is_some();
    let codex_parse = codex_observation.and_then(|file| {
        String::from_utf8(file.bytes.clone())
            .ok()
            .and_then(|text| text.parse::<toml::Value>().ok())
    });
    let mcp_servers = codex_parse
        .as_ref()
        .and_then(|value| value.get("mcp_servers"))
        .and_then(toml::Value::as_table)
        .map(|table| table.keys().cloned().collect::<Vec<_>>())
        .unwrap_or_default();
    findings.push(finding(
        "codex.agents",
        "codex",
        "project_instructions",
        json!(observations.iter().any(|file| file.relative == "AGENTS.md")),
        if observations.iter().any(|file| file.relative == "AGENTS.md") {
            "accepted"
        } else {
            "needs_review"
        },
        evidence(
            "agentic_file_detector",
            "AGENTS.md",
            1.0,
            Some("Project instructions are detected without replacing them."),
        ),
        None,
    ));
    findings.push(finding(
        "skill.inventory",
        "skill",
        "skill_count",
        json!({"count": skill_files.len(), "malformed": malformed_skills.clone()}),
        if malformed_skills.is_empty() {
            "accepted"
        } else {
            "needs_review"
        },
        evidence("agentic_file_detector", ".agents/skills/", 1.0, None),
        None,
    ));
    for path in malformed_skills {
        conflicts.push(ScanConflict {
            id: format!("conflict.skill.{}", &sha256_bytes(path.as_bytes())[..12]),
            path,
            kind: "other".into(),
            severity: "warn".into(),
            details: Some(
                "Skill frontmatter is incomplete or unreadable; it was not repaired during scan."
                    .into(),
            ),
        });
    }
    findings.push(finding(
        "subagent.inventory",
        "subagent",
        "subagent_count",
        json!({"count": subagent_files.len(), "malformed": malformed_subagents.clone()}),
        if malformed_subagents.is_empty() {
            "accepted"
        } else {
            "needs_review"
        },
        evidence("agentic_file_detector", ".codex/agents/", 1.0, None),
        None,
    ));
    for path in malformed_subagents {
        conflicts.push(ScanConflict {
            id: format!("conflict.subagent.{}", &sha256_bytes(path.as_bytes())[..12]),
            path,
            kind: "other".into(),
            severity: "block".into(),
            details: Some(
                "Subagent TOML is invalid or does not explicitly require fork_context=false."
                    .into(),
            ),
        });
    }
    findings.push(finding(
        "codex.config",
        "codex",
        "config_present",
        json!(codex_config),
        if codex_config && codex_parse.is_some() {
            "accepted"
        } else {
            "needs_review"
        },
        evidence(
            "toml_path_detector",
            ".codex/config.toml",
            1.0,
            Some("Configuration will be merged structurally if selected."),
        ),
        None,
    ));
    if codex_config && codex_parse.is_none() {
        conflicts.push(ScanConflict {
            id: "conflict.codex.config.parse".into(),
            path: ".codex/config.toml".into(),
            kind: "other".into(),
            severity: "block".into(),
            details: Some(
                "Codex configuration is not valid TOML and was not changed during scan.".into(),
            ),
        });
    }
    findings.push(finding(
        "mcp.inventory",
        "mcp",
        "server_ids",
        json!(mcp_servers),
        "accepted",
        evidence(
            "toml_parser",
            ".codex/config.toml",
            if codex_parse.is_some() { 1.0 } else { 0.5 },
            Some("MCP server IDs were read structurally without executing commands."),
        ),
        None,
    ));
    if observations.iter().any(|file| file.relative == "AGENTS.md") {
        conflicts.push(ScanConflict {
            id: "conflict.agents".into(),
            path: "AGENTS.md".into(),
            kind: "managed_file_exists".into(),
            severity: "warn".into(),
            details: Some("Existing project instructions require a three-way merge or explicit keep decision.".into()),
        });
    }
    if codex_config {
        conflicts.push(ScanConflict {
            id: "conflict.codex.config".into(),
            path: ".codex/config.toml".into(),
            kind: "managed_file_exists".into(),
            severity: "warn".into(),
            details: Some("Existing Codex configuration requires a structured merge.".into()),
        });
    }
}

fn detect_absolute_paths(observations: &[FileObservation], findings: &mut Vec<ScanFinding>) {
    let absolute =
        Regex::new(r"(?i)(?:[a-z]:\\|/users/|/home/|/mnt/)").expect("static absolute path regex");
    let matches: Vec<String> = observations
        .iter()
        .filter(|file| !file.is_symlink && !file.bytes.is_empty())
        .filter_map(|file| {
            let text = String::from_utf8_lossy(&file.bytes);
            absolute.is_match(&text).then_some(file.relative.clone())
        })
        .take(20)
        .collect();
    findings.push(finding(
        "paths.absolute",
        "codex",
        "absolute_path_files",
        json!(matches),
        if matches.is_empty() {
            "accepted"
        } else {
            "needs_review"
        },
        evidence(
            "absolute_path_detector",
            ".",
            if matches.is_empty() { 1.0 } else { 0.9 },
            Some("Only paths are reported; matching content is not copied into evidence."),
        ),
        Some("Review machine-local paths before adapting instructions or configuration."),
    ));
}

fn detect_existing_components(
    root: &Path,
    observations: &[FileObservation],
    conflicts: &mut Vec<ScanConflict>,
) {
    for path in ["paradox_wiki", ".agents/skills", ".codex/agents"] {
        let present = if path == "paradox_wiki" {
            fs::symlink_metadata(root.join(path))
                .ok()
                .is_some_and(|metadata| {
                    if is_link_metadata(&metadata) {
                        conflicts.push(ScanConflict {
                            id: "conflict.paradox_wiki.link".into(),
                            path: path.into(),
                            kind: "other".into(),
                            severity: "block".into(),
                            details: Some(
                                "The offline wiki directory is a link and was not followed.".into(),
                            ),
                        });
                    }
                    metadata.is_dir() || metadata.is_file()
                })
        } else {
            observations
                .iter()
                .any(|file| file.relative == path || file.relative.starts_with(&format!("{path}/")))
        };
        if present {
            conflicts.push(ScanConflict {
                id: format!("conflict.{}", path.replace('/', ".")),
                path: path.into(),
                kind: "managed_file_exists".into(),
                severity: "warn".into(),
                details: Some(
                    "Existing managed content will be compared by hash before update.".into(),
                ),
            });
        }
    }
}

/// The managed lock is intentionally outside the bounded file walk because
/// the scanner skips the app metadata directory. Reading this one fixed,
/// regular file lets an existing-project scan recognize a prior setup without
/// exposing the rest of the metadata directory to semantic analysis.
fn detect_managed_installation(
    root: &Path,
    findings: &mut Vec<ScanFinding>,
    conflicts: &mut Vec<ScanConflict>,
) {
    const LOCK_RELATIVE: &str = ".hoi4-mod-setup/install.lock.json";
    let lock_path = root.join(LOCK_RELATIVE);
    if path_has_link_component(&lock_path) {
        findings.push(finding(
            "installation.managed",
            "installation",
            "managed_setup",
            json!({"present": true, "valid": false, "state": "unsafe_path"}),
            "blocking",
            evidence(
                "managed_installation_detector",
                LOCK_RELATIVE,
                1.0,
                Some("The managed lock path contains a symlink or junction and was not read."),
            ),
            Some("Choose a project whose setup metadata is a regular, contained path."),
        ));
        conflicts.push(ScanConflict {
            id: "conflict.installation.lock_path".into(),
            path: LOCK_RELATIVE.into(),
            kind: "other".into(),
            severity: "block".into(),
            details: Some("The existing setup lock path contains a symlink or junction.".into()),
        });
        return;
    }
    let metadata = match fs::symlink_metadata(&lock_path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            findings.push(finding(
                "installation.managed",
                "installation",
                "managed_setup",
                json!({"present": false}),
                "accepted",
                evidence(
                    "managed_installation_detector",
                    LOCK_RELATIVE,
                    1.0,
                    Some("No HOI4 Mod Setup installation lock was found."),
                ),
                None,
            ));
            return;
        }
        Err(_) => {
            findings.push(finding(
                "installation.managed",
                "installation",
                "managed_setup",
                json!({"present": true, "valid": false, "state": "unreadable"}),
                "blocking",
                evidence(
                    "managed_installation_detector",
                    LOCK_RELATIVE,
                    0.9,
                    Some("The existing setup lock could not be inspected."),
                ),
                Some("Review the setup metadata before attempting repair."),
            ));
            return;
        }
    };
    if is_link_metadata(&metadata) || !metadata.is_file() {
        findings.push(finding(
            "installation.managed",
            "installation",
            "managed_setup",
            json!({"present": true, "valid": false, "state": "not_a_regular_file"}),
            "blocking",
            evidence(
                "managed_installation_detector",
                LOCK_RELATIVE,
                1.0,
                Some("The existing setup lock is not a regular file."),
            ),
            Some("Review the setup metadata before attempting repair."),
        ));
        conflicts.push(ScanConflict {
            id: "conflict.installation.lock_file".into(),
            path: LOCK_RELATIVE.into(),
            kind: "other".into(),
            severity: "block".into(),
            details: Some("The existing setup lock is not a regular file.".into()),
        });
        return;
    }
    if metadata.len() > MAX_TEXT_BYTES {
        findings.push(finding(
            "installation.managed",
            "installation",
            "managed_setup",
            json!({"present": true, "valid": false, "state": "too_large"}),
            "blocking",
            evidence(
                "managed_installation_detector",
                LOCK_RELATIVE,
                1.0,
                Some("The existing setup lock exceeds the bounded scan size."),
            ),
            Some("Review the setup metadata before attempting repair."),
        ));
        return;
    }
    let bytes = match fs::read(&lock_path) {
        Ok(bytes) => bytes,
        Err(_) => {
            findings.push(finding(
                "installation.managed",
                "installation",
                "managed_setup",
                json!({"present": true, "valid": false, "state": "unreadable"}),
                "blocking",
                evidence(
                    "managed_installation_detector",
                    LOCK_RELATIVE,
                    0.9,
                    Some("The existing setup lock could not be read."),
                ),
                Some("Review the setup metadata before attempting repair."),
            ));
            return;
        }
    };
    let lock = serde_json::from_slice::<Value>(&bytes)
        .ok()
        .and_then(|value| crate::migrations::migrate_lock(value).ok());
    let Some(lock) = lock else {
        findings.push(finding(
            "installation.managed",
            "installation",
            "managed_setup",
            json!({"present": true, "valid": false, "state": "invalid"}),
            "blocking",
            evidence(
                "managed_installation_detector",
                LOCK_RELATIVE,
                1.0,
                Some("The existing setup lock is not a valid installation record."),
            ),
            Some("Review or remove the invalid setup metadata before continuing."),
        ));
        conflicts.push(ScanConflict {
            id: "conflict.installation.lock_invalid".into(),
            path: LOCK_RELATIVE.into(),
            kind: "other".into(),
            severity: "block".into(),
            details: Some("The existing setup lock could not be validated.".into()),
        });
        return;
    };
    let mut component_ids = lock
        .components
        .iter()
        .map(|component| component.id.clone())
        .collect::<Vec<_>>();
    component_ids.sort();
    let workflow_3d_state = lock
        .optional_workflows
        .get("workflow.3d")
        .map(|workflow| workflow.state.as_str())
        .unwrap_or("not_selected");
    let workflow_3d_key_configured = lock
        .optional_workflows
        .get("workflow.3d")
        .is_some_and(|workflow| workflow.credential_reference.is_some());
    let workflow_super_events_state = lock
        .optional_workflows
        .get("workflow.super_events")
        .map(|workflow| workflow.state.as_str())
        .unwrap_or("not_selected");
    let portrait_pipeline = lock.portrait_pipeline.as_ref();
    let portrait_provider = portrait_pipeline
        .map(|portrait| portrait.provider.as_str())
        .unwrap_or("disabled");
    let portrait_enabled = portrait_pipeline
        .is_some_and(|portrait| portrait.enabled && portrait.provider != "disabled");
    let portrait_provider_status = portrait_pipeline
        .map(|portrait| portrait.provider_status.as_str())
        .unwrap_or("not_selected");
    findings.push(finding(
        "installation.managed",
        "installation",
        "managed_setup",
        json!({
            "present": true,
            "valid": true,
            "project_id": lock.project_id,
            "component_ids": component_ids,
            "workflow_3d_state": workflow_3d_state,
            "workflow_3d_key_configured": workflow_3d_key_configured,
            "workflow_super_events_state": workflow_super_events_state,
            "portrait_enabled": portrait_enabled,
            "portrait_provider": portrait_provider,
            "portrait_provider_status": portrait_provider_status,
            "portrait_workflow_commit": portrait_pipeline.map(|portrait| portrait.workflow_commit.clone()),
            "portrait_preferred_workflow": portrait_pipeline.map(|portrait| portrait.preferred_workflow.clone()),
            "portrait_mcp_registered": portrait_pipeline.is_some_and(|portrait| portrait.mcp_registered),
            "portrait_local_root": portrait_pipeline.map(|portrait| portrait.local_comfyui_root.clone()),
            "portrait_local_server_url": portrait_pipeline.map(|portrait| portrait.local_server_url.clone()),
            "portrait_runpod_url": portrait_pipeline.map(|portrait| portrait.runpod_url.clone()),
            "portrait_runpod_workspace": portrait_pipeline.map(|portrait| portrait.runpod_workspace.clone()),
        }),
        "accepted",
        evidence(
            "managed_installation_detector",
            LOCK_RELATIVE,
            1.0,
            Some("A valid managed setup was found; repair and optional workflow changes are available."),
        ),
        Some("Use the installed setup actions to repair files or add an optional workflow."),
    ));
}

fn relative_path(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

fn evidence(detector: &str, path: &str, confidence: f32, note: Option<&str>) -> ScanEvidence {
    ScanEvidence {
        detector: detector.into(),
        path: path.into(),
        line_start: None,
        line_end: None,
        excerpt_sha256: None,
        confidence,
        note: note.map(str::to_string),
    }
}

fn finding(
    id: &str,
    category: &str,
    key: &str,
    value: Value,
    status: &str,
    mut evidence: ScanEvidence,
    recommendation: Option<&str>,
) -> ScanFinding {
    if evidence.excerpt_sha256.is_none() {
        evidence.excerpt_sha256 = serde_json::to_vec(&value)
            .ok()
            .map(|bytes| sha256_bytes(&bytes));
    }
    ScanFinding {
        id: id.into(),
        category: category.into(),
        key: key.into(),
        value,
        status: status.into(),
        origin: "deterministic".into(),
        user_value: None,
        evidence: vec![evidence],
        recommendation: recommendation.map(str::to_string),
    }
}

fn summarize(findings: &[ScanFinding], conflicts: &[ScanConflict]) -> ScanSummary {
    let mut summary = ScanSummary::default();
    for finding in findings {
        match finding.status.as_str() {
            "accepted" | "edited" => summary.accepted += 1,
            "blocking" => summary.blocking += 1,
            _ => summary.needs_review += 1,
        }
    }
    summary.blocking += conflicts
        .iter()
        .filter(|conflict| conflict.severity == "block")
        .count() as u32;
    summary.warnings = conflicts
        .iter()
        .filter(|conflict| conflict.severity == "warn")
        .count() as u32;
    summary
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::{atomic::AtomicBool, Arc};
    use tempfile::tempdir;

    #[test]
    fn launcher_discovery_prefers_the_descriptor_named_for_the_project_folder() {
        let parent = tempdir().unwrap();
        let project = parent.path().join("example");
        fs::create_dir(&project).unwrap();
        let project_path = project.display().to_string().replace('\\', "\\\\");
        let descriptor = format!("name=\"Example\"\npath=\"{project_path}\"\n");
        fs::write(parent.path().join("other.mod"), &descriptor).unwrap();
        fs::write(parent.path().join("example.mod"), &descriptor).unwrap();

        let discovered = discover_launcher_descriptor(&project).unwrap().unwrap();

        assert_eq!(
            discovered.canonicalize().unwrap(),
            parent.path().join("example.mod").canonicalize().unwrap()
        );
    }

    #[test]
    fn launcher_discovery_returns_none_for_ambiguous_noncanonical_names() {
        let parent = tempdir().unwrap();
        let project = parent.path().join("example");
        fs::create_dir(&project).unwrap();
        let project_path = project.display().to_string().replace('\\', "\\\\");
        let descriptor = format!("name=\"Example\"\npath=\"{project_path}\"\n");
        fs::write(parent.path().join("first.mod"), &descriptor).unwrap();
        fs::write(parent.path().join("second.mod"), &descriptor).unwrap();

        assert!(discover_launcher_descriptor(&project).unwrap().is_none());
    }

    #[test]
    fn scan_is_read_only_and_detects_core_surfaces() {
        let directory = tempdir().unwrap();
        fs::write(
            directory.path().join("descriptor.mod"),
            "name=\"Example\"\n",
        )
        .unwrap();
        fs::create_dir_all(directory.path().join("events")).unwrap();
        fs::write(
            directory.path().join("events/example.txt"),
            "namespace_example = yes\n",
        )
        .unwrap();
        let before = fs::read_dir(directory.path()).unwrap().count();
        let result = scan_project(directory.path(), &ScanOptions::default()).unwrap();
        let after = fs::read_dir(directory.path()).unwrap().count();
        assert!(result.read_only);
        assert_eq!(before, after);
        assert!(result
            .findings
            .iter()
            .any(|finding| finding.id == "descriptor.name"));
        assert!(result
            .findings
            .iter()
            .any(|finding| finding.id == "namespace.primary"));
        assert!(result
            .findings
            .iter()
            .flat_map(|finding| &finding.evidence)
            .all(|evidence| evidence.excerpt_sha256.is_some()));
        assert!(!result.partial);
    }

    #[test]
    fn malformed_descriptor_is_reported_as_blocking_conflict() {
        let directory = tempdir().unwrap();
        fs::write(
            directory.path().join("descriptor.mod"),
            "version = \"0.1\"\n",
        )
        .unwrap();
        let result = scan_project(directory.path(), &ScanOptions::default()).unwrap();
        assert!(result
            .conflicts
            .iter()
            .any(|conflict| conflict.severity == "block"));
    }

    #[test]
    fn credential_shaped_files_are_excluded_from_scan_content() {
        let directory = tempdir().unwrap();
        fs::write(
            directory.path().join(".env"),
            "MESHY_API_KEY=secret-value\n",
        )
        .unwrap();
        fs::create_dir_all(directory.path().join("credentials")).unwrap();
        fs::write(
            directory.path().join("credentials/token.txt"),
            "secret-token-value",
        )
        .unwrap();
        let result = scan_project(directory.path(), &ScanOptions::default()).unwrap();
        assert!(result
            .conflicts
            .iter()
            .filter(|conflict| conflict.kind == "secret_excluded")
            .all(|conflict| !conflict
                .details
                .as_deref()
                .unwrap_or_default()
                .contains("secret-value")));
        assert!(result
            .conflicts
            .iter()
            .any(|conflict| conflict.path == ".env"));
        assert!(!result
            .findings
            .iter()
            .any(|finding| finding.value.to_string().contains("secret-value")));
    }

    #[test]
    fn unrelated_tooling_directories_and_artifacts_are_not_scanned() {
        let directory = tempdir().unwrap();
        fs::write(
            directory.path().join("descriptor.mod"),
            "name=\"Example\"\n",
        )
        .unwrap();
        fs::write(
            directory.path().join("events.txt"),
            "namespace_example = yes\n",
        )
        .unwrap();
        for name in [
            ".venv",
            "venv",
            "__pycache__",
            ".pytest_cache",
            ".mypy_cache",
            ".ruff_cache",
            ".idea",
            ".vscode",
            ".cache",
            "coverage",
            "node_modules",
            "target",
            "dist",
            "build",
            ".tools",
            ".tmp",
            "paradox_wiki",
        ] {
            fs::create_dir_all(directory.path().join(name)).unwrap();
            fs::write(
                directory.path().join(name).join("unrelated.txt"),
                format!("unrelated content from {name}"),
            )
            .unwrap();
        }
        for name in [
            ".DS_Store",
            "Thumbs.db",
            "debug.log",
            "cache.pyc",
            "editor.tmp",
        ] {
            fs::write(directory.path().join(name), "unrelated artifact").unwrap();
        }

        let mut progress = Vec::new();
        let result =
            scan_project_with_progress(directory.path(), &ScanOptions::default(), |update| {
                progress.push(update)
            })
            .unwrap();
        let serialized = serde_json::to_string(&result).unwrap();

        assert!(!serialized.contains("unrelated.txt"));
        assert!(!serialized.contains(".venv"));
        assert!(!serialized.contains("debug.log"));
        assert!(progress
            .iter()
            .all(|update| !update.current_path.contains(".venv")));
        assert!(progress
            .iter()
            .all(|update| !update.current_path.contains(".tools")));
        assert!(result.files_scanned >= 2);
    }

    #[test]
    fn skipped_managed_wiki_is_detected_without_reading_its_pages() {
        let directory = tempdir().unwrap();
        fs::write(
            directory.path().join("descriptor.mod"),
            "name=\"Example\"\n",
        )
        .unwrap();
        fs::create_dir_all(directory.path().join("paradox_wiki/pages")).unwrap();
        fs::write(
            directory.path().join("paradox_wiki/pages/large-page.md"),
            "wiki content that is not part of the project scan",
        )
        .unwrap();

        let result = scan_project(directory.path(), &ScanOptions::default()).unwrap();
        let serialized = serde_json::to_string(&result).unwrap();

        assert!(!serialized.contains("large-page.md"));
        assert!(result
            .conflicts
            .iter()
            .any(|conflict| conflict.path == "paradox_wiki"));
    }

    #[test]
    fn scan_streams_stage_path_and_counters_without_mutating_the_project() {
        let directory = tempdir().unwrap();
        fs::write(
            directory.path().join("descriptor.mod"),
            "name=\"Example\"\n",
        )
        .unwrap();
        fs::create_dir_all(directory.path().join("events")).unwrap();
        fs::write(
            directory.path().join("events/example.txt"),
            "namespace_example = yes\n",
        )
        .unwrap();
        let mut progress = Vec::new();

        let result =
            scan_project_with_progress(directory.path(), &ScanOptions::default(), |update| {
                progress.push(update)
            })
            .unwrap();

        assert!(progress
            .iter()
            .any(|update| update.stage == "discovering_files"));
        assert!(progress
            .iter()
            .any(|update| update.stage == "detecting_descriptors"));
        assert_eq!(
            progress.last().map(|update| update.stage.as_str()),
            Some("complete")
        );
        assert!(progress
            .iter()
            .any(|update| update.current_path == "descriptor.mod"));
        assert!(progress
            .last()
            .is_some_and(|update| update.files_scanned >= 2));
        assert_eq!(result.files_scanned, progress.last().unwrap().files_scanned);
        assert!(!result.cancelled);
    }

    #[test]
    fn cancelled_scan_returns_partial_result_and_cancelled_progress() {
        let directory = tempdir().unwrap();
        fs::write(
            directory.path().join("descriptor.mod"),
            "name=\"Example\"\n",
        )
        .unwrap();
        let cancellation = Arc::new(AtomicBool::new(true));
        let mut progress = Vec::new();
        let result = scan_project_with_progress(
            directory.path(),
            &ScanOptions {
                cancel_flag: Some(cancellation),
                ..ScanOptions::default()
            },
            |update| progress.push(update),
        )
        .unwrap();

        assert!(result.cancelled);
        assert!(result.partial);
        assert!(result
            .limits_hit
            .iter()
            .any(|limit| limit == "scan.cancelled"));
        assert_eq!(
            progress.last().map(|update| update.stage.as_str()),
            Some("cancelled")
        );
        assert!(result
            .conflicts
            .iter()
            .any(|conflict| conflict.id == "scan.cancelled"));
    }

    #[test]
    fn scan_budget_returns_partial_result_without_marking_it_cancelled() {
        let directory = tempdir().unwrap();
        fs::write(
            directory.path().join("descriptor.mod"),
            "name=\"Example\"\n",
        )
        .unwrap();
        fs::write(directory.path().join("README.md"), "example\n").unwrap();
        let result = scan_project(
            directory.path(),
            &ScanOptions {
                max_files: 1,
                ..ScanOptions::default()
            },
        )
        .unwrap();

        assert!(result.partial);
        assert!(!result.cancelled);
        assert!(result
            .limits_hit
            .iter()
            .any(|limit| limit == "scan.file_limit"));
    }

    #[test]
    fn scan_recognizes_a_valid_managed_setup_without_walking_metadata() {
        let directory = tempdir().unwrap();
        let metadata = directory.path().join(".hoi4-mod-setup");
        fs::create_dir_all(&metadata).unwrap();
        let mut lock: InstallationLock = serde_json::from_str(include_str!(
            "../../docs/examples/installation-lock.example.json"
        ))
        .unwrap();
        lock.optional_workflows.insert(
            "workflow.super_events".into(),
            OptionalWorkflowLock {
                state: "not_selected".into(),
                reason: None,
                credential_reference: None,
            },
        );
        fs::write(
            metadata.join("install.lock.json"),
            serde_json::to_vec(&lock).unwrap(),
        )
        .unwrap();

        let result = scan_project(directory.path(), &ScanOptions::default()).unwrap();
        let managed = result
            .findings
            .iter()
            .find(|finding| finding.id == "installation.managed")
            .expect("managed setup finding");
        assert_eq!(managed.status, "accepted");
        assert_eq!(managed.value["present"], true);
        assert_eq!(managed.value["valid"], true);
        assert_eq!(managed.value["workflow_super_events_state"], "not_selected");
        assert!(result
            .findings
            .iter()
            .flat_map(|finding| &finding.evidence)
            .any(|evidence| evidence.path == ".hoi4-mod-setup/install.lock.json"));
        assert_eq!(result.files_scanned, 0);
    }

    #[test]
    fn scan_remembers_an_installed_super_events_workflow() {
        let directory = tempdir().unwrap();
        let metadata = directory.path().join(".hoi4-mod-setup");
        fs::create_dir_all(&metadata).unwrap();
        let mut lock: InstallationLock = serde_json::from_str(include_str!(
            "../../docs/examples/installation-lock.example.json"
        ))
        .unwrap();
        lock.optional_workflows.insert(
            "workflow.super_events".into(),
            OptionalWorkflowLock {
                state: "ready".into(),
                reason: None,
                credential_reference: None,
            },
        );
        fs::write(
            metadata.join("install.lock.json"),
            serde_json::to_vec_pretty(&lock).unwrap(),
        )
        .unwrap();

        let result = scan_project(directory.path(), &ScanOptions::default()).unwrap();
        let managed = result
            .findings
            .iter()
            .find(|finding| finding.id == "installation.managed")
            .expect("managed setup finding");
        assert_eq!(managed.value["workflow_super_events_state"], "ready");
    }

    #[test]
    fn scan_reports_an_invalid_managed_setup_as_blocking() {
        let directory = tempdir().unwrap();
        let metadata = directory.path().join(".hoi4-mod-setup");
        fs::create_dir_all(&metadata).unwrap();
        fs::write(metadata.join("install.lock.json"), "{\"broken\":true}").unwrap();

        let result = scan_project(directory.path(), &ScanOptions::default()).unwrap();
        assert!(result
            .findings
            .iter()
            .any(|finding| finding.id == "installation.managed" && finding.status == "blocking"));
        assert!(result
            .conflicts
            .iter()
            .any(|conflict| conflict.id == "conflict.installation.lock_invalid"));
    }
}
