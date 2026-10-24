//! Tests for the service facade.
//!
//! They run the real scanner, the real gates and the real transaction engine
//! on temporary directories. The only stand-ins are the plan builder, which is
//! the host's to supply, and the provider reply, which a host would obtain
//! from its own model.

use super::*;
use crate::credentials::{save_ai_provider_key, MemoryCredentialStore};
use crate::readiness::manifest_wiki_pages;
use crate::security::sha256_bytes;
use crate::source::{SOURCE_NAME, SOURCE_OWNER};
use serde_json::json;
use std::fs;
use std::sync::atomic::{AtomicUsize, Ordering};
use tempfile::{tempdir, TempDir};

const LOCAL_ENDPOINT: &str = "http://127.0.0.1:11434/v1/chat/completions";

fn source_resolution() -> SourceResolution {
    let bytes = include_bytes!("../../../docs/source-manifest/hoi4-mod-setup.manifest.json");
    SourceResolution {
        identity: SourceIdentity {
            repository: format!("{SOURCE_OWNER}/{SOURCE_NAME}"),
            mode: SourceMode::Latest,
            resolved_revision: "a".repeat(40),
            requested_ref: None,
            release: None,
            manifest_sha256: sha256_bytes(bytes),
            manifest_origin: "remote".into(),
        },
        manifest: hoi4_setup_core::manifest::parse_manifest(bytes, None).unwrap(),
        manifest_bytes: bytes.to_vec(),
        branch: Some("main".into()),
    }
}

/// Builds the plan a host's builder would: one generated file, bound to the
/// verified root identity and the confirmed analysis it was handed.
struct FixtureBuilder {
    source: SourceIdentity,
}

impl PlanBuilder for FixtureBuilder {
    fn build(&self, request: &PlanRequest<'_>) -> Result<BuiltPlan, AppError> {
        assert!(request.project_root_exists);
        // The state a builder reads names the contained root.
        assert_eq!(
            project_root_field(request.state).unwrap(),
            request.project_root
        );
        let bytes = b"# Agent guidance\n".to_vec();
        let plan = InstallationPlan {
            schema_version: "1.0.0".into(),
            plan_id: Uuid::new_v4(),
            project_id: "example".into(),
            script_prefix: Some("example".into()),
            primary_namespace: Some("example".into()),
            created_at: Some(chrono::Utc::now().to_rfc3339()),
            maintenance_mode: None,
            source: self.source.clone(),
            ai_provider: "local".into(),
            ai_model: "fixture".into(),
            ai_reasoning_effort: "high".into(),
            ai_endpoint: Some(LOCAL_ENDPOINT.into()),
            ai_optimization_profile: ai::profile("local").unwrap().optimization_profile,
            primary_coding_environment: "codex".into(),
            additional_coding_environments: vec![],
            flatten_chat_sources: false,
            codex_analysis: Some(request.analysis.clone()),
            selected_components: vec!["core.agents".into(), "wiki.snapshot".into()],
            wiki_required_pages: manifest_wiki_pages(),
            wiki_metadata: Some(WikiInstallMetadata {
                snapshot_marker: None,
                required_media_policy: "all_declared".into(),
                source_status: "verified_snapshot".into(),
                license_status: "not_verified".into(),
                repository_license_status: "unknown".into(),
                notes: vec![],
            }),
            generated_artifacts: vec![],
            download_ledger: vec![],
            git_setup: None,
            credential_references: vec![],
            optional_workflows: Default::default(),
            portrait_pipeline: None,
            image_generation: None,
            live_testing: None,
            operations: vec![PlanOperation {
                id: "op-1".into(),
                component_id: "core.agents".into(),
                ownership: Some(Ownership::Managed),
                location_scope: None,
                action: OperationAction::Create,
                source_path: Some("generated:test".into()),
                destination: "AGENTS.md".into(),
                source_sha256: Some(sha256_bytes(&bytes)),
                source_size: Some(bytes.len() as u64),
                platform: Some(ManifestPlatform::All),
                executable: false,
                result_sha256: None,
                base_sha256: None,
                local_sha256: None,
                local_state: LocalState::Absent,
                resolution: None,
                external: false,
                rollback: RollbackAction::RemoveCreated,
                external_parent_identity: None,
            }],
            conflicts: vec![],
            external_actions: vec![],
            transaction: TransactionPlanInfo {
                stages: TRANSACTION_STAGES
                    .iter()
                    .map(|stage| (*stage).into())
                    .collect(),
                backup_root: request.state_root.join("backups").display().to_string(),
                staging_root: request.state_root.join("staging").display().to_string(),
                directories: Vec::new(),
                atomic_apply_expected: true,
                project_root_mode: ProjectRootMode::Existing,
                project_root_parent: None,
                project_root_leaf: None,
                project_root_identity: Some(request.project_root_identity.into()),
            },
            approvals: PlanApprovals {
                dry_run_reviewed: false,
                external_actions_reviewed: false,
                git_remote_approved: false,
                push_approved: false,
            },
        };
        Ok(BuiltPlan {
            prepared_files: vec![PreparedFile {
                operation_id: "op-1".into(),
                destination: "AGENTS.md".into(),
                expected_sha256: sha256_bytes(&bytes),
                bytes,
            }],
            plan,
            merge_contexts: HashMap::new(),
        })
    }
}

