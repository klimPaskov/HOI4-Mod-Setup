//! Tests for the default plan builder, driven through the service facade.
//!
//! The package is the offline fixture, so the planner runs its real manifest
//! validation, file selection, hash verification and adaptation; the session
//! runs its real gates; and the transaction engine applies the plan to a
//! temporary workspace. Nothing here needs the desktop, a credential vault,
//! the network or the user's application-data folders, so the tests also run
//! with `--no-default-features`.

use super::fixture::{self, SourceFixture, LOCAL_ENDPOINT, REVISION};
use super::tests::{analysis_request, evidence_from, provider_reply};
use super::*;
use crate::codex::{CodexAnalysisRequest, CodexAnalysisResult};
use crate::source::{resolve_source, SourceRequest};
use serde_json::json;
use std::fs;
use tempfile::{tempdir, TempDir};

/// A second commit the fixture can publish as "latest".
const OTHER_REVISION: &str = "fedcba9876543210fedcba9876543210fedcba98";

struct Hosted {
    /// Holds the workspace and the state root, which are siblings.
    _container: TempDir,
    workspace: PathBuf,
    state: PathBuf,
    source: Arc<SourceFixture>,
    service: SetupService,
}

/// The host's post-install runner. A real one starts the reviewed installers;
/// this one records that each finished.
fn ready_runner(
    _root: &Path,
    _plan: &InstallationPlan,
    component: &str,
) -> Result<crate::transaction::PostInstallActionOutcome, AppError> {
    Ok(crate::transaction::PostInstallActionOutcome {
        component_id: component.into(),
        state: "ready".into(),
        evidence: "Fixture runner: nothing was started.".into(),
    })
}

/// The Node runtime the tests hand the service. A Linux plan that registers
/// the MCP package in a coding client launches it with Node, and a real
/// runtime must be the distro's `/usr/bin/node`, which a CI runner or a
/// developer machine may not have. This one is never started: only its path
/// appears in the generated client configuration.
fn test_node() -> NodeRuntime {
    NodeRuntime::unverified_for_tests("/opt/hoi4-test/node/bin/node", "/opt/hoi4-test/npm")
}

fn hosted_with(source: SourceFixture) -> Hosted {
    hosted_configured(source, Some(test_node()))
}

fn hosted_configured(source: SourceFixture, node: Option<NodeRuntime>) -> Hosted {
    let container = tempdir().unwrap();
    let base = fs::canonicalize(container.path()).unwrap();
    let workspace = base.join("workspace");
    let state = base.join("state");
    fs::create_dir_all(&workspace).unwrap();
    fs::create_dir_all(&state).unwrap();
    let source = Arc::new(source);
    let service = SetupService::new(Arc::new(DeniedCredentialStore))
        .with_plan_builder(Arc::new(DefaultPlanBuilder::with_source(source.clone())))
        .with_post_install_runner(ready_runner);
    let service = match node {
        Some(node) => service.with_node_runtime(node),
        None => service,
    };
    Hosted {
        _container: container,
        workspace,
        state,
        source,
        service,
    }
}

fn hosted() -> Hosted {
    hosted_with(SourceFixture::new())
}

impl Hosted {
    fn session(&self) -> SetupSession {
        self.service
            .open_session(SessionConfig::new(&self.workspace, &self.state).unwrap())
    }

    fn project(&self) -> PathBuf {
        self.workspace.join("cold_war_curtain")
    }

    fn resolve(&self, request: SourceRequest) -> SourceResolution {
        resolve_source(self.source.as_ref(), &request).unwrap()
    }

    fn latest(&self) -> SourceResolution {
        self.resolve(SourceRequest {
            mode: SourceMode::Latest,
            requested_ref: None,
            release: None,
        })
    }
}

