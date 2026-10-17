// Opt-in, offline acceptance against the committed package checkout. This
// exercises real planning, adaptation, staging, apply, readiness and rollback;
// only setup-assistant responses and external installers are deterministic fakes.
use super::tests::{test_portrait_config, test_state_guard};
use super::*;

mod offline_lifecycle {
    use super::*;
    use crate::transaction::{run_transaction, PostInstallActionOutcome, TransactionOptions};
    use serde_json::json;
    use tempfile::tempdir;

    #[test]
    fn gptimage_plan_uses_the_declared_script_and_preserves_reviewed_arguments() {
        let manifest: RemoteManifest = serde_json::from_slice(include_bytes!(
            "../../docs/source-manifest/hoi4-mod-setup.manifest.json"
        ))
        .unwrap();
        let actions = manifest_external_actions(&manifest, &["imagegen.gptimage".into()]);
        assert_eq!(actions.len(), 1);
        let action = &actions[0];
        assert_eq!(action.arguments.len(), 7);
        assert_eq!(action.arguments[0], IMAGEGEN_GPTIMAGE_BOOTSTRAP);
        assert_eq!(action.arguments[1], "--package-root");
        assert_eq!(
            action.arguments[2],
            "<project_root>/.tools/image_generation/providers/gptimage"
        );
        assert_eq!(action.verified_package_name.as_deref(), Some("gptimage"));
        assert_eq!(action.verified_package_version.as_deref(), Some("0.1.0"));
        assert_eq!(action.privilege, "current_user");
        assert!(action.requires_approval);
        assert!(!action.contains_secret);
        if matches!(
            Platform::current(),
            Platform::Windows | Platform::Macos | Platform::Linux
        ) {
            let mut reordered = manifest;
            reordered
                .components
                .iter_mut()
                .find(|component| component.id == "imagegen.gptimage")
                .unwrap()
                .platforms = vec![
                ManifestPlatform::Macos,
                ManifestPlatform::Windows,
                ManifestPlatform::Linux,
            ];
            let reordered_actions =
                manifest_external_actions(&reordered, &["imagegen.gptimage".into()]);
            assert_eq!(reordered_actions[0].platform, Platform::current());
        }
    }

    #[test]
    fn fresh_planning_uses_the_resolved_manifest_default_when_primary_is_omitted() {
        let manifest: RemoteManifest = serde_json::from_slice(include_bytes!(
            "../../docs/source-manifest/hoi4-mod-setup.manifest.json"
        ))
        .unwrap();
        let (requested, selection) = requested_components_for_selection(
            &manifest,
            &serde_json::json!({"selectedComponents": []}),
            "codex",
        )
        .unwrap();
        assert_eq!(
            selection.primary,
            coding_environment::default_primary(&manifest)
        );
        assert_eq!(selection.primary, "claude_code");
        let selected = expand_components(&manifest, &requested).unwrap();
        assert!(selected.iter().any(|id| id == "core.claude.instructions"));
        let native = manifest
            .components
            .iter()
            .filter(|component| {
                component.coding_environment.as_deref() == Some(selection.primary.as_str())
                    && !component.optional
            })
            .collect::<Vec<_>>();
        assert!(!native.is_empty());
        assert!(native
            .iter()
            .all(|component| selected.contains(&component.id)));
    }

    struct IsolatedProfile {
        _root: tempfile::TempDir,
        previous: Vec<(&'static str, Option<std::ffi::OsString>)>,
    }

    impl IsolatedProfile {
        fn new() -> Self {
            assert!(
                crate::local_source::configured_repository().is_some(),
                "Set HOI4_MOD_SETUP_SOURCE_REPOSITORY to an explicit committed package checkout."
            );
            let root = tempdir().unwrap();
            ISOLATE_MESHY_VAULT_FOR_TEST.with(|isolated| isolated.set(true));
            let mut previous = Vec::new();
            for name in [
                "HOME",
                "USERPROFILE",
                "APPDATA",
                "LOCALAPPDATA",
                "XDG_CONFIG_HOME",
                "XDG_DATA_HOME",
                "XDG_CACHE_HOME",
            ] {
                previous.push((name, std::env::var_os(name)));
                let path = root.path().join(name);
                std::fs::create_dir(&path).unwrap();
                std::env::set_var(name, path);
            }
            Self {
                _root: root,
                previous,
            }
        }
    }

    impl Drop for IsolatedProfile {
        fn drop(&mut self) {
            ISOLATE_MESHY_VAULT_FOR_TEST.with(|isolated| isolated.set(false));
            for (name, value) in &self.previous {
                if let Some(value) = value {
                    std::env::set_var(name, value);
                } else {
                    std::env::remove_var(name);
                }
            }
            clear_codex_analysis_state().unwrap();
            clear_approved_scan_evidence().unwrap();
        }
    }

    fn fake_external_actions(
        _root: &Path,
        _plan: &InstallationPlan,
        component: &str,
    ) -> Result<PostInstallActionOutcome, AppError> {
        Ok(PostInstallActionOutcome {
            component_id: component.into(),
            state: if component == "workflow.3d" {
                "incomplete"
            } else {
                "ready"
            }
            .into(),
            evidence: "Offline fixture: no external process started; 3D key deliberately absent."
                .into(),
        })
    }