struct Fixture {
    /// Holds the workspace and the state root, which are siblings.
    _container: TempDir,
    workspace: PathBuf,
    state: PathBuf,
    project: PathBuf,
    service: SetupService,
    source: SourceResolution,
}

fn fixture() -> Fixture {
    let container = tempdir().unwrap();
    let base = fs::canonicalize(container.path()).unwrap();
    let workspace = base.join("workspace");
    let state = base.join("state");
    let project = workspace.join("example");
    fs::create_dir_all(&project).unwrap();
    fs::create_dir_all(&state).unwrap();
    fs::write(
        project.join("descriptor.mod"),
        "name=\"Example\"\nversion=\"0.1.0\"\nsupported_version=\"1.17.*\"\npicture=\"thumbnail.png\"\n",
    )
    .unwrap();
    let wiki_root = project.join("paradox_wiki");
    for page in manifest_wiki_pages() {
        let path = wiki_root.join(page.replace('/', std::path::MAIN_SEPARATOR_STR));
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, "# Test wiki page\n").unwrap();
    }
    let source = source_resolution();
    let service = SetupService::new(Arc::new(MemoryCredentialStore::default())).with_plan_builder(
        Arc::new(FixtureBuilder {
            source: source.identity.clone(),
        }),
    );
    Fixture {
        _container: container,
        workspace,
        state,
        project,
        service,
        source,
    }
}

impl Fixture {
    fn session(&self) -> SetupSession {
        self.service
            .open_session(SessionConfig::new(&self.workspace, &self.state).unwrap())
    }
}

fn evidence_from(scan: &ScanResult) -> Vec<ApprovedEvidence> {
    let finding = scan
        .findings
        .iter()
        .find(|finding| !finding.evidence.is_empty() && finding.model_excerpt.is_some())
        .expect("the fixture project produces a finding with evidence");
    let excerpt = finding.model_excerpt.clone().unwrap();
    vec![ApprovedEvidence {
        reference: finding.id.clone(),
        path: finding.evidence[0].path.clone(),
        excerpt_sha256: sha256_bytes(excerpt.as_bytes()),
        excerpt,
        confidence: None,
    }]
}

fn analysis_request(
    root: &Path,
    scan_id: Uuid,
    evidence: Vec<ApprovedEvidence>,
) -> AiAnalysisRequest {
    AiAnalysisRequest {
        provider: "local".into(),
        model: "fixture".into(),
        reasoning_effort: "high".into(),
        endpoint: LOCAL_ENDPOINT.into(),
        analysis: crate::codex::CodexAnalysisRequest {
            mode: "existing_project_semantics".into(),
            brief: String::new(),
            evidence,
            constraints: json!({}),
            analysis_purpose: Some("existing_project_import".into()),
            project_root: Some(root.display().to_string()),
            scan_id: Some(scan_id),
        },
    }
}

fn provider_reply(input_sha256: &str) -> Value {
    let mut reply: Value = serde_json::from_str(include_str!(
        "../../../docs/examples/codex-analysis.example.json"
    ))
    .unwrap();
    reply["mode"] = json!("existing_project_semantics");
    reply["input_sha256"] = json!(input_sha256);
    reply["component_recommendations"] = json!([]);
    for proposal in reply["proposals"].as_array_mut().unwrap() {
        proposal["evidence_refs"] = json!([]);
    }
    reply
}

fn reviewed_state(root: &Path, scan_id: Uuid) -> Value {
    json!({
        "aiProvider": "local",
        "aiModel": "fixture",
        "aiReasoningEffort": "high",
        "aiEndpoint": LOCAL_ENDPOINT,
        "description": "A Cold War alternate-history mod centered on a divided Europe.",
        "folderProfile": ["events", "common", "localisation/english", "docs"],
        "identity": {
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
        },
        "conventions": {
            "agents_profile": "default",
            "localisation_convention": "english",
            "documentation_convention": "markdown",
        },
        "componentRecommendations": [],
        "scanContext": {"scanId": scan_id},
    })
}

/// Everything the session returns on the way to a confirmed analysis.
struct Reviewed {
    scan_id: Uuid,
    record: CodexAnalysisRecord,
    state: Value,
}