/// Retain the reply as an analysis of `request`, confirm it with the values in
/// `state`, and put the confirmed record in `state`.
fn confirm(
    session: &SetupSession,
    result: CodexAnalysisResult,
    state: &mut Value,
) -> CodexAnalysisRecord {
    state["componentRecommendations"] = json!([]);
    let values = evidence::confirmation_values_from_state(state).unwrap();
    let fields = result
        .analysis
        .proposals
        .iter()
        .map(|proposal| {
            serde_json::to_value(&proposal.key)
                .unwrap()
                .as_str()
                .unwrap()
                .to_owned()
        })
        .collect::<Vec<_>>();
    let record = session
        .confirm_analysis(&result.record, &fields, &values)
        .unwrap();
    state["codexAnalysisRecord"] = serde_json::to_value(&record).unwrap();
    record
}

/// The analysis a new project needs: no scan, no evidence.
fn analyze_new(
    session: &SetupSession,
    source: &SourceResolution,
    state: &mut Value,
) -> CodexAnalysisRecord {
    let request = AiAnalysisRequest {
        provider: "local".into(),
        model: "fixture".into(),
        reasoning_effort: "high".into(),
        endpoint: LOCAL_ENDPOINT.into(),
        analysis: CodexAnalysisRequest {
            mode: "new_project_identity".into(),
            brief: "A Cold War alternate-history mod.".into(),
            evidence: vec![],
            constraints: json!({}),
            analysis_purpose: None,
            project_root: None,
            scan_id: None,
        },
    };
    let turn = session.analysis_turn(&request, None, source).unwrap();
    let mut reply = provider_reply(&turn.input_sha256);
    reply["mode"] = json!("new_project_identity");
    let result = session
        .record_analysis(request, None, source, reply)
        .unwrap();
    confirm(session, result, state)
}

/// The analysis an import needs: a scan of the root and approved evidence.
fn analyze_existing(
    session: &SetupSession,
    source: &SourceResolution,
    root: &Path,
    state: &mut Value,
) -> CodexAnalysisRecord {
    let scan = session
        .inspect(root, InspectOptions::default(), |_| {})
        .unwrap();
    let evidence = evidence_from(&scan);
    session
        .approve_evidence(root, scan.scan_id, &evidence)
        .unwrap();
    let canonical = session.resolve_project_path(root).unwrap().0;
    let request = analysis_request(&canonical, scan.scan_id, evidence);
    let turn = session.analysis_turn(&request, None, source).unwrap();
    let result = session
        .record_analysis(request, None, source, provider_reply(&turn.input_sha256))
        .unwrap();
    state["scanContext"] = json!({"scanId": scan.scan_id});
    confirm(session, result, state)
}

fn existing_project(hosted: &Hosted) -> PathBuf {
    let root = hosted.project();
    fs::create_dir_all(&root).unwrap();
    fixture::populate_existing_project(&root);
    root
}

fn approve_and_apply(session: &SetupSession, plan: &InstallationPlan) -> TransactionJournal {
    let digest = plan_sha256(plan).unwrap();
    session.approve_plan(plan.plan_id, &digest).unwrap();
    session.apply(plan.plan_id).unwrap()
}

