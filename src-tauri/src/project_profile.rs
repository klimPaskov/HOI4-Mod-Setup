//! Read-only project facts from metadata and explicitly bounded text samples.
use crate::flatten::BoundedReadRoot;
use crate::models::{ProfileStats, ScanEvidence, ScanFinding};
use crate::security::{is_link_metadata, redact_secrets, sha256_bytes};
use crate::AppError;
use regex::Regex;
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::OnceLock;

const MAX_FILES: u32 = 500_000;
const MAX_DIRECTORIES: u32 = 200_000;
const MAX_ENTRIES: usize = 50_000;
const MAX_NAMES_BYTES: usize = 8 * 1024 * 1024;
const MAX_GROUPS: usize = 96;
const SAMPLES_PER_GROUP: usize = 4;
const MAX_SAMPLES: u32 = 256;
const PREFIX_BYTES: usize = 32 * 1024;
const MAX_SAMPLE_BYTES: u64 = 8 * 1024 * 1024;

#[derive(Default)]
pub(crate) struct ProfileResult {
    pub stats: ProfileStats,
    pub findings: Vec<ScanFinding>,
    pub problems: BTreeSet<&'static str>,
}

#[derive(Default, Serialize)]
struct Group {
    files: u32,
    bytes: u64,
    sampled: u32,
    #[serde(skip)]
    candidates: BTreeSet<(String, String)>,
}

#[derive(Serialize)]
struct Sample {
    path: String,
    source_size: u64,
    sampled_bytes: usize,
    sample_sha256: String,
    prefix_only: bool,
    encoding: &'static str,
}

struct Collector<'a> {
    result: ProfileResult,
    groups: BTreeMap<String, Group>,
    roots: BTreeMap<String, u32>,
    naming: BTreeMap<String, u32>,
    cancel: &'a dyn Fn() -> bool,
    progress: &'a mut dyn FnMut(&ProfileStats, &str),
}

fn excluded(name: &str) -> bool {
    let lower = name.to_lowercase();
    lower.starts_with('.')
        || crate::scanner::sensitive_scan_path(name)
        || matches!(
            lower.as_str(),
            "node_modules"
                | "target"
                | "dist"
                | "build"
                | "out"
                | "paradox_wiki"
                | "venv"
                | "env"
                | "__pycache__"
                | "cache"
                | "coverage"
        )
        || lower.ends_with(".log")
        || lower.ends_with(".tmp")
        || lower.ends_with(".bak")
}

