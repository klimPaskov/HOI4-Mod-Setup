//! A service facade for hosting Setup's analysis, approval and transaction
//! gates on a server.
//!
//! The desktop commands reach these gates through process-wide stores, the
//! operating system's credential vault and the user's application-data
//! folders. A server hosts many accounts, so this module provides the same
//! gates with none of that ambient state:
//!
//! * [`SetupService`] holds the dependencies a host injects: a
//!   [`CredentialStore`], an optional [`PlanBuilder`], an optional
//!   [`NodeRuntime`] and an optional post-install runner.
//! * [`SetupSession`] owns the state of one onboarding session: the latest
//!   scan evidence, the analyses and the prepared plans. Two sessions never
//!   see each other's analyses or plans.
//! * [`SessionConfig`] names the only two directories a session may use: a
//!   workspace root that every project path must be inside, and a state root
//!   that holds the transaction journals, backups and staging. Nothing falls
//!   back to the desktop's application-data folders.
//!
//! The operations follow the order of an onboarding session:
//! [`SetupSession::inspect`], [`SetupSession::approve_evidence`],
//! [`SetupSession::analyze`] (or [`SetupSession::record_analysis`] for a reply
//! the host obtained itself), [`SetupSession::confirm_analysis`],
//! [`SetupSession::build_plan`], [`SetupSession::decide_conflict`],
//! [`SetupSession::approve_plan`], [`SetupSession::apply`],
//! [`SetupSession::inspect_recovery`] and [`SetupSession::recover`].
//!
//! Every gate is the one the desktop enforces, implemented by the same code:
//!
//! * a plan can be built only from an analysis that the selected provider
//!   returned in this session and that was then confirmed with exact values;
//! * evidence sent for analysis must be values the core scanned in the latest
//!   scan of the same root, approved after that scan;
//! * a plan binds the identity of the root the scan observed, and apply
//!   refuses a root that was replaced since;
//! * an approval is bound to the SHA-256 of the exact plan that was reviewed.
//!   Any later change to the plan withdraws it, and an apply attempt consumes
//!   it;
//! * after a partial apply the only recovery the transaction offers is
//!   rollback, and [`SetupSession::recover`] refuses any other action.
//!
//! The service performs no launcher discovery and never reads outside the
//! workspace root. Plans that write outside the project root are refused.

pub(crate) mod builder;
pub(crate) mod evidence;
pub(crate) mod inputs;
pub(crate) mod plans;
pub mod runtime;

use crate::ai::{self, AiProviderConfig};
use crate::codex::{AiAnalysisRequest, ApprovedEvidence, CodexAnalysisResult};
use crate::credentials::{validate_ai_provider_credential_for, CredentialStore};
use crate::models::*;
use crate::paths::{transaction_root, validate_project_root, validate_project_root_or_destination};
use crate::safe_fs::RootedDir;
use crate::scanner::{scan_project_with_progress, ScanOptions, ScanProgress};
use crate::security::sha256_bytes;
use crate::source::SourceResolution;
use crate::transaction::{
    discard_staging, find_incomplete_transaction, read_journal, recovery_action,
    resume_transaction_with_options, rollback_transaction, run_transaction,
    PostInstallActionRunner, TransactionOptions,
};
use crate::AppError;
use evidence::{
    approve_evidence, bind_component_registry, confirm_analysis, confirmed_analysis_for_plan,
    plan_root_identity, project_root_field, record_completed_scan, retain_analysis,
    same_persisted_codex_record, validate_evidence_request, ApprovedScanEvidence,
    PendingCodexAnalysis,
};
use plans::{approve_prepared_plan, resolve_conflict, store_prepared_plan_in, PreparedPlan};
use serde_json::Value;
use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex, MutexGuard};
use uuid::Uuid;

pub use crate::credentials::DeniedCredentialStore;
pub use crate::source::SourceFetcher;
pub use builder::DefaultPlanBuilder;
pub use plans::MergeContext;
pub use runtime::NodeRuntime;

/// The identity token of an existing directory.
///
/// The token is a platform-scoped identifier of the directory object (device
/// and inode on Unix, volume and file ID on Windows). It contains no path or
/// user data, is stable while the directory exists, and changes when the
/// directory is deleted and recreated or replaced by a swap. A plan binds the
/// token of the root that was scanned, and apply refuses a root whose token
/// differs.
///
/// `directory` must be absolute and must not be, or sit below, a symlink or
/// junction. This function does not check workspace containment; use
/// [`SetupSession::root_identity`] for that.
pub fn root_identity(directory: &Path) -> Result<String, AppError> {
    RootedDir::open_read(directory)?.identity_token()
}