#[test]
fn a_new_project_is_planned_approved_and_applied_through_the_facade() {
    let hosted = hosted();
    let session = hosted.session();
    let root = hosted.project();
    let mut state = fixture::planning_state("new", &root, None, None);
    analyze_new(&session, &hosted.latest(), &mut state);

    let plan = session.build_plan(&state).unwrap();

    // The plan names the verified source, binds the workspace it will create
    // the project in, and keeps its transaction state under the state root.
    assert_eq!(plan.source.resolved_revision, REVISION);
    assert_eq!(
        plan.transaction.project_root_mode,
        ProjectRootMode::CreateLeaf
    );
    assert_eq!(
        plan.transaction.project_root_identity,
        Some(root_identity(&hosted.workspace).unwrap())
    );
    assert!(Path::new(&plan.transaction.backup_root).starts_with(&hosted.state));
    assert!(Path::new(&plan.transaction.staging_root).starts_with(&hosted.state));
    assert!(!plan.approvals.dry_run_reviewed);
    assert!(plan.operations.iter().all(|operation| !operation.external));
    assert!(plan
        .generated_artifacts
        .iter()
        .all(|artifact| !artifact.external));
    for destination in ["CLAUDE.md", "descriptor.mod", "thumbnail.png", ".gitignore"] {
        assert!(
            plan.operations
                .iter()
                .any(|operation| operation.destination == destination),
            "{destination}"
        );
    }
    // Only manifest-declared files were fetched, and planning wrote nothing.
    let fetched = hosted.source.fetched.lock().unwrap().clone();
    assert!(!fetched.is_empty());
    assert!(fetched
        .iter()
        .all(|path| hosted.source.files.contains_key(path)));
    assert!(!root.exists());
    assert_eq!(fs::read_dir(&hosted.state).unwrap().count(), 0);

    // The reviewer approves the exact plan, bound to the workspace.
    let digest = plan_sha256(&plan).unwrap();
    let approved = session.approve_plan(plan.plan_id, &digest).unwrap();
    assert_eq!(approved.reviewed_plan_sha256, digest);
    assert_eq!(
        approved.project_root_identity,
        root_identity(&hosted.workspace).unwrap()
    );

    // Known limitation, kept visible: the transaction's readiness gate
    // requires a launcher registration for every new project, and a server
    // cannot write one outside the project root. Applying is refused after
    // the project's own files verify.
    let error = session.apply(plan.plan_id).unwrap_err().to_string();
    assert!(error.contains("readiness is blocked"), "{error}");
    assert!(error.contains("launcher.registration"), "{error}");
    assert!(!error.contains("skills.core"), "{error}");
    // No success lock was written for the unfinished install.
    assert!(!root.join(".hoi4-mod-setup/install.lock.json").exists());
}

#[test]
fn an_import_never_overwrites_a_user_file_without_a_decision() {
    let hosted = hosted();
    let session = hosted.session();
    let root = existing_project(&hosted);
    let mut state = fixture::planning_state("existing", &root, None, None);
    analyze_existing(&session, &hosted.latest(), &root, &mut state);

    let plan = session.build_plan(&state).unwrap();

    // Every user-edited file the package would write is a conflict that
    // starts as a no-op; the file that already equals the package's is not.
    let mut conflicts = plan
        .conflicts
        .iter()
        .map(|conflict| conflict.path.as_str())
        .collect::<Vec<_>>();
    conflicts.sort_unstable();
    assert_eq!(
        conflicts,
        [
            ".gitignore",
            "CLAUDE.md",
            "README.md",
            "descriptor.mod",
            "docs/linux.md"
        ]
    );
    assert!(plan
        .conflicts
        .iter()
        .all(|conflict| conflict.selected.is_none()));
    for conflict in &plan.conflicts {
        let operation = plan
            .operations
            .iter()
            .find(|operation| operation.destination == conflict.path)
            .unwrap();
        assert_eq!(operation.action, OperationAction::Skip, "{}", conflict.path);
        assert_eq!(operation.local_state, LocalState::Modified);
    }
    let unchanged = plan
        .operations
        .iter()
        .find(|operation| operation.destination == "docs/runtimes.md")
        .unwrap();
    assert_eq!(unchanged.local_state, LocalState::Unmodified);

    // Decide, review the changed plan, apply.
    for path in ["CLAUDE.md", "descriptor.mod"] {
        session
            .decide_conflict(plan.plan_id, path, "replace")
            .unwrap();
    }
    for path in [".gitignore", "README.md", "docs/linux.md"] {
        session.decide_conflict(plan.plan_id, path, "keep").unwrap();
    }
    let decided = session
        .decide_conflict(plan.plan_id, "docs/linux.md", "keep")
        .unwrap();
    let journal = approve_and_apply(&session, &decided);
    assert_eq!(journal.state, "completed");

    assert!(fs::read_to_string(root.join("CLAUDE.md"))
        .unwrap()
        .contains("agent guidance"));
    assert_eq!(
        fs::read(root.join("README.md")).unwrap(),
        b"# My own readme\n"
    );
    assert_eq!(fs::read(root.join(".gitignore")).unwrap(), b"*.log\n");
    assert_eq!(
        fs::read(root.join("docs/linux.md")).unwrap(),
        b"Edited by the user.\n"
    );
    // The descriptor was replaced by the generated one.
    assert!(fs::read_to_string(root.join("descriptor.mod"))
        .unwrap()
        .contains("name=\"Cold War Curtain\""));
    assert!(!fs::read_to_string(root.join("descriptor.mod"))
        .unwrap()
        .contains("0.0.9"));
    // Files the package does not own are untouched.
    assert_eq!(
        fs::read(root.join("events/cwc_events.txt")).unwrap(),
        b"namespace = cwc\n"
    );
}