fn extension(path: &str) -> String {
    Path::new(path)
        .extension()
        .and_then(|v| v.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
}

fn sample_group(path: &str) -> Option<String> {
    let lower = path.to_lowercase();
    let parts = lower.split('/').collect::<Vec<_>>();
    let root = parts[0];
    let ext = extension(path);
    if parts.len() == 1 && matches!(lower.as_str(), "readme.md" | "readme.txt" | "readme.rst") {
        return Some("documentation/root".into());
    }
    if matches!(root, "docs" | "doc" | "documentation")
        && matches!(ext.as_str(), "md" | "rst" | "txt")
    {
        if parts
            .iter()
            .any(|part| matches!(*part, "assets" | "formables"))
        {
            return None;
        }
        return Some(format!(
            "documentation/{}",
            parts.get(1).filter(|_| parts.len() > 2).unwrap_or(&"root")
        ));
    }
    if matches!(root, "localisation" | "localization") && matches!(ext.as_str(), "yml" | "yaml") {
        let stem = Path::new(&lower).file_stem()?.to_str()?;
        let language = stem
            .rsplit_once("_l_")
            .map(|(_, lang)| lang)
            .filter(|lang| {
                lang.len() <= 32 && lang.bytes().all(|c| c.is_ascii_lowercase() || c == b'_')
            })
            .unwrap_or("unknown");
        return Some(format!("localisation/{language}"));
    }
    if matches!(
        root,
        "common" | "history" | "events" | "decisions" | "interface" | "map"
    ) && matches!(ext.as_str(), "txt" | "gui" | "gfx" | "asset")
    {
        let category = if parts.len() > 2 && matches!(root, "common" | "history") {
            parts[1]
        } else {
            "root"
        };
        return Some(format!("{root}/{category}"));
    }
    None
}

impl Collector<'_> {
    fn stop(&mut self) -> bool {
        if (self.cancel)() {
            self.result.problems.insert("stopped");
        }
        !self.result.problems.is_disjoint(&BTreeSet::from([
            "stopped",
            "file_limit",
            "directory_limit",
        ]))
    }

    fn walk(
        &mut self,
        directory: &BoundedReadRoot,
        parent: &str,
        depth: usize,
    ) -> Result<(), AppError> {
        if self.stop() {
            return Ok(());
        }
        if depth > 64 {
            self.result.problems.insert("depth_limit");
            return Ok(());
        }
        if self.result.stats.directories_indexed >= MAX_DIRECTORIES {
            self.result.problems.insert("directory_limit");
            return Ok(());
        }
        self.result.stats.directories_indexed += 1;
        let mut names = Vec::new();
        let mut name_bytes = 0;
        let visited = directory.visit_directory_names(|entry| {
            if self.stop() {
                return false;
            }
            let Ok(name) = entry else {
                self.result.problems.insert("entry_read");
                return true;
            };
            let Some(name) = name.to_str() else {
                self.result.problems.insert("name_encoding");
                return true;
            };
            if excluded(name) {
                self.result.stats.excluded_entries =
                    self.result.stats.excluded_entries.saturating_add(1);
                return true;
            }
            if name.len() > 255 || name.contains('\\') || name.contains('/') {
                self.result.problems.insert("name_limit");
                return true;
            }
            name_bytes += name.len();
            if names.len() >= MAX_ENTRIES || name_bytes > MAX_NAMES_BYTES {
                self.result.problems.insert("entry_limit");
                return false;
            }
            names.push(name.to_owned());
            true
        });
        if visited.is_err() {
            // A folder that vanishes or changes during the walk is incomplete
            // coverage, never a failed scan.
            self.result.problems.insert("directory_read");
            return Ok(());
        }
        names.sort();
        let mut collision_keys = BTreeSet::new();
        for name in names {
            if self.stop() {
                break;
            }
            let relative = if parent.is_empty() {
                name.clone()
            } else {
                format!("{parent}/{name}")
            };
            if relative.len() > 4096 {
                self.result.problems.insert("path_limit");
                continue;
            }
            // Names legal on one platform but outside the portable path rules
            // (a colon, a trailing dot, a reserved device name) are skipped.
            let Ok(key) = crate::security::canonical_relative_key(&name) else {
                self.result.problems.insert("name_limit");
                continue;
            };
            if !collision_keys.insert(key) {
                self.result.problems.insert("case_collision");
                continue;
            }
            let Ok(Ok(metadata)) =
                directory.with_stable_path(|stable| std::fs::symlink_metadata(stable.join(&name)))
            else {
                self.result.problems.insert("metadata_read");
                continue;
            };
            if is_link_metadata(&metadata) {
                self.result.problems.insert("linked_entry");
                continue;
            }
            if metadata.is_dir() {
                if parent.is_empty() && self.roots.len() < MAX_GROUPS {
                    self.roots.entry(name.clone()).or_default();
                }
                match directory.open_child_directory(&name) {
                    Ok(child) => {
                        if self.walk(&child, &relative, depth + 1).is_err() {
                            self.result.problems.insert("directory_read");
                        }
                    }
                    Err(_) => {
                        self.result.problems.insert("directory_read");
                    }
                }
            } else if metadata.is_file() {
                if self.result.stats.files_indexed >= MAX_FILES {
                    self.result.problems.insert("file_limit");
                    break;
                }
                self.result.stats.files_indexed += 1;
                let root = relative.split('/').next().unwrap_or("root");
                if let Some(count) = self.roots.get_mut(root) {
                    *count += 1;
                }
                let style = if name.contains('_') {
                    "underscore"
                } else if name.contains('-') {
                    "hyphen"
                } else if name.contains(' ') {
                    "spaces"
                } else {
                    "other"
                };
                *self.naming.entry(style.into()).or_default() += 1;
                if let Some(group_name) = sample_group(&relative) {
                    if self.groups.contains_key(&group_name) || self.groups.len() < MAX_GROUPS {
                        let group = self.groups.entry(group_name).or_default();
                        group.files += 1;
                        group.bytes = group.bytes.saturating_add(metadata.len());
                        if relative.len() > 512 {
                            self.result.problems.insert("sample_path_limit");
                            continue;
                        }
                        let rank = if relative.eq_ignore_ascii_case("README.md") {
                            String::new()
                        } else {
                            sha256_bytes(relative.as_bytes())
                        };
                        group.candidates.insert((rank, relative.clone()));
                        if group.candidates.len() > SAMPLES_PER_GROUP {
                            group.candidates.pop_last();
                        }
                    } else {
                        self.result.problems.insert("group_limit");
                    }
                }
                if self.result.stats.files_indexed % 256 == 0 {
                    (self.progress)(&self.result.stats, &relative);
                }
            }
        }
        Ok(())
    }
}