/// Inspect, approve evidence, analyze and confirm: the full path a plan needs.
fn confirmed_analysis(fixture: &Fixture, session: &SetupSession) -> Reviewed {
    let scan = session
        .inspect(&fixture.project, InspectOptions::default(), |_| {})
        .unwrap();
    assert!(!scan.partial && !scan.cancelled);
    let evidence = evidence_from(&scan);
    session
        .approve_evidence(&fixture.project, scan.scan_id, &evidence)
        .unwrap();
    let root = session.existing_project_path(&fixture.project).unwrap();
    let request = analysis_request(&root, scan.scan_id, evidence);
    let turn = session
        .analysis_turn(&request, None, &fixture.source)
        .unwrap();
    let result = session
        .record_analysis(
            request,
            None,
            &fixture.source,
            provider_reply(&turn.input_sha256),
        )
        .unwrap();
    let mut state = reviewed_state(&fixture.project, scan.scan_id);
    let values = evidence::confirmation_values_from_state(&state).unwrap();
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
    Reviewed {
        scan_id: scan.scan_id,
        record,
        state,
    }
}

#[test]
fn a_full_session_inspects_analyzes_plans_approves_and_applies_in_its_own_roots() {
    let fixture = fixture();
    let session = fixture.session();
    let reviewed = confirmed_analysis(&fixture, &session);

    let plan = session.build_plan(&reviewed.state).unwrap();
    assert!(!plan.approvals.dry_run_reviewed);
    assert_eq!(
        plan.transaction.project_root_identity.as_deref(),
        Some(session.root_identity(&fixture.project).unwrap().as_str())
    );
    // Planning changed nothing in the project and created no state.
    assert!(!fixture.project.join("AGENTS.md").exists());
    assert_eq!(fs::read_dir(&fixture.state).unwrap().count(), 0);

    let reviewed_hash = plan_sha256(&plan).unwrap();
    let approved = session.approve_plan(plan.plan_id, &reviewed_hash).unwrap();
    assert_eq!(approved.reviewed_plan_sha256, reviewed_hash);
    assert_ne!(approved.approved_plan_sha256, reviewed_hash);

    let journal = session.apply(plan.plan_id).unwrap();
    assert_eq!(journal.state, "completed");
    assert_eq!(
        fs::read(fixture.project.join("AGENTS.md")).unwrap(),
        b"# Agent guidance\n"
    );
    // The journal is in the caller's state root, not in desktop app data.
    assert!(fixture
        .state
        .join("transactions")
        .join(plan.plan_id.to_string())
        .join("journal.json")
        .is_file());
    assert!(session
        .inspect_recovery(&fixture.project)
        .unwrap()
        .is_none());
    // The approval and the plan are spent.
    assert!(session.apply(plan.plan_id).is_err());
}

#[test]
fn two_sessions_do_not_see_each_others_analyses_or_plans() {
    let fixture = fixture();
    let first = fixture.session();
    let second = fixture.session();
    let reviewed = confirmed_analysis(&fixture, &first);

    // The second session never retained the analysis, so it cannot confirm it
    // and cannot build a plan from a confirmation it did not witness.
    let error = second
        .confirm_analysis(
            &reviewed.record,
            &reviewed.record.confirmed_fields,
            &evidence::confirmation_values_from_state(&reviewed.state).unwrap(),
        )
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("no longer available in the core session"),
        "{error}"
    );
    let error = second.build_plan(&reviewed.state).unwrap_err().to_string();
    assert!(
        error.contains("no longer present in the core session"),
        "{error}"
    );

    // A plan prepared by the first session does not exist in the second.
    let plan = first.build_plan(&reviewed.state).unwrap();
    let hash = plan_sha256(&plan).unwrap();
    for error in [
        second.approve_plan(plan.plan_id, &hash).unwrap_err(),
        second
            .decide_conflict(plan.plan_id, "AGENTS.md", "keep")
            .unwrap_err(),
        second.apply(plan.plan_id).unwrap_err(),
    ] {
        let error = error.to_string();
        assert!(
            error.contains("approval is required")
                || error.contains("not available in this session"),
            "{error}"
        );
    }
    assert!(!fixture.project.join("AGENTS.md").exists());

    // The first session's state is untouched by the second one's refusals.
    first.approve_plan(plan.plan_id, &hash).unwrap();
    assert_eq!(first.apply(plan.plan_id).unwrap().state, "completed");
}

#[test]
fn a_scan_in_one_session_does_not_approve_evidence_in_another() {
    let fixture = fixture();
    let first = fixture.session();
    let second = fixture.session();
    let scan = first
        .inspect(&fixture.project, InspectOptions::default(), |_| {})
        .unwrap();
    let evidence = evidence_from(&scan);
    assert!(second
        .approve_evidence(&fixture.project, scan.scan_id, &evidence)
        .is_err());
    first
        .approve_evidence(&fixture.project, scan.scan_id, &evidence)
        .unwrap();
}