#[test]
fn a_plan_cannot_write_outside_the_project_root() {
    let hosted = hosted();
    let session = hosted.session();
    let root = hosted.project();

    // The launcher registration lives in the player's mod folder, outside the
    // project root.
    let mut state = fixture::planning_state(
        "new",
        &root,
        Some(&hosted.workspace.join("cold_war_curtain.mod")),
        None,
    );
    analyze_new(&session, &hosted.latest(), &mut state);
    let error = session.build_plan(&state).unwrap_err().to_string();
    assert!(error.contains("outside the project root"), "{error}");

    // A root outside the workspace is not a project the session may plan.
    let outside = hosted.state.join("elsewhere");
    fs::create_dir_all(&outside).unwrap();
    let mut state = fixture::planning_state("new", &root, None, None);
    analyze_new(&session, &hosted.latest(), &mut state);
    state["identity"]["projectRoot"] = json!(outside.display().to_string());
    let error = session.build_plan(&state).unwrap_err().to_string();
    assert!(error.contains("outside the session workspace"), "{error}");

    // The 3D workflow and kept context files need the desktop's vault,
    // launcher and attachment store; a server refuses them rather than guess.
    for (key, value, message) in [
        ("meshSelected", json!(true), "3D workflow"),
        (
            "contextDocuments",
            json!([{"id": "doc", "keepCopy": true}]),
            "context documents and images",
        ),
        (
            "contextThumbnail",
            json!({"attachmentId": "image", "png": ""}),
            "context documents and images",
        ),
    ] {
        let mut state = fixture::planning_state("new", &root, None, None);
        state[key] = value;
        analyze_new(&session, &hosted.latest(), &mut state);
        let error = session.build_plan(&state).unwrap_err().to_string();
        assert!(error.contains(message), "{key}: {error}");
    }
    assert!(!root.exists());
}

#[test]
fn a_package_that_routes_a_file_outside_the_project_is_refused() {
    let mut source = SourceFixture::new();
    let mut manifest: Value = serde_json::from_slice(&source.manifest_bytes).unwrap();
    let components = manifest["components"].as_array_mut().unwrap();
    let instructions = components
        .iter_mut()
        .find(|component| component["id"] == "core.claude.instructions")
        .unwrap();
    instructions["destination"]["path"] = json!("../CLAUDE.md");
    source.manifest_bytes = serde_json::to_vec(&manifest).unwrap();

    // The package never resolves, so there is nothing to plan or analyze
    // against, and nothing is fetched.
    let request = SourceRequest {
        mode: SourceMode::Latest,
        requested_ref: None,
        release: None,
    };
    let error = resolve_source(&source, &request).unwrap_err().to_string();
    assert!(error.contains("destination"), "{error}");
    assert!(source.fetched.lock().unwrap().is_empty());
}