    fn state(root: &Path, mode: &str) -> Value {
        serde_json::json!({
            "mode": mode, "description": "A synthetic Atlantis mod for lifecycle verification.",
            "sourceMode": "latest", "aiProvider": "codex", "aiModel": "fixture-model",
            "aiReasoningEffort": "xhigh", "gitMode": "skip",
            "meshSelected": true, "meshKeyStatus": "missing", "superEventsSelected": true,
            "primaryCodingEnvironment": "claude_code",
            "additionalCodingEnvironments": ["codex", "cursor", "qoder", "opencode"],
            "selectedComponents": if Platform::current() == Platform::Windows {
                json!(["workflow.3d", "workflow.super_events", "workflow.live_tools"])
            } else {
                json!(["workflow.3d", "workflow.super_events", "workflow.live_testing"])
            },
            "liveTesting": {"provider": if Platform::current() == Platform::Windows { "hoi4_live_tools" } else { "clausewitz_mcp" }, "linux_ready": false},
            "portraitPipeline": test_portrait_config("cloud", true),
            "folderProfile": ["events", "common", "localisation/english", "gfx", "interface", "docs"],
            "conventions": {"agents_profile": "default", "localisation_convention": "english", "documentation_convention": "markdown"},
            "identity": {"displayName": "Atlantis Rising", "projectId": "atlantis_rising", "author": "Fixture",
                "version": "0.1.0", "supportedGameVersion": "1.19.*", "defaultBranch": "main",
                "scriptPrefix": "atl", "primaryNamespace": "atl", "descriptorTags": ["Alternative History"],
                "projectRoot": root, "launcherDescriptorPath": root.parent().unwrap().join("atlantis_rising.mod")}
        })
    }

    // Seed the same confirmed, digest-bound session record that a fake
    // provider would return. No real account/key or provider request is used.
    fn confirm(state: &mut Value) {
        let client = HttpSourceClient::new().unwrap();
        let resolution =
            resolve_source(&client, &source_request_from_state(state).unwrap()).unwrap();
        let root = project_root_from_state(state).unwrap();
        let scan_id = if root.exists() {
            let snapshot = crate::scanner::scan_project(&root, &Default::default()).unwrap();
            assert!(snapshot.read_only && !snapshot.partial);
            *codex_approved_evidence().lock().unwrap() = ApprovedScanEvidence {
                project_root: Some(root.clone()),
                project_root_identity: Some(
                    RootedDir::open_read(&root)
                        .unwrap()
                        .identity_token()
                        .unwrap(),
                ),
                scan_id: Some(snapshot.scan_id),
                entries: HashMap::new(),
                evidence_sha256: None,
            };
            Some(snapshot.scan_id)
        } else {
            None
        };
        let analysis_id = Uuid::new_v4();
        let record = CodexAnalysisRecord {
            engine: "codex_app_server".into(),
            auth_mode: "chatgpt".into(),
            provider: Some("codex".into()),
            model: Some("fixture-model".into()),
            reasoning_effort: Some("xhigh".into()),
            optimization_profile: Some(ai_optimization_profile("codex")),
            analysis_id,
            schema_version: "1.0.0".into(),
            input_sha256: "a".repeat(64),
            output_sha256: "b".repeat(64),
            confirmed_fields: crate::codex::REQUIRED_ANALYSIS_PROPOSAL_KEYS
                .iter()
                .map(|key| (*key).into())
                .collect(),
            confirmed_at: chrono::Utc::now().to_rfc3339(),
            account_identity_persisted: false,
            analysis_purpose: root.exists().then(|| "existing_project_import".into()),
            project_root: Some(root.display().to_string()),
            scan_id,
            evidence_sha256: Some("c".repeat(64)),
            source_revision: Some(resolution.identity.resolved_revision),
            source_manifest_sha256: Some(resolution.identity.manifest_sha256),
        };
        let values_hash =
            confirmation_values_sha256(&confirmation_values_from_state(state).unwrap()).unwrap();
        codex_analyses().lock().unwrap().insert(
            analysis_id,
            PendingCodexAnalysis {
                analysis: CodexAnalysis {
                    schema_version: "1.0.0".into(),
                    analysis_id,
                    mode: crate::codex::AnalysisMode::NewProjectIdentity,
                    input_sha256: record.input_sha256.clone(),
                    project_summary: "Synthetic offline fixture".into(),
                    proposals: Vec::new(),
                    component_recommendations: Vec::new(),
                    warnings: Vec::new(),
                },
                record: record.clone(),
                confirmed: Some(record.clone()),
                project_root: Some(root),
                scan_id,
                endpoint_fingerprint: None,
                confirmed_values_sha256: Some(values_hash),
            },
        );
        state["codexAnalysisRecord"] = serde_json::to_value(record).unwrap();
        if let Some(id) = scan_id {
            state["scanContext"] = serde_json::json!({"scanId": id});
        }
    }

    fn apply(
        root: &Path,
        plan: &mut InstallationPlan,
        prepared: &[PreparedFile],
        app: &Path,
    ) -> (TransactionJournal, InstallationLock) {
        plan.approvals.dry_run_reviewed = true;
        plan.approvals.external_actions_reviewed = true;
        run_transaction(
            root,
            plan,
            prepared,
            &TransactionOptions {
                app_data_root: Some(app.to_path_buf()),
                post_install_action_runner: Some(fake_external_actions),
                ..Default::default()
            },
        )
        .unwrap_or_else(|error| panic!("offline lifecycle failed: {error}"))
    }

