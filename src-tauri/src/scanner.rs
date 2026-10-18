use crate::models::*;
use crate::security::{is_link_metadata, path_has_link_component, sha256_bytes};
use crate::AppError;
use chrono::Utc;
pub(crate) use hoi4_setup_core::profile::sensitive_scan_path;
use regex::Regex;
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, OnceLock,
};
use std::time::{Duration, Instant};
use uuid::Uuid;

const MAX_FILES: usize = 150_000;
const MAX_DEPTH: usize = 64;
const MAX_DIRECTORIES: usize = 200_000;
const MAX_TOTAL_BYTES: u64 = 256 * 1024 * 1024;
const MAX_TEXT_BYTES: u64 = 16 * 1024 * 1024;
const MAX_RETAINED_BYTES: u64 = 128 * 1024 * 1024;
const MAX_THUMBNAIL_BYTES: u64 = 16 * 1024 * 1024;
// A lock for a project with every coding client holds several thousand file
// records plus generated thumbnail bytes; 2 MiB made such projects unscannable.
const MAX_INSTALLATION_LOCK_BYTES: u64 = 32 * 1024 * 1024;
const MAX_DIRECTORY_ENTRIES: usize = 50_000;
const MAX_DIRECTORY_ENTRY_NAME_BYTES: u64 = 8 * 1024 * 1024;
const MAX_RELATIVE_PATH_BYTES: usize = 4 * 1024;
const MAX_INVENTORY_PATH_BYTES: u64 = 64 * 1024 * 1024;
const MAX_SCAN_CONFLICTS: usize = 4_096;
const MAX_MALFORMED_AGENTIC_SAMPLES: usize = 512;
const MAX_GITIGNORE_SAMPLES: usize = 1_024;
const DEFAULT_SCAN_DURATION: Duration = Duration::from_secs(10 * 60);
const MAX_LAUNCHER_CANDIDATES: usize = 512;
const MAX_LAUNCHER_PARENT_ENTRIES: usize = 10_000;
const MAX_LAUNCHER_DESCRIPTOR_BYTES: u64 = 256 * 1024;
const MAX_GIT_HEAD_BYTES: u64 = 1024 * 1024;
// A semantic evidence identifier, never a path to open. The real approved
// external path remains in the launcher finding's value for user review.
const APPROVED_LAUNCHER_EVIDENCE_PATH: &str = "@approved-launcher/descriptor.mod";

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
    pub max_duration: Duration,
}

impl Default for ScanOptions {
    fn default() -> Self {
        Self {
            max_files: MAX_FILES,
            max_depth: MAX_DEPTH,
            approved_external_descriptor: None,
            cancel_after_files: None,
            cancel_flag: None,
            max_duration: DEFAULT_SCAN_DURATION,
        }
    }
}

pub fn discover_launcher_descriptor(root: &Path) -> Result<Option<PathBuf>, AppError> {
    discover_launcher_descriptor_with_check(root, || false)
}

#[derive(Debug)]
struct DiscoveredLauncherDescriptor {
    path: PathBuf,
    parent: crate::flatten::BoundedReadRoot,
}

fn launcher_candidate_names(
    parent_handle: &crate::flatten::BoundedReadRoot,
    mut should_stop: impl FnMut() -> bool,
) -> Result<Vec<OsString>, AppError> {
    let mut candidates = Vec::new();
    let mut entries_seen = 0_usize;
    let mut stopped = false;
    let mut limit_hit = false;
    let mut entry_error = false;
    parent_handle.visit_directory_names(|entry| {
        if should_stop() {
            stopped = true;
            return false;
        }
        if entries_seen >= MAX_LAUNCHER_PARENT_ENTRIES {
            limit_hit = true;
            return false;
        }
        entries_seen += 1;
        let name = match entry {
            Ok(name) => name,
            Err(_) => {
                entry_error = true;
                return false;
            }
        };
        let is_mod = Path::new(&name)
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("mod"));
        if is_mod {
            candidates.push(name);
        }
        true
    })?;
    if stopped {
        return Err(AppError::Scan("launcher discovery stopped".into()));
    }
    if limit_hit {
        return Err(AppError::Scan(
            "project parent exceeds the bounded launcher discovery entry limit".into(),
        ));
    }
    if entry_error {
        return Err(AppError::Scan(
            "one or more project-parent entries could not be inspected".into(),
        ));
    }
    if candidates.len() > MAX_LAUNCHER_CANDIDATES {
        return Err(AppError::Scan(format!(
            "project parent contains more than {MAX_LAUNCHER_CANDIDATES} launcher descriptor candidates"
        )));
    }
    candidates.sort();
    Ok(candidates)
}

pub(crate) fn discover_launcher_descriptor_with_check(
    root: &Path,
    should_stop: impl FnMut() -> bool,
) -> Result<Option<PathBuf>, AppError> {
    discover_launcher_descriptor_bound_with_check(root, should_stop)
        .map(|candidate| candidate.map(|candidate| candidate.path))
}