/// The digest a reviewer approves: the SHA-256 of the plan's canonical JSON.
/// It is the same digest the transaction journal records as `plan_sha256`.
pub fn plan_sha256(plan: &InstallationPlan) -> Result<String, AppError> {
    Ok(sha256_bytes(&serde_json::to_vec(plan)?))
}

/// The two directories a session may use.
#[derive(Debug, Clone)]
pub struct SessionConfig {
    workspace_root: PathBuf,
    state_root: PathBuf,
}

impl SessionConfig {
    /// Both directories must exist, be absolute, and be free of symlinks and
    /// junctions. They must not contain each other: journals, backups and
    /// staging are kept outside every project they protect.
    ///
    /// The host provisions both directories. The service never creates them.
    pub fn new(
        workspace_root: impl AsRef<Path>,
        state_root: impl AsRef<Path>,
    ) -> Result<Self, AppError> {
        let workspace_root = existing_absolute_directory(workspace_root.as_ref(), "workspace")?;
        let state_root = existing_absolute_directory(state_root.as_ref(), "state")?;
        if workspace_root.starts_with(&state_root) || state_root.starts_with(&workspace_root) {
            return Err(AppError::PathSecurity(
                "the state root and the workspace root must not contain each other".into(),
            ));
        }
        Ok(Self {
            workspace_root,
            state_root,
        })
    }

    pub fn workspace_root(&self) -> &Path {
        &self.workspace_root
    }

    pub fn state_root(&self) -> &Path {
        &self.state_root
    }
}

fn existing_absolute_directory(path: &Path, label: &str) -> Result<PathBuf, AppError> {
    if !path.is_absolute() {
        return Err(AppError::InvalidInput(format!(
            "the {label} root must be an absolute path"
        )));
    }
    validate_project_root(path)
        .map_err(|error| AppError::PathSecurity(format!("the {label} root is unusable: {error}")))
}

/// Everything a plan builder is given. The root, its identity and the
/// confirmed analysis have already passed the session's gates; a builder must
/// use them and must not look for its own.
pub struct PlanRequest<'a> {
    /// The reviewed state, with `identity.projectRoot` rewritten to
    /// `project_root`.
    pub state: &'a Value,
    /// The project root, inside the workspace. It does not exist yet when
    /// `project_root_exists` is false and the plan creates it.
    pub project_root: &'a Path,
    pub project_root_exists: bool,
    /// The identity the plan must carry in
    /// `transaction.project_root_identity`: the scanned root, or the parent of
    /// a root that will be created.
    pub project_root_identity: &'a str,
    /// The confirmed analysis, with its core-only root and scan bindings.
    pub analysis: &'a CodexAnalysisRecord,
    /// The state root. Journals, backups and staging live here.
    pub state_root: &'a Path,
    /// The Node runtime to register in coding-client MCP configuration, when
    /// the service was given one.
    pub node: Option<&'a NodeRuntime>,
}

/// What a plan builder returns. The session checks it against the request
/// before it stores the plan.
pub struct BuiltPlan {
    pub plan: InstallationPlan,
    pub prepared_files: Vec<PreparedFile>,
    /// Base, local and incoming bytes by operation ID, for conflicts that
    /// offer a merge.
    pub merge_contexts: HashMap<String, MergeContext>,
}

/// Builds an installation plan from a confirmed analysis and reviewed state.
///
/// [`DefaultPlanBuilder`] builds the same plans as the desktop from a
/// caller-given state root, cache root and Node runtime. A host can implement
/// this trait instead. The session enforces what a builder cannot be trusted with: the
/// plan must carry the identity and the confirmed analysis the session
/// verified, must be unapproved, and must not write outside the project root.
pub trait PlanBuilder: Send + Sync {
    fn build(&self, request: &PlanRequest<'_>) -> Result<BuiltPlan, AppError>;
}

/// The dependencies a host injects once and shares between sessions.
#[derive(Clone)]
pub struct SetupService {
    credentials: Arc<dyn CredentialStore + Send + Sync>,
    plan_builder: Option<Arc<dyn PlanBuilder>>,
    node: Option<NodeRuntime>,
    post_install_runner: Option<PostInstallActionRunner>,
}

impl SetupService {
    /// `credentials` is the only credential store the service reads. Pass
    /// [`DeniedCredentialStore`] when no provider credential is needed.
    pub fn new(credentials: Arc<dyn CredentialStore + Send + Sync>) -> Self {
        Self {
            credentials,
            plan_builder: None,
            node: None,
            post_install_runner: None,
        }
    }