#[test]
fn paths_outside_the_workspace_are_refused_everywhere() {
    let fixture = fixture();
    let session = fixture.session();
    let outside = fixture.state.join("elsewhere");
    fs::create_dir_all(&outside).unwrap();
    let traversal = PathBuf::from("..").join("state");

    for path in [&outside, &fixture.state, &traversal] {
        assert!(session.resolve_project_path(path).is_err(), "{path:?}");
        assert!(session.root_identity(path).is_err());
        assert!(session
            .inspect(path, InspectOptions::default(), |_| {})
            .is_err());
        assert!(session.inspect_recovery(path).is_err());
        assert!(session.approve_evidence(path, Uuid::new_v4(), &[]).is_err());
    }
    assert!(session
        .resolve_project_path(&outside)
        .unwrap_err()
        .to_string()
        .contains("outside the session workspace"));
    assert!(session
        .resolve_project_path(&traversal)
        .unwrap_err()
        .to_string()
        .contains("traversal"));
    // A sibling whose name merely starts with the workspace name is outside.
    let sibling = fixture.workspace.with_file_name("workspace-other");
    fs::create_dir_all(&sibling).unwrap();
    assert!(session.resolve_project_path(&sibling).is_err());

    // A relative path stays inside, and a plan state cannot name a root
    // outside.
    let (inside, exists) = session.resolve_project_path(Path::new("example")).unwrap();
    assert!(exists && inside.starts_with(&fixture.workspace));
    let reviewed = confirmed_analysis(&fixture, &session);
    let mut state = reviewed.state.clone();
    state["identity"]["projectRoot"] = json!(outside.display().to_string());
    assert!(session
        .build_plan(&state)
        .unwrap_err()
        .to_string()
        .contains("outside the session workspace"));
    // An analysis request cannot name an outside root either.
    let request = analysis_request(&outside, reviewed.scan_id, vec![]);
    assert!(session
        .analysis_turn(&request, None, &fixture.source)
        .is_err());
    // Recovery reads nothing for a root outside the workspace.
    assert!(session
        .recover(&outside, Uuid::new_v4(), RecoveryAction::Rollback)
        .is_err());
}

#[test]
fn a_link_out_of_the_workspace_is_refused() {
    let fixture = fixture();
    let session = fixture.session();
    let outside = fixture.state.join("target");
    fs::create_dir_all(&outside).unwrap();
    let link = fixture.workspace.join("link");
    #[cfg(unix)]
    let created = std::os::unix::fs::symlink(&outside, &link).is_ok();
    #[cfg(windows)]
    let created = std::os::windows::fs::symlink_dir(&outside, &link).is_ok();
    if !created {
        // Creating a symlink needs a privilege some Windows accounts lack.
        return;
    }
    assert!(session.resolve_project_path(&link).is_err());
    assert!(session
        .inspect(&link, InspectOptions::default(), |_| {})
        .is_err());
}

#[test]
fn a_session_needs_disjoint_existing_absolute_roots() {
    let fixture = fixture();
    let inside = fixture.workspace.join("example");
    assert!(SessionConfig::new(&fixture.workspace, &inside).is_err());
    assert!(SessionConfig::new(&inside, &fixture.workspace).is_err());
    assert!(SessionConfig::new(&fixture.workspace, &fixture.workspace).is_err());
    assert!(SessionConfig::new("workspace", &fixture.state).is_err());
    assert!(SessionConfig::new(&fixture.workspace, fixture.state.join("missing")).is_err());
    assert!(SessionConfig::new(&fixture.workspace, &fixture.state).is_ok());
}

#[test]
fn planning_and_applying_are_refused_without_a_confirmed_analysis() {
    let fixture = fixture();
    let session = fixture.session();
    let scan = session
        .inspect(&fixture.project, InspectOptions::default(), |_| {})
        .unwrap();
    let state = reviewed_state(&fixture.project, scan.scan_id);

    // No analysis at all.
    let error = session.build_plan(&state).unwrap_err().to_string();
    assert!(
        error.contains("must be confirmed before planning"),
        "{error}"
    );

    // An analysis the provider returned but nobody confirmed.
    let evidence = evidence_from(&scan);
    session
        .approve_evidence(&fixture.project, scan.scan_id, &evidence)
        .unwrap();
    let root = session.existing_project_path(&fixture.project).unwrap();
    let request = analysis_request(&root, scan.scan_id, evidence);
    let turn = session
        .analysis_turn(&request, None, &fixture.source)
        .unwrap();
    let result = session
        .record_analysis(
            request,
            None,
            &fixture.source,
            provider_reply(&turn.input_sha256),
        )
        .unwrap();
    let mut state = state;
    state["codexAnalysisRecord"] = serde_json::to_value(&result.record).unwrap();
    assert!(session.build_plan(&state).is_err());

    // With nothing built there is nothing to approve or apply, and nothing
    // was written anywhere.
    let unknown = Uuid::new_v4();
    assert!(session.approve_plan(unknown, "0").is_err());
    assert!(session.apply(unknown).is_err());
    assert!(!fixture.project.join("AGENTS.md").exists());
    assert_eq!(fs::read_dir(&fixture.state).unwrap().count(), 0);
}