#[test]
fn planning_is_refused_without_a_confirmed_analysis_and_fetches_nothing() {
    let hosted = hosted();
    let session = hosted.session();
    let root = hosted.project();
    let mut state = fixture::planning_state("new", &root, None, None);

    // No analysis at all.
    let error = session.build_plan(&state).unwrap_err().to_string();
    assert!(
        error.contains("must be confirmed before planning"),
        "{error}"
    );

    // An analysis the provider returned but nobody confirmed.
    let resolution = hosted.latest();
    let request = AiAnalysisRequest {
        provider: "local".into(),
        model: "fixture".into(),
        reasoning_effort: "high".into(),
        endpoint: LOCAL_ENDPOINT.into(),
        analysis: CodexAnalysisRequest {
            mode: "new_project_identity".into(),
            brief: "A Cold War alternate-history mod.".into(),
            evidence: vec![],
            constraints: json!({}),
            analysis_purpose: None,
            project_root: None,
            scan_id: None,
        },
    };
    let turn = session.analysis_turn(&request, None, &resolution).unwrap();
    let mut reply = provider_reply(&turn.input_sha256);
    reply["mode"] = json!("new_project_identity");
    let result = session
        .record_analysis(request, None, &resolution, reply)
        .unwrap();
    state["codexAnalysisRecord"] = serde_json::to_value(&result.record).unwrap();
    assert!(session.build_plan(&state).is_err());

    // An analysis another session confirmed does not count in this one.
    let other = hosted.session();
    let mut confirmed = fixture::planning_state("new", &root, None, None);
    analyze_new(&other, &resolution, &mut confirmed);
    assert!(session.build_plan(&confirmed).is_err());

    // The builder was never reached: it fetched no file, and the only
    // resolutions are the ones this test made for itself.
    assert!(hosted.source.fetched.lock().unwrap().is_empty());
    assert_eq!(
        hosted
            .source
            .resolutions
            .load(std::sync::atomic::Ordering::SeqCst),
        1
    );
    assert!(!root.exists());
}

#[test]
fn every_selected_file_is_checked_against_the_manifest_whatever_the_source_says() {
    let mut source = SourceFixture::new();
    // The source serves the right size with the wrong content.
    let original = source.files["CLAUDE_template.md"].clone();
    let mut tampered = original.clone();
    tampered[0] ^= 1;
    assert_eq!(tampered.len(), original.len());
    source.files.insert("CLAUDE_template.md".into(), tampered);
    let hosted = hosted_with(source);
    let session = hosted.session();
    let mut state = fixture::planning_state("new", &hosted.project(), None, None);
    analyze_new(&session, &hosted.latest(), &mut state);

    let error = session.build_plan(&state).unwrap_err().to_string();
    assert!(error.contains("checksum mismatch"), "{error}");
    assert!(error.contains("CLAUDE_template.md"), "{error}");
    // Nothing was stored for approval, and nothing was written.
    assert!(session.approve_plan(Uuid::new_v4(), "0").is_err());
    assert!(!hosted.project().exists());
    assert_eq!(fs::read_dir(&hosted.state).unwrap().count(), 0);

    // A source that serves the wrong size is refused the same way.
    let mut source = SourceFixture::new();
    source
        .files
        .get_mut("CLAUDE_template.md")
        .unwrap()
        .extend_from_slice(b"extra");
    let hosted = hosted_with(source);
    let session = hosted.session();
    let mut state = fixture::planning_state("new", &hosted.project(), None, None);
    analyze_new(&session, &hosted.latest(), &mut state);
    let error = session.build_plan(&state).unwrap_err().to_string();
    assert!(
        error.contains("checksum mismatch") || error.contains("size mismatch"),
        "{error}"
    );
}

#[test]
fn a_pinned_commit_is_honoured_and_a_different_source_is_refused() {
    // Latest has moved on; the reviewer pinned the earlier commit.
    let hosted = hosted_with(SourceFixture::new().with_latest(OTHER_REVISION));
    let session = hosted.session();
    let pinned_request = SourceRequest {
        mode: SourceMode::PinnedCommit,
        requested_ref: Some(REVISION.into()),
        release: None,
    };
    let pinned = hosted.resolve(pinned_request);
    assert_eq!(pinned.identity.resolved_revision, REVISION);
    let mut state = fixture::planning_state("new", &hosted.project(), None, None);
    state["sourceMode"] = json!("pinned_commit");
    state["pinnedRef"] = json!(REVISION);
    analyze_new(&session, &pinned, &mut state);

    let plan = session.build_plan(&state).unwrap();
    assert_eq!(plan.source.mode, SourceMode::PinnedCommit);
    assert_eq!(plan.source.resolved_revision, REVISION);
    assert_eq!(plan.source.requested_ref.as_deref(), Some(REVISION));
    assert!(plan
        .download_ledger
        .iter()
        .all(|file| file.source_revision == REVISION));

    // The same analysis cannot back a plan from the latest commit: it was
    // bound to a different revision.
    let mut latest_state = state.clone();
    latest_state["sourceMode"] = json!("latest");
    latest_state["pinnedRef"] = json!("");
    let error = session.build_plan(&latest_state).unwrap_err().to_string();
    assert!(
        error.contains("belongs to a different source manifest"),
        "{error}"
    );

    // A pin the source does not publish does not resolve.
    let mut unknown = state.clone();
    unknown["pinnedRef"] = json!(OTHER_REVISION.replace('f', "e"));
    assert!(session.build_plan(&unknown).is_err());
}