    pub fn with_plan_builder(mut self, builder: Arc<dyn PlanBuilder>) -> Self {
        self.plan_builder = Some(builder);
        self
    }

    /// The Node runtime handed to the plan builder. See [`NodeRuntime`].
    pub fn with_node_runtime(mut self, node: NodeRuntime) -> Self {
        self.node = Some(node);
        self
    }

    /// Runs manifest-declared, reviewed post-install actions after the files
    /// are verified. Without a runner no external action runs.
    pub fn with_post_install_runner(mut self, runner: PostInstallActionRunner) -> Self {
        self.post_install_runner = Some(runner);
        self
    }

    /// Open a session with its own empty state.
    pub fn open_session(&self, config: SessionConfig) -> SetupSession {
        SetupSession {
            service: self.clone(),
            config,
            state: SessionState::default(),
        }
    }
}

/// Options for [`SetupSession::inspect`].
#[derive(Debug, Clone, Default)]
pub struct InspectOptions {
    /// Setting the flag stops the scan. A stopped scan reports explicit
    /// incomplete coverage and approves no evidence.
    pub cancel: Option<Arc<AtomicBool>>,
}

/// A recovery action the transaction can take.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecoveryAction {
    /// Replay an interrupted run that had not started applying to the
    /// project, or finish a finalization.
    Resume,
    /// Restore the project after a run that started applying.
    Rollback,
    /// Remove the staging of a run that had not started applying.
    DiscardStaging,
}

impl RecoveryAction {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Resume => "resume",
            Self::Rollback => "rollback",
            Self::DiscardStaging => "discard_staging",
        }
    }
}

/// The recovery state of one project, read from the native journal.
#[derive(Debug, Clone)]
pub struct RecoveryStatus {
    pub journal: TransactionJournal,
    /// The only action the transaction permits, or `None` when the journal
    /// requires manual review.
    pub allowed_action: Option<RecoveryAction>,
}

/// The binding recorded when a plan is approved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApprovedPlan {
    pub plan_id: Uuid,
    /// The digest of the plan that was reviewed.
    pub reviewed_plan_sha256: String,
    /// The digest of the plan after the approval flags were set. Apply
    /// recomputes it and refuses a plan that differs.
    pub approved_plan_sha256: String,
    pub project_root_identity: String,
}

struct PlanApproval {
    approved_plan_sha256: String,
    project_root_identity: String,
}

#[derive(Default)]
struct SessionState {
    approved: Mutex<ApprovedScanEvidence>,
    analyses: Mutex<HashMap<Uuid, PendingCodexAnalysis>>,
    plans: Mutex<HashMap<Uuid, PreparedPlan>>,
    approvals: Mutex<HashMap<Uuid, PlanApproval>>,
    scan_generation: Mutex<Uuid>,
}

fn locked<'a, T>(mutex: &'a Mutex<T>, what: &str) -> Result<MutexGuard<'a, T>, AppError> {
    mutex
        .lock()
        .map_err(|_| AppError::Process(format!("the {what} store is unavailable")))
}

/// One onboarding session. All state is owned by the session.
pub struct SetupSession {
    service: SetupService,
    config: SessionConfig,
    state: SessionState,
}

impl SetupSession {
    pub fn workspace_root(&self) -> &Path {
        &self.config.workspace_root
    }

    pub fn state_root(&self) -> &Path {
        &self.config.state_root
    }

    /// Resolve a project path to its canonical form inside the workspace.
    ///
    /// A relative path is relative to the workspace root and may not contain
    /// `.`, `..` or a root. An absolute path must resolve inside the
    /// workspace root. A path through a symlink or junction is refused. The
    /// returned flag says whether the directory exists; a root that does not
    /// exist must have an existing parent.
    pub fn resolve_project_path(&self, path: &Path) -> Result<(PathBuf, bool), AppError> {
        let candidate = if path.is_absolute() {
            path.to_path_buf()
        } else {
            if path
                .components()
                .any(|part| !matches!(part, Component::Normal(_)))
            {
                return Err(AppError::PathSecurity(
                    "a relative project path may not contain traversal".into(),
                ));
            }
            self.config.workspace_root.join(path)
        };
        let (canonical, exists) = validate_project_root_or_destination(&candidate)?;
        if !canonical.starts_with(&self.config.workspace_root) {
            return Err(AppError::PathSecurity(
                "the project path is outside the session workspace".into(),
            ));
        }
        Ok((canonical, exists))
    }