#[test]
fn a_confirmation_is_bound_to_the_values_the_reviewer_saw() {
    let fixture = fixture();
    let session = fixture.session();
    let reviewed = confirmed_analysis(&fixture, &session);
    let mut changed = reviewed.state.clone();
    changed["description"] = json!("A different description that nobody confirmed.");
    let error = session.build_plan(&changed).unwrap_err().to_string();
    assert!(
        error.contains("no longer present in the core session"),
        "{error}"
    );
    let mut other_model = reviewed.state.clone();
    other_model["aiModel"] = json!("another-model");
    let error = session.build_plan(&other_model).unwrap_err().to_string();
    assert!(
        error.contains("does not match the selected provider"),
        "{error}"
    );
    assert!(session.build_plan(&reviewed.state).is_ok());
}

#[test]
fn evidence_must_be_approved_from_the_latest_scan_before_analysis() {
    let fixture = fixture();
    let session = fixture.session();
    let scan = session
        .inspect(&fixture.project, InspectOptions::default(), |_| {})
        .unwrap();
    let evidence = evidence_from(&scan);
    let root = session.existing_project_path(&fixture.project).unwrap();
    let request = analysis_request(&root, scan.scan_id, evidence.clone());
    // Not approved yet.
    let error = session
        .analysis_turn(&request, None, &fixture.source)
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("explicitly approved after the latest read-only scan"),
        "{error}"
    );
    // A value the core did not scan is not approvable.
    let mut forged = evidence.clone();
    forged[0].excerpt = "a value the scanner never produced".into();
    forged[0].excerpt_sha256 = sha256_bytes(forged[0].excerpt.as_bytes());
    let error = session
        .approve_evidence(&fixture.project, scan.scan_id, &forged)
        .unwrap_err()
        .to_string();
    assert!(error.contains("exact core-scanned value"), "{error}");
    // Another scan id is stale.
    let error = session
        .approve_evidence(&fixture.project, Uuid::new_v4(), &evidence)
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("stale or belongs to a different project"),
        "{error}"
    );
    session
        .approve_evidence(&fixture.project, scan.scan_id, &evidence)
        .unwrap();
    assert!(session
        .analysis_turn(&request, None, &fixture.source)
        .is_ok());
    // A new scan withdraws the approval.
    session
        .inspect(&fixture.project, InspectOptions::default(), |_| {})
        .unwrap();
    assert!(session
        .analysis_turn(&request, None, &fixture.source)
        .is_err());
}

#[test]
fn a_stopped_scan_approves_no_evidence() {
    let fixture = fixture();
    let session = fixture.session();
    let cancel = Arc::new(AtomicBool::new(true));
    let scan = session
        .inspect(
            &fixture.project,
            InspectOptions {
                cancel: Some(cancel),
            },
            |_| {},
        )
        .unwrap();
    assert!(scan.cancelled || scan.partial);
    let approved = session.state.approved.lock().unwrap();
    assert!(approved.scan_id.is_none() && approved.project_root.is_none());
}

#[test]
fn approval_is_bound_to_the_reviewed_plan() {
    let fixture = fixture();
    let session = fixture.session();
    let reviewed = confirmed_analysis(&fixture, &session);
    let plan = session.build_plan(&reviewed.state).unwrap();
    let hash = plan_sha256(&plan).unwrap();

    // Not approved: apply refuses.
    assert!(session
        .apply(plan.plan_id)
        .unwrap_err()
        .to_string()
        .contains("approval is required"));
    // A digest of some other plan approves nothing.
    let error = session
        .approve_plan(plan.plan_id, &"0".repeat(64))
        .unwrap_err()
        .to_string();
    assert!(error.contains("changed after it was reviewed"), "{error}");
    assert!(session.apply(plan.plan_id).is_err());

    session.approve_plan(plan.plan_id, &hash).unwrap();
    // Tampering with the stored plan after approval is caught at apply.
    session
        .state
        .plans
        .lock()
        .unwrap()
        .get_mut(&plan.plan_id)
        .unwrap()
        .plan
        .project_id = "tampered".into();
    session.state.approvals.lock().unwrap().insert(
        plan.plan_id,
        PlanApproval {
            approved_plan_sha256: "1".repeat(64),
            project_root_identity: session.root_identity(&fixture.project).unwrap(),
        },
    );
    assert!(session
        .apply(plan.plan_id)
        .unwrap_err()
        .to_string()
        .contains("changed after it was approved"));
    assert!(!fixture.project.join("AGENTS.md").exists());
}