pub(crate) fn clean_text(value: &str, limit: usize) -> String {
    static LOCAL: OnceLock<Regex> = OnceLock::new();
    static LINK: OnceLock<Regex> = OnceLock::new();
    static TOKEN: OnceLock<Regex> = OnceLock::new();
    static EMAIL: OnceLock<Regex> = OnceLock::new();
    // Links are replaced first so a URL scheme such as `https:/` is never
    // mistaken for a drive letter. Absolute roots must start a token so a
    // contained relative path such as `gfx/home/` is preserved.
    let link = LINK.get_or_init(|| {
        Regex::new(r"(?i)\b(?:[a-z][a-z0-9+.\-]*://\S+|file:\S+|mailto:\S+)").unwrap()
    });
    let value = link.replace_all(value, "[link]");
    let local = LOCAL.get_or_init(|| {
        Regex::new(
            r"(?i)(?:^|[^a-z0-9_.\-/\\])(?:[a-z]:[\\/]|\\\\|//[a-z0-9]|/(?:[a-z]|cygdrive|users|home|mnt|private|var|tmp|volumes|media|run|opt|root|srv)/|~/)",
        )
        .unwrap()
    });
    if local.is_match(&value) {
        return "[machine-local reference omitted]".into();
    }
    // Credential shapes beyond the shared redaction set: provider tokens,
    // cloud keys, JWTs, and plain secret assignments. The whole piece is
    // omitted so no key name or fragment remains.
    let token = TOKEN.get_or_init(|| {
        Regex::new(
            r"(?i)\b(?:gh[pousr]_[a-z0-9]{20,}|github_pat_[a-z0-9_]{20,}|xox[abprs]-[a-z0-9-]{10,}|akia[0-9a-z]{16}|hf_[a-z0-9]{20,}|eyj[a-z0-9_-]{8,}\.[a-z0-9_-]{8,}\.[a-z0-9_-]{8,})|\b(?:password|passwd|secret|token)\s*[:=]\s*\S+",
        )
        .unwrap()
    });
    if token.is_match(&value) || redact_secrets(&value, &[]) != value {
        return "[credential-shaped text omitted]".into();
    }
    let email = EMAIL
        .get_or_init(|| Regex::new(r"(?i)\b[a-z0-9._%+\-]+@[a-z0-9.\-]+\.[a-z]{2,}\b").unwrap());
    let value = email.replace_all(&value, "[email omitted]");
    let cleaned: String = value.chars().take(limit).collect();
    // Only a redaction fixed point may reach approved evidence.
    if redact_secrets(&cleaned, &[]) == cleaned {
        cleaned
    } else {
        "[credential-shaped text omitted]".into()
    }
}

fn compact_model_value(value: &Value, array_limit: usize) -> Value {
    match value {
        Value::String(text) if text.len() > 256 => {
            let mut end = 244;
            while !text.is_char_boundary(end) {
                end -= 1;
            }
            json!(format!("{} [truncated]", &text[..end]))
        }
        Value::Array(items) => Value::Array(
            items
                .iter()
                .take(array_limit)
                .map(|item| compact_model_value(item, array_limit))
                .collect(),
        ),
        Value::Object(items) => Value::Object(
            items
                .iter()
                .map(|(key, item)| (key.clone(), compact_model_value(item, array_limit)))
                .collect(),
        ),
        value => value.clone(),
    }
}