    fn existing_project_path(&self, path: &Path) -> Result<PathBuf, AppError> {
        match self.resolve_project_path(path)? {
            (canonical, true) => Ok(canonical),
            (_, false) => Err(AppError::PathSecurity(
                "the project directory does not exist".into(),
            )),
        }
    }

    /// The identity token of an existing directory inside the workspace.
    /// See [`root_identity`].
    pub fn root_identity(&self, directory: &Path) -> Result<String, AppError> {
        root_identity(&self.existing_project_path(directory)?)
    }

    /// Scan an existing project read-only.
    ///
    /// A new scan replaces whatever scan evidence the session held, including
    /// when the scan fails. Evidence from a stopped or partial scan can never
    /// be approved. The project root must stay the same directory object from
    /// start to end of the scan.
    pub fn inspect(
        &self,
        root: &Path,
        options: InspectOptions,
        progress: impl FnMut(ScanProgress),
    ) -> Result<ScanResult, AppError> {
        let generation = Uuid::new_v4();
        {
            let mut active = locked(&self.state.scan_generation, "scan generation")?;
            let mut approved = locked(&self.state.approved, "approved scan evidence")?;
            *active = generation;
            *approved = ApprovedScanEvidence::default();
        }
        let root = self.existing_project_path(root)?;
        let scanned_identity = root_identity(&root)?;
        let result = scan_project_with_progress(
            &root,
            &ScanOptions {
                cancel_flag: options.cancel,
                ..ScanOptions::default()
            },
            progress,
        )?;
        if root_identity(&root)? != scanned_identity {
            return Err(AppError::PathSecurity(
                "project root changed during the read-only scan".into(),
            ));
        }
        let active = locked(&self.state.scan_generation, "scan generation")?;
        let mut approved = locked(&self.state.approved, "approved scan evidence")?;
        if *active != generation {
            return Err(AppError::Scan(
                "scan result was superseded by a newer project scan".into(),
            ));
        }
        record_completed_scan(&mut approved, root, scanned_identity, &result)
            .map_err(AppError::Scan)?;
        Ok(result)
    }

    /// Approve the exact evidence that will be sent for analysis. Every item
    /// must be a value the core scanned in the latest scan of `root`.
    pub fn approve_evidence(
        &self,
        root: &Path,
        scan_id: Uuid,
        evidence: &[ApprovedEvidence],
    ) -> Result<(), AppError> {
        let root = self.existing_project_path(root)?;
        let mut approved = locked(&self.state.approved, "approved scan evidence")?;
        approve_evidence(&mut approved, &root, scan_id, evidence).map_err(AppError::Credential)
    }

    /// The prompt and input digest of one analysis turn, for a host that
    /// sends the turn to its own provider and returns the reply to
    /// [`SetupSession::record_analysis`].
    pub fn analysis_turn(
        &self,
        request: &AiAnalysisRequest,
        credential: Option<CredentialReference>,
        source: &SourceResolution,
    ) -> Result<AnalysisTurn, AppError> {
        let config = self.provider_config(request, credential)?;
        let mut request = request.clone();
        self.prepare_analysis(&mut request, source)?;
        let (prompt, input_sha256) = ai::analysis_turn(&config, &request)?;
        Ok(AnalysisTurn {
            prompt,
            input_sha256,
        })
    }

    /// Run one analysis turn against a hosted-API provider using the
    /// service's credential store, then keep the validated result in the
    /// session.
    ///
    /// Providers that run a local account tool (`codex`, the Claude account
    /// route, and the Gemini and Grok account routes) are refused: they are
    /// desktop-only. `source` is the resolved source revision the analysis
    /// belongs to; the result is bound to it.
    pub fn analyze(
        &self,
        mut request: AiAnalysisRequest,
        credential: Option<CredentialReference>,
        source: &SourceResolution,
    ) -> Result<CodexAnalysisResult, AppError> {
        let config = self.provider_config(&request, credential)?;
        self.prepare_analysis(&mut request, source)?;
        let result = ai::analyze(&self.service.credentials, &config, &request)?;
        self.retain_result(result, &request, source)
    }