#[test]
fn a_conflict_decision_withdraws_the_approval() {
    let fixture = fixture();
    let session = fixture.session();
    let reviewed = confirmed_analysis(&fixture, &session);
    let plan = session.build_plan(&reviewed.state).unwrap();
    session
        .approve_plan(plan.plan_id, &plan_sha256(&plan).unwrap())
        .unwrap();
    // The fixture plan offers no conflict, so the decision is refused, but a
    // refused decision must not leave the plan approved and mutable.
    assert!(session
        .decide_conflict(plan.plan_id, "AGENTS.md", "keep")
        .is_err());
    // Add a real conflict to see the decision path withdraw the approval.
    {
        let mut plans = session.state.plans.lock().unwrap();
        let prepared = plans.get_mut(&plan.plan_id).unwrap();
        prepared.plan.conflicts.push(PlanConflict {
            id: "conflict.op-1".into(),
            path: "AGENTS.md".into(),
            options: vec!["keep".into(), "skip".into()],
            selected: None,
            apply_to_identical: false,
        });
    }
    let decided = session
        .decide_conflict(plan.plan_id, "AGENTS.md", "skip")
        .unwrap();
    assert!(!decided.approvals.dry_run_reviewed);
    assert!(session
        .apply(plan.plan_id)
        .unwrap_err()
        .to_string()
        .contains("approval is required"));
    assert!(!fixture.project.join("AGENTS.md").exists());
}

#[test]
fn apply_refuses_a_root_replaced_after_approval() {
    let fixture = fixture();
    let session = fixture.session();
    let reviewed = confirmed_analysis(&fixture, &session);
    let plan = session.build_plan(&reviewed.state).unwrap();
    session
        .approve_plan(plan.plan_id, &plan_sha256(&plan).unwrap())
        .unwrap();

    // Swap the directory for an identical-looking one at the same path.
    let moved = fixture.workspace.join("example-original");
    fs::rename(&fixture.project, &moved).unwrap();
    fs::create_dir(&fixture.project).unwrap();

    let error = session.apply(plan.plan_id).unwrap_err().to_string();
    assert!(
        error.contains("project root") || error.contains("path security"),
        "{error}"
    );
    assert!(!fixture.project.join("AGENTS.md").exists());
    assert!(!moved.join("AGENTS.md").exists());
}

#[test]
fn a_plan_that_writes_outside_the_project_is_refused() {
    struct ExternalBuilder(FixtureBuilder);
    impl PlanBuilder for ExternalBuilder {
        fn build(&self, request: &PlanRequest<'_>) -> Result<BuiltPlan, AppError> {
            let mut built = self.0.build(request)?;
            built.plan.operations[0].external = true;
            Ok(built)
        }
    }
    let fixture = fixture();
    let service = SetupService::new(Arc::new(DeniedCredentialStore)).with_plan_builder(Arc::new(
        ExternalBuilder(FixtureBuilder {
            source: fixture.source.identity.clone(),
        }),
    ));
    let session =
        service.open_session(SessionConfig::new(&fixture.workspace, &fixture.state).unwrap());
    let reviewed = confirmed_analysis(&fixture, &session);
    assert!(session
        .build_plan(&reviewed.state)
        .unwrap_err()
        .to_string()
        .contains("outside the project root"));
}

#[test]
fn a_builder_cannot_drop_the_analysis_or_the_root_binding() {
    struct DroppingBuilder {
        inner: FixtureBuilder,
        drop_analysis: bool,
    }
    impl PlanBuilder for DroppingBuilder {
        fn build(&self, request: &PlanRequest<'_>) -> Result<BuiltPlan, AppError> {
            let mut built = self.inner.build(request)?;
            if self.drop_analysis {
                built.plan.codex_analysis = None;
            } else {
                built.plan.transaction.project_root_identity = Some("unix:1:1".into());
            }
            Ok(built)
        }
    }
    let fixture = fixture();
    for drop_analysis in [true, false] {
        let service = SetupService::new(Arc::new(DeniedCredentialStore)).with_plan_builder(
            Arc::new(DroppingBuilder {
                inner: FixtureBuilder {
                    source: fixture.source.identity.clone(),
                },
                drop_analysis,
            }),
        );
        let session =
            service.open_session(SessionConfig::new(&fixture.workspace, &fixture.state).unwrap());
        let reviewed = confirmed_analysis(&fixture, &session);
        let error = session.build_plan(&reviewed.state).unwrap_err().to_string();
        let expected = if drop_analysis {
            "does not carry the confirmed analysis"
        } else {
            "not bound to the verified project-root identity"
        };
        assert!(error.contains(expected), "{error}");
    }
}

