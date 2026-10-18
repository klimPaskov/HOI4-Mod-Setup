//! Read-only project profile: bounded inventory bookkeeping and deterministic
//! text samples turned into review findings. Every host walks its own tree
//! (the desktop with link-safe handles, the web edition over a dropped folder)
//! and shares [`Profiler`], so both produce the same findings.

use crate::adapters::{EntryKind, ProjectFs};
use crate::models::{ProfileStats, ScanEvidence, ScanFinding};
use crate::text::{canonical_relative_key, redact_secrets, sha256_bytes};
use crate::AppError;
use regex::Regex;
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::OnceLock;

pub const MAX_FILES: u32 = 500_000;
pub const MAX_DIRECTORIES: u32 = 200_000;
pub const MAX_ENTRIES: usize = 50_000;
pub const MAX_NAMES_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_GROUPS: usize = 96;
pub const SAMPLES_PER_GROUP: usize = 4;
pub const MAX_SAMPLES: u32 = 256;
pub const PREFIX_BYTES: usize = 32 * 1024;
pub const MAX_SAMPLE_BYTES: u64 = 8 * 1024 * 1024;

#[derive(Default)]
pub struct ProfileResult {
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

pub fn excluded(name: &str) -> bool {
    let lower = name.to_lowercase();
    lower.starts_with('.')
        || sensitive_scan_path(name)
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

pub fn extension(path: &str) -> String {
    Path::new(path)
        .extension()
        .and_then(|v| v.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
}

pub fn sample_group(path: &str) -> Option<String> {
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

pub fn clean_text(value: &str, limit: usize) -> String {
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

pub fn compact_model_value(value: &Value, array_limit: usize) -> Value {
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

pub fn add_finding(
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
pub fn assignments(text: &str) -> Vec<(String, String)> {
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

pub fn top_counts(values: BTreeMap<String, u32>) -> Vec<Value> {
    let mut values = values.into_iter().collect::<Vec<_>>();
    values.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    values
        .into_iter()
        .take(24)
        .map(|(value, files)| json!({"value": clean_text(&value, 128), "sample_files": files}))
        .collect()
}

pub fn count_observation(counts: &mut BTreeMap<String, u32>, value: String) {
    let key = if counts.contains_key(&value) || counts.len() < 256 {
        value
    } else {
        "[other values]".into()
    };
    *counts.entry(key).or_default() += 1;
}

pub fn sensitive_scan_path(relative: &str) -> bool {
    let normalized = relative.replace('\\', "/").to_lowercase();
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
    let stem = name.split('.').next().unwrap_or_default();
    if matches!(
        stem,
        "password"
            | "passwords"
            | "api-key"
            | "api_key"
            | "apikey"
            | "credentials"
            | "secret"
            | "secrets"
            | "private-key"
            | "private_key"
    ) {
        return true;
    }
    name == ".env"
        || name.starts_with(".env.")
        || name.ends_with(".pem")
        || name.ends_with(".key")
        || name.ends_with(".p12")
        || name.ends_with(".pfx")
}

/// Reads a sample: at most the first `PREFIX_BYTES` of a file and its full size.
pub type SampleReader<'a> = dyn FnMut(&str) -> Result<(Vec<u8>, u64), AppError> + 'a;

/// The outcome of recording one regular file during a walk.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileRecord {
    /// The file was counted; report progress when `true`.
    Recorded(bool),
    /// The file limit was reached; the walk stops.
    Limit,
}

/// Bounded profile bookkeeping shared by every host's directory walk, and the
/// sample analysis that turns the chosen samples into findings.
pub struct Profiler {
    pub result: ProfileResult,
    groups: BTreeMap<String, Group>,
    roots: BTreeMap<String, u32>,
    naming: BTreeMap<String, u32>,
}

impl Default for Profiler {
    fn default() -> Self {
        Self::new()
    }
}

impl Profiler {
    pub fn new() -> Self {
        Self {
            result: ProfileResult::default(),
            groups: BTreeMap::new(),
            roots: BTreeMap::new(),
            naming: BTreeMap::new(),
        }
    }

    pub fn problem(&mut self, code: &'static str) {
        self.result.problems.insert(code);
    }

    /// Whether the walk must stop: cancelled, or a hard limit was reached.
    pub fn should_stop(&mut self, cancel: &dyn Fn() -> bool) -> bool {
        if cancel() {
            self.result.problems.insert("stopped");
        }
        !self.result.problems.is_disjoint(&BTreeSet::from([
            "stopped",
            "file_limit",
            "directory_limit",
        ]))
    }

    /// Account for entering a directory; `false` when the walk must not
    /// descend into it.
    pub fn enter_directory(&mut self, depth: usize) -> bool {
        if depth > 64 {
            self.result.problems.insert("depth_limit");
            return false;
        }
        if self.result.stats.directories_indexed >= MAX_DIRECTORIES {
            self.result.problems.insert("directory_limit");
            return false;
        }
        self.result.stats.directories_indexed += 1;
        true
    }

    /// Whether an entry is excluded from the profile; excluded entries are
    /// counted.
    pub fn exclude(&mut self, name: &str) -> bool {
        if excluded(name) {
            self.result.stats.excluded_entries =
                self.result.stats.excluded_entries.saturating_add(1);
            true
        } else {
            false
        }
    }

    pub fn record_root_directory(&mut self, name: &str) {
        if self.roots.len() < MAX_GROUPS {
            self.roots.entry(name.to_owned()).or_default();
        }
    }

    pub fn record_file(&mut self, relative: &str, name: &str, size: u64) -> FileRecord {
        if self.result.stats.files_indexed >= MAX_FILES {
            self.result.problems.insert("file_limit");
            return FileRecord::Limit;
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
        if let Some(group_name) = sample_group(relative) {
            if self.groups.contains_key(&group_name) || self.groups.len() < MAX_GROUPS {
                let group = self.groups.entry(group_name).or_default();
                group.files += 1;
                group.bytes = group.bytes.saturating_add(size);
                if relative.len() > 512 {
                    self.result.problems.insert("sample_path_limit");
                    return FileRecord::Recorded(false);
                }
                let rank = if relative.eq_ignore_ascii_case("README.md") {
                    String::new()
                } else {
                    sha256_bytes(relative.as_bytes())
                };
                group.candidates.insert((rank, relative.to_owned()));
                if group.candidates.len() > SAMPLES_PER_GROUP {
                    group.candidates.pop_last();
                }
            } else {
                self.result.problems.insert("group_limit");
            }
        }
        FileRecord::Recorded(self.result.stats.files_indexed % 256 == 0)
    }

    /// Read the scheduled samples through `read` (first `PREFIX_BYTES` and the
    /// full size) and build the profile findings.
    pub fn analyze(
        mut self,
        read: &mut SampleReader<'_>,
        cancel: &dyn Fn() -> bool,
        progress: &mut dyn FnMut(&ProfileStats, &str),
    ) -> Result<ProfileResult, AppError> {
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
        let loc_key = LOC_KEY
            .get_or_init(|| Regex::new(r#"^\s*[A-Za-z0-9_.-]{1,128}:([0-9]*)\s*\""#).unwrap());
        let mut candidates = self
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
                self.result.problems.insert("stopped");
                break;
            }
            if self.result.stats.samples_read >= MAX_SAMPLES
                || self.result.stats.bytes_read >= MAX_SAMPLE_BYTES
            {
                break;
            }
            let (bytes, source_size) = match read(&path) {
                Ok(sample) => sample,
                Err(_) => {
                    self.result.problems.insert("sample_read");
                    continue;
                }
            };
            self.result.stats.samples_read += 1;
            self.result.stats.bytes_read += bytes.len() as u64;
            self.groups
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
            progress(&self.result.stats, &path);
        }
        let mut result = self.result;
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
        let roots = self
            .roots
            .into_iter()
            .map(|(directory, files)| json!({"directory":clean_text(&directory,128),"files":files}))
            .collect::<Vec<_>>();
        let mut categories = self.groups.into_iter().collect::<Vec<_>>();
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
            json!({"root_directories": roots, "content_categories": categories, "filename_separators": self.naming, "sample_encodings": encodings}),
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
}

/// Profile any project tree behind [`ProjectFs`]: a folder dropped into the
/// web edition, or a fake project in tests. The desktop host walks with its
/// own link-safe handles and shares [`Profiler`].
pub fn profile(
    fs: &dyn ProjectFs,
    cancel: &dyn Fn() -> bool,
    progress: &mut dyn FnMut(&ProfileStats, &str),
) -> Result<ProfileResult, AppError> {
    let mut profiler = Profiler::new();
    walk(fs, &mut profiler, "", 0, cancel, progress);
    profiler.analyze(
        &mut |path| fs.read_prefix(path, PREFIX_BYTES),
        cancel,
        progress,
    )
}

fn walk(
    fs: &dyn ProjectFs,
    profiler: &mut Profiler,
    parent: &str,
    depth: usize,
    cancel: &dyn Fn() -> bool,
    progress: &mut dyn FnMut(&ProfileStats, &str),
) {
    if profiler.should_stop(cancel) || !profiler.enter_directory(depth) {
        return;
    }
    let Ok(entries) = fs.list(parent) else {
        profiler.problem("directory_read");
        return;
    };
    let mut kept = Vec::new();
    let mut name_bytes = 0;
    for entry in entries {
        let name = entry.path.rsplit('/').next().unwrap_or_default().to_owned();
        if profiler.exclude(&name) {
            continue;
        }
        if name.len() > 255 {
            profiler.problem("name_limit");
            continue;
        }
        name_bytes += name.len();
        if kept.len() >= MAX_ENTRIES || name_bytes > MAX_NAMES_BYTES {
            profiler.problem("entry_limit");
            break;
        }
        kept.push((name, entry));
    }
    let mut collision_keys = BTreeSet::new();
    for (name, entry) in kept {
        if profiler.should_stop(cancel) {
            break;
        }
        if entry.path.len() > 4096 {
            profiler.problem("path_limit");
            continue;
        }
        let Ok(key) = canonical_relative_key(&name) else {
            profiler.problem("name_limit");
            continue;
        };
        if !collision_keys.insert(key) {
            profiler.problem("case_collision");
            continue;
        }
        match entry.kind {
            EntryKind::Link => profiler.problem("linked_entry"),
            EntryKind::Directory => {
                if parent.is_empty() {
                    profiler.record_root_directory(&name);
                }
                walk(fs, profiler, &entry.path, depth + 1, cancel, progress);
            }
            EntryKind::File => match profiler.record_file(&entry.path, &name, entry.size) {
                FileRecord::Limit => break,
                FileRecord::Recorded(true) => progress(&profiler.result.stats, &entry.path),
                FileRecord::Recorded(false) => {}
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory_fs::MemoryFs;

    fn sample_mod() -> MemoryFs {
        let mut fs = MemoryFs::new();
        fs.add_file("descriptor.mod", b"name=\"Demo\"\n".to_vec())
            .unwrap();
        fs.add_file("README.md", b"# Demo mod\nA divided Europe.\n".to_vec())
            .unwrap();
        fs.add_file(
            "events/demo_events.txt",
            b"add_namespace = demo\ncountry_event = { id = GER_demo.1 }\n".to_vec(),
        )
        .unwrap();
        fs.add_file(
            "localisation/english/demo_l_english.yml",
            b"\xef\xbb\xbfl_english:\n demo.1.t:0 \"Title\"\n".to_vec(),
        )
        .unwrap();
        fs.add_file(".env", b"MESHY_API_KEY=secret".to_vec())
            .unwrap();
        fs.add_file("node_modules/pkg/index.js", b"x".to_vec())
            .unwrap();
        fs
    }

    #[test]
    fn a_dropped_folder_is_profiled_like_a_desktop_project() {
        let fs = sample_mod();
        let mut reported = 0;
        let result = profile(&fs, &|| false, &mut |_, _| reported += 1).unwrap();
        assert_eq!(
            result.stats.files_indexed, 4,
            "excluded entries are not indexed"
        );
        assert_eq!(result.stats.excluded_entries, 2);
        assert_eq!(result.stats.coverage, "sampled");
        let finding = |id: &str| {
            result
                .findings
                .iter()
                .find(|finding| finding.id == format!("profile.{id}"))
                .unwrap()
                .value
                .clone()
        };
        let identifiers = finding("identifiers");
        assert_eq!(identifiers["namespace_declarations"][0]["value"], "demo");
        assert_eq!(identifiers["identifier_prefixes"][0]["value"], "GER");
        let localisation = finding("localisation");
        assert_eq!(localisation["utf8_bom"], 1);
        assert_eq!(localisation["language_headers"][0]["value"], "english");
        assert_eq!(localisation["versioned_quoted_keys"], 1);
        let documentation = finding("documentation");
        assert_eq!(documentation["documents"][0]["headings"][0], "Demo mod");
        assert!(reported >= 3, "each sample reports progress");
        let serialized = serde_json::to_string(&result.findings).unwrap();
        assert!(
            !serialized.contains("secret"),
            "sensitive files are never read"
        );
    }

    #[test]
    fn cancellation_stops_the_walk_and_marks_coverage_truncated() {
        let fs = sample_mod();
        let result = profile(&fs, &|| true, &mut |_, _| {}).unwrap();
        assert!(result.problems.contains("stopped"));
        assert_eq!(result.stats.coverage, "truncated");
        assert_eq!(result.stats.samples_read, 0);
    }
}