    /// Validate a structured reply the host obtained from the provider
    /// itself, and keep the result in the session. The deterministic
    /// validator is the one [`SetupSession::analyze`] uses, and the analysis
    /// identifier is assigned here: a reply cannot choose its own identity
    /// or skip the evidence bindings.
    pub fn record_analysis(
        &self,
        mut request: AiAnalysisRequest,
        credential: Option<CredentialReference>,
        source: &SourceResolution,
        reply: Value,
    ) -> Result<CodexAnalysisResult, AppError> {
        let config = self.provider_config(&request, credential)?;
        self.prepare_analysis(&mut request, source)?;
        let result = ai::analysis_from_reply(&config, &request, reply)?;
        self.retain_result(result, &request, source)
    }

    /// Confirm a retained analysis with the exact values the reviewer saw.
    /// Only an analysis this session retained can be confirmed, and the
    /// confirmation is bound to those values: a plan built from different
    /// values is refused.
    pub fn confirm_analysis(
        &self,
        record: &CodexAnalysisRecord,
        confirmed_fields: &[String],
        confirmed_values: &Value,
    ) -> Result<CodexAnalysisRecord, AppError> {
        let mut analyses = locked(&self.state.analyses, "analysis")?;
        confirm_analysis(&mut analyses, record, confirmed_fields, confirmed_values)
            .map_err(AppError::Credential)
    }

    /// Build and store an installation plan.
    ///
    /// The plan is built by the service's [`PlanBuilder`] only after the
    /// confirmed analysis is found in this session, bound to the same values,
    /// provider and (for an existing project) the same scan and root
    /// identity. The returned plan is unapproved.
    pub fn build_plan(&self, state: &Value) -> Result<InstallationPlan, AppError> {
        let builder = self.service.plan_builder.clone().ok_or_else(|| {
            AppError::InvalidInput("this service has no plan builder configured".into())
        })?;
        let (root, root_exists) = self.resolve_project_path(&project_root_field(state)?)?;
        let analysis = {
            let analyses = locked(&self.state.analyses, "analysis")?;
            confirmed_analysis_for_plan(&analyses, state, &root)?
        };
        let identity = {
            let approved = locked(&self.state.approved, "approved scan evidence")?;
            plan_root_identity(&approved, &root, root_exists, analysis.scan_id)?
        };
        let mut state = state.clone();
        bind_state_project_root(&mut state, &root);
        let built = builder.build(&PlanRequest {
            state: &state,
            project_root: &root,
            project_root_exists: root_exists,
            project_root_identity: &identity,
            analysis: &analysis,
            state_root: &self.config.state_root,
            node: self.service.node.as_ref(),
        })?;
        check_built_plan(&built, &identity, &analysis)?;
        let mut plans = locked(&self.state.plans, "prepared plan")?;
        store_prepared_plan_in(
            &mut plans,
            built.plan,
            built.prepared_files,
            built.merge_contexts,
            root,
        )
    }

    /// Record a keep, replace, rename, merge or skip decision for one
    /// conflict. The decision must be one the plan offered. Any decision
    /// changes the plan, so it withdraws an approval already given.
    pub fn decide_conflict(
        &self,
        plan_id: Uuid,
        path: &str,
        choice: &str,
    ) -> Result<InstallationPlan, AppError> {
        let mut plans = locked(&self.state.plans, "prepared plan")?;
        let prepared = plans.get_mut(&plan_id).ok_or_else(missing_plan)?;
        resolve_conflict(prepared, path, choice).map_err(AppError::InvalidInput)?;
        prepared.approved = false;
        prepared.plan.approvals.dry_run_reviewed = false;
        prepared.plan.approvals.external_actions_reviewed = false;
        prepared.plan.approvals.git_remote_approved = false;
        locked(&self.state.approvals, "plan approval")?.remove(&plan_id);
        Ok(prepared.plan.clone())
    }