#[test]
fn a_service_without_a_plan_builder_plans_nothing() {
    let fixture = fixture();
    let service = SetupService::new(Arc::new(DeniedCredentialStore));
    let session =
        service.open_session(SessionConfig::new(&fixture.workspace, &fixture.state).unwrap());
    let error = session
        .build_plan(&reviewed_state(&fixture.project, Uuid::new_v4()))
        .unwrap_err()
        .to_string();
    assert!(error.contains("no plan builder"), "{error}");
}

#[test]
fn rollback_is_the_only_recovery_after_a_partial_apply() {
    let fixture = fixture();
    let session = fixture.session();
    let reviewed = confirmed_analysis(&fixture, &session);
    let plan = session.build_plan(&reviewed.state).unwrap();
    session
        .approve_plan(plan.plan_id, &plan_sha256(&plan).unwrap())
        .unwrap();

    // Interrupt the transaction after its first operation reached the
    // project, as a crash between operations would.
    let (approved_plan, files) = {
        let plans = session.state.plans.lock().unwrap();
        let prepared = plans.get(&plan.plan_id).unwrap();
        (prepared.plan.clone(), prepared.prepared_files.clone())
    };
    let interrupted = run_transaction(
        &fixture.project,
        &approved_plan,
        &files,
        &TransactionOptions {
            app_data_root: Some(fixture.state.clone()),
            fail_after_operation: Some(0),
            ..Default::default()
        },
    );
    assert!(interrupted.is_err());
    assert!(fixture.project.join("AGENTS.md").exists());

    let status = session
        .inspect_recovery(&fixture.project)
        .unwrap()
        .expect("the interrupted transaction is found");
    assert_eq!(status.allowed_action, Some(RecoveryAction::Rollback));
    assert_eq!(status.journal.transaction_id, plan.plan_id);

    // Nothing but rollback is offered, and the others refuse.
    for refused in [RecoveryAction::Resume, RecoveryAction::DiscardStaging] {
        let error = session
            .recover(&fixture.project, plan.plan_id, refused)
            .unwrap_err()
            .to_string();
        assert!(error.contains("only rollback is"), "{error}");
        assert!(fixture.project.join("AGENTS.md").exists());
    }
    // A new application is refused while the incomplete transaction waits.
    let second = session.apply(plan.plan_id);
    assert!(second.is_err());

    session
        .recover(&fixture.project, plan.plan_id, RecoveryAction::Rollback)
        .unwrap();
    assert!(!fixture.project.join("AGENTS.md").exists());
    assert!(session
        .inspect_recovery(&fixture.project)
        .unwrap()
        .is_none());
}

#[test]
fn a_journal_bound_to_another_root_is_not_recoverable_through_this_one() {
    let fixture = fixture();
    let session = fixture.session();
    let reviewed = confirmed_analysis(&fixture, &session);
    let plan = session.build_plan(&reviewed.state).unwrap();
    session
        .approve_plan(plan.plan_id, &plan_sha256(&plan).unwrap())
        .unwrap();
    let (approved_plan, files) = {
        let plans = session.state.plans.lock().unwrap();
        let prepared = plans.get(&plan.plan_id).unwrap();
        (prepared.plan.clone(), prepared.prepared_files.clone())
    };
    let _ = run_transaction(
        &fixture.project,
        &approved_plan,
        &files,
        &TransactionOptions {
            app_data_root: Some(fixture.state.clone()),
            fail_after_operation: Some(0),
            ..Default::default()
        },
    );
    let other = fixture.workspace.join("other");
    fs::create_dir(&other).unwrap();
    let error = session
        .read_journal(&other, plan.plan_id)
        .unwrap_err()
        .to_string();
    assert!(error.contains("different project root"), "{error}");
    assert!(session
        .recover(&other, plan.plan_id, RecoveryAction::Rollback)
        .is_err());
    assert!(session.inspect_recovery(&other).unwrap().is_none());
    assert!(fixture.project.join("AGENTS.md").exists());
}

/// Counts reads and refuses them, so a test sees which store was consulted
/// without any network request being sent.
#[derive(Default)]
struct CountingStore {
    reads: AtomicUsize,
}

impl CredentialStore for CountingStore {
    fn save(&self, _: &str, _: &str) -> Result<CredentialReference, AppError> {
        unreachable!("the service never saves a credential")
    }

    fn save_scoped(&self, _: &str, _: &str, _: &str) -> Result<CredentialReference, AppError> {
        unreachable!("the service never saves a credential")
    }

    fn read(&self, _: &CredentialReference) -> Result<String, AppError> {
        self.reads.fetch_add(1, Ordering::SeqCst);
        Err(AppError::Credential("counted and refused".into()))
    }

