//! An offline copy of the workflow package for planning tests.
//!
//! The checked-in manifest describes the real package, whose files are not in
//! this repository. The fixture keeps the manifest's structure (components,
//! profiles, coding environments, destinations, ownership) and replaces every
//! file's size and SHA-256 evidence with that of deterministic synthetic
//! content, so the planner runs its real selection, download verification and
//! adaptation against bytes that match their evidence.

use crate::models::SourceMode;
use crate::security::sha256_bytes;
use crate::source::{SourceFetcher, SourceRequest};
use crate::AppError;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

/// The commit every fixture request resolves to.
pub(crate) const REVISION: &str = "0123456789abcdef0123456789abcdef01234567";

/// How many files of each component the fixture keeps.
const KEPT_PER_COMPONENT: usize = 4;

const INSTRUCTIONS_TEMPLATE: &str = "# [MOD_NAME] agent guidance\n\nNamespace: [MOD_PREFIX]\nGame: [HOI4_GAME_DIR]\nWorkshop: [STEAM_WORKSHOP_DIR]\n";

/// The release tag the fixture publishes at [`REVISION`].
pub(crate) const RELEASE: &str = "v1.0.0";

pub(crate) struct SourceFixture {
    pub manifest_bytes: Vec<u8>,
    /// Source path to content.
    pub files: BTreeMap<String, Vec<u8>>,
    /// Every file the planner asked for, in order.
    pub fetched: Mutex<Vec<String>>,
    /// How many times the planner asked for a commit.
    pub resolutions: AtomicUsize,
    /// The commit "latest" resolves to. The package is the same at every
    /// commit the fixture knows.
    pub latest: String,
}

fn content_for(path: &str) -> Vec<u8> {
    match path {
        "AGENTS_template.md" | "CLAUDE_template.md" => INSTRUCTIONS_TEMPLATE.as_bytes().to_vec(),
        // Skills and Markdown agents need the frontmatter readiness checks.
        _ if path.ends_with("/SKILL.md") || path.contains("/agents/") && path.ends_with(".md") => {
            format!(
                "---
name: fixture
description: Fixture content for {path}
---
"
            )
            .into_bytes()
        }
        // Claude Code reads this file for the HOI4 tools route.
        ".mcp.json" => {
            br#"{"mcpServers":{"hoi4_agent_tools":{"command":"hoi4-agent-tools","args":["serve"]}}}
"#
            .to_vec()
        }
        _ if path.ends_with(".json") => format!("{{\"fixture\": \"{path}\"}}\n").into_bytes(),
        // Codex agent definitions are rewritten to forbid inherited context.
        _ if path.ends_with(".toml") && path.contains("/agents/") => {
            format!("name = \"{path}\"\ndeveloper_instructions = \"Fixture agent.\"\n").into_bytes()
        }
        _ if path.ends_with(".toml") => format!("fixture = \"{path}\"\n").into_bytes(),
        _ => format!("Fixture content for {path}\n").into_bytes(),
    }
}

impl SourceFixture {
    pub(crate) fn new() -> Self {
        let mut manifest: Value = serde_json::from_slice(include_bytes!(
            "../../../docs/source-manifest/hoi4-mod-setup.manifest.json"
        ))
        .expect("the checked-in manifest is JSON");
        let mut files = BTreeMap::new();
        let required_wiki_pages: Vec<String> = manifest["wiki"]["required_pages"]
            .as_array()
            .expect("the manifest names its required wiki pages")
            .iter()
            .map(|page| format!("paradox_wiki/{}", page.as_str().expect("a page is a name")))
            .collect();
        for component in manifest["components"]
            .as_array_mut()
            .expect("the manifest declares components")
        {
            let Some(expected) = component
                .get_mut("expected_files")
                .and_then(Value::as_array_mut)
            else {
                continue;
            };
            // Large trees (hundreds of skills and wiki pages) keep only their
            // first files, which is enough to exercise every kind of
            // destination and keeps a plan fast to build in a debug test.
            // The wiki keeps the pages the manifest requires.
            let mut kept = 0;
            expected.retain(|file| {
                kept += 1;
                kept <= KEPT_PER_COMPONENT
                    || file["path"]
                        .as_str()
                        .is_some_and(|path| required_wiki_pages.iter().any(|page| page == path))
            });
            for file in expected {
                let path = file["path"]
                    .as_str()
                    .expect("evidence names a path")
                    .to_owned();
                let bytes = files
                    .entry(path.clone())
                    .or_insert_with(|| content_for(&path));
                file["sha256"] = json!(sha256_bytes(bytes));
                file["size"] = json!(bytes.len());
            }
        }
        Self {
            manifest_bytes: serde_json::to_vec_pretty(&manifest).expect("the manifest serializes"),
            files,
            fetched: Mutex::new(Vec::new()),
            resolutions: AtomicUsize::new(0),
            latest: REVISION.into(),
        }
    }