    /// Approve the plan the reviewer saw. `reviewed_plan_sha256` is
    /// [`plan_sha256`] of that plan: if the stored plan differs, nothing is
    /// approved. Approval binds the plan digest and the root identity.
    pub fn approve_plan(
        &self,
        plan_id: Uuid,
        reviewed_plan_sha256: &str,
    ) -> Result<ApprovedPlan, AppError> {
        let mut plans = locked(&self.state.plans, "prepared plan")?;
        let prepared = plans.get_mut(&plan_id).ok_or_else(missing_plan)?;
        let current = plan_sha256(&prepared.plan)?;
        if current != reviewed_plan_sha256 {
            return Err(AppError::Transaction(
                "the plan changed after it was reviewed; review it again".into(),
            ));
        }
        check_plan_stays_in_project(&prepared.plan)?;
        let identity = prepared
            .plan
            .transaction
            .project_root_identity
            .clone()
            .ok_or_else(|| {
                AppError::PathSecurity(
                    "installation plan has no project-root identity binding".into(),
                )
            })?;
        approve_prepared_plan(prepared);
        let approved_plan_sha256 = plan_sha256(&prepared.plan)?;
        locked(&self.state.approvals, "plan approval")?.insert(
            plan_id,
            PlanApproval {
                approved_plan_sha256: approved_plan_sha256.clone(),
                project_root_identity: identity.clone(),
            },
        );
        Ok(ApprovedPlan {
            plan_id,
            reviewed_plan_sha256: current,
            approved_plan_sha256,
            project_root_identity: identity,
        })
    }

    /// Apply an approved plan.
    ///
    /// Before any project file changes the session checks that the plan is
    /// approved, that its digest is the approved digest, that the root is
    /// still inside the workspace and is the directory object the plan bound,
    /// and that the plan writes only inside the project root. The approval is
    /// consumed by the attempt: after any failure the plan must be approved
    /// again, and a started transaction can then only be recovered.
    pub fn apply(&self, plan_id: Uuid) -> Result<TransactionJournal, AppError> {
        let approval = locked(&self.state.approvals, "plan approval")?
            .remove(&plan_id)
            .ok_or_else(|| {
                AppError::Transaction(
                    "explicit dry-run approval is required before installation".into(),
                )
            })?;
        let (plan, files, root) = {
            let plans = locked(&self.state.plans, "prepared plan")?;
            let prepared = plans.get(&plan_id).ok_or_else(missing_plan)?;
            if !prepared.approved || !prepared.plan.approvals.dry_run_reviewed {
                return Err(AppError::Transaction(
                    "explicit dry-run approval is required before installation".into(),
                ));
            }
            if plan_sha256(&prepared.plan)? != approval.approved_plan_sha256 {
                return Err(AppError::Transaction(
                    "the plan changed after it was approved".into(),
                ));
            }
            (
                prepared.plan.clone(),
                prepared.prepared_files.clone(),
                prepared.canonical_root.clone(),
            )
        };
        check_plan_stays_in_project(&plan)?;
        let (resolved, _) = self.resolve_project_path(&root)?;
        if resolved != root {
            return Err(AppError::PathSecurity(
                "installation root changed after review".into(),
            ));
        }
        let observed = match plan.transaction.project_root_mode {
            ProjectRootMode::Existing => root_identity(&root)?,
            ProjectRootMode::CreateLeaf => {
                let parent = root.parent().ok_or_else(|| {
                    AppError::PathSecurity("new project root has no existing parent".into())
                })?;
                root_identity(parent)?
            }
        };
        if observed != approval.project_root_identity {
            return Err(AppError::PathSecurity(
                "project root changed after the plan was approved".into(),
            ));
        }
        let (journal, _) = run_transaction(
            &root,
            &plan,
            &files,
            &TransactionOptions {
                app_data_root: Some(self.config.state_root.clone()),
                post_install_action_runner: self.service.post_install_runner,
                ..Default::default()
            },
        )?;
        locked(&self.state.plans, "prepared plan")?.remove(&plan_id);
        Ok(journal)
    }

    /// The native journal of an incomplete transaction on this project, and
    /// the only recovery action it permits. `None` means no transaction is
    /// waiting for recovery.
    pub fn inspect_recovery(&self, root: &Path) -> Result<Option<RecoveryStatus>, AppError> {
        let (root, _) = self.resolve_project_path(root)?;
        Ok(
            find_incomplete_transaction(&self.config.state_root, &root)?.map(|journal| {
                let allowed_action = allowed_action(&journal);
                RecoveryStatus {
                    journal,
                    allowed_action,
                }
            }),
        )
    }