    fn delete(&self, _: &CredentialReference) -> Result<(), AppError> {
        unreachable!("the service never deletes a credential")
    }
}

#[test]
fn analysis_reads_credentials_only_from_the_injected_store() {
    let fixture = fixture();
    let profile = ai::profile("claude").unwrap();
    let reference =
        save_ai_provider_key(&MemoryCredentialStore::default(), "claude", "sk-fixture").unwrap();
    let store = Arc::new(CountingStore::default());
    let service = SetupService::new(store.clone());
    let session =
        service.open_session(SessionConfig::new(&fixture.workspace, &fixture.state).unwrap());
    let request = AiAnalysisRequest {
        provider: "claude".into(),
        model: profile.default_model.clone().unwrap(),
        reasoning_effort: "high".into(),
        endpoint: profile.default_endpoint.clone().unwrap(),
        analysis: crate::codex::CodexAnalysisRequest {
            mode: "new_project_identity".into(),
            brief: "A historical mod".into(),
            evidence: vec![],
            constraints: json!({}),
            analysis_purpose: None,
            project_root: None,
            scan_id: None,
        },
    };
    let error = session
        .analyze(request.clone(), Some(reference.clone()), &fixture.source)
        .unwrap_err()
        .to_string();
    assert!(error.contains("counted and refused"), "{error}");
    assert_eq!(store.reads.load(Ordering::SeqCst), 1);

    // A reference scoped to another provider is refused before any read.
    let other =
        save_ai_provider_key(&MemoryCredentialStore::default(), "gemini", "sk-fixture").unwrap();
    assert!(session
        .analyze(request, Some(other), &fixture.source)
        .is_err());
    assert_eq!(store.reads.load(Ordering::SeqCst), 1);
}

#[test]
fn account_tool_providers_are_desktop_only() {
    let fixture = fixture();
    let session = fixture.session();
    for provider in ["codex", crate::claude_code::PROVIDER_ID, "gemini_account"] {
        let mut request = analysis_request(&fixture.project, Uuid::new_v4(), vec![]);
        request.provider = provider.into();
        let error = session
            .analyze(request, None, &fixture.source)
            .unwrap_err()
            .to_string();
        assert!(error.contains("desktop-only"), "{provider}: {error}");
    }
}

#[test]
fn a_reply_cannot_choose_its_identity_or_dodge_the_input_binding() {
    let fixture = fixture();
    let session = fixture.session();
    let scan = session
        .inspect(&fixture.project, InspectOptions::default(), |_| {})
        .unwrap();
    let evidence = evidence_from(&scan);
    session
        .approve_evidence(&fixture.project, scan.scan_id, &evidence)
        .unwrap();
    let root = session.existing_project_path(&fixture.project).unwrap();
    let request = analysis_request(&root, scan.scan_id, evidence);
    let turn = session
        .analysis_turn(&request, None, &fixture.source)
        .unwrap();

    // A reply written for a different input is rejected.
    assert!(session
        .record_analysis(
            request.clone(),
            None,
            &fixture.source,
            provider_reply(&"f".repeat(64))
        )
        .is_err());
    // A reply that names its own identifier gets the core's instead.
    let mut reply = provider_reply(&turn.input_sha256);
    reply["analysis_id"] = json!("11111111-1111-4111-8111-111111111111");
    let result = session
        .record_analysis(request, None, &fixture.source, reply)
        .unwrap();
    assert_ne!(
        result.record.analysis_id.to_string(),
        "11111111-1111-4111-8111-111111111111"
    );
    assert_eq!(
        result.record.source_revision.as_deref(),
        Some(fixture.source.identity.resolved_revision.as_str())
    );
}

#[test]
fn the_root_identity_changes_when_the_directory_is_replaced() {
    let fixture = fixture();
    let first = root_identity(&fixture.project).unwrap();
    assert_eq!(first, root_identity(&fixture.project).unwrap());
    assert_ne!(first, root_identity(&fixture.workspace).unwrap());
    assert!(root_identity(Path::new("relative")).is_err());
    fs::rename(&fixture.project, fixture.workspace.join("moved")).unwrap();
    fs::create_dir(&fixture.project).unwrap();
    assert_ne!(first, root_identity(&fixture.project).unwrap());
}

#[test]
fn a_state_root_inside_the_workspace_cannot_hide_a_project() {
    // The state root must stay disjoint from the workspace, so a transaction
    // can never be journaled inside the project it protects.
    let fixture = fixture();
    let nested = fixture.workspace.join("state");
    fs::create_dir(&nested).unwrap();
    assert!(SessionConfig::new(&fixture.workspace, &nested).is_err());
}

#[test]
fn a_host_can_share_the_service_and_its_sessions_between_threads() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<SetupService>();
    assert_send_sync::<SetupSession>();
}