fn add_finding(
    result: &mut ProfileResult,
    id: &str,
    category: &str,
    value: Value,
    confidence: f32,
) {
    let excerpt = [32, 16, 8, 4, 1].into_iter().find_map(|limit| {
        let mut projected = compact_model_value(&value, limit);
        if projected != value {
            projected["model_projection"] = json!({"array_items_per_field":limit,"text_bytes_per_field":256,"full_details_available_in_review":true});
        }
        let bytes = serde_json::to_vec(&projected).expect("profile value is JSON");
        // Approval re-checks that redaction leaves an excerpt unchanged, so a
        // projection that truncation turned credential-shaped is never offered.
        let stable = std::str::from_utf8(&bytes)
            .is_ok_and(|text| redact_secrets(text, &[]) == text);
        (bytes.len() <= 24 * 1024 && stable).then_some(bytes)
    }).unwrap_or_else(|| br#"{"model_projection":"summary omitted; review details locally"}"#.to_vec());
    result.findings.push(ScanFinding {
        id: format!("profile.{id}"), category: category.into(), key: format!("project_{id}"), value,
        model_excerpt: Some(String::from_utf8(excerpt.clone()).expect("JSON is UTF-8")), status: "needs_review".into(), origin: "deterministic".into(), user_value: None,
        confidence, blocking: false, proposed_action: None, decision_state: "pending".into(),
        evidence: vec![ScanEvidence { detector: "bounded_project_profile".into(), path: format!("@scan/profile/{id}"),
            line_start: None, line_end: None, excerpt_sha256: Some(sha256_bytes(&excerpt)), confidence,
            note: Some("Metadata inventory and deterministic samples; not exhaustive gameplay validation. Sample hashes identify the sampled prefix only.".into()) }],
        recommendation: Some("Preserve observed conventions; review sampled patterns before treating them as project-wide rules.".into()),
    });
}

/// Minimal lexical facts, not a game-script validator. Comments and quoted
/// strings cannot introduce fake assignments; strings are only assignment values.
fn assignments(text: &str) -> Vec<(String, String)> {
    let mut chars = text.chars().peekable();
    let mut tokens: Vec<(String, bool)> = Vec::new();
    while let Some(c) = chars.next() {
        if c.is_whitespace() {
            continue;
        }
        if c == '#' {
            for next in chars.by_ref() {
                if next == '\n' {
                    break;
                }
            }
            continue;
        }
        if c == '"' {
            let mut value = String::new();
            let mut closed = false;
            while let Some(next) = chars.next() {
                if next == '\\' {
                    chars.next();
                    value.push(' ');
                } else if next == '"' {
                    closed = true;
                    break;
                } else if value.len() < 256 {
                    value.push(next);
                }
            }
            if !closed {
                break;
            }
            tokens.push((value, true));
        } else if c.is_ascii_alphanumeric() || c == '_' {
            let mut value = String::from(c);
            while chars.peek().is_some_and(|next| {
                next.is_ascii_alphanumeric() || matches!(*next, '_' | '.' | '-')
            }) {
                value.push(chars.next().unwrap());
            }
            tokens.push((value, false));
        } else {
            tokens.push((c.to_string(), false));
        }
    }
    tokens
        .windows(3)
        .filter(|window| {
            !window[0].1
                && window[1].0 == "="
                && !window[2].0.is_empty()
                && window[2].0.len() <= 128
                && window[2]
                    .0
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'_' | b'.' | b'-'))
        })
        .map(|window| (window[0].0.clone(), window[2].0.clone()))
        .collect()
}

fn top_counts(values: BTreeMap<String, u32>) -> Vec<Value> {
    let mut values = values.into_iter().collect::<Vec<_>>();
    values.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    values
        .into_iter()
        .take(24)
        .map(|(value, files)| json!({"value": clean_text(&value, 128), "sample_files": files}))
        .collect()
}

fn count_observation(counts: &mut BTreeMap<String, u32>, value: String) {
    let key = if counts.contains_key(&value) || counts.len() < 256 {
        value
    } else {
        "[other values]".into()
    };
    *counts.entry(key).or_default() += 1;
}