#[test]
fn a_release_pin_resolves_to_its_commit_and_records_the_tag() {
    let hosted = hosted();
    let session = hosted.session();
    let release = hosted.resolve(SourceRequest {
        mode: SourceMode::PinnedRelease,
        requested_ref: Some(fixture::RELEASE.into()),
        release: Some(fixture::RELEASE.into()),
    });
    let mut state = fixture::planning_state("new", &hosted.project(), None, None);
    state["sourceMode"] = json!("pinned_release");
    state["pinnedRef"] = json!(fixture::RELEASE);
    analyze_new(&session, &release, &mut state);
    let plan = session.build_plan(&state).unwrap();
    assert_eq!(plan.source.mode, SourceMode::PinnedRelease);
    assert_eq!(plan.source.release.as_deref(), Some(fixture::RELEASE));
    assert_eq!(plan.source.resolved_revision, REVISION);
}

#[test]
fn the_cache_root_must_be_an_existing_absolute_directory() {
    let directory = tempdir().unwrap();
    assert!(DefaultPlanBuilder::new(directory.path()).is_ok());
    assert!(DefaultPlanBuilder::new("cache").is_err());
    assert!(DefaultPlanBuilder::new(directory.path().join("missing")).is_err());
}

#[test]
fn the_builder_is_shared_by_sessions_without_sharing_their_plans() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<DefaultPlanBuilder>();
    let hosted = hosted();
    let first = hosted.session();
    let second = hosted.session();
    let mut state = fixture::planning_state("new", &hosted.project(), None, None);
    analyze_new(&first, &hosted.latest(), &mut state);
    let plan = first.build_plan(&state).unwrap();
    // The second session never saw the analysis or the plan.
    assert!(second.build_plan(&state).is_err());
    assert!(second
        .approve_plan(plan.plan_id, &plan_sha256(&plan).unwrap())
        .is_err());
}

/// Whether this machine's plans launch the MCP package with Node.
const LINUX: bool = cfg!(target_os = "linux");

#[test]
fn a_linux_plan_needs_a_node_runtime_only_once_it_registers_the_mcp_package() {
    let hosted = hosted_configured(SourceFixture::new(), None);
    let session = hosted.session();

    // Claude Code and Codex read the MCP package from their configuration, so
    // on Linux the plan needs Node and says so instead of guessing a path.
    let mut state = fixture::planning_state("new", &hosted.project(), None, None);
    analyze_new(&session, &hosted.latest(), &mut state);
    let result = session.build_plan(&state);
    if LINUX {
        let error = result.unwrap_err().to_string();
        assert!(
            error.contains("trusted native Node runtime is unavailable"),
            "{error}"
        );
    } else {
        result.unwrap();
    }

    // Every coding client in the package registers the MCP package, so on
    // Linux a plan that reaches adaptation always needs Node. A request the
    // session refuses earlier never gets that far, so it reports its own
    // reason, with or without a runtime.
    let unanalysed = fixture::planning_state("new", &hosted.project(), None, None);
    let error = session.build_plan(&unanalysed).unwrap_err().to_string();
    assert!(!error.contains("Node runtime"), "{error}");
}
