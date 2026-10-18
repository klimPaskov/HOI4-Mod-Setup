//! The profile's side of the desktop project walk. The scanner enumerates
//! each directory once and hands the names to [`ProfileNames`] and
//! [`record_entry`]; bookkeeping, sample analysis, and findings come from the
//! shared core profile.
pub(crate) use hoi4_setup_core::profile::*;
use std::collections::BTreeSet;
use std::ffi::OsStr;
use std::fs::Metadata;

/// The names one directory contributes to the profile, collected while the
/// shared walk enumerates it.
#[derive(Default)]
pub(crate) struct ProfileNames {
    names: Vec<String>,
    name_bytes: usize,
    done: bool,
}

impl ProfileNames {
    /// Accept one enumerated name; `false` once the profile wants no more
    /// names from this directory.
    pub(crate) fn accept(
        &mut self,
        profiler: &mut Profiler,
        entry: Result<&OsStr, ()>,
        stop: &dyn Fn() -> bool,
    ) -> bool {
        if self.done {
            return false;
        }
        if profiler.should_stop(stop) {
            self.done = true;
            return false;
        }
        let Ok(name) = entry else {
            profiler.problem("entry_read");
            return true;
        };
        let Some(name) = name.to_str() else {
            profiler.problem("name_encoding");
            return true;
        };
        if profiler.exclude(name) {
            return true;
        }
        if name.len() > 255 || name.contains('\\') || name.contains('/') {
            profiler.problem("name_limit");
            return true;
        }
        self.name_bytes += name.len();
        if self.names.len() >= MAX_ENTRIES || self.name_bytes > MAX_NAMES_BYTES {
            profiler.problem("entry_limit");
            self.done = true;
            return false;
        }
        self.names.push(name.to_owned());
        true
    }

    pub(crate) fn into_sorted(mut self) -> Vec<String> {
        self.names.sort();
        self.names
    }
}

/// The profile-relative path of an accepted name, or `None` when the path
/// budget, the portable name rules, or a case collision skips it.
pub(crate) fn admit(
    profiler: &mut Profiler,
    collision_keys: &mut BTreeSet<String>,
    parent: &str,
    name: &str,
) -> Option<String> {
    let relative = if parent.is_empty() {
        name.to_owned()
    } else {
        format!("{parent}/{name}")
    };
    if relative.len() > 4096 {
        profiler.problem("path_limit");
        return None;
    }
    // Names legal on one platform but outside the portable path rules (a
    // colon, a trailing dot, a reserved device name) are skipped.
    let Ok(key) = crate::security::canonical_relative_key(name) else {
        profiler.problem("name_limit");
        return None;
    };
    if !collision_keys.insert(key) {
        profiler.problem("case_collision");
        return None;
    }
    Some(relative)
}

/// What the walk does next with an admitted profile entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProfileEntry {
    Skip,
    /// Descend into the directory.
    Directory,
    /// A file was counted; report progress when `true`.
    File(bool),
    /// The file limit was reached; the profile takes no more entries.
    Stop,
}

/// Record one admitted entry from its no-follow metadata. Links are never
/// followed; unreadable metadata is incomplete coverage, never a failed scan.
pub(crate) fn record_entry(
    profiler: &mut Profiler,
    parent: &str,
    name: &str,
    relative: &str,
    metadata: Option<&Metadata>,
) -> ProfileEntry {
    let Some(metadata) = metadata else {
        profiler.problem("metadata_read");
        return ProfileEntry::Skip;
    };
    if crate::security::is_link_metadata(metadata) {
        profiler.problem("linked_entry");
        return ProfileEntry::Skip;
    }
    if metadata.is_dir() {
        if parent.is_empty() {
            profiler.record_root_directory(name);
        }
        return ProfileEntry::Directory;
    }
    if !metadata.is_file() {
        return ProfileEntry::Skip;
    }
    match profiler.record_file(relative, name, metadata.len()) {
        FileRecord::Limit => ProfileEntry::Stop,
        FileRecord::Recorded(report) => ProfileEntry::File(report),
    }
}

/// Profile a tree on its own through the shared walk, with the setup
/// inventory switched off.
#[cfg(test)]
pub(crate) fn scan(
    root: &crate::flatten::BoundedReadRoot,
    cancel: &dyn Fn() -> bool,
    progress: &mut dyn FnMut(&crate::models::ProfileStats, &str),
) -> Result<ProfileResult, crate::AppError> {
    crate::scanner::profile_only(root, cancel, progress)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flatten::BoundedReadRoot;
    use crate::security::{redact_secrets, sha256_bytes};
    use serde_json::Value;
    use std::cell::Cell;
    use std::collections::BTreeMap;
    use std::fs;
    use std::path::Path;

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