    /// Read one transaction's journal, which must be bound to `root`.
    pub fn read_journal(
        &self,
        root: &Path,
        transaction_id: Uuid,
    ) -> Result<TransactionJournal, AppError> {
        let (root, _) = self.resolve_project_path(root)?;
        let path = self.journal_path(transaction_id);
        let journal = read_journal(&path)?;
        if journal.project_root.is_empty() {
            return Err(AppError::Transaction(
                "transaction journal has no project-root binding".into(),
            ));
        }
        if !journal_bound_to_root(&journal, &root)? {
            return Err(AppError::PathSecurity(
                "transaction journal is bound to a different project root".into(),
            ));
        }
        Ok(journal)
    }

    /// Take one recovery action on an incomplete transaction. The action must
    /// be the one the journal permits: after a partial apply that is rollback
    /// alone, and a resume is refused.
    pub fn recover(
        &self,
        root: &Path,
        transaction_id: Uuid,
        action: RecoveryAction,
    ) -> Result<TransactionJournal, AppError> {
        let (root, _) = self.resolve_project_path(root)?;
        let journal = self.read_journal(&root, transaction_id)?;
        match allowed_action(&journal) {
            Some(allowed) if allowed == action => {}
            Some(allowed) => {
                return Err(AppError::Transaction(format!(
                    "{} is not permitted for this transaction; only {} is",
                    action.as_str(),
                    allowed.as_str()
                )))
            }
            None => {
                return Err(AppError::Transaction(
                    "this transaction permits no recovery action; manual review is required".into(),
                ))
            }
        }
        match action {
            RecoveryAction::Resume => resume_transaction_with_options(
                &root,
                &self.config.state_root,
                transaction_id,
                self.service.post_install_runner,
            )
            .map(|(journal, _)| journal),
            RecoveryAction::Rollback => {
                let path = self.journal_path(transaction_id);
                let mut journal = read_journal(&path)?;
                rollback_transaction(&root, &mut journal, &path)?;
                Ok(journal)
            }
            RecoveryAction::DiscardStaging => {
                discard_staging(&root, &self.config.state_root, transaction_id)
            }
        }
    }

    fn journal_path(&self, transaction_id: Uuid) -> PathBuf {
        transaction_root(&self.config.state_root, transaction_id)
            .transaction
            .join("journal.json")
    }

    fn provider_config(
        &self,
        request: &AiAnalysisRequest,
        credential: Option<CredentialReference>,
    ) -> Result<AiProviderConfig, AppError> {
        let provider = request.provider.trim();
        if provider == "codex"
            || provider == crate::claude_code::PROVIDER_ID
            || crate::account_cli::is_account_provider(provider)
        {
            return Err(AppError::UnsupportedPlatform(format!(
                "the {provider} analysis route runs a local account tool and is desktop-only"
            )));
        }
        if let Some(reference) = credential.as_ref() {
            validate_ai_provider_credential_for(reference, provider)?;
        }
        Ok(AiProviderConfig {
            provider: request.provider.clone(),
            model: request.model.clone(),
            reasoning_effort: request.reasoning_effort.clone(),
            endpoint: request.endpoint.clone(),
            credential_reference: credential,
        })
    }

    /// Contain the request's root and check its evidence against the latest
    /// approved scan, then tell the analysis which components the source has.
    fn prepare_analysis(
        &self,
        request: &mut AiAnalysisRequest,
        source: &SourceResolution,
    ) -> Result<(), AppError> {
        if let Some(root) = request.analysis.project_root.as_deref() {
            request.analysis.project_root = Some(
                self.existing_project_path(Path::new(root))?
                    .display()
                    .to_string(),
            );
        }
        {
            let approved = locked(&self.state.approved, "approved scan evidence")?;
            validate_evidence_request(&approved, &request.analysis, &refuse_context_documents)?;
        }
        bind_component_registry(&mut request.analysis, source)
    }

    fn retain_result(
        &self,
        mut result: CodexAnalysisResult,
        request: &AiAnalysisRequest,
        source: &SourceResolution,
    ) -> Result<CodexAnalysisResult, AppError> {
        result.record.source_revision = Some(source.identity.resolved_revision.clone());
        result.record.source_manifest_sha256 = Some(source.identity.manifest_sha256.clone());
        let mut analyses = locked(&self.state.analyses, "analysis")?;
        retain_analysis(
            &mut analyses,
            result.record.analysis_id,
            PendingCodexAnalysis {
                analysis: result.analysis.clone(),
                record: result.record.clone(),
                confirmed: None,
                project_root: request.analysis.project_root.clone().map(PathBuf::from),
                scan_id: request.analysis.scan_id,
                endpoint_fingerprint: Some(sha256_bytes(request.endpoint.trim().as_bytes())),
                confirmed_values_sha256: None,
            },
        );
        Ok(result)
    }
}

