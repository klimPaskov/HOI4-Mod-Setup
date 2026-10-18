//! The desktop walk for the read-only project profile. Bookkeeping, sample
//! analysis, and findings come from the shared core profile.
use crate::flatten::BoundedReadRoot;
use crate::models::ProfileStats;
use crate::security::is_link_metadata;
use crate::AppError;
pub(crate) use hoi4_setup_core::profile::*;
use std::collections::BTreeSet;

struct Collector<'a> {
    profiler: Profiler,
    cancel: &'a dyn Fn() -> bool,
    progress: &'a mut dyn FnMut(&ProfileStats, &str),
}

impl Collector<'_> {
    fn walk(
        &mut self,
        directory: &BoundedReadRoot,
        parent: &str,
        depth: usize,
    ) -> Result<(), AppError> {
        if self.profiler.should_stop(self.cancel) || !self.profiler.enter_directory(depth) {
            return Ok(());
        }
        let mut names = Vec::new();
        let mut name_bytes = 0;
        let cancel = self.cancel;
        let profiler = &mut self.profiler;
        let visited = directory.visit_directory_names(|entry| {
            if profiler.should_stop(cancel) {
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
            name_bytes += name.len();
            if names.len() >= MAX_ENTRIES || name_bytes > MAX_NAMES_BYTES {
                profiler.problem("entry_limit");
                return false;
            }
            names.push(name.to_owned());
            true
        });
        if visited.is_err() {
            // A folder that vanishes or changes during the walk is incomplete
            // coverage, never a failed scan.
            self.profiler.problem("directory_read");
            return Ok(());
        }
        names.sort();
        let mut collision_keys = BTreeSet::new();
        for name in names {
            if self.profiler.should_stop(self.cancel) {
                break;
            }
            let relative = if parent.is_empty() {
                name.clone()
            } else {
                format!("{parent}/{name}")
            };
            if relative.len() > 4096 {
                self.profiler.problem("path_limit");
                continue;
            }
            // Names legal on one platform but outside the portable path rules
            // (a colon, a trailing dot, a reserved device name) are skipped.
            let Ok(key) = crate::security::canonical_relative_key(&name) else {
                self.profiler.problem("name_limit");
                continue;
            };
            if !collision_keys.insert(key) {
                self.profiler.problem("case_collision");
                continue;
            }
            let Ok(Ok(metadata)) =
                directory.with_stable_path(|stable| std::fs::symlink_metadata(stable.join(&name)))
            else {
                self.profiler.problem("metadata_read");
                continue;
            };
            if is_link_metadata(&metadata) {
                self.profiler.problem("linked_entry");
                continue;
            }
            if metadata.is_dir() {
                if parent.is_empty() {
                    self.profiler.record_root_directory(&name);
                }
                match directory.open_child_directory(&name) {
                    Ok(child) => {
                        if self.walk(&child, &relative, depth + 1).is_err() {
                            self.profiler.problem("directory_read");
                        }
                    }
                    Err(_) => {
                        self.profiler.problem("directory_read");
                    }
                }
            } else if metadata.is_file() {
                match self.profiler.record_file(&relative, &name, metadata.len()) {
                    FileRecord::Limit => break,
                    FileRecord::Recorded(true) => {
                        (self.progress)(&self.profiler.result.stats, &relative)
                    }
                    FileRecord::Recorded(false) => {}
                }
            }
        }
        Ok(())
    }
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
        profiler: Profiler::new(),
        cancel,
        progress,
    };
    collector.walk(&profile_root, "", 0)?;
    let Collector {
        profiler, progress, ..
    } = collector;
    profiler.analyze(
        &mut |path| profile_root.read_prefix_with_check(path, PREFIX_BYTES, cancel),
        cancel,
        progress,
    )
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