    fn maintenance_plan(
        original: &InstallationPlan,
        lock: &InstallationLock,
        root: &Path,
        mode: &str,
        operations: Vec<PlanOperation>,
        bytes: &[PreparedFile],
    ) -> (InstallationPlan, Vec<PreparedFile>) {
        let mut plan = original.clone();
        plan.plan_id = Uuid::new_v4();
        plan.maintenance_mode = Some(mode.into());
        plan.operations = operations;
        plan.generated_artifacts.clear();
        plan.conflicts.clear();
        plan.transaction.project_root_mode = ProjectRootMode::Existing;
        plan.transaction.project_root_parent = None;
        plan.transaction.project_root_leaf = None;
        plan.transaction.project_root_identity = Some(
            RootedDir::open_read(root)
                .unwrap()
                .identity_token()
                .unwrap(),
        );
        plan.transaction.directories.clear();
        plan.download_ledger.clear();
        let mut prepared = Vec::new();
        for operation in &mut plan.operations {
            if operation.action == OperationAction::Skip
                && (operation.local_state == LocalState::Modified
                    || matches!(
                        operation.resolution.as_deref(),
                        Some("review_required" | "merged_base_required" | "obsolete_review")
                    ))
            {
                // An explicit reviewed Keep decision, rather than deleting an
                // unresolved conflict to get through transaction validation.
                operation.resolution = Some("keep".into());
                plan.conflicts.push(PlanConflict {
                    id: format!("review-{}", operation.id),
                    path: operation.destination.clone(),
                    options: vec!["keep".into(), "replace".into()],
                    selected: Some("keep".into()),
                    apply_to_identical: false,
                });
            }
            if mode != "remove" {
                if let Some(mut ledger) = original
                    .download_ledger
                    .iter()
                    .find(|file| file.destination == operation.destination)
                    .cloned()
                {
                    ledger.operation_id = operation.id.clone();
                    plan.download_ledger.push(ledger);
                }
            }
            if mode != "remove" && operation.action != OperationAction::DeleteManaged {
                let Some(mut incoming) = bytes
                    .iter()
                    .find(|file| file.destination == operation.destination)
                    .cloned()
                else {
                    continue;
                };
                incoming.operation_id = operation.id.clone();
                // Match the production maintenance planner: adapted incoming
                // bytes have a result hash even when a healthy file is skipped.
                // The live Skip precondition remains its separate local hash.
                operation.result_sha256 = Some(incoming.expected_sha256.clone());
                prepared.push(incoming);
            }
        }
        // Removal is locally available without setup-assistant or external actions.
        if mode == "remove" {
            plan.external_actions.clear();
            plan.credential_references.clear();
        }
        assert_eq!(
            plan.primary_coding_environment,
            lock.primary_coding_environment
        );
        crate::transaction::bind_plan_external_parents(&mut plan).unwrap();
        (plan, prepared)
    }

    // Real, opt-in installers and MCP handshakes. Only semantic analysis is a
    // fixture; no account, image generation, paid operation or gameplay call.
    #[test]
    #[ignore = "real GPTImage dependencies require an explicit committed package checkout"]
    fn live_gptimage_installer_from_verified_source() {
        let _guard = test_state_guard();
        let _profile = IsolatedProfile::new();
        let root = tempdir().unwrap();
        let client = HttpSourceClient::new().unwrap();
        let resolution = resolve_source(
            &client,
            &SourceRequest {
                mode: SourceMode::Latest,
                requested_ref: None,
                release: None,
            },
        )
        .unwrap();
        let files = crate::source::select_component_files(
            &resolution.manifest,
            &[IMAGEGEN_GPTIMAGE_COMPONENT_ID.into()],
            &client
                .fetch_tree(&resolution.identity.resolved_revision)
                .unwrap(),
        )
        .unwrap();
        let mut plan: InstallationPlan = serde_json::from_str(include_str!(
            "../../docs/examples/installation-plan.example.json"
        ))
        .unwrap();
        let template = plan.operations[0].clone();
        plan.operations.clear();
        plan.source = resolution.identity.clone();
        plan.external_actions = manifest_external_actions(
            &resolution.manifest,
            &[IMAGEGEN_GPTIMAGE_COMPONENT_ID.into()],
        );
        for file in files {
            let hash = file.expected_sha256.as_ref().unwrap();
            let bytes = client
                .fetch_verified_file(
                    &resolution.identity.resolved_revision,
                    &file.source_path,
                    hash,
                    file.expected_size,
                )
                .unwrap();
            let destination = safe_join(root.path(), &file.destination).unwrap();
            std::fs::create_dir_all(destination.parent().unwrap()).unwrap();
            std::fs::write(destination, bytes).unwrap();
            let mut operation = template.clone();
            operation.component_id = file.component_id;
            operation.destination = file.destination;
            operation.source_sha256 = Some(hash.clone());
            operation.result_sha256 = Some(hash.clone());
            operation.source_size = file.expected_size;
            plan.operations.push(operation);
        }
        let outcome = run_reviewed_gptimage_bootstrap(root.path(), &plan).unwrap();
        eprintln!(
            "GPTImage real installer: {}: {}",
            outcome.state, outcome.evidence
        );
        assert_eq!(outcome.state, "ready", "{}", outcome.evidence);
    }