fn discover_launcher_descriptor_bound_with_check(
    root: &Path,
    mut should_stop: impl FnMut() -> bool,
) -> Result<Option<DiscoveredLauncherDescriptor>, AppError> {
    let root_handle = crate::flatten::BoundedReadRoot::open(root)
        .map_err(|error| AppError::Scan(format!("cannot resolve project root: {error}")))?;
    let root = root_handle.canonical().to_path_buf();
    let parent = root
        .parent()
        .ok_or_else(|| AppError::Scan("project root has no parent directory".into()))?;
    let parent_handle = crate::flatten::BoundedReadRoot::open(parent)
        .map_err(|error| AppError::Scan(format!("cannot inspect project parent: {error}")))?;
    let expected_name = root
        .file_name()
        .map(|name| format!("{}.mod", name.to_string_lossy()).to_ascii_lowercase());
    let mut candidates = launcher_candidate_names(&parent_handle, &mut should_stop)?
        .into_iter()
        .map(|name| parent.join(name))
        .collect::<Vec<_>>();
    candidates.sort_by(|left, right| {
        let left_exact = expected_name.as_deref().is_some_and(|expected| {
            left.file_name()
                .is_some_and(|name| name.to_string_lossy().eq_ignore_ascii_case(expected))
        });
        let right_exact = expected_name.as_deref().is_some_and(|expected| {
            right
                .file_name()
                .is_some_and(|name| name.to_string_lossy().eq_ignore_ascii_case(expected))
        });
        right_exact.cmp(&left_exact).then_with(|| left.cmp(right))
    });
    let mut matches = Vec::new();
    let mut canonical_mismatch = None;
    for path in candidates {
        if should_stop() {
            return Err(AppError::Scan("launcher discovery stopped".into()));
        }
        let Some(relative_name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        let bytes = match parent_handle.read_bounded_with_check(
            relative_name,
            MAX_LAUNCHER_DESCRIPTOR_BYTES,
            &mut should_stop,
        ) {
            Ok(bytes) => bytes,
            Err(AppError::Scan(_)) if should_stop() => {
                return Err(AppError::Scan("launcher discovery stopped".into()));
            }
            Err(_) => continue,
        };
        let Ok(descriptor) = crate::descriptors::parse_descriptor(&bytes) else {
            continue;
        };
        let Some(declared) = descriptor.fields.get("path") else {
            continue;
        };
        let exact_name = expected_name.as_deref().is_some_and(|expected| {
            path.file_name()
                .map(|name| name.to_string_lossy().to_ascii_lowercase() == expected)
                .unwrap_or(false)
        });
        // A hand-written value may use single backslashes, which the
        // escape-aware parser drops, so the raw quoted text is tried too.
        let raw = crate::descriptors::raw_quoted_field(&bytes, "path");
        if crate::descriptors::launcher_registers_project_root(&bytes, declared, &root) {
            matches.push((exact_name, path));
        } else if exact_name {
            // Show the text as written unless it uses escaped separators.
            let shown = match raw {
                Some(raw) if !raw.contains("\\\\") => raw,
                _ => declared.clone(),
            };
            canonical_mismatch = Some((path, shown));
        }
    }
    parent_handle.with_stable_path(|_| ()).map_err(|_| {
        AppError::Scan("project parent changed identity during launcher discovery".into())
    })?;
    root_handle.with_stable_path(|_| ()).map_err(|_| {
        AppError::Scan("project root changed identity during launcher discovery".into())
    })?;
    matches.sort_by(|left, right| right.0.cmp(&left.0).then_with(|| left.1.cmp(&right.1)));
    if matches.len() > 1 {
        let paths = matches
            .iter()
            .map(|(_, path)| crate::paths::user_facing_path(path))
            .collect::<Vec<_>>()
            .join(", ");
        return Err(AppError::Scan(format!(
            "multiple launcher descriptors register this project: {paths}"
        )));
    }
    if let Some((path, declared)) = canonical_mismatch {
        let declared = declared
            .chars()
            .filter(|character| !character.is_control())
            .take(260)
            .collect::<String>();
        let file = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        return Err(AppError::Scan(format!(
            "{file} tells the HOI4 launcher to load this mod from \"{declared}\", not from the selected folder {}. Choose the folder {file} names, or continue without the launcher file.",
            crate::paths::user_facing_path(&root)
        )));
    }
    Ok(matches
        .into_iter()
        .next()
        .map(|(_, path)| DiscoveredLauncherDescriptor {
            path,
            parent: parent_handle,
        }))
}

#[derive(Debug, Clone, Serialize)]
pub struct ScanProgress {
    pub stage: String,
    pub current_path: String,
    pub files_scanned: u32,
    pub directories_scanned: u32,
    pub bytes_read: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile_stats: Option<ProfileStats>,
}

#[derive(Debug, Clone)]
struct FileObservation {
    relative: String,
    bytes: Vec<u8>,
    is_symlink: bool,
}

#[derive(Default)]
struct ScanEvidenceState {
    absolute_paths: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ScanFileIdentity {
    volume: u64,
    file: u64,
}

#[cfg(windows)]
fn scan_path_identity(path: &Path) -> Option<ScanFileIdentity> {
    use std::fs::OpenOptions;
    use std::os::windows::fs::OpenOptionsExt;
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Storage::FileSystem::{
        GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION,
    };

    const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
    const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
    const FILE_SHARE_READ: u32 = 0x0000_0001;
    const FILE_SHARE_WRITE: u32 = 0x0000_0002;
    const FILE_SHARE_DELETE: u32 = 0x0000_0004;

    let file = OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)
        .ok()?;
    let mut information = unsafe { std::mem::zeroed::<BY_HANDLE_FILE_INFORMATION>() };
    let success = unsafe {
        GetFileInformationByHandle(file.as_raw_handle() as _, &mut information as *mut _)
    };
    if success == 0 {
        return None;
    }
    Some(ScanFileIdentity {
        volume: information.dwVolumeSerialNumber as u64,
        file: ((information.nFileIndexHigh as u64) << 32) | information.nFileIndexLow as u64,
    })
}

#[cfg(unix)]
fn scan_path_identity(path: &Path) -> Option<ScanFileIdentity> {
    use std::os::unix::fs::MetadataExt;

    let metadata = fs::symlink_metadata(path).ok()?;
    Some(ScanFileIdentity {
        volume: metadata.dev(),
        file: metadata.ino(),
    })
}

#[cfg(not(any(windows, unix)))]
fn scan_path_identity(_path: &Path) -> Option<ScanFileIdentity> {
    None
}

fn scan_path_identity_matches(path: &Path, expected: ScanFileIdentity) -> bool {
    fs::symlink_metadata(path)
        .ok()
        .filter(|metadata| !is_link_metadata(metadata))
        .and_then(|_| scan_path_identity(path))
        .is_some_and(|identity| identity == expected)
}

fn mark_scan_boundary_changed(root: &Path, path: &Path, conflicts: &mut Vec<ScanConflict>) {
    if conflicts
        .iter()
        .any(|conflict| conflict.id == "scan.filesystem_boundary_changed")
    {
        return;
    }
    conflicts.push(ScanConflict {
        id: "scan.filesystem_boundary_changed".into(),
        path: relative_path(root, path),
        kind: "other".into(),
        severity: "block".into(),
        details: Some(
            "The selected project or an inspected directory changed identity during the scan; no further content was inspected."
                .into(),
        ),
    });
}

#[derive(Clone, Copy)]
struct ScanContentPolicy {
    max_bytes: u64,
    retain: bool,
    absolute_paths: bool,
    /// Record that the file exists without reading it. Client folders and
    /// configuration files that can hold credentials are detected this way.
    presence_only: bool,
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
    let read_root = crate::flatten::BoundedReadRoot::open(root)
        .map_err(|error| AppError::Scan(format!("cannot open project root safely: {error}")))?;
    let root = read_root.canonical().to_path_buf();
    if !root.is_dir() {
        return Err(AppError::Scan("project root is not a directory".into()));
    }
    let root_identity = scan_path_identity(&root)
        .ok_or_else(|| AppError::Scan("project root identity could not be verified".into()))?;

    let deadline = Instant::now() + options.max_duration;
    emit_progress(&mut progress, "discovering_files", &root, &root, &[], 0, 0);
    // One walk feeds the setup inventory and the project profile.
    let no_extra_cancel = || false;
    let mut walk = ProjectWalk::new(
        &read_root,
        &root,
        options,
        root_identity,
        deadline,
        &no_extra_cancel,
        &mut progress,
    );
    walk.run(true);
    let ProjectWalk {
        mut observations,
        mut conflicts,
        mut total_bytes,
        directories,
        detector_evidence,
        mut profiler,
        ..
    } = walk;
    let mut profile_stats = None;
    observations.sort_by(|left, right| left.relative.cmp(&right.relative));
    append_case_collision_conflicts(&observations, &mut conflicts);
    let mut findings = Vec::new();
    if scan_stopped(options, observations.len()) {
        mark_cancelled(
            &root,
            &mut conflicts,
            &mut progress,
            &observations,
            directories,
            total_bytes,
        );
    } else if Instant::now() >= deadline {
        mark_timed_out(&root, &root, &mut conflicts);
    } else {
        macro_rules! detect_phase {
            ($stage:literal, $path:expr, $body:block) => {
                if !scan_path_identity_matches(&root, root_identity) {
                    mark_scan_boundary_changed(&root, &root, &mut conflicts);
                }
                if !scan_phase_stopped(
                    options,
                    deadline,
                    &root,
                    &mut conflicts,
                    &mut progress,
                    &observations,
                    directories,
                    total_bytes,
                ) {
                    emit_progress(
                        &mut progress,
                        $stage,
                        $path,
                        &root,
                        &observations,
                        directories,
                        total_bytes,
                    );
                    $body
                    if !scan_path_identity_matches(&root, root_identity) {
                        mark_scan_boundary_changed(&root, &root, &mut conflicts);
                    }
                }
            };
        }
        detect_phase!("detecting_descriptors", Path::new("descriptor.mod"), {
            let approved_launcher = options.approved_external_descriptor.as_deref().and_then(
                |requested| match bind_approved_launcher_descriptor(&root, requested, || {
                    scan_stopped(options, observations.len())
                        || Instant::now() >= deadline
                        || !scan_path_identity_matches(&root, root_identity)
                }) {
                    Ok(approved) => Some(approved),
                    Err(AppError::Scan(message)) if message == "launcher discovery stopped" => {
                        None
                    }
                    Err(_) => {
                        conflicts.push(ScanConflict {
                            id: "scan.external_descriptor_approval".into(),
                            path: "<launcher descriptor>".into(),
                            kind: "descriptor_mismatch".into(),
                            severity: "block".into(),
                            details: Some(
                                "The approved launcher descriptor no longer matches the current unique discovered candidate. Choose it again before continuing."
                                    .into(),
                            ),
                        });
                        None
                    }
                },
            );
            detect_descriptors(
                &root,
                observations
                    .iter()
                    .find(|file| file.relative.eq_ignore_ascii_case("descriptor.mod")),
                approved_launcher.as_ref(),
                &mut findings,
                &mut conflicts,
            );
        });
        detect_phase!("detecting_thumbnail", Path::new("thumbnail.png"), {
            detect_thumbnail(
                observations
                    .iter()
                    .find(|file| file.relative.eq_ignore_ascii_case("thumbnail.png")),
                &mut findings,
                &mut conflicts,
            );
        });
        detect_phase!("detecting_git", Path::new(".git"), {
            detect_git(
                &read_root,
                &root,
                &observations,
                &mut findings,
                &mut conflicts,
                || {
                    scan_stopped(options, observations.len())
                        || Instant::now() >= deadline
                        || !scan_path_identity_matches(&root, root_identity)
                },
            );
        });
        detect_phase!(
            "detecting_documentation",
            Path::new("README.md and docs/"),
            {
                detect_documentation(&observations, &mut findings);
            }
        );
        detect_phase!(
            "detecting_agentic_files",
            Path::new(".agents/ and .codex/"),
            {
                detect_agentic_files(&observations, &mut findings, &mut conflicts);
                detect_coding_environments(&observations, &mut findings, &mut conflicts);
            }
        );
        detect_phase!("detecting_paths", Path::new("absolute paths"), {
            detect_absolute_paths(&detector_evidence, &mut findings);
        });
        detect_phase!(
            "detecting_components",
            Path::new("managed component files"),
            {
                detect_existing_components(&root, &observations, &mut conflicts);
                detect_managed_installation(
                    &read_root,
                    &root,
                    &mut findings,
                    &mut conflicts,
                    || {
                        scan_stopped(options, observations.len())
                            || Instant::now() >= deadline
                            || !scan_path_identity_matches(&root, root_identity)
                    },
                );
                reconcile_coding_environment_finding(&mut findings);
            }
        );

        detect_phase!("profiling_project", Path::new("."), {
            let cancel = || scan_stopped(options, observations.len()) || Instant::now() >= deadline;
            let profile = profiler.take().unwrap_or_default().analyze(
                &mut |path| {
                    read_root.read_prefix_with_check(
                        path,
                        crate::project_profile::PREFIX_BYTES,
                        cancel,
                    )
                },
                &cancel,
                &mut |stats, path| {
                    progress(ScanProgress {
                        stage: if stats.samples_read == 0 {
                            "indexing_project"
                        } else {
                            "sampling_project"
                        }
                        .into(),
                        current_path: safe_scan_display_path(path),
                        files_scanned: observations.len() as u32,
                        directories_scanned: directories as u32,
                        bytes_read: total_bytes + stats.bytes_read,
                        profile_stats: Some(stats.clone()),
                    })
                },
            )?;
            for issue in &profile.problems {
                if *issue == "stopped" {
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
                        mark_timed_out(&root, &root, &mut conflicts);
                    }
                } else {
                    conflicts.push(ScanConflict { id: format!("scan.profile.{issue}"), path: "@scan/profile/coverage".into(),
                        kind: "other".into(), severity: "warn".into(),
                        details: Some("Project profiling skipped an unreadable, linked, or over-budget input. The profile is marked truncated; setup evidence is unaffected.".into()) });
                }
            }
            total_bytes += profile.stats.bytes_read;
            profile_stats = Some(profile.stats);
            findings.extend(profile.findings);
            // Values the files decide are offered as Detected, and the
            // analysis keeps them instead of the assistant's guesses.
            let detected = hoi4_setup_core::proposals::detect(&findings);
            if !detected.is_empty() {
                findings.push(finding(
                    "profile.detected",
                    "structure",
                    "detected_values",
                    json!(detected),
                    "accepted",
                    evidence(
                        "deterministic_detection",
                        "@scan/profile/detected",
                        1.0,
                        Some("Values decided by the project files: descriptor fields, the dominant namespace, localisation headers, content folders, and documentation."),
                    ),
                    None,
                ));
            }
        });

        if !scan_phase_stopped(
            options,
            deadline,
            &root,
            &mut conflicts,
            &mut progress,
            &observations,
            directories,
            total_bytes,
        ) {
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
                            APPROVED_LAUNCHER_EVIDENCE_PATH,
                            0.5,
                            Some("Approved launcher descriptor was not found."),
                        ),
                        Some("Choose a launcher descriptor destination during review."),
                    ));
                }
            }
        }
    }

    for finding in &mut findings {
        for evidence in &mut finding.evidence {
            evidence.path = safe_scan_display_path(&evidence.path);
            evidence.note = evidence
                .note
                .as_deref()
                .map(|note| crate::security::redact_secrets(note, &[]));
        }
    }
    for conflict in &mut conflicts {
        conflict.path = safe_scan_display_path(&conflict.path);
        conflict.details = conflict
            .details
            .as_deref()
            .map(|details| crate::security::redact_secrets(details, &[]));
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
        .filter(|conflict| scan_conflict_truncated_detector_evidence(&conflict.id))
        .map(|conflict| conflict.id.clone())
        .collect::<Vec<_>>();
    let cancelled = limits_hit.iter().any(|id| id == "scan.cancelled");
    let timed_out = limits_hit.iter().any(|id| id == "scan.timeout");
    emit_progress(
        &mut |mut event: ScanProgress| {
            event.profile_stats = profile_stats.clone();
            progress(event);
        },
        if cancelled {
            "cancelled"
        } else if timed_out {
            "timeout"
        } else {
            "complete"
        },
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
        profile_stats,
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

/// Profile a tree through the shared walk with the setup inventory off.
#[cfg(test)]
pub(crate) fn profile_only(
    read_root: &crate::flatten::BoundedReadRoot,
    cancel: &dyn Fn() -> bool,
    progress: &mut dyn FnMut(&ProfileStats, &str),
) -> Result<crate::project_profile::ProfileResult, AppError> {
    let root = read_root.canonical().to_path_buf();
    let root_identity = scan_path_identity(&root)
        .ok_or_else(|| AppError::Scan("project root identity could not be verified".into()))?;
    let options = ScanOptions::default();
    let mut walk_progress = |event: ScanProgress| {
        if let Some(stats) = &event.profile_stats {
            progress(stats, &event.current_path);
        }
    };
    let mut walk = ProjectWalk::new(
        read_root,
        &root,
        &options,
        root_identity,
        Instant::now() + options.max_duration,
        cancel,
        &mut walk_progress,
    );
    walk.run(false);
    let profiler = walk.profiler.take().unwrap_or_default();
    profiler.analyze(
        &mut |path| {
            read_root.read_prefix_with_check(path, crate::project_profile::PREFIX_BYTES, cancel)
        },
        cancel,
        progress,
    )
}

/// Only conflicts that made detector evidence incomplete turn the overall scan
/// into a partial result. Git inspection is deliberately bounded and advisory:
/// an incomplete Git status probe remains visible for review, but it does not
/// mean the targeted agentic project scan was truncated.
fn scan_conflict_truncated_detector_evidence(id: &str) -> bool {
    // A profile limitation (a linked entry, a category or path budget) is
    // deterministic, so rescanning cannot clear it. It stays a visible warning
    // and truncated profile coverage, while the setup evidence remains
    // complete; cancellation and timeout still make the scan partial.
    id.starts_with("scan.")
        && !id.starts_with("scan.sensitive.")
        && !id.starts_with("scan.profile.")
        && !matches!(
            id,
            "scan.git.inspection" | "scan.git.link" | "scan.git.head_link" | "scan.git.head_read"
        )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DirectoryEnumerationStop {
    Complete,
    Cancelled,
    TimedOut,
    TargetedEntryLimit,
    TargetedEntryNameBytesLimit,
}

#[derive(Debug)]
struct DirectoryEnumeration {
    entries: Vec<OsString>,
    sensitive_found: bool,
    entry_error: bool,
    stop: DirectoryEnumerationStop,
}

/// The targeted setup inventory's view of one directory enumeration. The
/// shared walk offers it every name; it keeps only setup-relevant entries
/// under its own entry and name-byte budgets.
struct TargetedNames {
    enumeration: DirectoryEnumeration,
    entry_name_bytes: u64,
    max_targeted_entries: usize,
}

impl TargetedNames {
    fn new(max_targeted_entries: usize) -> Self {
        Self {
            enumeration: DirectoryEnumeration {
                entries: Vec::new(),
                sensitive_found: false,
                entry_error: false,
                stop: DirectoryEnumerationStop::Complete,
            },
            entry_name_bytes: 0,
            max_targeted_entries,
        }
    }

    /// Accept one enumerated name; `false` once the setup inventory wants no
    /// more names from this directory.
    fn accept(
        &mut self,
        entry: &Result<OsString, std::io::Error>,
        current_relative: &str,
        options: &ScanOptions,
        observations_len: usize,
        deadline: Instant,
    ) -> bool {
        let result = &mut self.enumeration;
        if result.stop != DirectoryEnumerationStop::Complete {
            return false;
        }
        if scan_stopped(options, observations_len) {
            result.stop = DirectoryEnumerationStop::Cancelled;
            return false;
        }
        if Instant::now() >= deadline {
            result.stop = DirectoryEnumerationStop::TimedOut;
            return false;
        }
        let file_name = match entry {
            Ok(file_name) => file_name,
            Err(_) => {
                // A directory contributes at most one deterministic entry-error
                // conflict, regardless of how many raw iterator failures occur.
                result.entry_error = true;
                return true;
            }
        };
        let name = file_name.to_string_lossy();
        let relative = if current_relative.is_empty() {
            name.to_string()
        } else {
            format!("{current_relative}/{name}")
        };
        if !name.eq_ignore_ascii_case(".git") && sensitive_scan_path(&relative) {
            result.sensitive_found = true;
            return true;
        }
        // Registry client surfaces (for example `.vscode/mcp.json`) are
        // explicit scan targets even inside an otherwise ignored folder.
        let client_path = crate::coding_environment::is_client_scan_directory(&relative)
            || client_content_policy(&normalized_scan_path(&relative)).is_some();
        if (is_unrelated_scan_directory(&name) || is_unrelated_scan_file(&name)) && !client_path {
            return true;
        }
        if !targeted_scan_path_candidate(&relative) {
            return true;
        }
        if result.entries.len() >= self.max_targeted_entries {
            result.stop = DirectoryEnumerationStop::TargetedEntryLimit;
            return false;
        }
        let name_bytes = name.len() as u64;
        if self.entry_name_bytes.saturating_add(name_bytes) > MAX_DIRECTORY_ENTRY_NAME_BYTES {
            result.stop = DirectoryEnumerationStop::TargetedEntryNameBytesLimit;
            return false;
        }
        self.entry_name_bytes = self.entry_name_bytes.saturating_add(name_bytes);
        result.entries.push(file_name.clone());
        true
    }

    fn finish(mut self) -> DirectoryEnumeration {
        self.enumeration.entries.sort();
        self.enumeration
    }
}

#[cfg(test)]
fn enumerate_targeted_directory(
    directory: &crate::flatten::BoundedReadRoot,
    current_relative: &str,
    options: &ScanOptions,
    observations_len: usize,
    deadline: Instant,
    max_targeted_entries: usize,
) -> Result<DirectoryEnumeration, AppError> {
    let mut names = TargetedNames::new(max_targeted_entries);
    directory.visit_directory_names(|entry| {
        names.accept(
            &entry,
            current_relative,
            options,
            observations_len,
            deadline,
        )
    })?;
    Ok(names.finish())
}

/// One bounded, read-only, link-refusing walk of the project. Each directory
/// is enumerated once, and its names feed two inventories that keep their own
/// rules: the targeted setup inventory (setup surfaces only, with the scan's
/// file, directory, path, byte, conflict, identity-fence, sensitive-path, and
/// case-collision rules) and the project profile (the whole tree minus its
/// exclusions, with its own budgets). A directory is entered when either
/// inventory wants it, and an inventory that stops leaves the other running.
struct ProjectWalk<'a> {
    read_root: &'a crate::flatten::BoundedReadRoot,
    root: &'a Path,
    options: &'a ScanOptions,
    root_identity: ScanFileIdentity,
    deadline: Instant,
    /// An extra stop signal for the profile, beyond the scan's own.
    profile_cancel: &'a dyn Fn() -> bool,
    progress: &'a mut dyn FnMut(ScanProgress),
    observations: Vec<FileObservation>,
    conflicts: Vec<ScanConflict>,
    total_bytes: u64,
    retained_bytes: u64,
    inventory_path_bytes: u64,
    directories: usize,
    sensitive_path_warning_emitted: bool,
    detector_evidence: ScanEvidenceState,
    profiler: Option<crate::project_profile::Profiler>,
    /// A filesystem boundary changed; nothing further is inspected.
    halted: bool,
}

/// What the setup inventory does after one of its per-entry checks.
enum SetupStep {
    Continue,
    /// Skip this entry.
    Skip,
    /// The setup inventory takes nothing more from this directory.
    End,
}

impl<'a> ProjectWalk<'a> {
    fn new(
        read_root: &'a crate::flatten::BoundedReadRoot,
        root: &'a Path,
        options: &'a ScanOptions,
        root_identity: ScanFileIdentity,
        deadline: Instant,
        profile_cancel: &'a dyn Fn() -> bool,
        progress: &'a mut dyn FnMut(ScanProgress),
    ) -> Self {
        Self {
            read_root,
            root,
            options,
            root_identity,
            deadline,
            profile_cancel,
            progress,
            observations: Vec::new(),
            conflicts: Vec::new(),
            total_bytes: 0,
            retained_bytes: 0,
            inventory_path_bytes: 0,
            directories: 0,
            sensitive_path_warning_emitted: false,
            detector_evidence: ScanEvidenceState::default(),
            profiler: Some(crate::project_profile::Profiler::new()),
            halted: false,
        }
    }

    /// Walk from the project root with the requested inventories.
    fn run(&mut self, setup: bool) {
        let root = self.root;
        let setup = if setup {
            self.enter_setup(root, 0)
        } else {
            None
        };
        let profile = self.enter_profile(0);
        if setup.is_some() || profile {
            let read_root = self.read_root;
            self.walk(read_root, root, 0, setup, profile);
        }
    }

    fn cancelled(&mut self) {
        mark_cancelled(
            self.root,
            &mut self.conflicts,
            &mut self.progress,
            &self.observations,
            self.directories,
            self.total_bytes,
        );
    }

    fn timed_out(&mut self, current: &Path) {
        mark_timed_out(self.root, current, &mut self.conflicts);
    }

    fn boundary_changed(&mut self, current: &Path) {
        mark_scan_boundary_changed(self.root, current, &mut self.conflicts);
        self.halted = true;
    }

    fn profile_stopped(&self) -> bool {
        self.halted
            || scan_stopped(self.options, self.observations.len())
            || Instant::now() >= self.deadline
            || (self.profile_cancel)()
    }

    /// The profile's checks before a directory is enumerated.
    fn enter_profile(&mut self, depth: usize) -> bool {
        let stop = self.profile_stopped();
        self.profiler.as_mut().is_some_and(|profiler| {
            !profiler.should_stop(&|| stop) && profiler.enter_directory(depth)
        })
    }

    fn profile_progress(&mut self, path: &str) {
        let Some(profiler) = &self.profiler else {
            return;
        };
        let stats = profiler.result.stats.clone();
        (self.progress)(ScanProgress {
            stage: "indexing_project".into(),
            current_path: safe_scan_display_path(path),
            files_scanned: self.observations.len() as u32,
            directories_scanned: self.directories as u32,
            bytes_read: self.total_bytes + stats.bytes_read,
            profile_stats: Some(stats),
        });
    }

    /// The setup inventory's checks before a directory is enumerated; `Some`
    /// carries the directory identity its fence keeps for the directory.
    fn enter_setup(&mut self, current: &Path, depth: usize) -> Option<ScanFileIdentity> {
        let root = self.root;
        if conflict_budget_hit(&mut self.conflicts) || inventory_limit_hit(&self.conflicts) {
            return None;
        }
        if Instant::now() >= self.deadline {
            self.timed_out(current);
            return None;
        }
        if scan_stopped(self.options, self.observations.len()) {
            self.cancelled();
            return None;
        }
        if !scan_path_identity_matches(root, self.root_identity) {
            self.boundary_changed(root);
            return None;
        }
        if depth > self.options.max_depth.min(MAX_DEPTH) {
            self.conflicts.push(ScanConflict {
                id: format!(
                    "scan.depth.{}",
                    &sha256_bytes(relative_path(root, current).as_bytes())[..12]
                ),
                path: relative_path(root, current),
                kind: "other".into(),
                severity: "warn".into(),
                details: Some(
                    "Maximum scan depth reached; remaining files were not inspected.".into(),
                ),
            });
            return None;
        }
        if path_has_link_component(current) {
            self.conflicts.push(ScanConflict {
                id: format!(
                    "scan.directory_link.{}",
                    &sha256_bytes(relative_path(root, current).as_bytes())[..12]
                ),
                path: relative_path(root, current),
                kind: "other".into(),
                severity: "block".into(),
                details: Some("A linked directory was not followed during scan.".into()),
            });
            return None;
        }
        let directory_identity = match fs::symlink_metadata(current) {
            Ok(metadata) if metadata.is_dir() && !is_link_metadata(&metadata) => {
                match scan_path_identity(current) {
                    Some(identity) => identity,
                    None => {
                        self.boundary_changed(current);
                        return None;
                    }
                }
            }
            _ => {
                self.boundary_changed(current);
                return None;
            }
        };
        if self.directories >= MAX_DIRECTORIES {
            self.conflicts.push(ScanConflict {
                id: "scan.directory_limit".into(),
                path: relative_path(root, current),
                kind: "other".into(),
                severity: "warn".into(),
                details: Some(
                    "Maximum directory budget reached; remaining entries were not inspected."
                        .into(),
                ),
            });
            return None;
        }
        self.directories += 1;
        Some(directory_identity)
    }

    /// The setup inventory's checks after its enumeration; `None` when it
    /// takes nothing from this directory.
    fn finish_setup_enumeration(
        &mut self,
        current: &Path,
        current_relative: &str,
        directory_identity: ScanFileIdentity,
        enumeration: DirectoryEnumeration,
    ) -> Option<Vec<OsString>> {
        let root = self.root;
        if !scan_path_identity_matches(root, self.root_identity)
            || !scan_path_identity_matches(current, directory_identity)
        {
            self.boundary_changed(current);
            return None;
        }
        if enumeration.sensitive_found && !self.sensitive_path_warning_emitted {
            self.conflicts.push(ScanConflict {
                id: "scan.sensitive.excluded".into(),
                path: "<credential-shaped project paths>".into(),
                kind: "secret_excluded".into(),
                severity: "warn".into(),
                details: Some(
                    "Credential-shaped project content was excluded from the read-only scan."
                        .into(),
                ),
            });
            self.sensitive_path_warning_emitted = true;
        }
        if enumeration.entry_error && !conflict_budget_hit(&mut self.conflicts) {
            self.conflicts.push(ScanConflict {
                id: format!(
                    "scan.entry_error.{}",
                    &sha256_bytes(current_relative.as_bytes())[..12]
                ),
                path: current_relative.to_owned(),
                kind: "other".into(),
                severity: "warn".into(),
                details: Some("One or more directory entries could not be inspected.".into()),
            });
        }
        match enumeration.stop {
            DirectoryEnumerationStop::Complete => {}
            DirectoryEnumerationStop::Cancelled => {
                self.cancelled();
                return None;
            }
            DirectoryEnumerationStop::TimedOut => {
                self.timed_out(current);
                return None;
            }
            DirectoryEnumerationStop::TargetedEntryLimit => {
                self.conflicts.push(ScanConflict {
                    id: "scan.directory_entry_limit".into(),
                    path: current_relative.to_owned(),
                    kind: "other".into(),
                    severity: "warn".into(),
                    details: Some(
                        "One directory exceeded the bounded targeted-entry inventory; remaining relevant entries were not inspected."
                            .into(),
                    ),
                });
                return None;
            }
            DirectoryEnumerationStop::TargetedEntryNameBytesLimit => {
                self.conflicts.push(ScanConflict {
                    id: "scan.directory_entry_bytes_limit".into(),
                    path: current_relative.to_owned(),
                    kind: "other".into(),
                    severity: "warn".into(),
                    details: Some(
                        "One directory exceeded the bounded targeted entry-name inventory; remaining relevant entries were not inspected."
                            .into(),
                    ),
                });
                return None;
            }
        }
        if scan_stopped(self.options, self.observations.len()) {
            self.cancelled();
            return None;
        }
        if Instant::now() >= self.deadline {
            self.timed_out(current);
            return None;
        }
        Some(enumeration.entries)
    }

    /// The setup inventory's checks before each of its entries.
    fn setup_entry_step(
        &mut self,
        current: &Path,
        directory_identity: ScanFileIdentity,
        entry_index: usize,
    ) -> SetupStep {
        let root = self.root;
        if conflict_budget_hit(&mut self.conflicts) || inventory_limit_hit(&self.conflicts) {
            return SetupStep::End;
        }
        if Instant::now() >= self.deadline {
            self.timed_out(current);
            return SetupStep::End;
        }
        if entry_index % 128 == 0
            && (!scan_path_identity_matches(root, self.root_identity)
                || !scan_path_identity_matches(current, directory_identity))
        {
            self.boundary_changed(current);
            return SetupStep::End;
        }
        if self.observations.len() >= self.options.max_files.min(MAX_FILES) {
            self.conflicts.push(ScanConflict {
                id: "scan.file_limit".into(),
                path: relative_path(root, current),
                kind: "other".into(),
                severity: "warn".into(),
                details: Some(
                    "Scan file limit reached; remaining files were not inspected.".into(),
                ),
            });
            return SetupStep::End;
        }
        if scan_stopped(self.options, self.observations.len()) {
            self.cancelled();
            return SetupStep::End;
        }
        SetupStep::Continue
    }

    /// The setup inventory's path budgets for one entry.
    fn setup_path_step(&mut self, relative: &str) -> SetupStep {
        if relative.len() > MAX_RELATIVE_PATH_BYTES
            || relative.split('/').any(|segment| segment.len() > 255)
        {
            self.conflicts.push(ScanConflict {
                id: format!(
                    "scan.path_length.{}",
                    &sha256_bytes(relative.as_bytes())[..12]
                ),
                path: "<oversized relative path>".into(),
                kind: "other".into(),
                severity: "warn".into(),
                details: Some(
                    "A project path exceeded the bounded scan path length and was not inspected."
                        .into(),
                ),
            });
            return SetupStep::Skip;
        }
        if self
            .inventory_path_bytes
            .saturating_add(relative.len() as u64)
            > MAX_INVENTORY_PATH_BYTES
        {
            self.conflicts.push(ScanConflict {
                id: "scan.path_budget".into(),
                path: "<project tree>".into(),
                kind: "other".into(),
                severity: "warn".into(),
                details: Some(
                    "The aggregate project-path evidence budget was reached; remaining entries were not inspected."
                        .into(),
                ),
            });
            return SetupStep::End;
        }
        SetupStep::Continue
    }

    fn observe_link(&mut self, relative: String) {
        // A presence-only client marker is never read, so a linked one (for
        // example `GEMINI.md` linking to `AGENTS.md`) is reported without
        // blocking the scan.
        let presence_only =
            scan_content_policy(&relative).is_some_and(|policy| policy.presence_only);
        let prefix = if presence_only {
            "client.marker_link"
        } else {
            "scan.link"
        };
        self.conflicts.push(ScanConflict {
            id: format!("{prefix}.{}", &sha256_bytes(relative.as_bytes())[..12]),
            path: relative.clone(),
            kind: "other".into(),
            severity: if presence_only { "warn" } else { "block" }.into(),
            details: Some("Symlink or junction was not followed during scan.".into()),
        });
        self.inventory_path_bytes = self
            .inventory_path_bytes
            .saturating_add(relative.len() as u64);
        self.observations.push(FileObservation {
            relative,
            bytes: Vec::new(),
            is_symlink: true,
        });
    }

    /// Record one regular setup file; `End` when the scan stopped while it
    /// was read.
    fn observe_file(
        &mut self,
        current: &Path,
        directory_identity: ScanFileIdentity,
        path: &Path,
        relative: String,
        length: u64,
    ) -> SetupStep {
        let root = self.root;
        let Some(policy) = scan_content_policy(&relative) else {
            return SetupStep::Continue;
        };
        let mut bytes = match policy {
            policy if policy.presence_only => Vec::new(),
            policy
                if length > policy.max_bytes
                    || self.total_bytes.saturating_add(length) > MAX_TOTAL_BYTES =>
            {
                self.conflicts.push(ScanConflict {
                    id: format!(
                        "scan.bytes_limit.{}",
                        &sha256_bytes(relative.as_bytes())[..12]
                    ),
                    path: relative.clone(),
                    kind: "other".into(),
                    severity: "warn".into(),
                    details: Some(
                        "Detector-relevant content was not read because its bounded scan byte limit was reached."
                            .into(),
                    ),
                });
                Vec::new()
            }
            policy => {
                let remaining_budget = MAX_TOTAL_BYTES.saturating_sub(self.total_bytes);
                let (options, deadline, root_identity) =
                    (self.options, self.deadline, self.root_identity);
                let observations_len = self.observations.len();
                let read = self.read_root.read_bounded_with_check(
                    &relative,
                    policy.max_bytes.min(remaining_budget),
                    || {
                        scan_stopped(options, observations_len)
                            || Instant::now() >= deadline
                            || !scan_path_identity_matches(root, root_identity)
                            || !scan_path_identity_matches(current, directory_identity)
                    },
                );
                if scan_stopped(self.options, self.observations.len()) {
                    self.cancelled();
                    return SetupStep::End;
                }
                if Instant::now() >= self.deadline {
                    self.timed_out(path);
                    return SetupStep::End;
                }
                if !scan_path_identity_matches(root, self.root_identity)
                    || !scan_path_identity_matches(current, directory_identity)
                {
                    self.boundary_changed(current);
                    return SetupStep::End;
                }
                match read {
                    Ok(bytes) => bytes,
                    Err(error) => {
                        self.conflicts.push(ScanConflict {
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
            }
        };
        if self.total_bytes.saturating_add(bytes.len() as u64) > MAX_TOTAL_BYTES {
            self.conflicts.push(ScanConflict {
                id: "scan.total_bytes_limit".into(),
                path: relative.clone(),
                kind: "other".into(),
                severity: "warn".into(),
                details: Some(
                    "Detector content grew beyond the remaining aggregate scan byte budget.".into(),
                ),
            });
            bytes = Vec::new();
        } else if !bytes.is_empty() {
            self.total_bytes = self.total_bytes.saturating_add(bytes.len() as u64);
        }
        observe_detector_content(&relative, &bytes, policy, &mut self.detector_evidence);
        if policy.retain && !bytes.is_empty() {
            if self.retained_bytes.saturating_add(bytes.len() as u64) > MAX_RETAINED_BYTES {
                self.conflicts.push(ScanConflict {
                    id: "scan.retained_text_limit".into(),
                    path: relative.clone(),
                    kind: "other".into(),
                    severity: "warn".into(),
                    details: Some(
                        "Required parser content exceeded the bounded in-memory evidence budget."
                            .into(),
                    ),
                });
                bytes = Vec::new();
            } else {
                self.retained_bytes = self.retained_bytes.saturating_add(bytes.len() as u64);
            }
        } else {
            bytes = Vec::new();
        }
        self.inventory_path_bytes = self
            .inventory_path_bytes
            .saturating_add(relative.len() as u64);
        self.observations.push(FileObservation {
            relative,
            bytes,
            is_symlink: false,
        });
        if self.observations.len() == 1 || self.observations.len() % 128 == 0 {
            emit_progress(
                &mut self.progress,
                "discovering_files",
                path,
                root,
                &self.observations,
                self.directories,
                self.total_bytes,
            );
        }
        SetupStep::Continue
    }

    /// Enumerate one directory once and hand its names to the inventories
    /// that entered it: `setup` carries the setup fence identity, `profile`
    /// whether the profile entered.
    fn walk(
        &mut self,
        directory: &crate::flatten::BoundedReadRoot,
        current: &Path,
        depth: usize,
        mut setup: Option<ScanFileIdentity>,
        mut profile: bool,
    ) {
        let current_relative = relative_path(self.root, current);
        let mut targeted = TargetedNames::new(MAX_DIRECTORY_ENTRIES);
        let mut profile_names = crate::project_profile::ProfileNames::default();
        let (options, deadline, observations_len) =
            (self.options, self.deadline, self.observations.len());
        let profile_stop = {
            let (halted, cancel) = (self.halted, self.profile_cancel);
            move || {
                halted
                    || scan_stopped(options, observations_len)
                    || Instant::now() >= deadline
                    || cancel()
            }
        };
        let profiler = &mut self.profiler;
        let visited = directory.visit_directory_names(|entry| {
            let setup_more = setup.is_some()
                && targeted.accept(
                    &entry,
                    &current_relative,
                    options,
                    observations_len,
                    deadline,
                );
            let profile_more = profile
                && profiler.as_mut().is_some_and(|profiler| {
                    profile_names.accept(profiler, entry.as_deref().map_err(|_| ()), &profile_stop)
                });
            setup_more || profile_more
        });
        if visited.is_err() {
            if setup.is_some() {
                // Discard every name observed while the identity fence was open.
                self.boundary_changed(current);
            }
            if let (true, Some(profiler)) = (profile, self.profiler.as_mut()) {
                // A folder that vanishes or changes during the walk is
                // incomplete profile coverage, never a failed scan.
                profiler.problem("directory_read");
            }
            return;
        }
        let setup_entries = match setup {
            Some(identity) => {
                let entries = self.finish_setup_enumeration(
                    current,
                    &current_relative,
                    identity,
                    targeted.finish(),
                );
                if entries.is_none() {
                    setup = None;
                }
                entries.unwrap_or_default()
            }
            None => Vec::new(),
        };
        // Both inventories keep their own sorted order; byte order of the
        // platform name agrees with the profile's UTF-8 order.
        let mut entries = BTreeMap::<OsString, (bool, bool)>::new();
        for name in setup_entries {
            entries.entry(name).or_default().0 = true;
        }
        if profile {
            for name in profile_names.into_sorted() {
                entries.entry(OsString::from(name)).or_default().1 = true;
            }
        }
        let mut setup_index = 0_usize;
        let mut collision_keys = BTreeSet::new();
        for (name, (setup_entry, profile_entry)) in entries {
            let mut in_setup = false;
            if let (true, Some(identity)) = (setup_entry, setup) {
                match self.setup_entry_step(current, identity, setup_index) {
                    SetupStep::Continue => in_setup = true,
                    SetupStep::Skip => {}
                    SetupStep::End => setup = None,
                }
                setup_index += 1;
            }
            let path = current.join(&name);
            let relative = relative_path(self.root, &path);
            if in_setup {
                match self.setup_path_step(&relative) {
                    SetupStep::Continue => {}
                    SetupStep::Skip => in_setup = false,
                    SetupStep::End => {
                        in_setup = false;
                        setup = None;
                    }
                }
            }
            let mut profile_relative = None;
            if profile && profile_entry {
                let stop = self.profile_stopped();
                if let Some(profiler) = self.profiler.as_mut() {
                    if profiler.should_stop(&|| stop) {
                        profile = false;
                    } else if let Some(text) = name.to_str() {
                        profile_relative = crate::project_profile::admit(
                            profiler,
                            &mut collision_keys,
                            &current_relative,
                            text,
                        );
                    }
                }
            }
            if !in_setup && profile_relative.is_none() {
                if setup.is_none() && !profile {
                    break;
                }
                continue;
            }
            // Setup entries keep the scan's no-follow path metadata; profile
            // entries are inspected through the retained directory handle.
            let metadata = if in_setup {
                match fs::symlink_metadata(&path) {
                    Ok(metadata) => Some(metadata),
                    Err(error) => {
                        self.conflicts.push(ScanConflict {
                            id: format!(
                                "scan.metadata_error.{}",
                                &sha256_bytes(relative.as_bytes())[..12]
                            ),
                            path: relative.clone(),
                            kind: "other".into(),
                            severity: "warn".into(),
                            details: Some(format!("File metadata could not be inspected: {error}")),
                        });
                        in_setup = false;
                        None
                    }
                }
            } else {
                directory
                    .with_stable_path(|stable| fs::symlink_metadata(stable.join(&name)))
                    .ok()
                    .and_then(Result::ok)
            };
            let mut profile_child = false;
            if let (Some(profile_path), Some(profiler)) =
                (profile_relative.as_deref(), self.profiler.as_mut())
            {
                let text = name.to_str().unwrap_or_default();
                match crate::project_profile::record_entry(
                    profiler,
                    &current_relative,
                    text,
                    profile_path,
                    metadata.as_ref(),
                ) {
                    crate::project_profile::ProfileEntry::Skip => {}
                    crate::project_profile::ProfileEntry::Directory => profile_child = true,
                    crate::project_profile::ProfileEntry::File(report) => {
                        if report {
                            self.profile_progress(profile_path);
                        }
                    }
                    crate::project_profile::ProfileEntry::Stop => profile = false,
                }
            }
            let Some(metadata) = metadata else {
                continue;
            };
            if in_setup && is_link_metadata(&metadata) {
                self.observe_link(relative);
            } else if metadata.is_dir() && !is_link_metadata(&metadata) {
                let setup_child = in_setup && targeted_scan_directory(&relative);
                if !setup_child && !profile_child {
                    continue;
                }
                let child_setup = if setup_child {
                    self.enter_setup(&path, depth + 1)
                } else {
                    None
                };
                if child_setup.is_none() && !profile_child {
                    continue;
                }
                let child = match name.to_str() {
                    Some(text) => directory.open_child_directory(text).ok(),
                    None => None,
                };
                let Some(child) = child else {
                    if child_setup.is_some() {
                        self.boundary_changed(&path);
                    }
                    if let (true, Some(profiler)) = (profile_child, self.profiler.as_mut()) {
                        profiler.problem("directory_read");
                    }
                    continue;
                };
                let child_profile = profile_child && self.enter_profile(depth + 1);
                if child_setup.is_some() || child_profile {
                    self.walk(&child, &path, depth + 1, child_setup, child_profile);
                }
            } else if let (true, Some(identity)) = (in_setup && metadata.is_file(), setup) {
                if let SetupStep::End =
                    self.observe_file(current, identity, &path, relative, metadata.len())
                {
                    setup = None;
                }
            }
        }
    }
}

fn inventory_limit_hit(conflicts: &[ScanConflict]) -> bool {
    conflicts.iter().any(|conflict| {
        matches!(
            conflict.id.as_str(),
            "scan.file_limit"
                | "scan.directory_limit"
                | "scan.directory_entry_limit"
                | "scan.directory_entry_bytes_limit"
                | "scan.timeout"
                | "scan.filesystem_boundary_changed"
                | "scan.conflict_limit"
                | "scan.path_budget"
        )
    })
}

fn append_case_collision_conflicts(
    observations: &[FileObservation],
    conflicts: &mut Vec<ScanConflict>,
) {
    let mut normalized_paths = BTreeMap::<String, Vec<String>>::new();
    for observation in observations {
        normalized_paths
            .entry(observation.relative.to_lowercase())
            .or_default()
            .push(observation.relative.clone());
    }
    for paths in normalized_paths.values().filter(|paths| paths.len() > 1) {
        if conflict_budget_hit(conflicts) {
            break;
        }
        conflicts.push(ScanConflict {
            id: format!(
                "scan.case_collision.{}",
                &sha256_bytes(paths.join("\0").as_bytes())[..12]
            ),
            path: paths.join(", "),
            kind: "case_collision".into(),
            severity: "block".into(),
            details: Some(
                "Multiple setup paths differ only by letter case; choose one canonical path before continuing."
                    .into(),
            ),
        });
    }
}

fn conflict_budget_hit(conflicts: &mut Vec<ScanConflict>) -> bool {
    if conflicts.len() < MAX_SCAN_CONFLICTS.saturating_sub(1) {
        return false;
    }
    if !conflicts
        .iter()
        .any(|conflict| conflict.id == "scan.conflict_limit")
    {
        conflicts.push(ScanConflict {
            id: "scan.conflict_limit".into(),
            path: "<project tree>".into(),
            kind: "other".into(),
            severity: "warn".into(),
            details: Some(
                "The bounded scan conflict budget was reached; remaining entries were not inspected."
                    .into(),
            ),
        });
    }
    true
}

fn normalized_scan_path(relative: &str) -> String {
    let relative = relative.replace('\\', "/");
    relative.to_lowercase()
}

fn scan_content_policy(relative: &str) -> Option<ScanContentPolicy> {
    #[cfg(not(windows))]
    if relative.contains('\\') {
        return None;
    }
    let normalized = normalized_scan_path(relative);
    if normalized == "thumbnail.png" {
        return Some(ScanContentPolicy {
            max_bytes: MAX_THUMBNAIL_BYTES,
            retain: true,
            absolute_paths: false,
            presence_only: false,
        });
    }
    if normalized == "descriptor.mod" {
        return Some(ScanContentPolicy {
            max_bytes: MAX_LAUNCHER_DESCRIPTOR_BYTES,
            retain: true,
            absolute_paths: true,
            presence_only: false,
        });
    }
    if let Some(policy) = client_content_policy(&normalized) {
        return Some(policy);
    }
    let segments = normalized.split('/').collect::<Vec<_>>();
    let name = segments.last().copied().unwrap_or_default();
    let extension = name.rsplit_once('.').map(|(_, extension)| extension)?;
    let skill = matches!(
        segments.as_slice(),
        [".claude" | ".agents" | ".qoder", "skills", _, "skill.md"]
    );
    let subagent = matches!(segments.as_slice(), [".codex", "agents", _]) && extension == "toml";
    let codex_config = normalized == ".codex/config.toml";
    // Native coding-client files are a bounded, explicit scan surface. The
    // scanner records their paths for environment detection and retains only
    // small text/config files needed for deterministic validation; it never
    // walks arbitrary editor caches or client-owned project data.
    let native_root_instruction = normalized == "claude.md";
    let native_json = matches!(
        normalized.as_str(),
        ".mcp.json"
            | ".claude/settings.json"
            | ".cursor/settings.json"
            | ".cursor/mcp.json"
            | ".qoder/settings.json"
            | ".qoder/mcp.json"
            | ".opencode/settings.json"
            | ".opencode/mcp.json"
            | "opencode.json"
    );
    let native_map = matches!(
        normalized.as_str(),
        ".claude/agent-map.md"
            | ".cursor/agent-map.md"
            | ".qoder/agent-map.md"
            | ".opencode/agent-map.md"
    );
    let native_agent = matches!(
        segments.as_slice(),
        [".claude", "agents", _]
            | [".cursor", "agents", _]
            | [".qoder", "agents", _]
            | [".opencode", "agent", _]
    ) && extension == "md";
    let bounded_documentation = matches!(segments.as_slice(), ["docs", _] | ["docs", _, _])
        && !matches!(segments.get(1).copied(), Some("assets" | "formables"));
    let documentation =
        normalized == "agents.md" || normalized == "readme.md" || bounded_documentation;
    let root_text = matches!(
        normalized.as_str(),
        "agents.md" | "readme.md" | ".gitignore"
    );
    let documentation_text = documentation
        && matches!(
            extension,
            "cfg" | "json" | "md" | "ps1" | "py" | "sh" | "toml" | "txt" | "yaml" | "yml"
        );
    if !(skill
        || subagent
        || codex_config
        || native_root_instruction
        || native_json
        || native_map
        || native_agent
        || root_text
        || documentation_text)
    {
        return None;
    }
    Some(ScanContentPolicy {
        max_bytes: MAX_TEXT_BYTES,
        retain: skill
            || subagent
            || codex_config
            || native_root_instruction
            || native_json
            || native_map,
        absolute_paths: documentation_text
            || skill
            || subagent
            || codex_config
            || native_root_instruction
            || native_json
            || native_map
            || root_text,
        presence_only: false,
    })
}

/// Native client files from the coding-client registry. Small configuration
/// files are retained and parsed; other markers are observed by presence
/// only, so a client folder never exposes its contents to the scan.
fn client_content_policy(normalized: &str) -> Option<ScanContentPolicy> {
    if crate::coding_environment::config_format_for_path(normalized).is_some() {
        return Some(ScanContentPolicy {
            max_bytes: MAX_TEXT_BYTES,
            retain: true,
            absolute_paths: true,
            presence_only: false,
        });
    }
    let presence = Some(ScanContentPolicy {
        max_bytes: MAX_TEXT_BYTES,
        retain: false,
        absolute_paths: false,
        presence_only: true,
    });
    let marker = crate::coding_environment::NATIVE_CLIENTS
        .iter()
        .filter(|client| !crate::coding_environment::SUPPORTED.contains(&client.id))
        .flat_map(|client| client.markers.iter())
        .any(|marker| !marker.ends_with('/') && marker.eq_ignore_ascii_case(normalized));
    if marker {
        return presence;
    }
    let (parent, name) = normalized.rsplit_once('/')?;
    let extension = name.rsplit_once('.').map(|(_, extension)| extension)?;
    // `.github` and `.vscode` hold unrelated project files (workflows, editor
    // settings); only their exact client markers and config files count.
    (!matches!(parent, ".github" | ".vscode")
        && crate::coding_environment::is_client_scan_directory(parent)
        && matches!(extension, "md" | "json" | "jsonc" | "toml" | "yaml" | "yml"))
    .then_some(presence)?
}

fn targeted_scan_directory(relative: &str) -> bool {
    #[cfg(not(windows))]
    if relative.contains('\\') {
        return false;
    }
    let normalized = normalized_scan_path(relative);
    let segments = normalized.split('/').collect::<Vec<_>>();
    matches!(
        segments.as_slice(),
        [".agents"]
            | [".agents", "skills"]
            | [".agents", "skills", _]
            | [".codex"]
            | [".codex", "agents"]
            | [".claude"]
            | [".claude", "skills"]
            | [".claude", "skills", _]
            | [".claude", "agents"]
            | [".cursor"]
            | [".cursor", "agents"]
            | [".qoder"]
            | [".qoder", "agents"]
            | [".opencode"]
            | [".opencode", "agent"]
            | ["docs"]
    ) || (matches!(segments.as_slice(), ["docs", _])
        && !matches!(segments.get(1).copied(), Some("assets" | "formables")))
        || crate::coding_environment::is_client_scan_directory(&normalized)
}

pub(crate) fn targeted_scan_path_candidate(relative: &str) -> bool {
    targeted_scan_directory(relative) || scan_content_policy(relative).is_some()
}

fn absolute_path_regex() -> &'static Regex {
    static ABSOLUTE: OnceLock<Regex> = OnceLock::new();
    ABSOLUTE.get_or_init(|| {
        Regex::new(r"(?i)(?:[a-z]:[\\/]|\\\\[^\\/\s]+[\\/]|//[^/\s]+/|/users/|/home/|/mnt/)")
            .expect("static absolute path regex")
    })
}

fn observe_detector_content(
    relative: &str,
    bytes: &[u8],
    policy: ScanContentPolicy,
    evidence: &mut ScanEvidenceState,
) {
    if bytes.is_empty() {
        return;
    }
    let text = String::from_utf8_lossy(bytes);
    if policy.absolute_paths
        && evidence.absolute_paths.len() < 20
        && absolute_path_regex().is_match(&text)
    {
        evidence.absolute_paths.push(relative.to_string());
    }
}

fn mark_timed_out(root: &Path, current: &Path, conflicts: &mut Vec<ScanConflict>) {
    if conflicts
        .iter()
        .any(|conflict| conflict.id == "scan.timeout")
    {
        return;
    }
    conflicts.push(ScanConflict {
        id: "scan.timeout".into(),
        path: relative_path(root, current),
        kind: "other".into(),
        severity: "warn".into(),
        details: Some("Scan time limit reached; remaining files were not inspected.".into()),
    });
}

#[allow(clippy::too_many_arguments)]
fn scan_phase_stopped(
    options: &ScanOptions,
    deadline: Instant,
    root: &Path,
    conflicts: &mut Vec<ScanConflict>,
    progress: &mut impl FnMut(ScanProgress),
    observations: &[FileObservation],
    directories: usize,
    bytes_read: u64,
) -> bool {
    if conflicts.iter().any(|conflict| {
        matches!(
            conflict.id.as_str(),
            "scan.cancelled" | "scan.timeout" | "scan.filesystem_boundary_changed"
        )
    }) {
        return true;
    }
    if scan_stopped(options, observations.len()) {
        mark_cancelled(
            root,
            conflicts,
            progress,
            observations,
            directories,
            bytes_read,
        );
        return true;
    }
    if Instant::now() >= deadline {
        mark_timed_out(root, root, conflicts);
        return true;
    }
    false
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
            safe_scan_display_path(&relative_path(root, current))
        },
        files_scanned: observations.len() as u32,
        directories_scanned: directories as u32,
        bytes_read,
        profile_stats: None,
    });
}

fn safe_scan_display_path(path: &str) -> String {
    let redacted = crate::security::redact_secrets(path, &[]);
    if redacted.len() <= MAX_RELATIVE_PATH_BYTES {
        redacted
    } else {
        format!("<oversized path:{}>", &sha256_bytes(path.as_bytes())[..12])
    }
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

#[derive(Debug)]
struct ApprovedLauncherDescriptor {
    path: PathBuf,
    bytes: Vec<u8>,
}

fn bind_approved_launcher_descriptor(
    root: &Path,
    requested: &Path,
    mut should_stop: impl FnMut() -> bool,
) -> Result<ApprovedLauncherDescriptor, AppError> {
    if !requested.is_absolute() || path_has_link_component(requested) {
        return Err(AppError::PathSecurity(
            "launcher descriptor approval is not a direct regular path".into(),
        ));
    }
    let discovered = discover_launcher_descriptor_bound_with_check(root, &mut should_stop)?
        .ok_or_else(|| {
            AppError::Scan("no unique launcher descriptor currently registers this project".into())
        })?;
    bind_discovered_launcher_descriptor(root, requested, discovered, should_stop)
}

fn bind_discovered_launcher_descriptor(
    root: &Path,
    requested: &Path,
    discovered: DiscoveredLauncherDescriptor,
    mut should_stop: impl FnMut() -> bool,
) -> Result<ApprovedLauncherDescriptor, AppError> {
    if should_stop() {
        return Err(AppError::Scan("launcher discovery stopped".into()));
    }
    let requested_parent_path = requested.parent().ok_or_else(|| {
        AppError::PathSecurity("launcher descriptor approval has no parent directory".into())
    })?;
    let requested_parent = crate::flatten::BoundedReadRoot::open(requested_parent_path)?;
    if !requested_parent.same_directory_identity(&discovered.parent) {
        return Err(AppError::PathSecurity(
            "launcher descriptor approval does not match the current discovered candidate".into(),
        ));
    }
    let requested_name = requested
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| AppError::PathSecurity("launcher descriptor name is invalid".into()))?;
    let discovered_name = discovered
        .path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| AppError::PathSecurity("launcher descriptor name is invalid".into()))?;
    if !requested_name.eq_ignore_ascii_case(discovered_name) {
        return Err(AppError::PathSecurity(
            "launcher descriptor approval does not match the current discovered candidate".into(),
        ));
    }
    let bytes = discovered.parent.read_bounded_with_check(
        discovered_name,
        MAX_LAUNCHER_DESCRIPTOR_BYTES,
        &mut should_stop,
    )?;
    let descriptor = crate::descriptors::parse_descriptor(&bytes)?;
    if !descriptor.fields.get("path").is_some_and(|declared| {
        crate::descriptors::launcher_registers_project_root(&bytes, declared, root)
    }) {
        return Err(AppError::PathSecurity(
            "approved launcher descriptor no longer registers the selected project".into(),
        ));
    }
    if should_stop() {
        return Err(AppError::Scan("launcher discovery stopped".into()));
    }
    Ok(ApprovedLauncherDescriptor {
        path: discovered.path,
        bytes,
    })
}

fn detect_descriptors(
    root: &Path,
    descriptor: Option<&FileObservation>,
    approved_external: Option<&ApprovedLauncherDescriptor>,
    findings: &mut Vec<ScanFinding>,
    conflicts: &mut Vec<ScanConflict>,
) {
    if let Some(descriptor) = descriptor.filter(|file| !file.is_symlink) {
        let parsed = crate::descriptors::parse_descriptor(&descriptor.bytes);
        if let Ok(parsed) = parsed {
            let metadata = parsed
                .fields
                .iter()
                .filter(|(key, _)| {
                    matches!(
                        key.as_str(),
                        "version"
                            | "supported_version"
                            | "tags"
                            | "dependencies"
                            | "replace_path"
                            | "picture"
                    )
                })
                .map(|(key, value)| {
                    (
                        key.clone(),
                        json!(crate::project_profile::clean_text(value, 1024)),
                    )
                })
                .collect::<serde_json::Map<String, Value>>();
            if !metadata.is_empty() {
                findings.push(finding(
                    "descriptor.metadata", "descriptor", "descriptor_metadata", json!(metadata), "needs_review",
                    evidence("descriptor_parser", "descriptor.mod", 1.0, Some("Parsed descriptor fields; long field values are bounded to 1024 characters.")),
                    Some("Preserve existing descriptor metadata unless explicitly changed during review."),
                ));
            }
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

    if let Some(approved_external) = approved_external {
        let external = &approved_external.path;
        let external_value = external.display().to_string();
        findings.push(finding(
            "descriptor.launcher",
            "descriptor",
            "launcher_descriptor",
            json!({"path": external_value, "exists": true}),
            "accepted",
            evidence(
                "approved_external_descriptor",
                APPROVED_LAUNCHER_EVIDENCE_PATH,
                1.0,
                None,
            ),
            None,
        ));
        match crate::descriptors::parse_descriptor(&approved_external.bytes) {
            Ok(parsed)
                if parsed.fields.contains_key("name")
                    && parsed.fields.get("path").is_some_and(|declared| {
                        crate::descriptors::launcher_registers_project_root(
                            &approved_external.bytes,
                            declared,
                            root,
                        )
                    }) => {}
            _ => conflicts.push(ScanConflict {
                id: "conflict.launcher.malformed".into(),
                path: APPROVED_LAUNCHER_EVIDENCE_PATH.into(),
                kind: "descriptor_mismatch".into(),
                severity: "block".into(),
                details: Some(
                    "Approved launcher descriptor is not a valid descriptor with name and path fields."
                        .into(),
                ),
            }),
        }
    }
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
    read_root: &crate::flatten::BoundedReadRoot,
    root: &Path,
    observations: &[FileObservation],
    findings: &mut Vec<ScanFinding>,
    conflicts: &mut Vec<ScanConflict>,
    mut should_stop: impl FnMut() -> bool,
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
    let (head, head_read_error) = if git_is_link {
        (None, false)
    } else if git_metadata
        .as_ref()
        .is_some_and(|metadata| metadata.is_dir())
    {
        if head_is_link {
            (None, false)
        } else {
            match read_root.read_bounded_with_check(
                ".git/HEAD",
                MAX_GIT_HEAD_BYTES,
                &mut should_stop,
            ) {
                Ok(bytes) => match String::from_utf8(bytes) {
                    Ok(value) => (Some(value.trim().to_string()), false),
                    Err(_) => (None, true),
                },
                Err(_) => (None, true),
            }
        }
    } else {
        (None, false)
    };
    let setup_paths = observations
        .iter()
        .filter(|file| !file.is_symlink)
        .map(|file| file.relative.clone())
        .collect::<Vec<_>>();
    let inspection = crate::git::inspect_agentic_read_only_bound_with_check(
        read_root,
        &setup_paths,
        &mut should_stop,
    );
    let mut ignore_files = inspection.ignore_files.clone();
    ignore_files.extend(
        observations
            .iter()
            .filter(|file| {
                !file.is_symlink
                    && (file.relative.eq_ignore_ascii_case(".gitignore")
                        || file.relative.to_ascii_lowercase().ends_with("/.gitignore"))
            })
            .map(|file| file.relative.clone())
            .take(MAX_GITIGNORE_SAMPLES),
    );
    ignore_files.sort();
    ignore_files.dedup();
    let status = &inspection.status;
    let status_value = json!({
        "inspection_scope": "agentic_setup",
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
    let finding_status = if !is_git || inspection.status_probe == "targeted_complete" {
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
            "@scan/git-summary",
            if !is_git {
                0.8
            } else if inspection.status_probe == "targeted_complete" {
                1.0
            } else {
                0.7
            },
            Some("Git metadata and bounded read-only status evidence for Agentic setup paths were collected without changing the repository."),
        ),
        (finding_status == "accepted")
            .then_some("Git state is available for review and maintenance planning."),
    ));
    if is_git
        && !matches!(
            inspection.status_probe.as_str(),
            "targeted_complete" | "linked_worktree_not_followed"
        )
    {
        conflicts.push(ScanConflict {
            id: if inspection.status_probe == "partial" {
                "scan.git.limit"
            } else {
                "scan.git.inspection"
            }
            .into(),
            path: "@scan/git-summary".into(),
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
            id: "scan.git.link".into(),
            path: "@scan/git-summary".into(),
            kind: "other".into(),
            severity: "block".into(),
            details: Some("The .git entry is a link; Git metadata was not followed.".into()),
        });
    } else if head_is_link {
        conflicts.push(ScanConflict {
            id: "scan.git.head_link".into(),
            path: "@scan/git-summary".into(),
            kind: "other".into(),
            severity: "block".into(),
            details: Some("Git HEAD is a link; metadata was not followed.".into()),
        });
    } else if head_read_error {
        conflicts.push(ScanConflict {
            id: "scan.git.head_read".into(),
            path: "@scan/git-summary".into(),
            kind: "other".into(),
            severity: "warn".into(),
            details: Some("Git HEAD could not be read as bounded UTF-8 text.".into()),
        });
    } else if git_metadata
        .as_ref()
        .is_some_and(|metadata| metadata.is_file())
    {
        conflicts.push(ScanConflict {
            id: "conflict.git.worktree".into(),
            path: "@scan/git-summary".into(),
            kind: "other".into(),
            severity: "warn".into(),
            details: Some(
                "The project uses a linked Git worktree; Git actions need explicit review.".into(),
            ),
        });
    }
}

fn detect_documentation(observations: &[FileObservation], findings: &mut Vec<ScanFinding>) {
    let docs = observations
        .iter()
        .filter(|file| {
            if file.is_symlink {
                return false;
            }
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
    // The canonical library is `.claude/skills`; `.agents/skills` is either
    // its runtime copy or, in older setups, the canonical library itself.
    let skill_files_under = |root: &str| -> Vec<&FileObservation> {
        observations
            .iter()
            .filter(|file| {
                let lower = file.relative.to_ascii_lowercase();
                !file.is_symlink && lower.starts_with(root) && lower.ends_with("skill.md")
            })
            .collect()
    };
    let claude_skill_files = skill_files_under(".claude/skills/");
    let (skill_root, skill_files) = if claude_skill_files.is_empty() {
        (".agents/skills/", skill_files_under(".agents/skills/"))
    } else {
        (".claude/skills/", claude_skill_files)
    };
    let malformed_skills = skill_files
        .iter()
        .filter(|file| !skill_frontmatter_is_valid(&String::from_utf8_lossy(&file.bytes)))
        .map(|file| file.relative.clone())
        .take(MAX_MALFORMED_AGENTIC_SAMPLES + 1)
        .collect::<Vec<_>>();
    let claude_agent_files: Vec<&FileObservation> = observations
        .iter()
        .filter(|file| {
            let lower = file.relative.to_ascii_lowercase();
            !file.is_symlink && lower.starts_with(".claude/agents/") && lower.ends_with(".md")
        })
        .collect();
    let codex_agent_files: Vec<&FileObservation> = observations
        .iter()
        .filter(|file| {
            let lower = file.relative.to_ascii_lowercase();
            !file.is_symlink && lower.starts_with(".codex/agents/") && lower.ends_with(".toml")
        })
        .collect();
    let subagent_root = if claude_agent_files.is_empty() {
        ".codex/agents/"
    } else {
        ".claude/agents/"
    };
    let subagent_count = if claude_agent_files.is_empty() {
        codex_agent_files.len()
    } else {
        claude_agent_files.len()
    };
    let malformed_claude_agents = claude_agent_files
        .iter()
        .filter(|file| !skill_frontmatter_is_valid(&String::from_utf8_lossy(&file.bytes)))
        .map(|file| file.relative.clone());
    let malformed_subagents = codex_agent_files
        .iter()
        .filter(|file| {
            let text = String::from_utf8_lossy(&file.bytes);
            let Ok(value) = text.parse::<toml::Value>() else {
                return true;
            };
            // Mirror the installer: a subagent either declares a top-level
            // `fork_context = false` or requires it in its parsed developer
            // instructions. A comment alone never counts.
            match value.get("fork_context").and_then(toml::Value::as_bool) {
                Some(fork_context) => fork_context,
                None => !value
                    .get("developer_instructions")
                    .and_then(toml::Value::as_str)
                    .is_some_and(|instructions| instructions.contains("fork_context=false")),
            }
        })
        .map(|file| file.relative.clone())
        .chain(malformed_claude_agents)
        .take(MAX_MALFORMED_AGENTIC_SAMPLES + 1)
        .collect::<Vec<_>>();
    let agentic_samples_truncated = malformed_skills.len() > MAX_MALFORMED_AGENTIC_SAMPLES
        || malformed_subagents.len() > MAX_MALFORMED_AGENTIC_SAMPLES;
    let malformed_skills = malformed_skills
        .into_iter()
        .take(MAX_MALFORMED_AGENTIC_SAMPLES)
        .collect::<Vec<_>>();
    let malformed_subagents = malformed_subagents
        .into_iter()
        .take(MAX_MALFORMED_AGENTIC_SAMPLES)
        .collect::<Vec<_>>();
    if agentic_samples_truncated {
        conflicts.push(ScanConflict {
            id: "scan.agentic_cardinality_limit".into(),
            path: format!("{skill_root} and {subagent_root}"),
            kind: "other".into(),
            severity: "warn".into(),
            details: Some(
                "Malformed agentic-file evidence exceeded the bounded sample limit.".into(),
            ),
        });
    }
    let codex_observation = observations
        .iter()
        .find(|file| !file.is_symlink && file.relative.eq_ignore_ascii_case(".codex/config.toml"));
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
    let agents_present = observations
        .iter()
        .any(|file| !file.is_symlink && file.relative.eq_ignore_ascii_case("AGENTS.md"));
    findings.push(finding(
        "codex.agents",
        "codex",
        "project_instructions",
        json!(agents_present),
        if agents_present {
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
        evidence("agentic_file_detector", skill_root, 1.0, None),
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
        json!({"count": subagent_count, "malformed": malformed_subagents.clone()}),
        if malformed_subagents.is_empty() {
            "accepted"
        } else {
            "needs_review"
        },
        evidence("agentic_file_detector", subagent_root, 1.0, None),
        None,
    ));
    for path in malformed_subagents {
        conflicts.push(ScanConflict {
            id: format!("conflict.subagent.{}", &sha256_bytes(path.as_bytes())[..12]),
            path,
            kind: "other".into(),
            severity: "block".into(),
            details: Some(
                "Subagent definition is unreadable, lacks a name or description, or (for Codex) does not require fork_context=false."
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
    if agents_present {
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

fn detect_coding_environments(
    observations: &[FileObservation],
    findings: &mut Vec<ScanFinding>,
    conflicts: &mut Vec<ScanConflict>,
) {
    let has = |path: &str| {
        observations
            .iter()
            .any(|file| !file.is_symlink && file.relative.eq_ignore_ascii_case(path))
    };
    let under = |prefix: &str| {
        let prefix = prefix.to_ascii_lowercase();
        observations
            .iter()
            .any(|file| !file.is_symlink && file.relative.to_ascii_lowercase().starts_with(&prefix))
    };
    // Canonical Codex agent TOMLs are a shared synchronization source and
    // are installed for every coding-environment selection.  They therefore
    // cannot, by themselves, prove that the Codex native package is present;
    // the registry's Codex marker is its project configuration.
    let detected = crate::coding_environment::detected_clients(has, under);
    let parse = |path: &str, format: crate::native_mcp::McpFormat| -> bool {
        observations
            .iter()
            .find(|file| !file.is_symlink && file.relative.eq_ignore_ascii_case(path))
            .is_some_and(|file| {
                if format.is_toml() {
                    std::str::from_utf8(&file.bytes)
                        .ok()
                        .is_some_and(|text| text.parse::<toml::Value>().is_ok())
                } else {
                    crate::native_mcp::parse_json_lenient(&file.bytes).is_ok()
                }
            })
    };
    for client in crate::coding_environment::NATIVE_CLIENTS {
        for (path, format) in client.config_files {
            if has(path) && !parse(path, *format) {
                let kind = if format.is_toml() { "TOML" } else { "JSON" };
                // The original clients' configurations are part of their
                // installed packages; another client's file only warns, so an
                // unrelated editor configuration never blocks a scan.
                let legacy = crate::coding_environment::SUPPORTED.contains(&client.id);
                conflicts.push(ScanConflict {
                    id: format!("conflict.{}.config.parse", client.id),
                    path: (*path).into(),
                    kind: "other".into(),
                    severity: if legacy { "block" } else { "warn" }.into(),
                    details: Some(format!("{} configuration is not valid {kind}.", client.id)),
                });
            }
        }
    }
    // A scan cannot infer a user-recorded primary from loose files. Codex is
    // the compatibility default until an installation lock supplies one.
    let primary = "codex";
    let evidence_path = detected
        .first()
        .and_then(|id| crate::coding_environment::native_client(id))
        .and_then(|client| client.markers.first())
        .map(|marker| marker.trim_end_matches('/'))
        .unwrap_or("@scan/coding-environments");
    findings.push(finding(
        "coding.environments",
        "coding_environment",
        "installed_environments",
        json!({"detected": detected, "primary": primary, "additional": []}),
        "accepted",
        evidence(
            "coding_environment_detector",
            evidence_path,
            1.0,
            Some("Native coding-client files were detected without changing the project."),
        ),
        None,
    ));
}

fn reconcile_coding_environment_finding(findings: &mut [ScanFinding]) {
    let recorded = findings
        .iter()
        .find(|finding| finding.id == "installation.managed")
        .filter(|finding| finding.value["valid"] == true)
        .and_then(|finding| {
            let primary = finding.value["primary_coding_environment"]
                .as_str()?
                .to_string();
            let additional = finding.value["additional_coding_environments"]
                .as_array()?
                .iter()
                .filter_map(Value::as_str)
                .map(ToOwned::to_owned)
                .collect::<Vec<_>>();
            Some((primary, additional))
        });
    let Some((primary, additional)) = recorded else {
        return;
    };
    let Some(coding) = findings
        .iter_mut()
        .find(|finding| finding.id == "coding.environments")
    else {
        return;
    };
    let mut detected = coding.value["detected"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    for environment in std::iter::once(&primary).chain(additional.iter()) {
        if !detected
            .iter()
            .any(|value| value.as_str() == Some(environment.as_str()))
        {
            detected.push(Value::String(environment.clone()));
        }
    }
    coding.value = json!({
        "detected": detected,
        "primary": primary,
        "additional": additional,
    });
    coding.evidence.push(evidence(
        "managed_installation_detector",
        ".hoi4-mod-setup/install.lock.json",
        1.0,
        Some("The recorded primary and additional coding environments were read from the validated installation lock."),
    ));
}

fn detect_absolute_paths(evidence_state: &ScanEvidenceState, findings: &mut Vec<ScanFinding>) {
    let matches = &evidence_state.absolute_paths;
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
            "@scan/absolute-paths",
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
    for path in [
        "paradox_wiki",
        ".claude/skills",
        ".claude/agents",
        ".agents/skills",
        ".codex/agents",
    ] {
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
            observations.iter().any(|file| {
                let relative = file.relative.to_ascii_lowercase();
                let path = path.to_ascii_lowercase();
                !file.is_symlink && (relative == path || relative.starts_with(&format!("{path}/")))
            })
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
    read_root: &crate::flatten::BoundedReadRoot,
    root: &Path,
    findings: &mut Vec<ScanFinding>,
    conflicts: &mut Vec<ScanConflict>,
    mut should_stop: impl FnMut() -> bool,
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
            id: "scan.managed_lock_link".into(),
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
            conflicts.push(ScanConflict {
                id: "scan.managed_lock_metadata".into(),
                path: LOCK_RELATIVE.into(),
                kind: "other".into(),
                severity: "block".into(),
                details: Some("The existing setup lock metadata could not be inspected.".into()),
            });
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
            id: "scan.managed_lock_type".into(),
            path: LOCK_RELATIVE.into(),
            kind: "other".into(),
            severity: "block".into(),
            details: Some("The existing setup lock is not a regular file.".into()),
        });
        return;
    }
    if metadata.len() > MAX_INSTALLATION_LOCK_BYTES {
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
        conflicts.push(ScanConflict {
            id: "scan.managed_lock_bytes_limit".into(),
            path: LOCK_RELATIVE.into(),
            kind: "other".into(),
            severity: "block".into(),
            details: Some("The existing setup lock exceeded the bounded read limit.".into()),
        });
        return;
    }
    let bytes = match read_root.read_bounded_with_check(
        LOCK_RELATIVE,
        MAX_INSTALLATION_LOCK_BYTES,
        &mut should_stop,
    ) {
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
            conflicts.push(ScanConflict {
                id: "scan.managed_lock_read".into(),
                path: LOCK_RELATIVE.into(),
                kind: "other".into(),
                severity: "block".into(),
                details: Some("The existing setup lock could not be read safely.".into()),
            });
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
            "primary_coding_environment": lock.primary_coding_environment,
            "additional_coding_environments": lock.additional_coding_environments,
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
            "image_generation": lock.image_generation,
            "live_testing": lock.live_testing.clone().unwrap_or_else(|| crate::live_testing::legacy_selection(&lock.components)),
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
    // Machine-local portrait routes restore the settings UI only. They live in
    // a separate local-only finding that the approval boundary refuses to
    // pass to a setup assistant.
    if let Some(portrait) = portrait_pipeline {
        findings.push(finding(
            LOCAL_ONLY_PORTRAIT_ROUTES_FINDING,
            "installation",
            "portrait_routes",
            json!({
                "local_root": portrait.local_comfyui_root,
                "local_server_url": portrait.local_server_url,
                "runpod_url": portrait.runpod_url,
                "runpod_workspace": portrait.runpod_workspace,
            }),
            "accepted",
            evidence(
                "managed_installation_detector",
                LOCK_RELATIVE,
                1.0,
                Some("Local portrait routes are kept on this computer and are not sent for analysis."),
            ),
            None,
        ));
    }
}

/// Finding ID for machine-local portrait routes. It is never approved as
/// setup-assistant evidence.
pub const LOCAL_ONLY_PORTRAIT_ROUTES_FINDING: &str = "installation.portrait_routes";

/// Bounded skill frontmatter check. The file must open with a `---` line and
/// close the block with another `---` line. Inside it, top-level `key: value`
/// lines (plain or quoted keys) must define exactly one non-empty `name` and
/// one non-empty `description`; a block scalar (`>` or `|`) counts as
/// non-empty when an indented line follows. Duplicate top-level keys and an
/// unterminated block are invalid. This is not a full YAML parser.
fn skill_frontmatter_is_valid(text: &str) -> bool {
    const MAX_FRONTMATTER_LINES: usize = 200;
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut lines = text.lines().map(|line| line.trim_end_matches('\r'));
    if lines.next() != Some("---") {
        return false;
    }
    let mut keys = BTreeSet::new();
    let mut name_present = false;
    let mut description_present = false;
    let mut pending_block: Option<&str> = None;
    for (index, line) in lines.enumerate() {
        if index >= MAX_FRONTMATTER_LINES {
            return false;
        }
        if line == "---" {
            return pending_block.is_none() && name_present && description_present;
        }
        let indented = line.starts_with(' ') || line.starts_with('\t');
        if let Some(key) = pending_block.take() {
            if indented && !line.trim().is_empty() {
                match key {
                    "name" => name_present = true,
                    "description" => description_present = true,
                    _ => {}
                }
                continue;
            }
        }
        if indented || line.trim().is_empty() || line.trim_start().starts_with('#') {
            continue;
        }
        let Some((raw_key, raw_value)) = line.split_once(':') else {
            continue;
        };
        let key = raw_key
            .trim()
            .trim_matches(|character| character == '"' || character == '\'');
        if key.is_empty() || !keys.insert(key.to_owned()) {
            return false;
        }
        let value = raw_value.trim();
        let scalar = value.trim_matches(|character| character == '"' || character == '\'');
        let present = if matches!(value.chars().next(), Some('>') | Some('|')) {
            pending_block = Some(if key == "name" {
                "name"
            } else if key == "description" {
                "description"
            } else {
                "other"
            });
            false
        } else {
            !scalar.trim().is_empty()
        };
        match key {
            "name" => name_present = present,
            "description" => description_present = present,
            _ => {}
        }
    }
    false
}

fn relative_path(root: &Path, path: &Path) -> String {
    let relative = path
        .strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .into_owned();
    #[cfg(windows)]
    {
        relative.replace('\\', "/")
    }
    #[cfg(not(windows))]
    {
        relative
    }
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
    mut value: Value,
    status: &str,
    mut evidence: ScanEvidence,
    recommendation: Option<&str>,
) -> ScanFinding {
    redact_scan_value(&mut value);
    let semantic_value =
        (id == "descriptor.launcher").then(|| json!({"approved":true,"exists":true}));
    let model_excerpt = match semantic_value.as_ref().unwrap_or(&value) {
        Value::String(text) => text.clone(),
        value => serde_json::to_string(value).expect("scan finding value is JSON"),
    };
    if evidence.excerpt_sha256.is_none() {
        let excerpt = match semantic_value.as_ref().unwrap_or(&value) {
            Value::String(text) => Some(text.clone()),
            value => serde_json::to_string(value).ok(),
        };
        evidence.excerpt_sha256 = excerpt.map(|excerpt| sha256_bytes(excerpt.as_bytes()));
    }
    let confidence = evidence.confidence;
    let blocking = status == "blocking";
    let proposed_action = recommendation.map(str::to_string);
    ScanFinding {
        id: id.into(),
        category: category.into(),
        key: key.into(),
        value,
        model_excerpt: Some(model_excerpt),
        status: status.into(),
        origin: "deterministic".into(),
        user_value: None,
        confidence,
        blocking,
        proposed_action,
        decision_state: if status == "accepted" {
            "accepted".into()
        } else {
            "pending".into()
        },
        evidence: vec![evidence],
        recommendation: recommendation.map(str::to_string),
    }
}

fn redact_scan_value(value: &mut Value) {
    match value {
        Value::String(text) => *text = crate::security::redact_secrets(text, &[]),
        Value::Array(values) => values.iter_mut().for_each(redact_scan_value),
        Value::Object(values) => values.values_mut().for_each(redact_scan_value),
        _ => {}
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
    fn only_truncated_detector_evidence_marks_a_scan_partial() {
        for advisory in [
            "scan.git.inspection",
            "scan.git.link",
            "scan.git.head_link",
            "scan.git.head_read",
            "scan.sensitive.config",
        ] {
            assert!(
                !scan_conflict_truncated_detector_evidence(advisory),
                "{advisory} should stay a review conflict without truncating the targeted scan"
            );
        }

        for truncated in [
            "scan.file_limit",
            "scan.depth.limit",
            "scan.read_error.AGENTS.md",
            "scan.managed_lock_read",
            "scan.git.limit",
            "scan.git.file_limit",
            "scan.cancelled",
            "scan.timeout",
        ] {
            assert!(
                scan_conflict_truncated_detector_evidence(truncated),
                "{truncated} should mark detector evidence partial"
            );
        }
    }

    #[test]
    fn launcher_discovery_reports_duplicate_matching_registrations() {
        let parent = tempdir().unwrap();
        let project = parent.path().join("example");
        fs::create_dir(&project).unwrap();
        let canonical_project = crate::paths::validate_project_root_or_destination(&project)
            .unwrap()
            .0;
        let project_path = crate::paths::user_facing_path(&canonical_project).replace('\\', "\\\\");
        let descriptor = format!("name=\"Example\"\npath=\"{project_path}\"\n");
        fs::write(parent.path().join("other.mod"), &descriptor).unwrap();
        fs::write(parent.path().join("example.mod"), &descriptor).unwrap();

        let error = discover_launcher_descriptor(&project).unwrap_err();
        assert!(error.to_string().contains("multiple launcher descriptors"));
        assert!(error.to_string().contains("example.mod"));
        assert!(error.to_string().contains("other.mod"));
    }

    #[test]
    fn launcher_discovery_reports_ambiguous_noncanonical_names() {
        let parent = tempdir().unwrap();
        let project = parent.path().join("example");
        fs::create_dir(&project).unwrap();
        let canonical_project = crate::paths::validate_project_root_or_destination(&project)
            .unwrap()
            .0;
        let project_path = crate::paths::user_facing_path(&canonical_project).replace('\\', "\\\\");
        let descriptor = format!("name=\"Example\"\npath=\"{project_path}\"\n");
        fs::write(parent.path().join("first.mod"), &descriptor).unwrap();
        fs::write(parent.path().join("second.mod"), &descriptor).unwrap();

        assert!(discover_launcher_descriptor(&project)
            .unwrap_err()
            .to_string()
            .contains("multiple launcher descriptors"));
    }

    #[test]
    fn launcher_discovery_reports_a_mismatched_canonical_registration() {
        let parent = tempdir().unwrap();
        let project = parent.path().join("example");
        fs::create_dir(&project).unwrap();
        let other = parent.path().join("other");
        let other_path = other.display().to_string().replace('\\', "\\\\");
        fs::write(
            parent.path().join("example.mod"),
            format!("name=\"Example\"\npath=\"{other_path}\"\n"),
        )
        .unwrap();

        let error = discover_launcher_descriptor(&project)
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("example.mod tells the HOI4 launcher to load this mod from"),
            "{error}"
        );
        assert!(error.contains(&other.display().to_string()), "{error}");
        assert!(
            error.contains("continue without the launcher file"),
            "{error}"
        );
    }

    #[test]
    fn launcher_discovery_accepts_the_relative_user_folder_form() {
        let user = tempdir().unwrap();
        let mods = user.path().join("mod");
        let project = mods.join("example");
        fs::create_dir_all(&project).unwrap();
        fs::write(
            mods.join("example.mod"),
            "name=\"Example\"\npath=\"mod/example\"\n",
        )
        .unwrap();
        let found = discover_launcher_descriptor(&project).unwrap().unwrap();
        assert!(found.ends_with("example.mod"), "{found:?}");
    }

    #[test]
    fn launcher_discovery_reports_candidate_count_truncation() {
        let parent = tempdir().unwrap();
        let project = parent.path().join("example");
        fs::create_dir(&project).unwrap();
        for index in 0..=MAX_LAUNCHER_CANDIDATES {
            fs::write(
                parent.path().join(format!("candidate-{index:04}.mod")),
                "name=\"Candidate\"\npath=\"/not-the-selected-project\"\n",
            )
            .unwrap();
        }

        let error = discover_launcher_descriptor(&project).unwrap_err();
        assert!(error
            .to_string()
            .contains("more than 512 launcher descriptor candidates"));
    }

    #[test]
    fn launcher_discovery_honors_cancellation_during_parent_enumeration() {
        let parent = tempdir().unwrap();
        let project = parent.path().join("example");
        fs::create_dir(&project).unwrap();
        for index in 0..32 {
            fs::write(
                parent.path().join(format!("candidate-{index:02}.mod")),
                "name=\"Candidate\"\npath=\"/not-the-selected-project\"\n",
            )
            .unwrap();
        }
        let mut checks = 0_usize;

        let error = discover_launcher_descriptor_with_check(&project, || {
            checks += 1;
            checks > 4
        })
        .unwrap_err();

        assert!(error.to_string().contains("launcher discovery stopped"));
        assert!(
            checks <= 8,
            "launcher cancellation should be observed promptly"
        );
    }

    #[cfg(unix)]
    #[test]
    fn launcher_candidate_enumeration_is_handle_bound_across_a_swap_back() {
        let container = tempdir().unwrap();
        let approved = container.path().join("approved-parent");
        let moved = container.path().join("approved-parent-moved");
        let outside = container.path().join("outside-parent");
        fs::create_dir(&approved).unwrap();
        fs::create_dir(&outside).unwrap();
        fs::write(approved.join("approved.mod"), "name=\"Approved\"\n").unwrap();
        fs::write(outside.join("outside.mod"), "name=\"Outside\"\n").unwrap();
        let retained = crate::flatten::BoundedReadRoot::open(&approved).unwrap();

        fs::rename(&approved, &moved).unwrap();
        std::os::unix::fs::symlink(&outside, &approved).unwrap();
        let names = launcher_candidate_names(&retained, || false).unwrap();
        fs::remove_file(&approved).unwrap();
        fs::rename(&moved, &approved).unwrap();

        assert_eq!(names, vec![OsString::from("approved.mod")]);
    }

    #[cfg(unix)]
    #[test]
    fn launcher_approval_rejects_a_parent_swap_after_discovery() {
        let container = tempdir().unwrap();
        let parent = container.path().join("approved-parent");
        let moved = container.path().join("approved-parent-moved");
        let outside = container.path().join("outside-parent");
        let project = parent.join("example");
        fs::create_dir_all(&project).unwrap();
        fs::create_dir_all(outside.join("example")).unwrap();
        let project_path = crate::paths::user_facing_path(
            &crate::paths::validate_project_root_or_destination(&project)
                .unwrap()
                .0,
        )
        .replace('\\', "\\\\");
        let launcher = parent.join("example.mod");
        fs::write(
            &launcher,
            format!("name=\"Approved\"\npath=\"{project_path}\"\n"),
        )
        .unwrap();
        fs::write(
            outside.join("example.mod"),
            format!("name=\"Outside\"\npath=\"{project_path}\"\n"),
        )
        .unwrap();
        let discovered = discover_launcher_descriptor_bound_with_check(&project, || false)
            .unwrap()
            .unwrap();

        fs::rename(&parent, &moved).unwrap();
        std::os::unix::fs::symlink(&outside, &parent).unwrap();
        let result = bind_discovered_launcher_descriptor(&project, &launcher, discovered, || false);
        fs::remove_file(&parent).unwrap();
        fs::rename(&moved, &parent).unwrap();

        assert!(matches!(result, Err(AppError::PathSecurity(_))));
    }

    #[test]
    fn scan_is_read_only_and_detects_core_surfaces() {
        let directory = tempdir().unwrap();
        fs::write(
            directory.path().join("descriptor.mod"),
            "name=\"Example\"\n",
        )
        .unwrap();
        fs::create_dir_all(directory.path().join(".agents/skills/example")).unwrap();
        fs::write(
            directory.path().join(".agents/skills/example/SKILL.md"),
            "---\nname: example\ndescription: Example skill.\n---\n",
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
            .any(|finding| finding.id == "skill.inventory"));
        assert!(result
            .findings
            .iter()
            .flat_map(|finding| &finding.evidence)
            .all(|evidence| evidence.excerpt_sha256.is_some()));
        for finding in &result.findings {
            let excerpt = match &finding.value {
                Value::String(text) => text.clone(),
                value => serde_json::to_string(value).unwrap(),
            };
            let expected = sha256_bytes(excerpt.as_bytes());
            assert!(finding
                .evidence
                .iter()
                .all(|evidence| evidence.excerpt_sha256.as_deref() == Some(expected.as_str())));
        }
        assert!(!result.partial);
    }

    fn shared_walk_project() -> tempfile::TempDir {
        let directory = tempdir().unwrap();
        let put = |path: &str, text: &str| {
            let path = directory.path().join(path);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, text).unwrap();
        };
        put(
            "descriptor.mod",
            "name=\"Shared\"\nsupported_version=\"1.19.*\"\n",
        );
        put("README.md", "# Shared walk\nOne enumeration per folder.\n");
        put("AGENTS.md", "# Agents\n");
        put("docs/guide.md", "# Guide\nKeep namespaces short.\n");
        put("docs/notes/style.md", "# Style\n");
        put(
            ".claude/skills/example/SKILL.md",
            "---\nname: example\ndescription: Example\n---\n",
        );
        put(
            "events/shared.txt",
            "add_namespace = shared\ncountry_event = { id = SHR.1 }\n",
        );
        put(
            "common/national_focus/SHR.txt",
            "focus_tree = { id = SHR_focus }\n",
        );
        put(
            "localisation/english/shared_l_english.yml",
            "l_english:\n SHR.1.t:0 \"Title\"\n",
        );
        put("node_modules/pkg/index.js", "ignored");
        put(".env", "API_KEY=never-read");
        directory
    }

    #[test]
    fn one_walk_gives_the_same_profile_as_a_profile_only_walk() {
        let directory = shared_walk_project();
        let result = scan_project(directory.path(), &ScanOptions::default()).unwrap();
        let alone = crate::project_profile::scan(
            &crate::flatten::BoundedReadRoot::open(directory.path()).unwrap(),
            &|| false,
            &mut |_, _| {},
        )
        .unwrap();
        let shared = result
            .findings
            .iter()
            .filter(|finding| {
                finding.id.starts_with("profile.") && finding.id != "profile.detected"
            })
            .map(|finding| (finding.id.clone(), finding.value.clone()))
            .collect::<BTreeMap<_, _>>();
        let separate = alone
            .findings
            .iter()
            .map(|finding| (finding.id.clone(), finding.value.clone()))
            .collect::<BTreeMap<_, _>>();
        assert!(!shared.is_empty());
        assert_eq!(shared, separate);
        let stats = result.profile_stats.as_ref().unwrap();
        assert_eq!(stats.files_indexed, alone.stats.files_indexed);
        assert_eq!(
            stats.files_indexed, 8,
            "dot-folders and excluded entries stay out"
        );
        assert_eq!(stats.directories_indexed, alone.stats.directories_indexed);
        // The setup inventory still sees only setup surfaces.
        let detected = |id: &str| result.findings.iter().any(|finding| finding.id == id);
        assert!(detected("descriptor.name"));
        assert!(detected("skill.inventory"));
        assert!(!result.partial, "{:?}", result.limits_hit);
        assert!(result
            .conflicts
            .iter()
            .any(|conflict| conflict.id == "scan.sensitive.excluded"));
        assert!(!serde_json::to_string(&result)
            .unwrap()
            .contains("never-read"));
    }

    #[test]
    fn a_setup_inventory_limit_leaves_the_profile_complete() {
        let directory = shared_walk_project();
        let options = ScanOptions {
            max_files: 1,
            ..ScanOptions::default()
        };
        let result = scan_project(directory.path(), &options).unwrap();
        assert!(result.limits_hit.iter().any(|id| id == "scan.file_limit"));
        assert!(result.files_scanned <= 1);
        // The profile keeps its own budgets: every non-excluded file is
        // indexed below the setup depth and file limits.
        let stats = result.profile_stats.as_ref().unwrap();
        assert_eq!(stats.files_indexed, 8);
        assert!(stats.samples_read > 0);
    }

    #[test]
    fn a_setup_depth_limit_does_not_stop_the_profile_from_descending() {
        let directory = shared_walk_project();
        let options = ScanOptions {
            max_depth: 0,
            ..ScanOptions::default()
        };
        let result = scan_project(directory.path(), &options).unwrap();
        assert!(result
            .conflicts
            .iter()
            .any(|conflict| conflict.id.starts_with("scan.depth.")));
        assert!(!result
            .findings
            .iter()
            .any(|finding| finding.id == "skill.inventory"
                && finding.value["count"].as_u64().unwrap_or(0) > 0));
        assert_eq!(result.profile_stats.as_ref().unwrap().files_indexed, 8);
    }

    #[test]
    fn cancellation_during_the_shared_walk_stops_both_inventories() {
        let directory = shared_walk_project();
        let cancel = Arc::new(AtomicBool::new(false));
        let options = ScanOptions {
            cancel_flag: Some(cancel.clone()),
            ..ScanOptions::default()
        };
        let mut setup_files_at_cancel = None;
        let result = scan_project_with_progress(directory.path(), &options, |update| {
            // Cancel once the walk has recorded its first setup file.
            if update.stage == "discovering_files" && update.files_scanned >= 1 {
                cancel.store(true, Ordering::SeqCst);
                setup_files_at_cancel.get_or_insert(update.files_scanned);
            }
        })
        .unwrap();
        assert!(result.cancelled);
        assert!(result.limits_hit.iter().any(|id| id == "scan.cancelled"));
        assert_eq!(Some(result.files_scanned), setup_files_at_cancel);
        assert!(
            result.profile_stats.is_none(),
            "a cancelled walk has no profile"
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_linked_folder_in_a_shared_directory_is_refused_by_both_inventories() {
        let directory = shared_walk_project();
        let outside = tempdir().unwrap();
        fs::write(outside.path().join("outside.md"), "# Outside\n").unwrap();
        std::os::unix::fs::symlink(outside.path(), directory.path().join("docs/linked")).unwrap();
        let result = scan_project(directory.path(), &ScanOptions::default()).unwrap();
        assert!(result
            .conflicts
            .iter()
            .any(|conflict| conflict.id.starts_with("scan.link.") && conflict.severity == "block"));
        assert!(result
            .conflicts
            .iter()
            .any(|conflict| conflict.id == "scan.profile.linked_entry"));
        assert!(!serde_json::to_string(&result).unwrap().contains("Outside"));
    }

    #[test]
    fn profile_limits_truncate_the_profile_without_blocking_scan_approval() {
        let directory = tempdir().unwrap();
        fs::write(
            directory.path().join("descriptor.mod"),
            "name=\"Budget\"\nsupported_version=\"1.19.*\"\n",
        )
        .unwrap();
        for index in 0..110 {
            let folder = directory.path().join(format!("common/category_{index:03}"));
            fs::create_dir_all(&folder).unwrap();
            fs::write(folder.join("entry.txt"), "example = { }\n").unwrap();
        }
        let result = scan_project(directory.path(), &ScanOptions::default()).unwrap();
        assert!(
            !result.partial,
            "a deterministic profile budget must not block approval"
        );
        assert!(result.conflicts.iter().any(
            |conflict| conflict.id == "scan.profile.group_limit" && conflict.severity == "warn"
        ));
        assert!(!result
            .limits_hit
            .iter()
            .any(|id| id.starts_with("scan.profile.")));
        assert_eq!(result.profile_stats.as_ref().unwrap().coverage, "truncated");
        assert!(scan_conflict_truncated_detector_evidence("scan.timeout"));
        assert!(scan_conflict_truncated_detector_evidence("scan.cancelled"));
        assert!(!scan_conflict_truncated_detector_evidence(
            "scan.profile.linked_entry"
        ));
    }

    #[test]
    fn client_markers_are_observed_by_presence_and_unrelated_configs_never_block() {
        let directory = tempdir().unwrap();
        let root = directory.path();
        let write = |relative: &str, content: &str| {
            let path = root.join(relative);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, content).unwrap();
        };
        write(
            "descriptor.mod",
            "name=\"Clients\"\nsupported_version=\"1.19.*\"\n",
        );
        write(
            ".vscode/mcp.json",
            "{\n  // VS Code comment\n  \"servers\": {},\n}\n",
        );
        write(".vscode/settings.json", "not json at all");
        write("GEMINI.md", "@./AGENTS.md\n");
        write(
            ".continue/config.yaml",
            "apiKey: presence-only-sentinel-value\n",
        );
        write(".kiro/settings/mcp.json", "{broken");
        let result = scan_project(root, &ScanOptions::default()).unwrap();
        assert!(!result.partial);
        let detected = result
            .findings
            .iter()
            .find(|finding| finding.id == "coding.environments")
            .map(|finding| finding.value["detected"].clone())
            .unwrap();
        for client in ["github_copilot", "gemini_cli", "continue", "kiro"] {
            assert!(
                detected.as_array().unwrap().iter().any(|id| id == client),
                "{client} not detected: {detected}"
            );
        }
        assert!(!result
            .conflicts
            .iter()
            .any(|conflict| conflict.path == ".vscode/mcp.json"));
        let kiro = result
            .conflicts
            .iter()
            .find(|conflict| conflict.id == "conflict.kiro.config.parse")
            .unwrap();
        assert_eq!(kiro.severity, "warn");
        assert!(!serde_json::to_string(&result)
            .unwrap()
            .contains("presence-only-sentinel-value"));
    }

    #[cfg(unix)]
    #[test]
    fn a_linked_presence_only_marker_warns_without_making_the_scan_partial() {
        let directory = tempdir().unwrap();
        let root = directory.path();
        fs::write(root.join("descriptor.mod"), "name=\"Linked\"\n").unwrap();
        fs::write(root.join("AGENTS.md"), "# Rules\n").unwrap();
        std::os::unix::fs::symlink("AGENTS.md", root.join("GEMINI.md")).unwrap();
        let result = scan_project(root, &ScanOptions::default()).unwrap();
        assert!(!result.partial);
        let link = result
            .conflicts
            .iter()
            .find(|conflict| conflict.path == "GEMINI.md")
            .unwrap();
        assert_eq!(link.severity, "warn");
        assert!(link.id.starts_with("client.marker_link."));
    }

    #[test]
    fn installed_subagents_that_require_fork_context_false_in_instructions_are_valid() {
        let directory = tempdir().unwrap();
        fs::create_dir_all(directory.path().join(".codex/agents")).unwrap();
        fs::write(
            directory.path().join(".codex/agents/explorer.toml"),
            "name = 'explorer'\ndeveloper_instructions = '''\nExplore.\n\nThe parent must spawn this subagent with fork_context=false.'''\n",
        )
        .unwrap();
        fs::write(
            directory.path().join(".codex/agents/inherits.toml"),
            "name = 'inherits'\nfork_context = true\ndeveloper_instructions = 'fork_context=false'\n",
        )
        .unwrap();

        let result = scan_project(directory.path(), &ScanOptions::default()).unwrap();
        let blocked = |path: &str| {
            result.conflicts.iter().any(|conflict| {
                conflict.id.starts_with("conflict.subagent.") && conflict.path == path
            })
        };

        assert!(!blocked(".codex/agents/explorer.toml"));
        assert!(blocked(".codex/agents/inherits.toml"));
    }

    #[test]
    fn subagent_requires_a_parsed_top_level_fork_context_false() {
        let directory = tempdir().unwrap();
        fs::create_dir_all(directory.path().join(".codex/agents")).unwrap();
        fs::write(
            directory.path().join(".codex/agents/reviewer.toml"),
            "name = \"reviewer\"\n# fork_context=false\n",
        )
        .unwrap();

        let result = scan_project(directory.path(), &ScanOptions::default()).unwrap();

        assert!(result.conflicts.iter().any(|conflict| {
            conflict.id.starts_with("conflict.subagent.")
                && conflict.path == ".codex/agents/reviewer.toml"
                && conflict.severity == "block"
        }));
    }

    #[test]
    fn secret_shaped_finding_values_are_redacted_before_leaving_the_scanner() {
        let directory = tempdir().unwrap();
        let secret = ["msy", "_scannerSecretValue123456789"].concat();
        fs::write(
            directory.path().join("descriptor.mod"),
            format!("name=\"MESHY_API_KEY={secret}\"\n"),
        )
        .unwrap();

        let serialized = serde_json::to_string(
            &scan_project(directory.path(), &ScanOptions::default()).unwrap(),
        )
        .unwrap();

        assert!(!serialized.contains(&secret));
        assert!(serialized.contains("[REDACTED]"));
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
            .any(|conflict| conflict.path == "<credential-shaped project paths>"));
        let serialized = serde_json::to_string(&result).unwrap();
        assert!(!serialized.contains("\".env\""));
        assert!(!serialized.contains("credentials/token.txt"));
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
            directory.path().join("AGENTS.md"),
            "# Project instructions\n",
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
        fs::create_dir_all(directory.path().join(".codex/agents")).unwrap();
        fs::write(
            directory.path().join(".codex/agents/example.toml"),
            "name = \"example\"\ndescription = \"Example subagent\"\n",
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
    fn default_inventory_budget_covers_the_documented_very_large_profile() {
        let options = ScanOptions::default();
        assert_eq!(options.max_files, 150_000);
        assert_eq!(options.max_depth, 64);
        assert_eq!(MAX_DIRECTORIES, 200_000);
    }

    #[test]
    fn filesystem_identity_fence_detects_directory_replacement() {
        let parent = tempdir().unwrap();
        let project = parent.path().join("project");
        fs::create_dir(&project).unwrap();
        let identity = scan_path_identity(&project).unwrap();

        fs::rename(&project, parent.path().join("original-project")).unwrap();
        fs::create_dir(&project).unwrap();

        assert!(!scan_path_identity_matches(&project, identity));
    }

    #[test]
    fn binary_art_is_outside_the_targeted_inventory() {
        let directory = tempdir().unwrap();
        let descriptor = b"name=\"Example\"\n";
        fs::write(directory.path().join("descriptor.mod"), descriptor).unwrap();
        let texture = fs::File::create(directory.path().join("large-texture.dds")).unwrap();
        texture.set_len(MAX_TOTAL_BYTES + 1).unwrap();

        let result = scan_project(directory.path(), &ScanOptions::default()).unwrap();

        assert_eq!(result.files_scanned, 1);
        assert_eq!(result.bytes_read, descriptor.len() as u64);
        assert!(!result.partial);
        assert!(!result
            .limits_hit
            .iter()
            .any(|limit| limit.starts_with("scan.bytes_limit")));
    }

    #[test]
    fn unrelated_root_data_files_are_outside_the_targeted_inventory() {
        let directory = tempdir().unwrap();
        let descriptor = b"name=\"Example\"\n";
        fs::write(directory.path().join("descriptor.mod"), descriptor).unwrap();
        fs::File::create(directory.path().join("content-dump.json"))
            .unwrap()
            .set_len(MAX_TOTAL_BYTES + 1)
            .unwrap();
        fs::File::create(directory.path().join("gameplay-dump.txt"))
            .unwrap()
            .set_len(MAX_TOTAL_BYTES + 1)
            .unwrap();

        let result = scan_project(directory.path(), &ScanOptions::default()).unwrap();

        assert_eq!(result.files_scanned, 1);
        assert_eq!(result.bytes_read, descriptor.len() as u64);
        assert!(!result.partial);
    }

    #[test]
    fn detector_relevant_oversized_text_still_returns_an_honest_partial_result() {
        let directory = tempdir().unwrap();
        fs::create_dir_all(directory.path().join(".agents/skills/oversized")).unwrap();
        let script =
            fs::File::create(directory.path().join(".agents/skills/oversized/SKILL.md")).unwrap();
        script.set_len(MAX_TEXT_BYTES + 1).unwrap();

        let result = scan_project(directory.path(), &ScanOptions::default()).unwrap();

        assert!(result.partial);
        assert!(result
            .limits_hit
            .iter()
            .any(|limit| limit.starts_with("scan.bytes_limit")));
    }

    #[test]
    fn oversized_approved_launcher_descriptor_returns_an_honest_partial_result() {
        let parent = tempdir().unwrap();
        let project = parent.path().join("project");
        fs::create_dir(&project).unwrap();
        let launcher = parent.path().join("project.mod");
        fs::File::create(&launcher)
            .unwrap()
            .set_len(MAX_LAUNCHER_DESCRIPTOR_BYTES + 1)
            .unwrap();

        let result = scan_project(
            &project,
            &ScanOptions {
                approved_external_descriptor: Some(launcher),
                ..ScanOptions::default()
            },
        )
        .unwrap();

        assert!(result.partial);
        assert!(result
            .limits_hit
            .iter()
            .any(|limit| limit == "scan.external_descriptor_approval"));
    }

    #[test]
    fn approved_launcher_descriptor_must_still_point_to_the_selected_project() {
        let parent = tempdir().unwrap();
        let project = parent.path().join("project");
        let other = parent.path().join("other");
        fs::create_dir(&project).unwrap();
        fs::create_dir(&other).unwrap();
        let launcher = parent.path().join("project.mod");
        fs::write(
            &launcher,
            format!(
                "name=\"Project\"\npath=\"{}\"\n",
                other.display().to_string().replace('\\', "\\\\")
            ),
        )
        .unwrap();

        let result = scan_project(
            &project,
            &ScanOptions {
                approved_external_descriptor: Some(launcher),
                ..ScanOptions::default()
            },
        )
        .unwrap();

        assert!(result.conflicts.iter().any(|conflict| {
            conflict.id == "scan.external_descriptor_approval" && conflict.severity == "block"
        }));
    }

    #[test]
    fn forged_launcher_path_is_rejected_even_when_it_registers_the_selected_project() {
        let parent = tempdir().unwrap();
        let outside = tempdir().unwrap();
        let project = parent.path().join("project");
        fs::create_dir(&project).unwrap();
        let descriptor = format!(
            "name=\"Project\"\npath=\"{}\"\n",
            project.display().to_string().replace('\\', "\\\\")
        );
        fs::write(parent.path().join("project.mod"), &descriptor).unwrap();
        let forged = outside.path().join("forged.mod");
        fs::write(&forged, descriptor).unwrap();

        let result = scan_project(
            &project,
            &ScanOptions {
                approved_external_descriptor: Some(forged.clone()),
                ..ScanOptions::default()
            },
        )
        .unwrap();

        assert!(result.conflicts.iter().any(|conflict| {
            conflict.id == "scan.external_descriptor_approval" && conflict.severity == "block"
        }));
        assert!(!result
            .findings
            .iter()
            .any(|finding| finding.id == "descriptor.launcher"));
    }

    #[test]
    fn a_lock_for_every_coding_client_stays_within_the_scan_bound() {
        // A realistic record shape for the largest selection: canonical
        // skills, the shared copy, three client copies, and every projection.
        let record = serde_json::json!({
            "path": ".kiro/skills/hoi4-decisions-missions/templates/formable_state_puzzle/formable_state_puzzle_scripted_localisation.txt",
            "location_scope": "project", "component_id": "runtime.kiro.skills",
            "source_path": ".claude/skills/hoi4-decisions-missions/templates/formable_state_puzzle/formable_state_puzzle_scripted_localisation.txt",
            "source_revision": "a".repeat(40), "source_sha256": "b".repeat(64), "source_size": 4096,
            "base_sha256": "c".repeat(64), "installed_sha256": "d".repeat(64), "installed_size": 4096,
            "ownership": "managed", "preserved_local": false, "external": false, "executable": false
        });
        let bytes = serde_json::to_vec_pretty(&vec![record; 6_000]).unwrap();
        assert!(
            (bytes.len() as u64) < MAX_INSTALLATION_LOCK_BYTES / 4,
            "{}",
            bytes.len()
        );
    }

    #[test]
    fn oversized_managed_lock_returns_an_honest_partial_result() {
        let directory = tempdir().unwrap();
        let metadata = directory.path().join(".hoi4-mod-setup");
        fs::create_dir(&metadata).unwrap();
        fs::File::create(metadata.join("install.lock.json"))
            .unwrap()
            .set_len(MAX_INSTALLATION_LOCK_BYTES + 1)
            .unwrap();

        let result = scan_project(directory.path(), &ScanOptions::default()).unwrap();

        assert!(result.partial);
        assert!(result
            .limits_hit
            .iter()
            .any(|limit| limit == "scan.managed_lock_bytes_limit"));
    }

    #[test]
    fn generated_documentation_assets_are_not_inventoried_or_read() {
        let directory = tempdir().unwrap();
        let descriptor = b"name=\"Example\"\n";
        fs::write(directory.path().join("descriptor.mod"), descriptor).unwrap();
        fs::create_dir_all(directory.path().join("docs/assets")).unwrap();
        let generated =
            fs::File::create(directory.path().join("docs/assets/catalog.json")).unwrap();
        generated.set_len(MAX_TOTAL_BYTES + 1).unwrap();

        let result = scan_project(directory.path(), &ScanOptions::default()).unwrap();

        assert_eq!(result.files_scanned, 1);
        assert_eq!(result.bytes_read, descriptor.len() as u64);
        assert!(!result.partial);
    }

    #[test]
    fn elapsed_scan_deadline_returns_a_terminal_partial_result() {
        let directory = tempdir().unwrap();
        fs::write(
            directory.path().join("descriptor.mod"),
            "name=\"Example\"\n",
        )
        .unwrap();
        let mut progress = Vec::new();

        let result = scan_project_with_progress(
            directory.path(),
            &ScanOptions {
                max_duration: Duration::ZERO,
                ..ScanOptions::default()
            },
            |update| progress.push(update),
        )
        .unwrap();

        assert!(result.partial);
        assert!(!result.cancelled);
        assert!(result
            .limits_hit
            .iter()
            .any(|limit| limit == "scan.timeout"));
        assert_eq!(
            progress.last().map(|update| update.stage.as_str()),
            Some("timeout")
        );
    }

    #[test]
    fn detector_policy_is_limited_to_agentic_setup_surfaces() {
        let thumbnail = scan_content_policy("thumbnail.png").unwrap();
        let descriptor = scan_content_policy("descriptor.mod").unwrap();
        let skill = scan_content_policy(".agents/skills/example/SKILL.md").unwrap();
        let subagent = scan_content_policy(".codex/agents/example.toml").unwrap();

        assert_eq!(thumbnail.max_bytes, MAX_THUMBNAIL_BYTES);
        assert!(thumbnail.retain);
        assert_eq!(descriptor.max_bytes, MAX_LAUNCHER_DESCRIPTOR_BYTES);
        assert!(skill.retain);
        assert!(subagent.retain);
        assert!(scan_content_policy("events/example.txt").is_none());
        assert!(scan_content_policy("localisation/english/example_l_english.yml").is_none());
        assert!(scan_content_policy("gfx/models/tank.dds").is_none());
        assert!(scan_content_policy("docs/assets/generated.json").is_none());
        assert!(scan_content_policy("docs/formables/generated.json").is_none());
        assert!(scan_content_policy(".agents/skills/example/references/large.md").is_none());
        assert!(scan_content_policy(".codex/agents/archive/example.toml").is_none());
        assert!(scan_content_policy("docs/guides/archive/large.md").is_none());
        assert!(!targeted_scan_directory(
            ".agents/skills/example/references"
        ));
        assert!(!targeted_scan_directory("docs/guides/archive"));
    }

    #[test]
    fn absolute_path_detector_covers_windows_slashes_and_unc_routes() {
        let detector = absolute_path_regex();
        for absolute in [
            r"C:\mods\example",
            "C:/mods/example",
            r"\\server\share\example",
            "//server/share/example",
            "/Users/example/mod",
        ] {
            assert!(
                detector.is_match(absolute),
                "missed absolute path: {absolute}"
            );
        }
        assert!(!detector.is_match("docs/example.md"));
    }

    #[test]
    fn skill_frontmatter_check_accepts_valid_variants_and_rejects_malformed_blocks() {
        for valid in [
            "---\nname: example\ndescription: Example skill.\n---\nBody",
            "\u{feff}---\r\nname: example\r\ndescription: Example skill.\r\n---\r\n",
            "---\n\"description\": 'Quoted keys work.'\n\"name\": \"example\"\n---\n",
            "---\nname: example\ndescription: >\n  Folded text\n  continues.\nmetadata:\n  type: user\n---\n",
        ] {
            assert!(skill_frontmatter_is_valid(valid), "rejected {valid:?}");
        }
        for invalid in [
            "name: example\ndescription: Missing opening delimiter.\n---\n",
            "---\nname: example\ndescription: Unterminated block.\n",
            "---\nname: example\nname: again\ndescription: Duplicate keys.\n---\n",
            "---\nname: \"\"\ndescription: Empty name.\n---\n",
            "---\nname: example\ndescription: >\n---\n",
            "---\nname: example\n---\ndescription: Outside the block.\n",
            "---\n  name: nested\n  description: Not top-level.\n---\n",
        ] {
            assert!(!skill_frontmatter_is_valid(invalid), "accepted {invalid:?}");
        }
    }

    #[test]
    fn crlf_skill_frontmatter_is_valid() {
        let observations = vec![FileObservation {
            relative: ".agents/skills/example/SKILL.md".into(),
            bytes: b"---\r\nname: example\r\ndescription: Example skill.\r\n---\r\n".to_vec(),
            is_symlink: false,
        }];
        let mut findings = Vec::new();
        let mut conflicts = Vec::new();

        detect_agentic_files(&observations, &mut findings, &mut conflicts);

        assert!(!conflicts
            .iter()
            .any(|conflict| conflict.id.starts_with("conflict.skill.")));
    }

    #[test]
    fn unicode_case_collisions_are_bounded() {
        let mut observations = Vec::new();
        for index in 0..=MAX_SCAN_CONFLICTS {
            observations.push(FileObservation {
                relative: format!("docs/{index}/ÄGENT.md"),
                bytes: Vec::new(),
                is_symlink: false,
            });
            observations.push(FileObservation {
                relative: format!("docs/{index}/ägent.md"),
                bytes: Vec::new(),
                is_symlink: false,
            });
        }
        let mut conflicts = Vec::new();

        append_case_collision_conflicts(&observations, &mut conflicts);

        assert_eq!(conflicts.len(), MAX_SCAN_CONFLICTS);
        assert!(conflicts
            .iter()
            .any(|conflict| conflict.id == "scan.conflict_limit"));
    }

    #[test]
    fn bounded_scan_detects_all_native_coding_environment_packages() {
        let directory = tempdir().unwrap();
        let write = |relative: &str, content: &str| {
            let path = directory.path().join(relative);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).unwrap();
            }
            fs::write(path, content).unwrap();
        };
        let json = r#"{"mcpServers":{}}"#;
        write(".codex/config.toml", "model = \"default\"\n");
        write(".codex/agents/reviewer.toml", "name = \"reviewer\"\n");
        write("CLAUDE.md", "# Claude\n");
        write(".mcp.json", json);
        write(".claude/settings.json", json);
        write(".claude/agent-map.md", "# Claude agents\n");
        write(".claude/agents/reviewer.md", "# Reviewer\n");
        write(".cursor/settings.json", json);
        write(".cursor/mcp.json", json);
        write(".cursor/agent-map.md", "# Cursor agents\n");
        write(".cursor/agents/reviewer.md", "# Reviewer\n");
        write(".qoder/settings.json", json);
        write(".qoder/mcp.json", json);
        write(".qoder/agent-map.md", "# Qoder agents\n");
        write(".qoder/agents/reviewer.md", "# Reviewer\n");
        write("opencode.json", json);
        write(".opencode/settings.json", json);
        write(".opencode/mcp.json", json);
        write(".opencode/agent-map.md", "# OpenCode agents\n");
        write(".opencode/agent/reviewer.md", "# Reviewer\n");

        let result = scan_project(directory.path(), &ScanOptions::default()).unwrap();
        let finding = result
            .findings
            .iter()
            .find(|finding| finding.id == "coding.environments")
            .expect("coding environment finding");
        let detected = finding.value["detected"]
            .as_array()
            .expect("detected environment list")
            .iter()
            .filter_map(Value::as_str)
            .collect::<Vec<_>>();
        for environment in ["codex", "claude_code", "cursor", "qoder", "opencode"] {
            assert!(detected.contains(&environment), "missing {environment}");
        }
        assert!(!result.partial);
        assert!(!result
            .limits_hit
            .iter()
            .any(|limit| limit == "scan.file_limit"));

        write(".cursor/settings.json", "not json");
        let invalid = scan_project(directory.path(), &ScanOptions::default()).unwrap();
        assert!(invalid
            .conflicts
            .iter()
            .any(|conflict| conflict.id == "conflict.cursor.config.parse"));
    }

    #[test]
    fn shared_canonical_agents_do_not_imply_a_codex_environment() {
        let directory = tempdir().unwrap();
        fs::create_dir_all(directory.path().join(".codex/agents")).unwrap();
        fs::write(
            directory.path().join(".codex/agents/shared.toml"),
            "name = \"shared\"\n",
        )
        .unwrap();

        let result = scan_project(directory.path(), &ScanOptions::default()).unwrap();
        let finding = result
            .findings
            .iter()
            .find(|finding| finding.id == "coding.environments")
            .expect("coding environment finding");
        let detected = finding.value["detected"]
            .as_array()
            .expect("detected environment list")
            .iter()
            .filter_map(Value::as_str)
            .collect::<Vec<_>>();
        assert!(!detected.contains(&"codex"));
    }

    #[test]
    fn unrelated_gameplay_assets_are_not_inventoried() {
        let directory = tempdir().unwrap();
        for index in 0..20_100 {
            fs::File::create(directory.path().join(format!("texture-{index:05}.dds"))).unwrap();
        }

        let result = scan_project(directory.path(), &ScanOptions::default()).unwrap();

        assert_eq!(result.files_scanned, 0);
        assert_eq!(result.bytes_read, 0);
        assert!(!result.partial);
        assert!(!result
            .limits_hit
            .iter()
            .any(|limit| limit == "scan.file_limit"));
    }

    #[test]
    fn irrelevant_paths_do_not_consume_the_targeted_entry_budget() {
        let relevant = (0..=MAX_DIRECTORY_ENTRIES)
            .map(|index| format!("events/event-{index:05}.txt"))
            .chain(std::iter::once("descriptor.mod".into()))
            .filter(|path| targeted_scan_path_candidate(path))
            .collect::<Vec<_>>();

        assert_eq!(relevant, vec!["descriptor.mod"]);
    }

    #[test]
    fn directory_budget_is_applied_after_irrelevant_names_are_filtered() {
        let directory = tempdir().unwrap();
        for index in 0..32 {
            fs::File::create(directory.path().join(format!("texture-{index:03}.dds"))).unwrap();
        }
        fs::write(
            directory.path().join("descriptor.mod"),
            "name=\"Example\"\n",
        )
        .unwrap();

        let retained = crate::flatten::BoundedReadRoot::open(directory.path()).unwrap();
        let enumeration = enumerate_targeted_directory(
            &retained,
            "",
            &ScanOptions::default(),
            0,
            Instant::now() + Duration::from_secs(5),
            1,
        )
        .unwrap();

        assert_eq!(enumeration.stop, DirectoryEnumerationStop::Complete);
        assert_eq!(enumeration.entries, vec![OsString::from("descriptor.mod")]);
    }

    #[test]
    fn linked_setup_files_never_become_positive_detector_facts() {
        let observations = vec![
            FileObservation {
                relative: "AGENTS.md".into(),
                bytes: Vec::new(),
                is_symlink: true,
            },
            FileObservation {
                relative: "docs/guide.md".into(),
                bytes: Vec::new(),
                is_symlink: true,
            },
            FileObservation {
                relative: ".agents/skills/example/SKILL.md".into(),
                bytes: Vec::new(),
                is_symlink: true,
            },
            FileObservation {
                relative: ".codex/config.toml".into(),
                bytes: Vec::new(),
                is_symlink: true,
            },
            FileObservation {
                relative: ".cursor/agents/example.md".into(),
                bytes: Vec::new(),
                is_symlink: true,
            },
        ];
        let mut findings = Vec::new();
        let mut conflicts = Vec::new();

        detect_documentation(&observations, &mut findings);
        detect_agentic_files(&observations, &mut findings, &mut conflicts);
        detect_coding_environments(&observations, &mut findings, &mut conflicts);

        let value = |id: &str| {
            findings
                .iter()
                .find(|finding| finding.id == id)
                .map(|finding| finding.value.clone())
                .unwrap()
        };
        assert_eq!(value("documentation.inventory"), json!(0));
        assert_eq!(value("codex.agents"), json!(false));
        assert_eq!(value("skill.inventory")["count"], json!(0));
        assert_eq!(value("codex.config"), json!(false));
        assert_eq!(value("coding.environments")["detected"], json!([]));
    }

    #[cfg(unix)]
    #[test]
    fn unix_backslashes_are_not_separators() {
        assert!(!targeted_scan_path_candidate(r"docs\guide.md"));
    }

    #[cfg(unix)]
    #[test]
    fn case_colliding_setup_paths_are_blocking_when_filesystem_preserves_case() {
        let directory = tempdir().unwrap();
        let upper = directory.path().join("AGENTS.md");
        let lower = directory.path().join("agents.md");
        fs::write(&upper, "# One\n").unwrap();
        fs::write(&lower, "# Two\n").unwrap();

        if fs::read_to_string(&upper).unwrap() != "# One\n"
            || fs::read_to_string(&lower).unwrap() != "# Two\n"
        {
            return;
        }

        let result = scan_project(directory.path(), &ScanOptions::default()).unwrap();

        assert!(result.conflicts.iter().any(|conflict| {
            conflict.id.starts_with("scan.case_collision.") && conflict.severity == "block"
        }));
    }

    #[cfg(unix)]
    #[test]
    fn retained_enumeration_is_handle_bound_across_a_swap_back() {
        let directory = tempdir().unwrap();
        let approved = directory.path().join("approved");
        let moved = directory.path().join("approved-moved");
        let outside = directory.path().join("outside");
        fs::create_dir(&approved).unwrap();
        fs::create_dir(&outside).unwrap();
        fs::write(approved.join("AGENTS.md"), "# Approved\n").unwrap();
        fs::write(outside.join("descriptor.mod"), "name=\"Outside\"\n").unwrap();
        let retained = crate::flatten::BoundedReadRoot::open(&approved).unwrap();

        fs::rename(&approved, &moved).unwrap();
        std::os::unix::fs::symlink(&outside, &approved).unwrap();
        let result = enumerate_targeted_directory(
            &retained,
            "",
            &ScanOptions::default(),
            0,
            Instant::now() + Duration::from_secs(5),
            MAX_DIRECTORY_ENTRIES,
        )
        .unwrap();
        fs::remove_file(&approved).unwrap();
        fs::rename(&moved, &approved).unwrap();

        assert_eq!(result.entries, vec![OsString::from("AGENTS.md")]);
    }

    #[cfg(windows)]
    #[test]
    fn retained_enumeration_prevents_directory_replacement_on_windows() {
        let directory = tempdir().unwrap();
        let approved = directory.path().join("approved");
        let moved = directory.path().join("approved-moved");
        fs::create_dir(&approved).unwrap();
        fs::write(approved.join("descriptor.mod"), "name=\"Approved\"\n").unwrap();
        let retained = crate::flatten::BoundedReadRoot::open(&approved).unwrap();

        assert!(fs::rename(&approved, &moved).is_err());
        let result = enumerate_targeted_directory(
            &retained,
            "",
            &ScanOptions::default(),
            0,
            Instant::now() + Duration::from_secs(5),
            MAX_DIRECTORY_ENTRIES,
        )
        .unwrap();

        assert_eq!(result.stop, DirectoryEnumerationStop::Complete);
        assert_eq!(result.entries, vec![OsString::from("descriptor.mod")]);
    }

    #[test]
    fn sensitive_irrelevant_paths_are_bounded_without_truncating_the_scan() {
        let directory = tempdir().unwrap();
        fs::write(
            directory.path().join("descriptor.mod"),
            "name=\"Example\"\n",
        )
        .unwrap();
        for index in 0..96 {
            fs::write(
                directory.path().join(format!("secret-{index:03}.key")),
                "not read",
            )
            .unwrap();
        }

        let result = scan_project(directory.path(), &ScanOptions::default()).unwrap();
        let sensitive = result
            .conflicts
            .iter()
            .filter(|conflict| conflict.id.starts_with("scan.sensitive."))
            .count();

        assert_eq!(sensitive, 1);
        let conflict = result
            .conflicts
            .iter()
            .find(|conflict| conflict.id == "scan.sensitive.excluded")
            .unwrap();
        assert_eq!(conflict.path, "<credential-shaped project paths>");
        let serialized = serde_json::to_string(&result).unwrap();
        assert!(!serialized.contains("secret-000.key"));
        assert!(!serialized.contains("secret-095.key"));
        assert_eq!(result.files_scanned, 1);
        assert!(!result.partial);
    }

    #[test]
    #[ignore = "requires HOI4_MOD_SETUP_SCAN_FIXTURE to point at an approved external mod"]
    fn external_very_large_mod_fixture_completes_without_mutation() {
        let root = std::env::var_os("HOI4_MOD_SETUP_SCAN_FIXTURE")
            .map(PathBuf::from)
            .expect("HOI4_MOD_SETUP_SCAN_FIXTURE is required");

        let before_guard = scanner_mutation_guard_entries(&root);
        let scan_started = Instant::now();
        let mut last_stage = String::new();
        let result = scan_project_with_progress(&root, &ScanOptions::default(), |progress| {
            if progress.stage != last_stage {
                eprintln!(
                    "stage={} elapsed={:?}",
                    progress.stage,
                    scan_started.elapsed()
                );
                last_stage = progress.stage;
            }
        })
        .unwrap();
        let after_guard = scanner_mutation_guard_entries(&root);

        eprintln!(
            "files={} directories={} detector_bytes={} partial={} limits={:?}",
            result.files_scanned,
            result.directories_scanned,
            result.bytes_read,
            result.partial,
            result.limits_hit
        );
        if let Some(git) = result
            .findings
            .iter()
            .find(|finding| finding.id == "git.repository")
        {
            eprintln!("git={}", git.value);
        }
        assert!(result.files_scanned <= MAX_FILES as u32);
        assert!(result.files_scanned > 0);
        assert!(
            !result.partial,
            "large fixture hit limits: {:?}",
            result.limits_hit
        );
        assert!(!result.cancelled);
        assert!(result.read_only);
        assert_eq!(
            before_guard, after_guard,
            "scan changed app-owned project metadata"
        );
    }

    fn scanner_mutation_guard_entries(root: &Path) -> Vec<String> {
        let mut starts = vec![root.join(".hoi4-mod-setup")];
        if let Ok(entries) = fs::read_dir(root) {
            starts.extend(entries.flatten().filter_map(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .to_ascii_lowercase()
                    .contains("hoi4-mod-setup-scan")
                    .then(|| entry.path())
            }));
        }
        metadata_tree_entries(root, starts)
    }

    fn metadata_tree_entries(root: &Path, mut pending: Vec<PathBuf>) -> Vec<String> {
        let mut entries = Vec::new();
        while let Some(path) = pending.pop() {
            let Ok(metadata) = fs::symlink_metadata(&path) else {
                continue;
            };
            let relative = relative_path(root, &path);
            let modified = metadata
                .modified()
                .ok()
                .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|duration| duration.as_nanos())
                .unwrap_or_default();
            entries.push(format!(
                "{}:{}:{}:{}",
                relative,
                metadata.len(),
                modified,
                is_link_metadata(&metadata)
            ));
            if !metadata.is_dir() || is_link_metadata(&metadata) {
                continue;
            }
            let directory = path;
            let Ok(children) = fs::read_dir(&directory) else {
                continue;
            };
            for child in children.flatten() {
                pending.push(child.path());
            }
        }
        entries.sort();
        entries
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
        lock.primary_coding_environment = "cursor".into();
        lock.additional_coding_environments = vec!["qoder".into()];
        fs::write(
            metadata.join("install.lock.json"),
            serde_json::to_vec(&lock).unwrap(),
        )
        .unwrap();

        let result = scan_project(directory.path(), &ScanOptions::default()).unwrap();
        let schema: Value =
            serde_json::from_str(include_str!("../../docs/schemas/scan-result.schema.json"))
                .unwrap();
        let validator = jsonschema::draft202012::new(&schema).unwrap();
        let mut encoded = serde_json::to_value(&result).unwrap();
        if encoded["platform"] == "unsupported" {
            encoded["platform"] = Value::String("macos".into());
        }
        validator
            .validate(&encoded)
            .expect("managed scan result must satisfy the authoritative schema");
        let managed = result
            .findings
            .iter()
            .find(|finding| finding.id == "installation.managed")
            .expect("managed setup finding");
        assert_eq!(managed.status, "accepted");
        assert_eq!(managed.value["present"], true);
        assert_eq!(managed.value["valid"], true);
        assert_eq!(managed.value["workflow_super_events_state"], "not_selected");
        // Machine-local portrait routes never appear in the model-visible
        // managed summary; they are only in the local-only finding.
        let managed_text = managed.value.to_string();
        assert!(!managed_text.contains("runpod_workspace"));
        assert!(!managed_text.contains("local_root"));
        let routes = result
            .findings
            .iter()
            .find(|finding| finding.id == LOCAL_ONLY_PORTRAIT_ROUTES_FINDING)
            .expect("local-only portrait route finding");
        assert_eq!(
            routes.value["runpod_workspace"],
            "/workspace/comfyui-hoi4-portraits"
        );
        let coding = result
            .findings
            .iter()
            .find(|finding| finding.id == "coding.environments")
            .expect("coding environments finding");
        assert_eq!(coding.value["primary"], "cursor");
        assert_eq!(coding.value["additional"], json!(["qoder"]));
        assert!(coding.value["detected"]
            .as_array()
            .is_some_and(|detected| detected.iter().any(|value| value == "cursor")
                && detected.iter().any(|value| value == "qoder")));
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