/// The prompt and input digest of one analysis turn.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnalysisTurn {
    pub prompt: String,
    pub input_sha256: String,
}

fn refuse_context_documents(_: &str, _: &str, _: &str) -> Result<(), AppError> {
    Err(AppError::InvalidInput(
        "context documents are not available through the service".into(),
    ))
}

fn missing_plan() -> AppError {
    AppError::Transaction("installation plan is not available in this session".into())
}

/// Whether a journal is bound to `project_root` by the root recorded when the
/// transaction started.
pub(crate) fn journal_bound_to_root(
    journal: &TransactionJournal,
    project_root: &Path,
) -> Result<bool, AppError> {
    if journal.project_root.is_empty() {
        return Ok(false);
    }
    let bound_root = validate_project_root_or_destination(Path::new(&journal.project_root))?.0;
    Ok(if cfg!(target_os = "windows") {
        bound_root
            .to_string_lossy()
            .eq_ignore_ascii_case(&project_root.to_string_lossy())
    } else {
        bound_root == project_root
    })
}

/// The one recovery action a journal permits, as the transaction itself
/// derives it from the journal's recorded state.
fn allowed_action(journal: &TransactionJournal) -> Option<RecoveryAction> {
    match recovery_action(journal).as_str() {
        "resume" => Some(RecoveryAction::Resume),
        "rollback" => Some(RecoveryAction::Rollback),
        "discard_staging" => Some(RecoveryAction::DiscardStaging),
        _ => None,
    }
}

/// A plan applied through the service may write only inside its project root.
/// Operations on external destinations (a launcher folder, a user-level client
/// configuration) are desktop-only.
fn check_plan_stays_in_project(plan: &InstallationPlan) -> Result<(), AppError> {
    if let Some(operation) = plan.operations.iter().find(|operation| operation.external) {
        return Err(AppError::PathSecurity(format!(
            "the plan writes outside the project root: {}",
            operation.destination
        )));
    }
    Ok(())
}

/// Hold a builder to the facts the session verified.
fn check_built_plan(
    built: &BuiltPlan,
    identity: &str,
    analysis: &CodexAnalysisRecord,
) -> Result<(), AppError> {
    let plan = &built.plan;
    if plan.transaction.project_root_identity.as_deref() != Some(identity) {
        return Err(AppError::PathSecurity(
            "the built plan is not bound to the verified project-root identity".into(),
        ));
    }
    if !plan
        .codex_analysis
        .as_ref()
        .is_some_and(|record| same_persisted_codex_record(record, analysis))
    {
        return Err(AppError::Credential(
            "the built plan does not carry the confirmed analysis".into(),
        ));
    }
    let approvals = &plan.approvals;
    if approvals.dry_run_reviewed
        || approvals.external_actions_reviewed
        || approvals.git_remote_approved
        || approvals.push_approved
    {
        return Err(AppError::Transaction(
            "a built plan must not arrive approved".into(),
        ));
    }
    check_plan_stays_in_project(plan)
}

/// Make the state name the contained root, so a builder that reads the root
/// from the state reads the one the session verified.
fn bind_state_project_root(state: &mut Value, root: &Path) {
    let root = Value::String(root.display().to_string());
    if let Some(identity) = state
        .get("identity")
        .and_then(Value::as_str)
        .and_then(|text| serde_json::from_str::<Value>(text).ok())
    {
        let mut identity = identity;
        if let Some(object) = identity.as_object_mut() {
            object.insert("projectRoot".into(), root.clone());
        }
        if let Ok(text) = serde_json::to_string(&identity) {
            state["identity"] = Value::String(text);
            return;
        }
    }
    if let Some(identity) = state.get_mut("identity").and_then(Value::as_object_mut) {
        identity.insert("projectRoot".into(), root);
    }
}

#[cfg(test)]
mod builder_tests;
#[cfg(test)]
pub(crate) mod fixture;
#[cfg(test)]
mod tests;