    #[test]
    #[ignore = "real Agent Tools installation requires an explicit committed package checkout"]
    fn live_mcp_installer_from_verified_source() {
        let _guard = test_state_guard();
        let _profile = IsolatedProfile::new();
        let root = tempdir().unwrap();
        std::fs::write(
            root.path().join("descriptor.mod"),
            "name=\"MCP fixture\"\nsupported_version=\"1.19.*\"\n",
        )
        .unwrap();
        let client = HttpSourceClient::new().unwrap();
        let resolution = resolve_source(
            &client,
            &SourceRequest {
                mode: SourceMode::Latest,
                requested_ref: None,
                release: None,
            },
        )
        .unwrap();
        let selected = vec![
            "mcp.hoi4_agent_tools.bootstrap".into(),
            "mcp.hoi4_agent_tools".into(),
        ];
        let files = crate::source::select_component_files(
            &resolution.manifest,
            &selected,
            &client
                .fetch_tree(&resolution.identity.resolved_revision)
                .unwrap(),
        )
        .unwrap();
        let mut plan: InstallationPlan = serde_json::from_str(include_str!(
            "../../docs/examples/installation-plan.example.json"
        ))
        .unwrap();
        let template = plan.operations[0].clone();
        plan.operations.clear();
        plan.source = resolution.identity.clone();
        plan.external_actions = manifest_external_actions(&resolution.manifest, &selected);
        for file in files {
            let hash = file.expected_sha256.as_ref().unwrap();
            let bytes = client
                .fetch_verified_file(
                    &resolution.identity.resolved_revision,
                    &file.source_path,
                    hash,
                    file.expected_size,
                )
                .unwrap();
            let destination = safe_join(root.path(), &file.destination).unwrap();
            std::fs::create_dir_all(destination.parent().unwrap()).unwrap();
            std::fs::write(destination, bytes).unwrap();
            let mut operation = template.clone();
            operation.component_id = file.component_id;
            operation.destination = file.destination;
            operation.source_sha256 = Some(hash.clone());
            operation.result_sha256 = Some(hash.clone());
            operation.source_size = file.expected_size;
            plan.operations.push(operation);
        }
        let outcome = run_reviewed_mcp_bootstrap(root.path(), &plan).unwrap();
        eprintln!(
            "Agent Tools real installer: {}: {}",
            outcome.state, outcome.evidence
        );
        assert_eq!(outcome.state, "ready", "{}", outcome.evidence);
    }