pub(crate) fn scan(
    root: &BoundedReadRoot,
    cancel: &dyn Fn() -> bool,
    progress: &mut dyn FnMut(&ProfileStats, &str),
) -> Result<ProfileResult, AppError> {
    // A fresh, identity-matched descriptor has its own directory-stream offset.
    let profile_root = BoundedReadRoot::open(root.canonical())?;
    if !profile_root.same_directory_identity(root) {
        return Err(AppError::PathSecurity("profile root changed".into()));
    }
    let mut collector = Collector {
        result: ProfileResult::default(),
        groups: BTreeMap::new(),
        roots: BTreeMap::new(),
        naming: BTreeMap::new(),
        cancel,
        progress,
    };
    collector.walk(&profile_root, "", 0)?;
    let mut samples = Vec::new();
    let mut namespaces = BTreeMap::new();
    let mut prefixes = BTreeMap::new();
    let mut languages = BTreeMap::new();
    let mut localisation = json!({"files_sampled":0,"utf8_bom":0,"utf8_without_bom":0,"non_utf8":0,"versioned_quoted_keys":0,"unversioned_quoted_keys":0});
    let mut documentation = Vec::new();
    let mut encodings: BTreeMap<&str, u32> = BTreeMap::new();
    static LOC_HEADER: OnceLock<Regex> = OnceLock::new();
    static LOC_KEY: OnceLock<Regex> = OnceLock::new();
    let header = LOC_HEADER.get_or_init(|| Regex::new(r"^\s*l_([a-z_]{1,32})\s*:").unwrap());
    let loc_key =
        LOC_KEY.get_or_init(|| Regex::new(r#"^\s*[A-Za-z0-9_.-]{1,128}:([0-9]*)\s*\""#).unwrap());
    let mut candidates = collector
        .groups
        .iter()
        .flat_map(|(name, group)| {
            group
                .candidates
                .iter()
                .enumerate()
                .map(|(round, (_, path))| (round, name.clone(), path.clone()))
        })
        .collect::<Vec<_>>();
    candidates.sort();
    for (_, group_name, path) in candidates {
        if cancel() {
            collector.result.problems.insert("stopped");
            break;
        }
        if collector.result.stats.samples_read >= MAX_SAMPLES
            || collector.result.stats.bytes_read >= MAX_SAMPLE_BYTES
        {
            break;
        }
        let (bytes, source_size) =
            match profile_root.read_prefix_with_check(&path, PREFIX_BYTES, cancel) {
                Ok(sample) => sample,
                Err(_) => {
                    collector.result.problems.insert("sample_read");
                    continue;
                }
            };
        collector.result.stats.samples_read += 1;
        collector.result.stats.bytes_read += bytes.len() as u64;
        collector
            .groups
            .get_mut(&group_name)
            .expect("scheduled group exists")
            .sampled += 1;
        let body = bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(&bytes);
        let text = match std::str::from_utf8(body) {
            Ok(text) => Some(text),
            Err(error) if error.error_len().is_none() && source_size > bytes.len() as u64 => {
                std::str::from_utf8(&body[..error.valid_up_to()]).ok()
            }
            _ => None,
        }
        .filter(|text| !text.contains('\0'));
        let encoding = if text.is_none() {
            "non_utf8_or_binary"
        } else if body.len() != bytes.len() {
            "utf8_bom"
        } else {
            "utf8"
        };
        *encodings.entry(encoding).or_default() += 1;
        samples.push(Sample {
            path: clean_text(&path, 512),
            source_size,
            sampled_bytes: bytes.len(),
            sample_sha256: sha256_bytes(&bytes),
            prefix_only: source_size > bytes.len() as u64,
            encoding,
        });
        if group_name.starts_with("localisation/") {
            for key in [
                "files_sampled",
                match encoding {
                    "utf8_bom" => "utf8_bom",
                    "utf8" => "utf8_without_bom",
                    _ => "non_utf8",
                },
            ] {
                localisation[key] = json!(localisation[key].as_u64().unwrap_or(0) + 1);
            }
            let mut seen = BTreeSet::new();
            for line in text.unwrap_or("").lines() {
                if let Some(capture) = header.captures(line) {
                    seen.insert(capture[1].to_owned());
                }
                if let Some(capture) = loc_key.captures(line) {
                    let key = if capture[1].is_empty() {
                        "unversioned_quoted_keys"
                    } else {
                        "versioned_quoted_keys"
                    };
                    localisation[key] = json!(localisation[key].as_u64().unwrap_or(0) + 1);
                }
            }
            for language in seen.into_iter().take(64) {
                count_observation(&mut languages, language);
            }
        } else if group_name.starts_with("documentation/") {
            if let Some(text) = text.filter(|_| documentation.len() < 24) {
                let mut fenced = false;
                let mut headings = Vec::new();
                let mut overview = String::new();
                for line in text.lines() {
                    let line = line.trim();
                    if line.starts_with("```") || line.starts_with("~~~") {
                        fenced = !fenced;
                        continue;
                    }
                    if fenced
                        || line.is_empty()
                        || line.starts_with("![")
                        || line.starts_with("[![")
                        || line.starts_with("<!--")
                    {
                        continue;
                    }
                    if line.starts_with('#') {
                        if headings.len() < 6 {
                            headings.push(clean_text(line.trim_start_matches('#').trim(), 120));
                        }
                    } else if overview.is_empty() && !line.starts_with('|') {
                        overview = clean_text(line, 600);
                    }
                }
                documentation.push(json!({"path": clean_text(&path, 512), "headings": headings, "overview": overview}));
            }
        } else if let Some(text) = text {
            let mut file_namespaces = BTreeSet::new();
            let mut file_prefixes = BTreeSet::new();
            for (key, value) in assignments(text) {
                if matches!(key.as_str(), "namespace" | "add_namespace") {
                    file_namespaces.insert(value);
                } else if key == "id" {
                    if let Some(prefix) = value.split(['_', '.']).next().filter(|part| {
                        part.len() >= 2
                            && part.len() <= 32
                            && part.bytes().all(|c| c.is_ascii_alphabetic())
                    }) {
                        file_prefixes.insert(prefix.to_owned());
                    }
                }
            }
            for namespace in file_namespaces.into_iter().take(64) {
                count_observation(&mut namespaces, namespace);
            }
            for prefix in file_prefixes.into_iter().take(64) {
                count_observation(&mut prefixes, prefix);
            }
        }
        (collector.progress)(&collector.result.stats, &path);
    }
    let mut result = collector.result;
    result.stats.coverage = if result.problems.is_empty() {
        "sampled"
    } else {
        "truncated"
    }
    .into();
    let stats = serde_json::to_value(&result.stats)?;
    let issues = result.problems.iter().copied().collect::<Vec<_>>();
    add_finding(
        &mut result,
        "coverage",
        "structure",
        json!({"coverage": stats, "sampling":"deterministic path-hash sampling, up to four files per category and 256 files total; first 32 KiB per file", "issues": issues, "samples": samples}),
        1.0,
    );
    let roots = collector
        .roots
        .into_iter()
        .map(|(directory, files)| json!({"directory":clean_text(&directory,128),"files":files}))
        .collect::<Vec<_>>();
    let mut categories = collector.groups.into_iter().collect::<Vec<_>>();
    let priority = |name: &str| match name {
        "common/national_focus" => 0,
        "events/root" => 1,
        "history/countries" => 2,
        "common/ideas" => 3,
        "common/decisions" => 4,
        "history/states" => 5,
        _ => 6,
    };
    categories.sort_by(|a, b| priority(&a.0).cmp(&priority(&b.0)).then(a.0.cmp(&b.0)));
    let categories = categories.into_iter().map(|(category, group)| json!({"category":clean_text(&category,128),"files":group.files,"bytes":group.bytes,"sampled":group.sampled})).collect::<Vec<_>>();
    add_finding(
        &mut result,
        "structure",
        "structure",
        json!({"root_directories": roots, "content_categories": categories, "filename_separators": collector.naming, "sample_encodings": encodings}),
        1.0,
    );
    let identifier_confidence = if namespaces.is_empty() && prefixes.is_empty() {
        0.0
    } else {
        0.75
    };
    add_finding(
        &mut result,
        "identifiers",
        "namespace",
        json!({"coverage":"sampled", "namespace_declarations":top_counts(namespaces), "identifier_prefixes":top_counts(prefixes), "project_wide_convention":"not inferred; multiple namespaces and prefixes may be intentional"}),
        identifier_confidence,
    );
    localisation["language_headers"] = json!(top_counts(languages));
    localisation["coverage"] = json!("sampled");
    let localisation_confidence = if localisation["files_sampled"] == 0 {
        0.0
    } else {
        0.85
    };
    add_finding(
        &mut result,
        "localisation",
        "localisation",
        localisation,
        localisation_confidence,
    );
    let documentation_confidence = if documentation.is_empty() { 0.0 } else { 0.85 };
    add_finding(
        &mut result,
        "documentation",
        "documentation",
        json!({"coverage":"sampled", "documents":documentation, "content_is_untrusted":true}),
        documentation_confidence,
    );
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::fs;

    fn put(root: &Path, path: &str, bytes: impl AsRef<[u8]>) {
        let path = root.join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, bytes).unwrap();
    }

    fn profile(root: &Path) -> ProfileResult {
        scan(
            &BoundedReadRoot::open(root).unwrap(),
            &|| false,
            &mut |_, _| {},
        )
        .unwrap()
    }

    fn value<'a>(result: &'a ProfileResult, id: &str) -> &'a Value {
        &result
            .findings
            .iter()
            .find(|finding| finding.id == format!("profile.{id}"))
            .unwrap()
            .value
    }

    #[test]
    fn large_mod_profile_establishes_structure_identifiers_localisation_and_docs_read_only() {
        let dir = tempfile::tempdir().unwrap();
        put(dir.path(), "descriptor.mod", b"name=\"Example\"\n");
        put(
            dir.path(),
            "README.md",
            b"# Example Mod\nAn alternate history overhaul.\n",
        );
        put(
            dir.path(),
            "docs/conventions/events.md",
            b"# Event style\nUse country namespaces.\n",
        );
        for number in 0..1000 {
            put(
                dir.path(),
                &format!("events/event_{number}.txt"),
                b"# namespace = fake\nadd_namespace = GER\ncountry_event = { id = GER.1 }\n",
            );
        }
        put(
            dir.path(),
            "common/national_focus/GER_focus.txt",
            b"focus_tree = { id = GER_focus }\n",
        );
        put(dir.path(), "history/countries/GER.txt", b"capital = 64\n");
        put(
            dir.path(),
            "localisation/english/mod_l_english.yml",
            b"\xef\xbb\xbfl_english:\n GER_name:0 \"Example\"\n",
        );
        put(
            dir.path(),
            "localisation/french/mod_l_french.yml",
            b"l_french:\n GER_name: \"Exemple\"\n",
        );
        put(dir.path(), "gfx/large.dds", [0, 255, 0, 255]);
        put(dir.path(), ".env", b"API_KEY=do-not-read");
        put(dir.path(), "docs/secrets.txt", b"do-not-read");
        let before = tree_hashes(dir.path());
        let result = profile(dir.path());
        assert!(result.problems.is_empty(), "{:?}", result.problems);
        assert_eq!(result.stats.files_indexed, 1008);
        assert_eq!(result.stats.samples_read, 10);
        assert_eq!(tree_hashes(dir.path()), before);
        assert_eq!(
            value(&result, "structure")["content_categories"][0]["category"],
            "common/national_focus"
        );
        assert!(value(&result, "identifiers")["namespace_declarations"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v["value"] == "GER" && v["sample_files"] == 4));
        let loc = value(&result, "localisation");
        assert_eq!(loc["utf8_bom"], 1);
        assert_eq!(loc["versioned_quoted_keys"], 1);
        assert_eq!(loc["unversioned_quoted_keys"], 1);
        assert_eq!(loc["language_headers"].as_array().unwrap().len(), 2);
        assert!(value(&result, "documentation")
            .to_string()
            .contains("alternate history overhaul"));
        let serialized = serde_json::to_string(&result.findings).unwrap();
        assert!(!serialized.contains("do-not-read"));
        assert!(!serialized.contains("namespace = fake"));
        for finding in &result.findings {
            let excerpt = finding.model_excerpt.as_ref().unwrap();
            assert!(excerpt.len() <= 24 * 1024);
            assert_eq!(
                finding.evidence[0].excerpt_sha256.as_ref().unwrap(),
                &sha256_bytes(excerpt.as_bytes())
            );
        }
        let again = profile(dir.path());
        assert_eq!(
            serde_json::to_value(result.findings).unwrap(),
            serde_json::to_value(again.findings).unwrap()
        );
    }

    fn tree_hashes(root: &Path) -> BTreeMap<String, String> {
        fn walk(root: &Path, path: &Path, values: &mut BTreeMap<String, String>) {
            for entry in fs::read_dir(path).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    walk(root, &path, values);
                } else {
                    values.insert(
                        path.strip_prefix(root).unwrap().display().to_string(),
                        sha256_bytes(&fs::read(path).unwrap()),
                    );
                }
            }
        }
        let mut values = BTreeMap::new();
        walk(root, root, &mut values);
        values
    }

    #[test]
    fn samples_are_fair_bounded_and_prefix_hashes_are_explicit() {
        let dir = tempfile::tempdir().unwrap();
        for group in 0..80 {
            for sample in 0..5 {
                put(
                    dir.path(),
                    &format!("common/group_{group:02}/file_{sample}.txt"),
                    vec![b' '; PREFIX_BYTES + 40],
                );
            }
        }
        let result = profile(dir.path());
        assert!(result.problems.is_empty());
        assert_eq!(result.stats.samples_read, MAX_SAMPLES);
        assert_eq!(result.stats.bytes_read, MAX_SAMPLE_BYTES);
        let categories = value(&result, "structure")["content_categories"]
            .as_array()
            .unwrap();
        assert_eq!(categories.len(), 80);
        assert!(categories
            .iter()
            .all(|item| item["sampled"].as_u64().unwrap() >= 3));
        assert!(value(&result, "coverage")["samples"]
            .as_array()
            .unwrap()
            .iter()
            .all(|item| item["prefix_only"] == true));
        assert!(result.findings.iter().all(|finding| finding
            .model_excerpt
            .as_ref()
            .unwrap()
            .len()
            <= 24 * 1024));
    }

    #[test]
    fn cancellation_discards_complete_coverage_and_does_not_write() {
        let dir = tempfile::tempdir().unwrap();
        put(dir.path(), "events/example.txt", b"namespace = test");
        let before = tree_hashes(dir.path());
        let result = scan(
            &BoundedReadRoot::open(dir.path()).unwrap(),
            &|| true,
            &mut |_, _| {},
        )
        .unwrap();
        assert_eq!(result.stats.coverage, "truncated");
        assert!(result.problems.contains("stopped"));
        assert_eq!(tree_hashes(dir.path()), before);
    }

    #[test]
    fn prefix_reader_limits_bytes_and_checks_cancellation() {
        let dir = tempfile::tempdir().unwrap();
        put(dir.path(), "sample.txt", b"abcdefghij");
        let root = BoundedReadRoot::open(dir.path()).unwrap();
        assert_eq!(
            root.read_prefix_with_check("sample.txt", 4, &|| false)
                .unwrap(),
            (b"abcd".to_vec(), 10)
        );
        assert!(root
            .read_prefix_with_check("sample.txt", 4, &|| true)
            .is_err());
        assert!(root
            .read_prefix_with_check("../sample.txt", 4, &|| false)
            .is_err());
        let calls = Cell::new(0);
        assert!(root
            .read_prefix_with_check("sample.txt", 4, &|| {
                calls.set(calls.get() + 1);
                calls.get() > 1
            })
            .is_err());
        put(dir.path(), "empty.txt", b"");
        assert!(root
            .read_prefix_with_check("empty.txt", 4, &|| true)
            .is_err());
        assert_eq!(
            root.read_prefix_with_check("empty.txt", 4, &|| false)
                .unwrap(),
            (Vec::new(), 0)
        );
        assert!(root
            .read_prefix_with_check("sample.txt", 0, &|| false)
            .is_err());
        assert!(root
            .read_prefix_with_check("sample.txt", 1024 * 1024 + 1, &|| false)
            .is_err());
    }

    #[cfg(unix)]
    #[test]
    fn linked_gameplay_is_not_followed_and_makes_coverage_partial() {
        let dir = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        put(outside.path(), "example.txt", b"namespace = outside");
        std::os::unix::fs::symlink(outside.path(), dir.path().join("events")).unwrap();
        let result = profile(dir.path());
        assert!(result.problems.contains("linked_entry"));
        assert_eq!(result.stats.samples_read, 0);
        assert!(!serde_json::to_string(&result.findings)
            .unwrap()
            .contains("outside"));
    }

    #[test]
    fn lexing_and_redaction_do_not_promote_comments_quotes_or_local_paths() {
        assert_eq!(
            assignments(
                "# namespace = fake\ntext = \"namespace = fake\"\nnamespace = real\nid = \"GER.1\""
            ),
            vec![
                ("namespace".into(), "real".into()),
                ("id".into(), "GER.1".into())
            ]
        );
        assert_eq!(
            clean_text("Read C:\\Users\\private\\file", 600),
            "[machine-local reference omitted]"
        );
        assert_eq!(
            clean_text("See https://example.com/?token=private", 600),
            "See [link]"
        );
        assert_eq!(
            clean_text("Docs: https://example.com/a and http://x.test/b", 600),
            "Docs: [link] and [link]"
        );
        for local in [
            "Copy D:/Games/mod",
            r"Read C:\mods\x",
            r"\\server\share\mod",
            "/home/user/mod",
            "/Users/user/mod",
            "/mnt/c/mod",
            "~/mods/x",
        ] {
            assert_eq!(
                clean_text(local, 600),
                "[machine-local reference omitted]",
                "{local}"
            );
        }
        assert_eq!(clean_text("file:///C:/Users/x", 600), "[link]");
        for relative in [
            "gfx/home/flags.dds",
            "common/users/x.txt",
            "events/mnt_events.txt",
        ] {
            assert_eq!(clean_text(relative, 600), relative);
        }
        assert_eq!(
            clean_text("ratio a:b and key: value", 600),
            "ratio a:b and key: value"
        );
        assert_eq!(clean_text("abcdef", 3), "abc");
        // Every cleaned value is a redaction fixed point, including when the
        // limit cuts a redaction marker, so approval never rejects it later.
        // Synthetic credential shapes are assembled at runtime so the
        // committed-secret scanner never sees a token-shaped literal.
        let tail = "abcdefghijklmnopqrstuvwxyz0123456789";
        let provider = format!("{}-{tail}", "sk");
        let github = format!("{}_{tail}", "ghp");
        for (text, limit) in [
            (format!("Set api_key={provider} in config"), 600),
            (format!("api_key={provider}"), 14),
            (format!("token {github} here"), 20),
        ] {
            let text = text.as_str();
            let cleaned = clean_text(text, limit);
            assert_eq!(redact_secrets(&cleaned, &[]), cleaned, "{text} at {limit}");
            assert!(!cleaned.contains("qrstuvwxyz0123456789"), "{text}");
        }
    }
}