    /// A fixture whose latest commit is `revision`.
    pub(crate) fn with_latest(mut self, revision: &str) -> Self {
        self.latest = revision.into();
        self
    }

    fn knows(&self, revision: &str) -> bool {
        revision == REVISION || revision == self.latest
    }
}

impl SourceFetcher for SourceFixture {
    fn resolve_commit(
        &self,
        request: &SourceRequest,
    ) -> Result<(String, Option<String>, Option<String>), AppError> {
        self.resolutions.fetch_add(1, Ordering::SeqCst);
        match request.mode {
            SourceMode::Latest => Ok((self.latest.clone(), Some("main".into()), None)),
            SourceMode::PinnedCommit if request.requested_ref.as_deref() == Some(REVISION) => {
                Ok((REVISION.into(), None, None))
            }
            SourceMode::PinnedRelease if request.release.as_deref() == Some(RELEASE) => {
                Ok((REVISION.into(), None, Some(RELEASE.into())))
            }
            _ => Err(AppError::Source(
                "the fixture does not publish that revision".into(),
            )),
        }
    }

    fn fetch_manifest(&self, revision: &str) -> Result<Vec<u8>, AppError> {
        if !self.knows(revision) {
            return Err(AppError::Source(format!(
                "unknown fixture revision {revision}"
            )));
        }
        Ok(self.manifest_bytes.clone())
    }

    fn fetch_verified_file(
        &self,
        revision: &str,
        path: &str,
        _expected_sha256: &str,
        _expected_size: Option<u64>,
    ) -> Result<Vec<u8>, AppError> {
        if !self.knows(revision) {
            return Err(AppError::Source(format!(
                "unknown fixture revision {revision}"
            )));
        }
        self.fetched.lock().unwrap().push(path.to_owned());
        // Like a careless host, the fixture does not check the evidence it
        // was given: the planner has to.
        self.files
            .get(path)
            .cloned()
            .ok_or_else(|| AppError::Source(format!("the fixture has no file {path}")))
    }
}

pub(crate) const LOCAL_ENDPOINT: &str = "http://127.0.0.1:11434/v1/chat/completions";

/// The reviewed state of a project that the fixture package plans. `launcher`
/// is the user-level launcher registration the desktop requires for a new
/// project; a server has none.
pub(crate) fn planning_state(
    mode: &str,
    root: &std::path::Path,
    launcher: Option<&std::path::Path>,
    choice: Option<&str>,
) -> Value {
    let mut identity = json!({
        "displayName": "Cold War Curtain",
        "projectId": "cold_war_curtain",
        "author": "Fixture",
        "version": "0.1.0",
        "supportedGameVersion": "1.17.*",
        "projectRoot": root.display().to_string(),
        "defaultBranch": "main",
        "scriptPrefix": "cwc",
        "primaryNamespace": "cwc",
        "descriptorTags": ["Alternative History", "Events"],
    });
    if let Some(launcher) = launcher {
        identity["launcherDescriptorPath"] = json!(launcher.display().to_string());
    }
    let mut state = json!({
        "mode": mode,
        "description": "A Cold War alternate-history mod centered on a divided Europe.",
        "sourceMode": "latest",
        "aiProvider": "local",
        "aiModel": "fixture",
        "aiReasoningEffort": "high",
        "aiEndpoint": LOCAL_ENDPOINT,
        "gitMode": "skip",
        "primaryCodingEnvironment": "claude_code",
        "additionalCodingEnvironments": ["codex"],
        "selectedComponents": [],
        "folderProfile": ["events", "common", "localisation/english", "gfx", "docs"],
        "conventions": {
            "agents_profile": "default",
            "localisation_convention": "english",
            "documentation_convention": "markdown",
        },
        "identity": identity,
    });
    if let Some(choice) = choice {
        state["conflictChoice"] = json!(choice);
    }
    state
}

/// An existing project whose files collide with the package in every way a
/// plan distinguishes: user files the package does not own, user-edited files
/// the package would write, and a file that already equals the package's.
pub(crate) fn populate_existing_project(root: &std::path::Path) {
    let files: [(&str, &[u8]); 8] = [
        (
            "descriptor.mod",
            b"name=\"Cold War Curtain\"\nversion=\"0.0.9\"\nsupported_version=\"1.16.*\"\n",
        ),
        ("README.md", b"# My own readme\n"),
        (".gitignore", b"*.log\n"),
        ("CLAUDE.md", b"# My own instructions\n"),
        ("docs/linux.md", b"Edited by the user.\n"),
        // Equal to the package's copy, so it is not a conflict.
        (
            "docs/runtimes.md",
            b"Fixture content for docs/runtimes.md\n",
        ),
        ("events/cwc_events.txt", b"namespace = cwc\n"),
        ("common/ideas/cwc_ideas.txt", b"ideas = {}\n"),
    ];
    for (path, bytes) in files {
        let path = root.join(path);
        std::fs::create_dir_all(path.parent().expect("a file has a parent")).unwrap();
        std::fs::write(path, bytes).unwrap();
    }
}