    #[test]
    #[ignore = "live scratch acceptance installs published MCP packages into isolated profiles"]
    fn live_scratch_install_starts_every_registered_server_for_claude_and_codex() {
        use std::io::{BufRead, BufReader, Write};
        use std::process::{Command, Stdio};
        use std::sync::mpsc;
        use std::time::Duration;

        let _guard = test_state_guard();
        let _profile = IsolatedProfile::new();
        let revision = resolve_source(
            &HttpSourceClient::new().unwrap(),
            &SourceRequest {
                mode: SourceMode::Latest,
                requested_ref: None,
                release: None,
            },
        )
        .unwrap()
        .identity
        .resolved_revision;
        for primary in ["claude_code", "codex"] {
            let parent = tempdir().unwrap();
            let app = tempdir().unwrap();
            let root = parent.path().join("atlantis_rising");
            let mut selection = state(&root, "new");
            selection["sourceMode"] = json!("pinned_commit");
            selection["pinnedRef"] = json!(revision);
            selection["primaryCodingEnvironment"] = json!(primary);
            selection["additionalCodingEnvironments"] = json!([if primary == "codex" {
                "claude_code"
            } else {
                "codex"
            }]);
            selection["selectedComponents"] = json!([]);
            selection["meshSelected"] = json!(false);
            selection["superEventsSelected"] = json!(false);
            selection["portraitPipeline"] = json!({"enabled":false,"provider":"disabled"});
            selection["liveTesting"] = json!({"provider":"none"});
            selection["imageGeneration"] =
                json!({"primary_provider":"gptimage","additional_providers":[]});
            eprintln!("scratch: {primary}: confirming the pinned source");
            confirm(&mut selection);
            eprintln!("scratch: {primary}: preparing the install plan");
            let (mut plan, prepared) = build_plan(&selection).unwrap();
            eprintln!(
                "scratch: {primary}: applying {} files and real runtime installers",
                prepared.len()
            );
            plan.approvals.dry_run_reviewed = true;
            plan.approvals.external_actions_reviewed = true;
            let (journal, lock) = run_transaction(
                &root,
                &plan,
                &prepared,
                &TransactionOptions {
                    app_data_root: Some(app.path().to_owned()),
                    post_install_action_runner: Some(run_reviewed_post_install_actions),
                    ..Default::default()
                },
            )
            .unwrap();
            assert_eq!(journal.state, "completed");
            eprintln!(
                "scratch: {primary}: transaction completed; checking installed skills and servers"
            );
            assert_eq!(lock.primary_coding_environment, primary);
            assert!(
                lock.image_generation
                    .as_ref()
                    .unwrap()
                    .installed_package_tree_sha256
                    .is_some(),
                "GPTImage installer did not finish: {:?}",
                journal.stages[9].evidence
            );
            let target = crate::mcp::reviewed_plan_target(&plan.external_actions).unwrap();
            let verified_tools = format!("verified_tools={}", target.required_tools.len());
            assert!(
                journal.stages[9].evidence.iter().any(|evidence| {
                    evidence.starts_with("external-action:mcp.hoi4_agent_tools:ready:")
                        && evidence.contains(&verified_tools)
                }),
                "the production installer did not complete the real MCP handshake"
            );
            for file in lock.files.iter().filter(|file| !file.external) {
                assert!(
                    root.join(&file.path).is_file(),
                    "missing installed file {}",
                    file.path
                );
            }
            let skills = lock
                .files
                .iter()
                .filter(|file| {
                    file.path.starts_with(".claude/skills/") && file.path.ends_with("/SKILL.md")
                })
                .collect::<Vec<_>>();
            assert!(!skills.is_empty());
            for file in &skills {
                let projection = file.path.replacen(".claude/skills/", ".agents/skills/", 1);
                assert!(
                    root.join(projection).is_file(),
                    "missing Codex skill projection {}",
                    file.path
                );
            }
            let claude: Value =
                serde_json::from_slice(&std::fs::read(root.join(".mcp.json")).unwrap()).unwrap();
            let codex: toml::Value =
                toml::from_str(&std::fs::read_to_string(root.join(".codex/config.toml")).unwrap())
                    .unwrap();
            let expected = ["hoi4_agent_tools", "hoi4_image_gptimage"];
            let claude_servers = claude["mcpServers"].as_object().unwrap();
            let codex_servers = codex["mcp_servers"].as_table().unwrap();
            assert_eq!(claude_servers.len(), expected.len());
            assert_eq!(codex_servers.len(), expected.len());
            for server in expected {
                assert!(claude_servers.contains_key(server));
                assert!(codex_servers.contains_key(server));
            }
            let mut command =
                Command::new(crate::process::find_path_executable(&["node.exe", "node"]).unwrap());
            command
                .arg(root.join(".tools/image_generation/providers/gptimage/src/server.js"))
                .current_dir(&root)
                .env_clear()
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::null());
            for name in [
                "PATH",
                "SystemRoot",
                "HOME",
                "USERPROFILE",
                "APPDATA",
                "LOCALAPPDATA",
                "TEMP",
                "TMP",
            ] {
                if let Some(value) = std::env::var_os(name) {
                    command.env(name, value);
                }
            }
            #[cfg(windows)]
            {
                use std::os::windows::process::CommandExt;
                command.creation_flags(0x0800_0000);
            }
            let mut child = command.spawn().unwrap();
            let stdout = child.stdout.take().unwrap();
            let (sender, receiver) = mpsc::channel();
            let reader = std::thread::spawn(move || {
                for line in BufReader::new(stdout).lines().take(64) {
                    if sender.send(line).is_err() {
                        break;
                    }
                }
            });
            let stdin = child.stdin.as_mut().unwrap();
            writeln!(stdin, "{}", json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"setup-scratch-acceptance","version":env!("CARGO_PKG_VERSION")}}})).unwrap();
            let handshake = (|| {
                let response: Value = serde_json::from_str(
                    &receiver
                        .recv_timeout(Duration::from_secs(120))
                        .map_err(|_| "GPTImage initialize timed out")?
                        .map_err(|_| "GPTImage stdout failed")?,
                )
                .map_err(|_| "GPTImage emitted invalid JSON")?;
                if response["id"] != 1 || response["result"]["serverInfo"]["name"] != "gptimage" {
                    return Err("GPTImage initialize failed");
                }
                writeln!(
                    stdin,
                    "{}",
                    json!({"jsonrpc":"2.0","method":"notifications/initialized"})
                )
                .map_err(|_| "GPTImage notification failed")?;
                writeln!(
                    stdin,
                    "{}",
                    json!({"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}})
                )
                .map_err(|_| "GPTImage tools/list send failed")?;
                let response: Value = serde_json::from_str(
                    &receiver
                        .recv_timeout(Duration::from_secs(120))
                        .map_err(|_| "GPTImage tools/list timed out")?
                        .map_err(|_| "GPTImage stdout failed")?,
                )
                .map_err(|_| "GPTImage emitted invalid JSON")?;
                if response["id"] != 2
                    || response["result"]["tools"]
                        .as_array()
                        .is_none_or(Vec::is_empty)
                {
                    return Err("GPTImage tools/list failed");
                }
                Ok(())
            })();
            let _ = child.kill();
            let _ = child.wait();
            reader.join().unwrap();
            handshake.unwrap();
            // The journal already proves the real Agent Tools handshake.
            // Check native client binding and readiness without starting it twice.
            let installed_target = installed_mcp_target(&root, &lock).unwrap();
            assert_eq!(installed_target.package_version, target.package_version);
            let mut detected = crate::readiness::project_input(&root, "atlantis_rising").unwrap();
            assert_eq!(detected.mcp_status, "health_not_run");
            assert!(detected.mcp_blocking);
            detected.mcp_status = "pass".into();
            let report = crate::readiness::evaluate(&detected);
            assert!(
                report.core_ready,
                "scratch readiness blocked: {:?}",
                report
                    .checks
                    .iter()
                    .filter(|check| check.blocking && check.status != "pass")
                    .collect::<Vec<_>>()
            );
            eprintln!("scratch install: primary={primary}; skills={}; both clients registered; Agent Tools {} and GPTImage initialize/tools-list passed", skills.len(), target.package_version);
        }
    }

    #[test]
    #[ignore = "offline skill ceiling regression requires an explicit committed package checkout"]
    fn offline_image_guidance_preserves_the_published_feature_assets_skill_byte_ceiling() {
        let _guard = test_state_guard();
        let _profile = IsolatedProfile::new();
        let client = HttpSourceClient::new().unwrap();
        let resolution = resolve_source(
            &client,
            &SourceRequest {
                mode: SourceMode::Latest,
                requested_ref: None,
                release: None,
            },
        )
        .unwrap();
        let source_path = ".claude/skills/hoi4-feature-assets/SKILL.md";
        let evidence = resolution
            .manifest
            .components
            .iter()
            .flat_map(|component| component.expected_files.iter())
            .find(|file| file.path == source_path)
            .unwrap();
        let source = client
            .fetch_verified_file(
                &resolution.identity.resolved_revision,
                source_path,
                evidence.sha256.as_deref().unwrap(),
                evidence.size,
            )
            .unwrap();
        let providers = resolution.manifest.image_generation.as_ref().unwrap()["providers"]
            .as_array()
            .unwrap();
        let image = ImageGenerationSelection {
            primary_provider: "openai_api".into(),
            additional_providers: providers
                .iter()
                .filter_map(|row| row["id"].as_str())
                .filter(|id| *id != "openai_api" && *id != "none")
                .map(str::to_owned)
                .collect(),
            ..Default::default()
        };
        let guided =
            adapt_image_generation_guidance(&source, &resolution.manifest, &image).unwrap();
        let root = tempdir().unwrap();
        let adapted = adapt_image_generation_client_config(
            &guided,
            source_path,
            &resolution.manifest,
            &image,
            &CodingEnvironmentSelection {
                primary: "codex".into(),
                additional: vec![],
            },
            root.path(),
            &LiveTestingSelection {
                provider: "none".into(),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(
            adapted.len() <= SKILL_BYTE_LIMIT,
            "published skill adapted to {} bytes",
            adapted.len()
        );
        assert!(std::str::from_utf8(&adapted)
            .unwrap()
            .contains(IMAGE_GUIDANCE_REFERENCE));
        let reference = image_generation_guidance_artifact(&resolution.manifest, &image)
            .unwrap()
            .unwrap();
        for id in std::iter::once(&image.primary_provider).chain(image.additional_providers.iter())
        {
            assert!(
                reference.content.contains(&format!("`{id}`")),
                "missing {id}"
            );
        }
        assert!(reference.content.contains("one image per call"));
        assert!(reference.content.contains("alpha below 255"));
    }

    #[test]
    #[ignore = "offline acceptance requires an explicit committed package checkout"]
    fn offline_all_component_install_import_repair_remove_and_rollback() {
        let _guard = test_state_guard();
        let _profile = IsolatedProfile::new();
        let parent = tempdir().unwrap();
        let app = tempdir().unwrap();
        let root = parent.path().join("atlantis_rising");
        let mut state = state(&root, "new");
        confirm(&mut state);
        eprintln!("offline: building fresh all-component plan");
        let (mut plan, prepared) = build_plan(&state).unwrap();
        assert!(!root.exists(), "planning must leave the new root absent");
        for id in [
            "mcp.hoi4_agent_tools",
            "workflow.3d",
            "workflow.super_events",
            "workflow.portraits.cloud",
            if Platform::current() == Platform::Windows {
                "workflow.live_tools"
            } else {
                "workflow.live_testing"
            },
        ] {
            assert!(
                plan.selected_components
                    .iter()
                    .any(|selected| selected == id),
                "missing {id}"
            );
        }
        eprintln!("offline: applying {} files", plan.operations.len());
        let (journal, lock) = apply(&root, &mut plan, &prepared, app.path());
        assert_eq!(journal.state, "completed");
        assert_eq!(lock.primary_coding_environment, "claude_code");
        assert_eq!(
            lock.optional_workflows["workflow.3d"].state,
            if Platform::current() == Platform::Windows {
                "incomplete"
            } else {
                "unsupported_platform"
            }
        );
        if Platform::current() == Platform::Windows {
            assert_eq!(
                lock.optional_workflows["workflow.live_tools"].state,
                "ready"
            );
        } else {
            assert_eq!(
                lock.live_testing.as_ref().unwrap().provider,
                "clausewitz_mcp"
            );
            assert!(!lock.live_testing.as_ref().unwrap().linux_ready);
            assert!(!std::fs::read_to_string(root.join(".mcp.json"))
                .unwrap()
                .contains("clausewitz_linux"));
        }
        for path in [
            "CLAUDE.md",
            "AGENTS.md",
            ".claude/skills",
            ".claude/agents",
            ".agents/skills",
            ".codex/agents",
            ".mcp.json",
            "descriptor.mod",
            "thumbnail.png",
        ] {
            assert!(root.join(path).exists(), "missing {path}");
        }
        let note = root.join("events/user_event.txt");
        std::fs::write(&note, "user gameplay content").unwrap();
        let skill = lock
            .files
            .iter()
            .find(|file| {
                file.path.starts_with(".claude/skills/") && file.path.ends_with("/SKILL.md")
            })
            .unwrap();
        let damaged = lock
            .files
            .iter()
            .find(|file| file.path.starts_with("docs/") && file.ownership == Ownership::Managed)
            .unwrap();
        let modified_skill = format!(
            "{}\nUser modification retained.\n",
            std::fs::read_to_string(root.join(&skill.path)).unwrap()
        );
        std::fs::write(root.join(&skill.path), &modified_skill).unwrap();
        std::fs::remove_file(root.join(&damaged.path)).unwrap();
        let operations = crate::transaction::repair_operations(&lock, &root).unwrap();
        assert_eq!(
            operations
                .iter()
                .find(|op| op.destination == skill.path)
                .unwrap()
                .action,
            OperationAction::Skip
        );
        assert_eq!(
            operations
                .iter()
                .find(|op| op.destination == damaged.path)
                .unwrap()
                .action,
            OperationAction::Create
        );
        let (mut repair, repair_bytes) =
            maintenance_plan(&plan, &lock, &root, "repair", operations, &prepared);
        eprintln!("offline: repairing missing documentation, preserving modified skill");
        let (_, repaired) = apply(&root, &mut repair, &repair_bytes, app.path());
        assert!(root.join(&damaged.path).is_file());
        let operations = crate::transaction::managed_removal_operations(&repaired, &root).unwrap();
        let (mut removal, removal_bytes) = maintenance_plan(
            &repair,
            &repaired,
            &root,
            "remove",
            operations,
            &repair_bytes,
        );
        eprintln!("offline: removing unchanged managed files");
        let (mut removed, _) = apply(&root, &mut removal, &removal_bytes, app.path());
        assert_eq!(
            std::fs::read_to_string(root.join(&skill.path)).unwrap(),
            modified_skill
        );
        assert_eq!(
            std::fs::read_to_string(&note).unwrap(),
            "user gameplay content"
        );
        assert!(!root.join(&damaged.path).exists());
        let journal_path = crate::paths::transaction_root(app.path(), removal.plan_id)
            .transaction
            .join("journal.json");
        eprintln!("offline: rolling back removal");
        crate::transaction::rollback_transaction(&root, &mut removed, &journal_path).unwrap();
        assert_eq!(
            serde_json::to_value(read_project_lock(&root).unwrap()).unwrap(),
            serde_json::to_value(&repaired).unwrap()
        );
        assert!(root.join(&damaged.path).is_file());

        // Import into an unmanaged mod, including a real read-only scan and
        // explicit keep decision for its existing project guidance.
        let import_parent = tempdir().unwrap();
        let import_root = import_parent.path().join("atlantis_rising");
        std::fs::create_dir(&import_root).unwrap();
        std::fs::write(
            import_root.join("descriptor.mod"),
            std::fs::read(root.join("descriptor.mod")).unwrap(),
        )
        .unwrap();
        std::fs::write(
            import_root.join("thumbnail.png"),
            std::fs::read(root.join("thumbnail.png")).unwrap(),
        )
        .unwrap();
        std::fs::write(
            import_root.join("AGENTS.md"),
            "# User rules\nPreserve my gameplay.\n",
        )
        .unwrap();
        let mut import = super::offline_lifecycle::state(&import_root, "existing");
        import["conflictChoice"] = Value::String("keep".into());
        confirm(&mut import);
        eprintln!("offline: planning and applying existing-mod import");
        let (mut import_plan, import_bytes) = build_plan(&import).unwrap();
        let (_, imported) = apply(&import_root, &mut import_plan, &import_bytes, app.path());
        assert!(imported.files.iter().any(|file| file.path == "CLAUDE.md"));
        assert_eq!(
            std::fs::read_to_string(import_root.join("AGENTS.md")).unwrap(),
            "# User rules\nPreserve my gameplay.\n"
        );
    }

    #[test]
    fn public_035_manifest_still_validates_with_the_current_schema() {
        let bytes = include_bytes!("../../docs/fixtures/v0.3.5-manifest.json");
        assert_eq!(
            sha256_bytes(bytes),
            "4550334afaea4cb3c6ad6d83219b02cadf3ee60d46112d7e46a352ee889ceb10"
        );
        let manifest =
            crate::source::parse_manifest(bytes, Some("7e50711580a3fa377f1acb834bb19a51128eacd2"))
                .unwrap();
        assert_eq!(manifest.schema_version, "2.0.0");
        assert!(manifest
            .components
            .iter()
            .any(|component| component.destination.path == ".agents/skills/"));
        assert!(manifest
            .components
            .iter()
            .any(|component| component.destination.path == ".codex/agents/"));
    }

    fn incoming_files(plan: &InstallationPlan, bytes: &[PreparedFile]) -> Vec<LockedFile> {
        plan.operations
            .iter()
            .filter_map(|operation| {
                let content = bytes
                    .iter()
                    .find(|file| file.operation_id == operation.id)?;
                Some(LockedFile {
                    path: operation.destination.clone(),
                    location_scope: operation.location_scope.clone(),
                    component_id: operation.component_id.clone(),
                    source_path: operation.source_path.clone().unwrap(),
                    source_revision: plan.source.resolved_revision.clone(),
                    source_sha256: operation.source_sha256.clone().unwrap(),
                    source_size: operation.source_size,
                    base_sha256: None,
                    installed_sha256: content.expected_sha256.clone(),
                    installed_size: Some(content.bytes.len() as u64),
                    ownership: operation.ownership.unwrap(),
                    preserved_local: false,
                    external: operation.external,
                    generated_content: None,
                    generated_bytes: None,
                    executable: operation.executable,
                    platform: operation.platform,
                    installer_created: false,
                })
            })
            .collect()
    }

    #[test]
    #[ignore = "offline upgrade acceptance requires the explicit checkout to retain public source history"]
    fn offline_public_035_upgrade_preserves_codex_and_modified_files_and_rolls_back() {
        let _guard = test_state_guard();
        let _profile = IsolatedProfile::new();
        let parent = tempdir().unwrap();
        let app = tempdir().unwrap();
        let root = parent.path().join("atlantis_rising");
        let mut legacy_state = state(&root, "new");
        legacy_state["sourceMode"] = serde_json::json!("pinned_commit");
        legacy_state["pinnedRef"] = serde_json::json!("0bb4917dca228886aa9a52963bc09e7078666d64");
        legacy_state["primaryCodingEnvironment"] = serde_json::json!("codex");
        legacy_state["additionalCodingEnvironments"] = serde_json::json!([]);
        legacy_state["selectedComponents"] = serde_json::json!([]);
        legacy_state["meshSelected"] = serde_json::json!(false);
        legacy_state["superEventsSelected"] = serde_json::json!(false);
        legacy_state["portraitPipeline"] =
            serde_json::to_value(test_portrait_config("disabled", false)).unwrap();
        confirm(&mut legacy_state);
        let (mut legacy_plan, legacy_bytes) = build_plan(&legacy_state).unwrap();
        assert_eq!(
            legacy_plan.source.manifest_sha256,
            "4550334afaea4cb3c6ad6d83219b02cadf3ee60d46112d7e46a352ee889ceb10"
        );
        let (_, legacy_lock) = apply(&root, &mut legacy_plan, &legacy_bytes, app.path());
        assert!(root.join(".agents/skills").is_dir());
        assert!(root.join(".codex/agents").is_dir());
        assert!(!root.join("CLAUDE.md").exists());
        let mut target_state = legacy_state;
        target_state["mode"] = serde_json::json!("existing");
        target_state["sourceMode"] = serde_json::json!("latest");
        target_state["conflictChoice"] = serde_json::json!("replace");
        confirm(&mut target_state);
        let (target_plan, target_bytes) = build_plan(&target_state).unwrap();
        let incoming = incoming_files(&target_plan, &target_bytes);
        let modified = legacy_lock
            .files
            .iter()
            .find(|file| file.path.ends_with("/SKILL.md") && !file.external)
            .unwrap();
        let modified_content = format!(
            "{}\nKeep this legacy edit.\n",
            std::fs::read_to_string(root.join(&modified.path)).unwrap()
        );
        std::fs::write(root.join(&modified.path), &modified_content).unwrap();
        let before = legacy_lock
            .files
            .iter()
            .map(|file| {
                let path = if file.external {
                    PathBuf::from(&file.path)
                } else {
                    root.join(&file.path)
                };
                (path.clone(), std::fs::read(path).unwrap())
            })
            .collect::<Vec<_>>();
        let old_lock_bytes = std::fs::read(root.join(".hoi4-mod-setup/install.lock.json")).unwrap();
        let operations =
            crate::transaction::update_operations(&legacy_lock, &incoming, &root).unwrap();
        assert_eq!(
            operations
                .iter()
                .find(|operation| operation.destination == modified.path)
                .unwrap()
                .action,
            OperationAction::Skip
        );
        let (mut update, update_bytes) = maintenance_plan(
            &target_plan,
            &legacy_lock,
            &root,
            "update",
            operations,
            &target_bytes,
        );
        let created_paths = update
            .operations
            .iter()
            .filter(|operation| operation.action == OperationAction::Create)
            .map(|operation| {
                if operation.external {
                    PathBuf::from(&operation.destination)
                } else {
                    root.join(&operation.destination)
                }
            })
            .collect::<Vec<_>>();
        let (mut journal, updated) = apply(&root, &mut update, &update_bytes, app.path());
        assert_eq!(
            updated.primary_coding_environment, "codex",
            "upgrade must retain the legacy primary"
        );
        assert!(root.join("CLAUDE.md").is_file());
        assert!(root.join(".claude/skills").is_dir());
        assert!(root.join(".claude/agents").is_dir());
        assert!(root.join(".codex/agents").is_dir());
        assert_eq!(
            std::fs::read_to_string(root.join(&modified.path)).unwrap(),
            modified_content
        );
        let journal_path = crate::paths::transaction_root(app.path(), update.plan_id)
            .transaction
            .join("journal.json");
        crate::transaction::rollback_transaction(&root, &mut journal, &journal_path).unwrap();
        assert_eq!(
            std::fs::read(root.join(".hoi4-mod-setup/install.lock.json")).unwrap(),
            old_lock_bytes
        );
        for (path, bytes) in before {
            assert_eq!(std::fs::read(path).unwrap(), bytes);
        }
        for path in created_paths {
            assert!(
                !path.exists(),
                "upgrade rollback left a newly created file: {}",
                path.display()
            );
        }
        assert!(!root.join("CLAUDE.md").exists());
    }
}
