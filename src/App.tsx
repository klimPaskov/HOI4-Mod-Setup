import { useEffect, useRef, useState } from "react";
import type { Dispatch, KeyboardEvent as ReactKeyboardEvent, ReactNode, SetStateAction } from "react";
import { applyInstallation, approveInstallation, approveScanEvidence, buildInstallationPlan, buildMaintenancePlan, cancelCodexLogin, cancelScan, confirmCodexAnalysis, discardInstallationStaging, evaluateReadiness, findInterruptedTransaction, isTauriRuntime, logoutCodexResult, openCodexLoginUrlResult, openExternalUrlResult, openInCodex, pickLauncherFolder, pickProjectFolder, prepareGitOnlineAction, previewDescriptors, previewInstallationConflict, previewSourceManifestResult, readAiAccount, readAiProviderProfiles, readCodexAccount, readTransactionJournal, removeAiProviderCredential, removeMeshyCredential, resolveInstallationConflict, resumeInstallation, rollbackInstallation, run3DHealthCheck, runAiAnalysisResult, runCodexAnalysisResult, runGitOnlineAction, runMcpHealthCheck, scanProject, startCodexLogin, storeAiProviderCredential, storeMeshyCredential, suggestProjectPaths, waitForCodexLoginResult } from "./lib/tauri";
import { deriveGeneratedIdentity } from "./identity";
import type { AiProviderId, AiProviderProfile, CodexAnalysisRequest, ComponentRow, ConflictChoice, ConflictPreview, FolderSelection, GeneratedArtifactPreview, GitOnlineAction, GitOnlinePlan, GitOnlineResult, InstallationPlan, ManifestComponentPreview, PhaseId, ProjectIdentity, ReadinessReport, RecoveryChoice, ScanFinding, ScanProgress, ScreenId, SourceManifestPreview, StatusTone, WizardState, WorkflowHealthResult, WorkflowState } from "./types";
import appIcon from "../src-tauri/icons/icon.png";

const PHASES: Array<{ id: PhaseId; label: string }> = [
  { id: "project", label: "Project" },
  { id: "review", label: "Review" },
  { id: "components", label: "Components" },
  { id: "integrations", label: "Integrations" },
  { id: "git", label: "Git" },
  { id: "install", label: "Install" },
  { id: "ready", label: "Ready" },
];

const MAINTENANCE_PHASES = [
  { id: "overview", label: "Overview" },
  { id: "update", label: "Update" },
  { id: "conflict", label: "Conflicts" },
  { id: "recovery", label: "Recovery" },
] as const;

const DEFAULT_DESCRIPTION = "A Cold War total conversion focused on Southeast Asia, with new countries, political routes, decisions, events, custom doctrines, and long campaigns that can diverge from history.";
const DEFAULT_GENERATED_IDENTITY = deriveGeneratedIdentity("Cold War Curtain", DEFAULT_DESCRIPTION);

const DEFAULT_IDENTITY: ProjectIdentity = {
  displayName: "Cold War Curtain",
  projectId: DEFAULT_GENERATED_IDENTITY.projectId,
  author: "",
  version: "0.1.0",
  supportedGameVersion: "1.17.*",
  projectRoot: "",
  defaultBranch: "main",
  scriptPrefix: DEFAULT_GENERATED_IDENTITY.scriptPrefix,
  primaryNamespace: DEFAULT_GENERATED_IDENTITY.primaryNamespace,
  descriptorTags: DEFAULT_GENERATED_IDENTITY.descriptorTags,
  launcherDescriptorPath: "",
};

const INITIAL_SCAN_PROGRESS: ScanProgress = {
  stage: "discovering_files",
  currentPath: ".",
  filesScanned: 0,
  directoriesScanned: 0,
  bytesRead: 0,
};

const DEFAULT_COMPONENTS: ComponentRow[] = [
  { id: "core.agents", title: "Project instructions", detail: "AGENTS.md adapted to your mod", size: "—", selected: true, required: true },
  { id: "core.skills", title: "Skills", detail: "Current HOI4 workflow skills", size: "—", selected: true, required: true },
  { id: "core.subagents", title: "Subagents", detail: "Focused agent profiles", size: "—", selected: true, required: true },
  { id: "codex.config", title: "Codex and MCP", detail: "Project configuration", size: "—", selected: true, required: true },
  { id: "mcp.hoi4_agent_tools", title: "HOI4 Agent Tools MCP", detail: "Optional HOI4-aware coding tools", size: "—", selected: true, required: false, platform: "windows" },
  { id: "wiki.snapshot", title: "Offline wiki", detail: "Installed under paradox_wiki/", size: "—", selected: true, required: true },
];

const FALLBACK_AI_PROFILES: AiProviderProfile[] = [
  { id: "codex", display_name: "Codex", protocol: "codex_app_server", requires_credential: false, optimization_profile: "Codex project and ChatGPT Chat" },
  { id: "claude", display_name: "Claude", protocol: "anthropic_messages", requires_credential: true, optimization_profile: "Claude Code / Anthropic conventions" },
  { id: "kimi", display_name: "Kimi", protocol: "openai_compatible", requires_credential: true, optimization_profile: "Kimi coding conventions" },
  { id: "glm", display_name: "GLM", protocol: "openai_compatible", requires_credential: true, optimization_profile: "GLM coding conventions" },
  { id: "deepseek", display_name: "DeepSeek", protocol: "openai_compatible", requires_credential: true, optimization_profile: "DeepSeek coding conventions" },
  { id: "local", display_name: "Local model", protocol: "openai_compatible", requires_credential: false, optimization_profile: "Local model conventions" },
  { id: "custom", display_name: "Other provider", protocol: "openai_compatible", requires_credential: true, optimization_profile: "User-supplied provider conventions" },
];

function aiProviderLabel(provider: AiProviderId | undefined, profiles: AiProviderProfile[] = FALLBACK_AI_PROFILES): string {
  const selectedProvider = provider ?? "codex";
  return profiles.find((profile) => profile.id === selectedProvider)?.display_name ?? selectedProvider;
}

function ChoiceIcon({ kind }: { kind: "plus" | "search" | "sparkle" | "circle" }) {
  const paths = {
    plus: <><path d="M12 5v14M5 12h14" /></>,
    search: <><circle cx="10.8" cy="10.8" r="5.6" /><path d="m15.1 15.1 4 4" /></>,
    sparkle: <><path d="m12 3 1.6 6.4L20 11l-6.4 1.6L12 19l-1.6-6.4L4 11l6.4-1.6L12 3Z" /></>,
    circle: <><circle cx="12" cy="12" r="6.5" /><circle cx="12" cy="12" r="2" /></>,
  };
  return <span className="choice-icon" aria-hidden="true"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round">{paths[kind]}</svg></span>;
}

const initialState: WizardState = {
  screen: "welcome",
  mode: "new",
  recoveryEntry: false,
  identity: DEFAULT_IDENTITY,
  identityOverrides: [],
  projectPathsOverridden: false,
  projectPathStatus: "resolving",
  description: DEFAULT_DESCRIPTION,
  sourceMode: "latest",
  pinnedRef: "",
  aiProvider: "codex",
  aiModel: "default",
  aiEndpoint: "",
  aiAccount: null,
  aiProfiles: undefined,
  selectedComponents: DEFAULT_COMPONENTS.filter((component) => component.selected).map((component) => component.id),
  components: DEFAULT_COMPONENTS,
  folderProfile: DEFAULT_GENERATED_IDENTITY.folderProfile,
  meshSelected: false,
  superEventsSelected: false,
  meshKeyDraft: "",
  meshKeyStatus: "missing",
  meshCredentialReference: undefined,
  flattenForChat: false,
  gitMode: "initialize",
  gitBranch: "main",
  initialCommit: true,
  gitRemoteName: "origin",
  gitRemoteUrl: "",
  gitOnlineAction: "none",
  gitHubRepository: DEFAULT_GENERATED_IDENTITY.projectId,
  existingInstallationDetected: false,
  installedWorkflow3dState: "not_selected",
  installedSuperEventsState: "not_selected",
  installProgress: 0,
  installStage: "Preflight",
  conflictChoice: undefined,
  recoveryChoice: "resume",
  readiness: null,
  codexAccount: null,
  codexLoginPending: false,
  codexAnalysis: undefined,
  codexAnalysisRecord: undefined,
  draftSaved: true,
};

const SCREEN_PHASE: Record<ScreenId, PhaseId> = {
  welcome: "project",
  description: "project",
  identity: "project",
  scan: "review",
  findings: "review",
  components: "components",
  workflows: "integrations",
  mesh: "integrations",
  mcp: "integrations",
  git: "git",
  "dry-run": "install",
  install: "install",
  ready: "ready",
  update: "install",
  conflict: "install",
  recovery: "install",
};

const screenCopy: Record<ScreenId, { title: string; supporting?: string; status?: { label: string; tone: StatusTone } }> = {
  welcome: { title: "Start a mod project", supporting: "Choose a starting point." },
  description: { title: "Describe the mod", supporting: "A few sentences are enough." },
  identity: { title: "Project identity", supporting: "Confirm the names and paths used by HOI4 and your selected AI provider." },
  scan: { title: "Scanning project", supporting: "Read-only scan in progress.", status: { label: "Read only", tone: "info" } },
  findings: { title: "Confirm scan findings", supporting: "Edit only the values that are wrong." },
  components: { title: "Choose what to install", supporting: "Recommended components are selected.", status: { label: "Recommended", tone: "info" } },
  workflows: { title: "Optional workflows", supporting: "These choices never block the core setup." },
  mesh: { title: "3D model workflow", supporting: "Connect Meshy.ai; provider charges may apply.", status: { label: "Key required", tone: "review" } },
  mcp: { title: "MCP and credentials", supporting: "Review detected servers and required variables.", status: { label: "Review", tone: "info" } },
  git: { title: "Choose Git setup", supporting: "Keep your project local or connect it online." },
  "dry-run": { title: "Review changes", supporting: "Nothing has been applied yet.", status: { label: "Dry run", tone: "info" } },
  install: { title: "Installing components", supporting: "Staging managed files." },
  ready: { title: "Project ready", supporting: "Setup checks passed.", status: { label: "Ready for Codex", tone: "pass" } },
  update: { title: "Update and repair", supporting: "Manage the installed workflow." },
  conflict: { title: "Resolve AGENTS.md", supporting: "Choose the result before continuing." },
  recovery: { title: "Installation was interrupted", supporting: "Resume from the last safe checkpoint." },
};

function phaseIndex(screen: ScreenId): number {
  return PHASES.findIndex((phase) => phase.id === SCREEN_PHASE[screen]);
}

const GENERATED_IDENTITY_FIELDS = ["projectId", "scriptPrefix", "primaryNamespace", "descriptorTags", "folderProfile"] as const;

function managedInstallationDetails(findings: ScanFinding[]): { present: boolean; valid: boolean; workflow3d: WorkflowState; superEvents: WorkflowState; meshKeyConfigured: boolean } {
  const finding = findings.find((candidate) => candidate.id === "installation.managed");
  if (!finding) return { present: false, valid: false, workflow3d: "not_selected", superEvents: "not_selected", meshKeyConfigured: false };
  try {
    const value = JSON.parse(finding.value) as { present?: boolean; valid?: boolean; workflow_3d_state?: WorkflowState; workflow_super_events_state?: WorkflowState; workflow_3d_key_configured?: boolean };
    return {
      present: value.present === true,
      valid: value.valid === true,
      workflow3d: value.workflow_3d_state ?? "not_selected",
      superEvents: value.workflow_super_events_state ?? "not_selected",
      meshKeyConfigured: value.workflow_3d_key_configured === true,
    };
  } catch {
    return { present: false, valid: false, workflow3d: "not_selected", superEvents: "not_selected", meshKeyConfigured: false };
  }
}

async function sha256Text(value: string): Promise<string> {
  const bytes = new TextEncoder().encode(value);
  const digest = await globalThis.crypto.subtle.digest("SHA-256", bytes);
  return Array.from(new Uint8Array(digest), (byte) => byte.toString(16).padStart(2, "0")).join("");
}

function nextScreen(state: WizardState): ScreenId {
  switch (state.screen) {
    case "welcome": return state.recoveryEntry ? "identity" : state.mode === "new" ? "description" : "identity";
    case "description": return "identity";
    case "identity": return state.recoveryEntry ? "recovery" : state.mode === "existing" ? "scan" : "components";
    case "scan": return "findings";
    case "findings": return "components";
    case "components": return "workflows";
    case "workflows": return state.meshSelected ? "mesh" : "mcp";
    case "mesh": return "mcp";
    case "mcp": return "git";
    case "git": return "dry-run";
    case "dry-run": return "install";
    case "install": return "ready";
    case "ready": return "ready";
    case "conflict": return "dry-run";
    case "recovery": return "install";
    default: return "ready";
  }
}

function previousScreen(state: WizardState): ScreenId {
  switch (state.screen) {
    case "description": return "welcome";
    case "identity": return state.recoveryEntry ? "welcome" : state.mode === "existing" ? "welcome" : "description";
    case "scan": return "identity";
    case "findings": return "scan";
    case "components": return state.mode === "existing" ? "findings" : "identity";
    case "workflows": return "components";
    case "mesh": return "workflows";
    case "mcp": return state.meshSelected ? "mesh" : "workflows";
    case "git": return "mcp";
    case "dry-run": return "git";
    case "install": return "dry-run";
    case "ready": return "install";
    case "update": return "ready";
    default: return "welcome";
  }
}

export default function App() {
  const [state, setState] = useState<WizardState>(initialState);
  const headingRef = useRef<HTMLHeadingElement>(null);
  const [scanComplete, setScanComplete] = useState(false);
  const [scanError, setScanError] = useState<string>();
  const [scanProgress, setScanProgress] = useState<ScanProgress>(INITIAL_SCAN_PROGRESS);
  const [scanRequestId, setScanRequestId] = useState<string>();
  const [scanCancellationRequested, setScanCancellationRequested] = useState(false);
  const [scanPartial, setScanPartial] = useState(false);
  const [scanLimitsHit, setScanLimitsHit] = useState<string[]>([]);
  const [selectedFinding, setSelectedFinding] = useState("localisation");
  const [findings, setFindings] = useState<ScanFinding[]>([]);
  const [semanticAnalysisPending, setSemanticAnalysisPending] = useState(false);

  useEffect(() => {
    if (state.screen !== "welcome" && state.screen !== "description" && state.screen !== "identity" && state.screen !== "findings") return;
    if (state.aiProvider === "codex") {
      if (state.codexAccount !== null) return;
      void readCodexAccount().then((account) => {
        if (account) {
          setState((current) => ({ ...current, codexAccount: account }));
        } else if (isTauriRuntime()) {
          setState((current) => ({ ...current, codexAccount: { available: false, authenticated: false, auth_mode: "", usage_limited: false, error: "Codex could not be reached." } }));
        }
      });
      return;
    }
    if (!state.aiProfiles) {
      void readAiProviderProfiles().then((profiles) => setState((current) => ({ ...current, aiProfiles: profiles.length ? profiles : FALLBACK_AI_PROFILES })));
    }
    if (state.aiAccount !== null) return;
    void readAiAccount(state.aiProvider, state.aiModel, state.aiEndpoint).then((account) => {
      if (account) setState((current) => ({ ...current, aiAccount: account }));
    });
  }, [state.screen, state.codexAccount, state.aiProvider, state.aiModel, state.aiEndpoint, state.aiAccount, state.aiProfiles]);

  useEffect(() => {
    if (!isTauriRuntime() || state.mode !== "new" || state.projectPathsOverridden) return;
    const projectId = state.identity.projectId.trim();
    if (!/^[a-z][a-z0-9_]{1,63}$/.test(projectId)) return;
    let active = true;
    setState((current) => ({
      ...current,
      projectPathStatus: "resolving",
      projectPathMessage: "Finding the HOI4 mod folder…",
    }));
    void suggestProjectPaths(projectId).then((paths) => {
      if (!active) return;
      if (!paths) {
        setState((current) => ({
          ...current,
          projectPathStatus: "unavailable",
          projectPathMessage: "The standard HOI4 mod folder was not found. Choose its location once.",
        }));
        return;
      }
      const collision = paths.project_exists || paths.launcher_descriptor_exists;
      setState((current) => {
        if (current.mode !== "new" || current.projectPathsOverridden || current.identity.projectId !== projectId) return current;
        return {
          ...current,
          identity: {
            ...current.identity,
            projectRoot: paths.project_root,
            launcherDescriptorPath: paths.launcher_descriptor_path,
          },
          projectPathStatus: collision ? "collision" : "ready",
          projectPathMessage: collision
            ? "A mod already uses this project ID. Change the mod name or project ID."
            : `The project will be created in ${paths.mod_directory}.`,
        };
      });
    });
    return () => {
      active = false;
    };
  }, [state.mode, state.identity.projectId, state.projectPathsOverridden]);

  useEffect(() => {
    if (state.screen === "scan" && state.mode === "existing") {
      setScanComplete(false);
      setScanError(undefined);
      setScanProgress(INITIAL_SCAN_PROGRESS);
      setScanCancellationRequested(false);
      setScanRequestId(undefined);
      setScanPartial(false);
      setScanLimitsHit([]);
      setFindings([]);
      setState((current) => ({ ...current, scanContext: undefined, codexAnalysis: undefined, codexAnalysisRecord: undefined }));
      if (!state.identity.projectRoot.trim()) {
        setScanError("Choose an accessible project folder before scanning.");
        return;
      }
      let active = true;
      let requestId: string | undefined;
      let finished = false;
      void scanProject(
        state.identity.projectRoot,
        setScanProgress,
        (id) => {
          requestId = id;
          setScanRequestId(id);
        },
        state.identity.launcherDescriptorPath,
      ).then((result) => {
        if (!active) return;
        if (!result) {
          setScanError("The selected folder could not be scanned. Choose an accessible project folder.");
          return;
        }
        setScanProgress((current) => ({
          ...current,
          stage: result.cancelled ? "cancelled" : "complete",
          filesScanned: result.filesScanned,
          directoriesScanned: result.directoriesScanned,
          bytesRead: result.bytesRead,
        }));
        setScanPartial(result.partial);
        setScanLimitsHit(result.limitsHit);
        if (result.cancelled) {
          setScanError("The read-only scan was cancelled. No scan evidence was approved.");
          setScanComplete(false);
          return;
        }
        setFindings(result.findings);
        const managed = managedInstallationDetails(result.findings);
        const launcherFinding = result.findings.find((finding) => finding.id === "descriptor.launcher");
        let detectedLauncherPath: string | undefined;
        if (launcherFinding) {
          try {
            const value = JSON.parse(launcherFinding.value) as { path?: string };
            detectedLauncherPath = value.path;
          } catch {
            detectedLauncherPath = undefined;
          }
        }
        setState((current) => ({
          ...current,
          identity: detectedLauncherPath
            ? { ...current.identity, launcherDescriptorPath: detectedLauncherPath }
            : current.identity,
          existingInstallationDetected: managed.present && managed.valid,
          installedWorkflow3dState: managed.workflow3d,
          installedSuperEventsState: managed.superEvents,
          meshSelected: managed.workflow3d !== "not_selected" && managed.workflow3d !== "unsupported_platform",
          superEventsSelected: managed.superEvents !== "not_selected" && managed.superEvents !== "unsupported_platform",
          meshKeyStatus: managed.meshKeyConfigured ? "present" : "missing",
          scanContext: {
            scanId: result.scanId,
            projectRoot: result.projectRoot,
            completedAt: result.completedAt,
            partial: result.partial,
            limitsHit: result.limitsHit,
          },
        }));
        setScanComplete(true);
      }).finally(() => {
        finished = true;
        setScanRequestId(undefined);
      });
      return () => {
        active = false;
        if (requestId && !finished) void cancelScan(requestId);
      };
    }
    setScanComplete(false);
    setScanError(undefined);
    setScanProgress(INITIAL_SCAN_PROGRESS);
    setScanRequestId(undefined);
    setScanPartial(false);
    setScanLimitsHit([]);
  }, [state.screen, state.mode, state.identity.projectRoot]);

  useEffect(() => {
    if (!state.identity.projectRoot || state.transaction) return;
    void findInterruptedTransaction(state.identity.projectRoot).then((journal) => {
      if (journal) setState((current) => current.transaction ? current : ({ ...current, transaction: journal, screen: "recovery", transactionError: "An interrupted transaction needs a recovery decision." }));
    });
  }, [state.identity.projectRoot, state.screen, state.transaction]);

  useEffect(() => {
    if (state.screen === "ready" && !state.readiness) {
      void evaluateReadiness(state.identity.projectRoot || "<selected project>", state.identity.projectId, state.meshSelected ? state.meshKeyStatus === "verified" ? "ready" : "incomplete" : "not_selected").then((result) => {
        if (result) setState((current) => ({ ...current, readiness: result }));
      });
    }
  }, [state.screen, state.readiness, state.identity.projectId, state.identity.projectRoot, state.meshSelected, state.meshKeyStatus]);

  useEffect(() => {
    headingRef.current?.focus({ preventScroll: true });
  }, [state.screen]);

  const update = (patch: Partial<WizardState>) => setState((current) => {
    const overrides = new Set(current.identityOverrides ?? []);
    if (Object.prototype.hasOwnProperty.call(patch, "identity")) overrides.clear();
    if (Object.prototype.hasOwnProperty.call(patch, "folderProfile")) overrides.add("folderProfile");
    return { ...current, ...patch, identityOverrides: Array.from(overrides), draftSaved: true };
  });
  const updateIdentity = (patch: Partial<ProjectIdentity>) => setState((current) => {
    const oldProjectId = current.identity.projectId;
    const identity = { ...current.identity, ...patch };
    let gitHubRepository = current.gitHubRepository;
    const overrides = new Set(current.identityOverrides ?? []);
    for (const field of GENERATED_IDENTITY_FIELDS) {
      if (field !== "folderProfile" && Object.prototype.hasOwnProperty.call(patch, field)) overrides.add(field);
    }
    if (Object.prototype.hasOwnProperty.call(patch, "displayName")) {
      const generated = deriveGeneratedIdentity(identity.displayName, current.description);
      if (!overrides.has("projectId")) identity.projectId = generated.projectId;
      if (!overrides.has("scriptPrefix")) identity.scriptPrefix = generated.scriptPrefix;
      if (!overrides.has("primaryNamespace")) identity.primaryNamespace = generated.primaryNamespace;
      if (!overrides.has("descriptorTags")) identity.descriptorTags = generated.descriptorTags;
    }
    if (identity.projectId !== oldProjectId && current.identity.launcherDescriptorPath) {
      const oldFile = current.identity.launcherDescriptorPath.split(/[\\/]/).pop()?.toLowerCase();
      const oldProjectFile = `${oldProjectId}.mod`.toLowerCase();
      if (oldFile === oldProjectFile || oldFile === "project.mod") {
        identity.launcherDescriptorPath = current.identity.launcherDescriptorPath.replace(/[^\\/]+$/, `${identity.projectId}.mod`);
      }
    }
    if (identity.projectId !== oldProjectId && (!gitHubRepository || gitHubRepository === oldProjectId)) gitHubRepository = identity.projectId;
    return { ...current, identity, gitHubRepository, identityOverrides: Array.from(overrides), transactionError: undefined, draftSaved: true };
  });
  const updateDescription = (description: string) => setState((current) => {
    const overrides = new Set(current.identityOverrides ?? []);
    const oldProjectId = current.identity.projectId;
    const generated = deriveGeneratedIdentity(current.identity.displayName, description);
    const identity = { ...current.identity };
    if (!overrides.has("projectId")) identity.projectId = generated.projectId;
    if (!overrides.has("scriptPrefix")) identity.scriptPrefix = generated.scriptPrefix;
    if (!overrides.has("primaryNamespace")) identity.primaryNamespace = generated.primaryNamespace;
    if (!overrides.has("descriptorTags")) identity.descriptorTags = generated.descriptorTags;
    const gitHubRepository = (!current.gitHubRepository || current.gitHubRepository === oldProjectId) ? generated.projectId : current.gitHubRepository;
    return {
      ...current,
      description,
      identity,
      gitHubRepository,
      folderProfile: overrides.has("folderProfile") ? current.folderProfile : generated.folderProfile,
      codexAnalysis: undefined,
      codexAnalysisRecord: undefined,
      draftSaved: true,
    };
  });
  const chooseProjectFolder = () => pickProjectFolder();
  const chooseLauncherFolder = () => pickLauncherFolder();
  const buildCodexEvidence = async (sourceFindings: ScanFinding[]) => Promise.all(sourceFindings.filter((finding) => finding.status !== "rejected").map(async (finding) => ({
    reference: finding.id,
    path: finding.evidencePath ?? "",
    excerpt: finding.evidenceExcerpt ?? finding.value,
    excerpt_sha256: await sha256Text(finding.evidenceExcerpt ?? finding.value),
    confidence: finding.confidence,
  })));
  const runSemanticAnalysis = async (mode: "new_project_identity" | "existing_project_semantics"): Promise<boolean> => {
    if (state.codexAnalysisRecord) return true;
    const provider = state.aiProvider;
    let accountReady = true;
    let providerAccount = state.aiAccount;
    if (provider === "codex") {
      if (!state.codexAccount?.available) {
        if (isTauriRuntime()) update({ transactionError: "Codex is required for planning." });
        return false;
      }
      if (state.codexAccount.error) {
        update({ transactionError: state.codexAccount.error });
        return false;
      }
      if (state.codexAccount.usage_limited) {
        update({ transactionError: "Codex usage is currently limited. Planning is paused; recovery remains available." });
        return false;
      }
      accountReady = state.codexAccount.authenticated && state.codexAccount.auth_mode === "chatgpt";
      if (!accountReady) {
        update({ transactionError: "Sign in with ChatGPT through Codex before planning the setup." });
        return false;
      }
    } else {
      providerAccount = providerAccount ?? await readAiAccount(provider, state.aiModel, state.aiEndpoint);
      if (!providerAccount?.available) {
        if (isTauriRuntime()) update({ aiAccount: providerAccount, transactionError: `${aiProviderLabel(provider, state.aiProfiles)} is not configured. Enter its endpoint and connect before planning the setup.` });
        return false;
      }
      if (providerAccount.error) {
        update({ aiAccount: providerAccount, transactionError: providerAccount.error });
        return false;
      }
      if (providerAccount.usage_limited) {
        update({ aiAccount: providerAccount, transactionError: `${aiProviderLabel(provider, state.aiProfiles)} usage is currently limited. Planning is paused; recovery remains available.` });
        return false;
      }
      accountReady = providerAccount.authenticated;
      if (!accountReady) {
        update({ aiAccount: providerAccount, transactionError: `Connect ${aiProviderLabel(provider, state.aiProfiles)} before planning the setup.` });
        return false;
      }
    }
    const evidence = await buildCodexEvidence(findings);
    if (mode === "existing_project_semantics" && !state.scanContext) {
      update({ transactionError: "The completed scan is no longer available. Rerun the read-only scan." });
      return false;
    }
    if (mode === "existing_project_semantics" && state.scanContext?.partial) {
      update({ transactionError: "The read-only scan reached a safety limit. Complete an untruncated scan before semantic analysis." });
      return false;
    }
    if (mode === "existing_project_semantics" && evidence.some((item) => !item.path || !item.excerpt)) {
      update({ transactionError: "The completed scan did not provide complete evidence for semantic analysis. Rerun the read-only scan." });
      return false;
    }
    if (mode === "existing_project_semantics" && state.scanContext) {
      const approved = await approveScanEvidence(state.scanContext.projectRoot, state.scanContext.scanId, evidence);
      if (!approved) {
        update({ transactionError: "The edited scan results could not be confirmed. Run the read-only scan again before continuing." });
        return false;
      }
    }
    const request: CodexAnalysisRequest = {
      mode,
      brief: state.description,
      evidence: mode === "existing_project_semantics" ? evidence : [],
      constraints: { platform: "windows_or_macos", project_id_pattern: "^[a-z][a-z0-9_]{1,63}$", no_workshop_id: true },
      analysis_purpose: mode === "existing_project_semantics" ? "existing_project_import" : undefined,
      project_root: mode === "existing_project_semantics" ? state.scanContext?.projectRoot : undefined,
      scan_id: mode === "existing_project_semantics" ? state.scanContext?.scanId : undefined,
    };
    const response = provider === "codex"
      ? await runCodexAnalysisResult(request)
      : await runAiAnalysisResult({
        ...request,
        provider,
        model: state.aiModel,
        endpoint: state.aiEndpoint,
      });
    const result = response.value;
    if (!result) {
      update({
        transactionError: response.error
          ? `${aiProviderLabel(provider, state.aiProfiles)} planning could not finish: ${response.error} Your draft and scan remain unchanged.`
          : `${aiProviderLabel(provider, state.aiProfiles)} analysis could not be completed. Your draft and scan remain unchanged.`,
      });
      return false;
    }
    const proposal = (key: string) => result.analysis.proposals.find((item) => item.key === key)?.value;
    const displayName = proposal("display_name");
    const projectId = proposal("project_id");
    const description = proposal("project_description");
    const folderProfile = proposal("folder_profile");
    const scriptPrefix = proposal("script_prefix");
    const primaryNamespace = proposal("primary_namespace");
    const descriptorTags = proposal("descriptor_tags");
    const generated = deriveGeneratedIdentity(
      typeof displayName === "string" ? displayName : state.identity.displayName,
      typeof description === "string" ? description : state.description,
    );
    setState((current) => ({
      ...current,
      aiAccount: provider === "codex" ? current.aiAccount : providerAccount,
      codexAnalysis: result.analysis,
      codexAnalysisRecord: result.record,
      identity: {
        ...current.identity,
        displayName: typeof displayName === "string" ? displayName : current.identity.displayName,
        projectId: current.identityOverrides?.includes("projectId") ? current.identity.projectId : typeof projectId === "string" ? projectId : current.identity.projectId || generated.projectId,
        scriptPrefix: current.identityOverrides?.includes("scriptPrefix") ? current.identity.scriptPrefix : typeof scriptPrefix === "string" && scriptPrefix.trim() ? scriptPrefix : current.identity.scriptPrefix || generated.scriptPrefix,
        primaryNamespace: current.identityOverrides?.includes("primaryNamespace") ? current.identity.primaryNamespace : typeof primaryNamespace === "string" && primaryNamespace.trim() ? primaryNamespace : current.identity.primaryNamespace || generated.primaryNamespace,
        descriptorTags: current.identityOverrides?.includes("descriptorTags") ? current.identity.descriptorTags : Array.isArray(descriptorTags) && descriptorTags.every((item) => typeof item === "string") && descriptorTags.length > 0 ? descriptorTags as string[] : current.identity.descriptorTags?.length ? current.identity.descriptorTags : generated.descriptorTags,
      },
      description: typeof description === "string" ? description : current.description,
      folderProfile: current.identityOverrides?.includes("folderProfile") ? current.folderProfile : Array.isArray(folderProfile) && folderProfile.every((item) => typeof item === "string") && folderProfile.length > 0 ? folderProfile as string[] : current.folderProfile?.length ? current.folderProfile : generated.folderProfile,
      transactionError: undefined,
      draftSaved: true,
    }));
    return true;
  };
  const prepareMaintenanceReanalysis = async (): Promise<boolean> => {
    if (!isTauriRuntime()) {
      update({ transactionError: `${aiProviderLabel(state.aiProvider, state.aiProfiles)} reanalysis is available in the packaged desktop app.` });
      return false;
    }
    if (!state.identity.projectRoot.trim()) {
      update({ transactionError: "Choose the installed project folder before reanalysis." });
      return false;
    }
    const scanned = await scanProject(
      state.identity.projectRoot,
      undefined,
      undefined,
      state.identity.launcherDescriptorPath,
    );
    if (!scanned) {
      update({ transactionError: "The installed project could not be scanned. Nothing was changed." });
      return false;
    }
    setFindings(scanned.findings);
    const managed = managedInstallationDetails(scanned.findings);
    const scanWasPartial = scanned.partial || scanned.cancelled;
    update({
      existingInstallationDetected: managed.present && managed.valid,
      installedWorkflow3dState: managed.workflow3d,
      meshSelected: managed.workflow3d !== "not_selected" && managed.workflow3d !== "unsupported_platform",
      meshKeyStatus: managed.meshKeyConfigured ? "present" : "missing",
      scanContext: {
        scanId: scanned.scanId,
        projectRoot: scanned.projectRoot,
        completedAt: scanned.completedAt,
        partial: scanned.partial,
        limitsHit: scanned.limitsHit,
      },
      maintenanceEvidenceReady: !scanWasPartial,
      maintenanceCodexAnalysisRecord: undefined,
      codexAnalysis: undefined,
      codexAnalysisRecord: undefined,
      screen: "update",
      transactionError: scanWasPartial
        ? scanned.cancelled
          ? "The read-only scan was cancelled. No evidence is available for reanalysis."
          : "The read-only scan reached a safety limit. Complete an untruncated scan before reanalysis."
        : `Read-only evidence is ready. Review the input below, then run ${aiProviderLabel(state.aiProvider, state.aiProfiles)} reanalysis.`,
    });
    return !scanWasPartial;
  };
  const runMaintenanceReanalysis = async (): Promise<boolean> => {
    if (!state.maintenanceEvidenceReady) return prepareMaintenanceReanalysis();
    if (!state.scanContext) {
      update({ transactionError: "Prepare the latest read-only evidence before reanalysis." });
      return false;
    }
    const provider = state.aiProvider ?? "codex";
    const providerLabel = aiProviderLabel(provider, state.aiProfiles);
    let codexAccount = state.codexAccount;
    let aiAccount = state.aiAccount;
    if (provider === "codex") {
      codexAccount = codexAccount ?? await readCodexAccount();
      if (!codexAccount || !codexAccount.available) {
        update({ transactionError: "Codex is required to review the project again." });
        return false;
      }
      if (codexAccount.error) {
        update({ codexAccount, transactionError: codexAccount.error });
        return false;
      }
      if (codexAccount.usage_limited) {
        update({ codexAccount, transactionError: "Codex usage is currently limited. Reanalysis is paused; recovery remains available." });
        return false;
      }
      if (!codexAccount.authenticated || codexAccount.auth_mode !== "chatgpt") {
        update({ codexAccount, transactionError: "Sign in with ChatGPT through Codex before reviewing the project again." });
        return false;
      }
    } else {
      aiAccount = aiAccount ?? await readAiAccount(provider, state.aiModel, state.aiEndpoint);
      if (!aiAccount || !aiAccount.available) {
        update({ aiAccount, transactionError: `${providerLabel} is not configured for reanalysis. Check the endpoint and connection.` });
        return false;
      }
      if (aiAccount.error) {
        update({ aiAccount, transactionError: aiAccount.error });
        return false;
      }
      if (aiAccount.usage_limited || !aiAccount.authenticated) {
        update({ aiAccount, transactionError: `${providerLabel} is not available for reanalysis. Planning is paused; recovery remains available.` });
        return false;
      }
    }
    const evidence = await buildCodexEvidence(findings);
    if (!evidence.length || evidence.some((item) => !item.path || !item.excerpt)) {
      update({ transactionError: "The approved scan did not provide complete evidence for reanalysis. Prepare the read-only evidence again." });
      return false;
    }
    const approved = await approveScanEvidence(state.scanContext.projectRoot, state.scanContext.scanId, evidence);
    if (!approved) {
      update({ transactionError: "The edited scan results could not be confirmed. Prepare the read-only scan again." });
      return false;
    }
    const request: CodexAnalysisRequest = {
      mode: "existing_project_semantics",
      brief: "Review the installed HOI4 project for semantic changes before a workflow update. Preserve deterministic facts, identify convention or instruction changes, and propose only reviewable values. Do not write files or approve operations.",
      evidence,
      constraints: {
        platform: "windows_or_macos",
        analysis_purpose: "maintenance_reanalysis",
        project_id_pattern: "^[a-z][a-z0-9_]{1,63}$",
        no_workshop_id: true,
      },
      analysis_purpose: "maintenance_reanalysis",
      project_root: state.scanContext.projectRoot,
      scan_id: state.scanContext.scanId,
    };
    const response = provider === "codex"
      ? await runCodexAnalysisResult(request)
      : await runAiAnalysisResult({
        ...request,
        provider,
        model: state.aiModel,
        endpoint: state.aiEndpoint,
      });
    const result = response.value;
    if (!result) {
      update({
        transactionError: response.error
          ? `${providerLabel} reanalysis could not finish: ${response.error} The installed project and evidence remain unchanged.`
          : `${providerLabel} reanalysis could not be completed. The installed project and evidence remain unchanged.`,
      });
      return false;
    }
    update({
      codexAccount,
      aiAccount,
      codexAnalysis: result.analysis,
      codexAnalysisRecord: result.record,
      maintenanceCodexAnalysisRecord: result.record,
      transactionError: `Review and confirm the ${providerLabel} reanalysis before creating the update plan.`,
      screen: "update",
    });
    return true;
  };
  const confirmAnalysis = async () => {
    if (!state.codexAnalysis || !state.codexAnalysisRecord) return;
    const confirmedFields = state.codexAnalysis.proposals.map((proposal) => proposal.key);
    const record = await confirmCodexAnalysis(state.codexAnalysisRecord, confirmedFields, {
      description: state.description,
      folderProfile: state.folderProfile ?? [],
      identity: state.identity,
    });
    if (!record) {
      update({ transactionError: `${aiProviderLabel(state.aiProvider, state.aiProfiles)} suggestions could not be confirmed.` });
      return;
    }
    update({
      codexAnalysisRecord: record,
      maintenanceCodexAnalysisRecord: state.maintenanceCodexAnalysisRecord ? record : state.maintenanceCodexAnalysisRecord,
      transactionError: undefined,
    });
  };
  const applyReviewedPlan = async (plan: InstallationPlan) => {
    if (plan.conflicts.some((conflict) => !conflict.selected)) {
      update({ plan, screen: "conflict", transactionError: undefined });
      return;
    }
    if (!(await approveInstallation(plan.plan_id))) {
      update({ transactionError: "The reviewed installation plan could not be approved. Nothing was changed." });
      return;
    }
    const journal = await applyInstallation(plan, state.identity.projectRoot);
    if (!journal) {
      const interrupted = await readTransactionJournal(state.identity.projectRoot, plan.plan_id);
      if (interrupted?.state === "interrupted") {
        update({ transaction: interrupted, transactionError: "Installation was interrupted. Choose a recovery action before continuing.", screen: "recovery" });
      } else {
        update({ transactionError: "Installation could not start. Nothing was changed." });
      }
      return;
    }
    const maintenance = state.maintenanceMode !== undefined;
    update({
      plan: maintenance ? undefined : plan,
      maintenanceMode: maintenance ? undefined : state.maintenanceMode,
      maintenanceCodexAnalysisRecord: maintenance ? undefined : state.maintenanceCodexAnalysisRecord,
      maintenanceEvidenceReady: maintenance ? undefined : state.maintenanceEvidenceReady,
      codexAnalysis: maintenance ? undefined : state.codexAnalysis,
      codexAnalysisRecord: maintenance ? undefined : state.codexAnalysisRecord,
      transaction: journal,
      transactionError: undefined,
      readiness: maintenance ? null : state.readiness,
      installProgress: 100,
      installStage: journal.last_checkpoint,
      screen: maintenance ? "ready" : "install",
    });
  };
  const prepareSetupPlan = async () => {
    const plan = await buildInstallationPlan({ ...state, conflictChoice: undefined });
    if (!plan) {
      update({ transactionError: "The installation plan is unavailable. Nothing was changed." });
      return;
    }
    update({ plan, conflictChoice: undefined, transactionError: undefined, screen: plan.conflicts.some((conflict) => !conflict.selected) ? "conflict" : "dry-run" });
  };
  const startMaintenance = async (mode: "update" | "repair" | "reinstall" | "remove") => {
    if (mode === "update" && !state.maintenanceCodexAnalysisRecord?.confirmed_fields.length) {
      if (!state.maintenanceEvidenceReady) {
        await prepareMaintenanceReanalysis();
      } else if (!state.maintenanceCodexAnalysisRecord) {
        update({ transactionError: `Run ${aiProviderLabel(state.aiProvider, state.aiProfiles)} reanalysis and confirm its proposals before checking for updates.` });
      } else {
        update({ transactionError: `Confirm the ${aiProviderLabel(state.aiProvider, state.aiProfiles)} reanalysis before checking for updates.` });
      }
      return;
    }
    const addOptionalComponents = state.existingInstallationDetected === true
      && (mode === "repair" || mode === "update")
      ? [
        ...(state.meshSelected && state.installedWorkflow3dState === "not_selected" ? ["workflow.3d"] : []),
        ...(state.superEventsSelected && state.installedSuperEventsState === "not_selected" ? ["workflow.super_events"] : []),
      ]
      : [];
    const plan = await buildMaintenancePlan(mode, state.identity.projectRoot, state.maintenanceCodexAnalysisRecord, addOptionalComponents);
    if (!plan) {
      update({ transactionError: "The maintenance plan is unavailable. Nothing was changed." });
      return;
    }
    update({ plan, maintenanceMode: mode, transactionError: undefined, screen: plan.conflicts.some((conflict) => !conflict.selected) ? "conflict" : "update" });
  };
  const chooseConflict = async (choice: ConflictChoice) => {
    const plan = state.plan;
    const conflict = plan?.conflicts.find((candidate) => !candidate.selected);
    if (!plan || !conflict) {
      update({ conflictChoice: choice });
      return;
    }
    const updatedPlan = await resolveInstallationConflict(plan.plan_id, conflict.path, choice);
    if (!updatedPlan) {
      update({ transactionError: "That conflict choice could not be applied. No files were changed." });
      return;
    }
    update({ plan: updatedPlan, conflictChoice: choice, transactionError: undefined });
  };
  const handleRecovery = async () => {
    const transaction = state.transaction;
    if (!transaction) {
      update({ transactionError: "No interrupted transaction is available for this project." });
      return;
    }
    const transactionId = transaction.transaction_id;
    if (state.recoveryChoice === "resume") {
      if (!transaction.recovery.resume_allowed) {
        update({ transactionError: "This setup cannot continue safely. Choose Undo changes or inspect the project." });
        return;
      }
      const resumed = await resumeInstallation(state.identity.projectRoot, transactionId);
      if (!resumed) {
        update({ transactionError: "Setup could not continue safely. Review the project or choose Undo changes." });
        return;
      }
      update({ transaction: resumed, transactionError: undefined, installProgress: 100, installStage: resumed.last_checkpoint, screen: "install" });
      return;
    }
    if (state.recoveryChoice === "rollback") {
      if (!transaction.recovery.rollback_allowed) {
        update({ transactionError: "Undo changes is not available for this setup state." });
        return;
      }
      const rolledBack = await rollbackInstallation(state.identity.projectRoot, transactionId);
      if (!rolledBack) {
        update({ transactionError: "Undo changes was refused because the project needs review." });
        return;
      }
      update({ transaction: rolledBack, transactionError: undefined, readiness: null, screen: "ready" });
      return;
    }
    if (!transaction.recovery.discard_staging_allowed) {
      update({ transactionError: "Staging cannot be discarded after project apply has started." });
      return;
    }
    const discarded = await discardInstallationStaging(state.identity.projectRoot, transactionId);
    if (!discarded) {
      update({ transactionError: "The staging directory could not be discarded safely." });
      return;
    }
    update({ transaction: discarded, transactionError: undefined, screen: state.maintenanceMode ? "update" : "dry-run" });
  };
  const goNext = async () => {
    if (state.screen === "recovery") {
      await handleRecovery();
      return;
    }
    if (state.screen === "welcome" && !state.recoveryEntry && isTauriRuntime() && !providerReady(state)) {
      update({ transactionError: state.aiProvider === "codex" ? "Sign in with ChatGPT through Codex before starting setup. Recovery remains available." : `Connect ${aiProviderLabel(state.aiProvider, state.aiProfiles)} before starting setup. Recovery remains available.` });
      return;
    }
    if (state.screen === "description" && state.mode === "new" && !state.codexAnalysisRecord) {
      if (!isTauriRuntime()) {
        update({ screen: nextScreen(state) });
        return;
      }
      if (semanticAnalysisPending) return;
      setSemanticAnalysisPending(true);
      update({ transactionError: undefined });
      try {
        const handled = await runSemanticAnalysis("new_project_identity");
        if (handled) {
          update({ screen: "identity" });
          return;
        }
      } catch {
        update({ transactionError: `${aiProviderLabel(state.aiProvider, state.aiProfiles)} planning stopped unexpectedly. Your description is still saved; try again.` });
      } finally {
        setSemanticAnalysisPending(false);
      }
      return;
    }
    if (state.screen === "identity" && state.mode === "existing" && !state.identity.projectRoot.trim()) {
      update({ transactionError: "Choose an accessible project folder before scanning." });
      return;
    }
    if (state.screen === "scan" && !scanComplete) {
      update({ transactionError: scanError ?? "The read-only scan is still pending. Nothing has been changed." });
      return;
    }
    if (state.screen === "findings" && state.mode === "existing" && !state.codexAnalysisRecord) {
      if (!isTauriRuntime()) {
        update({ screen: nextScreen(state) });
        return;
      }
      const handled = await runSemanticAnalysis("existing_project_semantics");
      if (handled) return;
      if (isTauriRuntime()) return;
    }
    if ((state.screen === "identity" && state.mode === "new")
      || (state.screen === "findings" && state.mode === "existing")) {
      if (isTauriRuntime() && !state.codexAnalysisRecord?.confirmed_fields.length) {
        update({ transactionError: `Review and confirm the ${aiProviderLabel(state.aiProvider, state.aiProfiles)} proposals before continuing.` });
        return;
      }
    }
    if (state.screen === "findings" && isTauriRuntime() && !state.codexAnalysisRecord) {
      update({ transactionError: `Review and confirm the ${aiProviderLabel(state.aiProvider, state.aiProfiles)} proposals before continuing.` });
      return;
    }
    if (state.screen === "dry-run") {
      const plan = state.plan ?? (state.conflictChoice ? await buildInstallationPlan(state) : null);
      if (!plan) {
        update({ transactionError: "Review the plan or resolve conflicts before installation." });
        return;
      }
      await applyReviewedPlan(plan);
      return;
    }
    if (state.screen === "update" && state.plan) {
      await applyReviewedPlan(state.plan);
      return;
    }
    if (state.screen === "conflict") {
      if (!state.plan || state.plan.conflicts.some((conflict) => !conflict.selected)) return;
      if (state.maintenanceMode) {
        await applyReviewedPlan(state.plan);
      } else {
        update({ screen: "dry-run" });
      }
      return;
    }
    if (state.screen === "install" && state.installProgress >= 100) {
      update({ screen: "ready" });
      return;
    }
    update({ screen: nextScreen(state), installProgress: state.installProgress });
  };
  const goBack = () => update({ screen: previousScreen(state) });
  const cancelActiveScan = async () => {
    if (!scanRequestId || scanComplete || scanCancellationRequested) return;
    setScanCancellationRequested(true);
    const result = await cancelScan(scanRequestId);
    if (result.value === null && result.error && !/no longer running/i.test(result.error)) {
      setScanCancellationRequested(false);
      setScanError(`The scan could not be cancelled: ${result.error}`);
    }
  };
  const openMaintenance = (screen: "update" | "conflict" | "recovery") => update({ screen, plan: undefined, maintenanceMode: undefined, maintenanceCodexAnalysisRecord: undefined, maintenanceEvidenceReady: undefined, transactionError: undefined });

  const copy = state.screen === "ready"
    ? {
      title: "Project ready",
      supporting: state.readiness ? state.readiness.coreReady ? `Core requirements passed for ${aiProviderLabel(state.aiProvider, state.aiProfiles)}.` : "Resolve blocking checks before continuing." : "Checking core requirements.",
      status: state.readiness ? state.readiness.coreReady ? { label: `Ready for ${aiProviderLabel(state.aiProvider, state.aiProfiles)}`, tone: "pass" as const } : { label: "Needs review", tone: "block" as const } : { label: "Checking readiness", tone: "info" as const },
    }
    : screenCopy[state.screen];
  return (
    <div className="app-shell">
      <header className="titlebar">
        <img className="brand-mark" src={appIcon} alt="" aria-hidden="true" />
        <span className="brand-name">HOI4 Mod Setup</span>
      </header>
      <div className="workspace">
        <PhaseRail screen={state.screen} />
        <main className="main-viewport" aria-labelledby="screen-title" aria-describedby="screen-supporting" onKeyDown={closeDisclosureOnEscape}>
          <div className="visually-hidden" role="status" aria-live="polite" aria-atomic="true">{copy.title}</div>
          <ScreenFrame screen={state.screen} copy={copy} state={state} canAdvance={!semanticAnalysisPending && canAdvanceFromScreen(state, { scanComplete, scanError, scanPartial, findings })} pending={semanticAnalysisPending} headingRef={headingRef} onBack={goBack} onNext={goNext} onMaintenance={openMaintenance} onPrepareConflicts={prepareSetupPlan}>
            {renderScreen(state, update, updateIdentity, updateDescription, findings, selectedFinding, setSelectedFinding, setFindings, scanComplete, scanError, scanProgress, scanPartial, scanLimitsHit, scanRequestId, scanCancellationRequested, cancelActiveScan, openMaintenance, startMaintenance, runMaintenanceReanalysis, chooseConflict, chooseProjectFolder, chooseLauncherFolder, confirmAnalysis)}
          </ScreenFrame>
        </main>
      </div>
    </div>
  );
}

function PhaseRail({ screen }: { screen: ScreenId }) {
  const maintenance = screen === "update" || screen === "conflict" || screen === "recovery";
  const currentMaintenance = maintenance ? screen === "update" ? 1 : screen === "conflict" ? 2 : 3 : -1;
  const current = phaseIndex(screen);
  const phases = maintenance ? MAINTENANCE_PHASES : PHASES;
  return <nav className="phase-rail" aria-label={maintenance ? "Maintenance phases" : "Setup phases"}>
    <div className="rail-label">{maintenance ? "MANAGE" : "SETUP"}</div>
    {phases.map((phase, index) => {
      const active = maintenance ? index === currentMaintenance : phase.id === SCREEN_PHASE[screen];
      const completed = maintenance ? index < currentMaintenance : index < current;
      return <div key={phase.id} className={`phase-item ${active ? "active" : ""} ${completed ? "completed" : ""}`} aria-current={active ? "step" : undefined}>
        <span className="phase-number">{completed ? "✓" : index + 1}</span><span>{phase.label}</span>
      </div>;
    })}
    <ExternalLink className="rail-repo" href="https://github.com/klimPaskov/Agentic-HOI4-Modding">Agentic-HOI4-Modding <span aria-hidden="true">↗</span></ExternalLink>
  </nav>;
}

function ExternalLink({ href, className, children }: { href: string; className?: string; children: ReactNode }) {
  return <a
    className={["external-link", className].filter(Boolean).join(" ")}
    href={href}
    onClick={(event) => {
      if (!isTauriRuntime()) return;
      event.preventDefault();
      void openExternalUrlResult(href);
    }}
  >{children}</a>;
}

function ScreenFrame({ screen, copy, state, canAdvance, pending, headingRef, onBack, onNext, onMaintenance, onPrepareConflicts, children }: { screen: ScreenId; copy: { title: string; supporting?: string; status?: { label: string; tone: StatusTone } }; state: WizardState; canAdvance: boolean; pending: boolean; headingRef: { current: HTMLHeadingElement | null }; onBack: () => void; onNext: () => void; onMaintenance: (screen: "update" | "conflict" | "recovery") => void; onPrepareConflicts: () => void; children: ReactNode }) {
  const installDone = screen === "install" && state.installProgress >= 100;
  const unresolvedConflicts = state.plan?.conflicts.some((conflict) => !conflict.selected) === true;
  const primaryLabel = pending && screen === "description" ? "Preparing details…" : screen === "welcome" ? "Continue" : screen === "findings" && !state.codexAnalysis ? `Review with ${aiProviderLabel(state.aiProvider, state.aiProfiles)}` : screen === "dry-run" ? "Start installation" : screen === "install" ? (installDone ? "Continue" : "") : screen === "ready" ? "Finish" : screen === "recovery" ? "Continue" : screen === "conflict" ? "Apply" : screen === "update" ? (state.plan ? "Apply reviewed plan" : "") : "Next";
  const showBack = !["welcome", "install"].includes(screen);
  return <>
    <div className="content-scroll">
      <div className="screen-heading">
        <div><div className="eyebrow">{(screen === "update" ? "Update" : screen === "conflict" ? "Conflicts" : screen === "recovery" ? "Recovery" : SCREEN_PHASE[screen]).toUpperCase()}</div><h1 id="screen-title" ref={headingRef} tabIndex={-1}>{copy.title}</h1>{copy.supporting && <p id="screen-supporting">{copy.supporting}</p>}</div>
        {copy.status && <Status label={copy.status.label} tone={copy.status.tone} />}
      </div>
      {children}
    </div>
    <footer className="footer-bar">
      <span className="footer-note" role={state.transactionError ? "alert" : undefined}>{footerNote(screen, state)}</span>
      <div className="footer-actions">
        {screen === "ready" && <button className="button secondary" onClick={() => onMaintenance("update")}>Update and repair</button>}
        {screen === "dry-run" && (!state.plan || unresolvedConflicts) && <button className="button secondary" onClick={onPrepareConflicts}>{state.plan ? "Resolve conflicts" : "Prepare changes"}</button>}
        {showBack && <button className="button secondary" onClick={onBack}>Back</button>}
        {primaryLabel && <button className="button primary" onClick={onNext} disabled={!canAdvance} aria-busy={pending || undefined}>{primaryLabel}</button>}
      </div>
    </footer>
  </>;
}

function footerNote(screen: ScreenId, state: WizardState): string {
  if (state.transactionError) return state.transactionError;
  if (screen === "welcome") return providerReady(state) ? "Nothing is changed until the dry run." : state.aiProvider === "codex" ? "Sign in with ChatGPT to continue." : `Connect ${aiProviderLabel(state.aiProvider, state.aiProfiles)} to continue.`;
  if (screen === "scan") return state.transactionError ?? "No project files are being modified.";
  if (screen === "workflows") return "Optional workflows can be changed from Update and Repair.";
  if (screen === "mesh") return "The key is never written into the project or lock file.";
  if (screen === "mcp") return "Only variable names appear in configuration.";
  if (screen === "git") return "Online actions always ask for separate approval.";
  if (screen === "dry-run") {
    if (!state.plan) return "Prepare the changes before installation.";
    if (state.plan.conflicts.some((conflict) => !conflict.selected)) return "Resolve blocking conflicts before installation.";
    return "The reviewed changes are ready to install.";
  }
  if (screen === "install") return state.installProgress >= 100 ? "Setup saved. Readiness is next." : "Saving your progress…";
  if (screen === "ready") return state.transactionError ?? "Readiness checks saved.";
  if (screen === "update") return state.transactionError ?? "User-modified files are never overwritten silently.";
  if (screen === "conflict") return "A preview and validation run follow the selected resolution.";
  if (screen === "recovery") return "Recovery actions are reversible until apply begins.";
  return state.draftSaved ? "Draft saved locally." : "";
}

function closeDisclosureOnEscape(event: ReactKeyboardEvent<HTMLElement>) {
  if (event.key !== "Escape") return;
  const details = (event.target as HTMLElement).closest("details[open]") as HTMLDetailsElement | null;
  if (!details) return;
  details.open = false;
  details.querySelector<HTMLElement>("summary")?.focus();
  event.preventDefault();
  event.stopPropagation();
}

function recoveryChoiceAllowed(state: WizardState): boolean {
  if (state.screen !== "recovery" || !state.transaction) return false;
  if (state.recoveryChoice === "resume") return state.transaction.recovery.resume_allowed;
  if (state.recoveryChoice === "rollback") return state.transaction.recovery.rollback_allowed;
  return state.transaction.recovery.discard_staging_allowed;
}

function providerReady(state: WizardState): boolean {
  if (state.aiProvider === "codex") {
    return Boolean(
      state.codexAccount?.available
      && state.codexAccount.authenticated
      && state.codexAccount.auth_mode === "chatgpt"
      && !state.codexAccount.usage_limited
      && !state.codexAccount.error,
    );
  }
  return Boolean(
    state.aiAccount?.available
    && state.aiAccount.authenticated
    && !state.aiAccount.usage_limited
    && !state.aiAccount.error,
  );
}

function canAdvanceFromScreen(
  state: WizardState,
  scan: { scanComplete: boolean; scanError?: string; scanPartial: boolean; findings: ScanFinding[] },
): boolean {
  const planningReady = providerReady(state);
  switch (state.screen) {
    case "welcome":
      return planningReady;
    case "description":
      return planningReady
        && Boolean(state.identity.displayName.trim())
        && Boolean(state.description.trim());
    case "identity":
      if (state.recoveryEntry || state.mode === "existing") return Boolean(state.identity.projectRoot.trim());
      if (!isTauriRuntime()) {
        return planningReady
          && [
            state.identity.displayName,
            state.identity.projectId,
            state.identity.scriptPrefix,
            state.identity.primaryNamespace,
          ].every((value) => Boolean(value?.trim()))
          && Boolean(state.identity.descriptorTags?.length)
          && Boolean(state.folderProfile?.length);
      }
      return planningReady
        && (state.projectPathStatus === "ready" || state.projectPathStatus === "manual")
        && Boolean(state.codexAnalysisRecord?.confirmed_fields.length)
        && [
          state.identity.displayName,
          state.identity.projectId,
          state.identity.scriptPrefix,
          state.identity.primaryNamespace,
          state.identity.projectRoot,
          state.identity.launcherDescriptorPath,
        ].every((value) => Boolean(value?.trim()))
        && Boolean(state.identity.descriptorTags?.length)
        && Boolean(state.folderProfile?.length);
    case "scan":
      return scan.scanComplete && !scan.scanError && !scan.scanPartial;
    case "findings":
      if (!planningReady || scan.findings.length === 0) return false;
      return !state.codexAnalysis || Boolean(state.codexAnalysisRecord?.confirmed_fields.length);
    case "components":
      return planningReady
        && Boolean(state.manifestPreview || !isTauriRuntime())
        && state.selectedComponents.length > 0;
    case "git":
      if (state.gitMode === "skip") return true;
      if (!state.gitBranch.trim()) return false;
      if (state.gitOnlineAction === "push_remote") return Boolean(state.gitRemoteName.trim() && state.gitRemoteUrl.trim());
      if (state.gitOnlineAction === "create_public_github") return Boolean(state.gitHubRepository?.trim());
      return true;
    case "dry-run":
      return Boolean(state.plan) && !state.plan?.conflicts.some((conflict) => !conflict.selected);
    case "install":
      return state.installProgress >= 100;
    case "recovery":
      return recoveryChoiceAllowed(state);
    case "conflict":
      return Boolean(state.plan) && !state.plan?.conflicts.some((conflict) => !conflict.selected);
    case "update":
      return Boolean(state.plan);
    default:
      return true;
  }
}

function renderScreen(state: WizardState, update: (patch: Partial<WizardState>) => void, updateIdentity: (patch: Partial<ProjectIdentity>) => void, updateDescription: (description: string) => void, findings: ScanFinding[], selectedFinding: string, setSelectedFinding: (id: string) => void, setFindings: Dispatch<SetStateAction<ScanFinding[]>>, scanComplete: boolean, scanError: string | undefined, scanProgress: ScanProgress, scanPartial: boolean, scanLimitsHit: string[], scanRequestId: string | undefined, scanCancellationRequested: boolean, onCancelScan: () => Promise<void>, onMaintenance: (screen: "update" | "conflict" | "recovery") => void, startMaintenance: (mode: "update" | "repair" | "reinstall" | "remove") => void, onReanalyze: () => Promise<boolean>, chooseConflict: (choice: ConflictChoice) => void, onPickProjectFolder: () => Promise<FolderSelection | null>, onPickLauncherFolder: () => Promise<FolderSelection | null>, onConfirmAnalysis: () => Promise<void>) {
  switch (state.screen) {
    case "welcome": return <Welcome state={state} update={update} />;
    case "description": return <Description state={state} updateDescription={updateDescription} updateIdentity={updateIdentity} />;
    case "identity": return <Identity state={state} update={update} updateIdentity={updateIdentity} onPickProjectFolder={onPickProjectFolder} onPickLauncherFolder={onPickLauncherFolder} onConfirmAnalysis={onConfirmAnalysis} />;
    case "scan": return <Scan state={state} complete={scanComplete} error={scanError} progress={scanProgress} partial={scanPartial} limitsHit={scanLimitsHit} canCancel={Boolean(scanRequestId)} cancellationRequested={scanCancellationRequested} onCancel={onCancelScan} />;
    case "findings": return <Findings state={state} findings={findings} selected={selectedFinding} setSelected={setSelectedFinding} setFindings={setFindings} onConfirmAnalysis={onConfirmAnalysis} onManageExisting={() => onMaintenance("update")} />;
    case "components": return <Components state={state} update={update} />;
    case "workflows": return <Workflows state={state} update={update} />;
    case "mesh": return <Mesh state={state} update={update} />;
    case "mcp": return <Mcp state={state} />;
    case "git": return <Git state={state} update={update} />;
    case "dry-run": return <DryRun state={state} update={update} />;
    case "install": return <Install state={state} />;
    case "ready": return <Ready state={state} update={update} onMaintenance={onMaintenance} />;
    case "update": return <Update state={state} update={update} findings={findings} setFindings={setFindings} onMaintenance={onMaintenance} onStartMaintenance={startMaintenance} onReanalyze={onReanalyze} />;
    case "conflict": return <Conflict state={state} update={update} onChoice={chooseConflict} />;
    case "recovery": return <Recovery state={state} update={update} onPickProjectFolder={onPickProjectFolder} onStartMaintenance={startMaintenance} />;
  }
}

export function Welcome({ state, update }: { state: WizardState; update: (patch: Partial<WizardState>) => void }) {
  const [aiKeyDraft, setAiKeyDraft] = useState("");
  const activeCodexLoginId = useRef<string | undefined>(undefined);
  const selectedProvider = state.aiProvider ?? "codex";
  const desktopRuntime = isTauriRuntime();
  const profiles = state.aiProfiles?.length ? state.aiProfiles : FALLBACK_AI_PROFILES;
  const profile = profiles.find((candidate) => candidate.id === selectedProvider) ?? profiles[0];
  const selectedLabel = aiProviderLabel(selectedProvider, profiles);
  const selectProvider = (provider: AiProviderId) => {
    update({
      aiProvider: provider,
      aiModel: provider === "codex" ? "default" : "",
      aiEndpoint: "",
      aiAccount: null,
      codexAnalysis: undefined,
      codexAnalysisRecord: undefined,
      flattenForChat: provider === "codex" ? state.flattenForChat : false,
      meshSelected: provider === "codex" ? state.meshSelected : false,
      selectedComponents: provider === "codex"
        ? Array.from(new Set([...state.selectedComponents, "codex.config"]))
        : state.selectedComponents.filter((id) => !["codex.config", "mcp.hoi4_agent_tools", "workflow.3d"].includes(id)),
      transactionError: undefined,
    });
    setAiKeyDraft("");
  };
  const account = state.codexAccount;
  const signIn = async (mode: "browser" | "device") => {
    if (!desktopRuntime) {
      update({ transactionError: "Open the installed desktop app to sign in." });
      return;
    }
    if (state.codexLoginPending && state.codexLogin?.login_id) {
      await cancelCodexLogin(state.codexLogin.login_id);
    }
    update({ codexLoginPending: true, transactionError: undefined });
    const login = await startCodexLogin(mode);
    if (login) {
      activeCodexLoginId.current = login.login_id ?? undefined;
      update({ codexLogin: login, codexLoginPending: Boolean(login.login_id), transactionError: login.error ?? undefined });
      const loginUrl = mode === "browser" ? login.auth_url : login.verification_url;
      if (loginUrl) {
        const opened = await openCodexLoginUrlResult(loginUrl);
        if (opened.error) update({ transactionError: `${opened.error} Use the sign-in link shown below.` });
      }
      if (login.login_id) {
        const loginId = login.login_id;
        void waitForCodexLoginResult(loginId).then(({ value: accountStatus, error }) => {
          if (activeCodexLoginId.current !== loginId) return;
          activeCodexLoginId.current = undefined;
          update({ codexLoginPending: false, codexAccount: accountStatus ?? state.codexAccount ?? null, transactionError: error ?? accountStatus?.error ?? undefined });
        });
      }
    }
    else update({ codexLoginPending: false, transactionError: "Codex sign-in could not be started." });
  };
  const refresh = async () => {
    const next = await readCodexAccount();
    if (next) update({ codexAccount: next, transactionError: next.error ?? undefined });
    else if (isTauriRuntime()) update({ codexAccount: { available: false, authenticated: false, auth_mode: "", usage_limited: false, error: "Codex could not be reached." } });
  };
  const cancel = async () => {
    const loginId = state.codexLogin?.login_id;
    if (loginId && await cancelCodexLogin(loginId)) {
      if (activeCodexLoginId.current === loginId) activeCodexLoginId.current = undefined;
      update({ codexLoginPending: false, transactionError: "Codex login cancelled. You can retry or use the device-code flow." });
    }
  };
  const openLoginUrl = async (url: string) => {
    const result = await openCodexLoginUrlResult(url);
    update({ transactionError: result.error });
  };
  const signOut = async () => {
    activeCodexLoginId.current = undefined;
    const result = await logoutCodexResult();
    const next = await readCodexAccount();
    update({
      codexLoginPending: false,
      codexAccount: next,
      codexAnalysis: undefined,
      codexAnalysisRecord: undefined,
      maintenanceCodexAnalysisRecord: undefined,
      transactionError: result.error ?? undefined,
    });
  };
  const refreshProvider = async () => {
    const next = await readAiAccount(selectedProvider, state.aiModel, state.aiEndpoint);
    if (next) update({ aiAccount: next, transactionError: next.error ?? undefined });
    else if (isTauriRuntime()) update({ aiAccount: { available: false, authenticated: false, provider: selectedProvider, model: state.aiModel, auth_mode: "unconfigured", usage_limited: false, error: "The selected provider could not be reached by the desktop bridge." } });
  };
  const connectProvider = async () => {
    if (!aiKeyDraft.trim()) {
      await refreshProvider();
      return;
    }
    const credential = aiKeyDraft;
    setAiKeyDraft("");
    const stored = await storeAiProviderCredential(selectedProvider, credential);
    if (!stored) {
      update({ transactionError: `The ${selectedLabel} credential could not be stored. No project files were changed.` });
      return;
    }
    update({ aiAccount: null, transactionError: undefined });
  };
  const removeProviderCredential = async () => {
    if (await removeAiProviderCredential(selectedProvider)) {
      update({ aiAccount: null, transactionError: undefined });
    } else {
      update({ transactionError: `The ${selectedLabel} credential was not removed; no project files were changed.` });
    }
  };
  return <div className="stack wide welcome-screen"><div className="choice-grid">
    <button type="button" className={`choice-card ${state.mode === "new" ? "selected" : ""}`} aria-pressed={state.mode === "new"} onClick={() => update({ mode: "new" })}>
      <ChoiceIcon kind="plus" /><span className="choice-radio" aria-hidden="true" /><h2>Create new mod</h2><p>Start from a short description.</p>
    </button>
    <button type="button" className={`choice-card ${state.mode === "existing" ? "selected" : ""}`} aria-pressed={state.mode === "existing"} onClick={() => update({ mode: "existing" })}>
      <ChoiceIcon kind="search" /><span className="choice-radio" aria-hidden="true" /><h2>Import existing mod</h2><p>Scan the project without changing it.</p>
    </button>
  </div><section><div className="section-label">Planning provider</div><div className="panel recent-list provider-panel"><label className="field"><span className="field-label">AI provider</span><select className="text-input" value={state.aiProvider} onChange={(event) => selectProvider(event.target.value as AiProviderId)}>{profiles.map((candidate) => <option key={candidate.id} value={candidate.id}>{candidate.display_name}</option>)}</select></label><p className="muted provider-help">Codex is the default. The project follows the provider you choose.</p>{state.aiProvider !== "codex" && <><Field label="Model" value={state.aiModel} onChange={(value) => update({ aiModel: value, aiAccount: null, codexAnalysis: undefined, codexAnalysisRecord: undefined })} placeholder="Enter the provider-supplied model identifier" mono /><Field label="Endpoint" value={state.aiEndpoint} onChange={(value) => update({ aiEndpoint: value, aiAccount: null, codexAnalysis: undefined, codexAnalysisRecord: undefined })} placeholder={state.aiProvider === "local" ? "Enter the loopback HTTP endpoint" : "Enter the provider-supplied HTTPS endpoint"} mono /><p className="muted provider-help">Hosted providers use HTTPS. Local models use a loopback HTTP address.</p>{profile?.requires_credential && <><label className="field"><span className="field-label">Provider API key</span><input className="text-input" type="password" value={aiKeyDraft} onChange={(event) => setAiKeyDraft(event.target.value)} autoComplete="off" /></label><p className="muted provider-help">Saved securely by your operating system.</p></>}<div className="button-row"><button type="button" className="button secondary" onClick={() => void connectProvider()}>{aiKeyDraft ? "Store and connect" : "Check configuration"}</button>{state.aiAccount?.authenticated && profile?.requires_credential && <button type="button" className="text-button" onClick={() => void removeProviderCredential()}>Delete stored key</button>}</div>{state.aiAccount && <p className="callout" role="status">{state.aiAccount.error ?? (state.aiAccount.authenticated ? `${selectedLabel} is configured for ${state.aiModel}; the first request checks capability.` : `Connect ${selectedLabel} before continuing.`)}</p>}{state.aiProvider === "local" && <p className="muted provider-help">No hosted account is needed for a local model.</p>}</>}</div></section>{state.aiProvider === "codex" && <section><div className="section-label">Codex access</div><div className="panel recent-list">
    {!desktopRuntime && <p className="muted">Open the installed desktop app to sign in with ChatGPT.</p>}
    {desktopRuntime && !account && <p className="muted" role="status">Checking Codex…</p>}
    {desktopRuntime && account && account.available && account.authenticated && account.auth_mode === "chatgpt" && <><p><strong>Signed in with ChatGPT</strong>{account.email ? ` · ${account.email}` : ""}</p>{account.usage_limited && <p className="callout review" role="status">Codex usage is currently limited. Planning is paused until usage is available again; recovery remains available.</p>}<button type="button" className="text-button" onClick={() => void refresh()}>Refresh account status</button><button type="button" className="text-button" onClick={() => void signOut()}>Sign out</button></>}
    {desktopRuntime && account && (!account.available || !account.authenticated || account.auth_mode !== "chatgpt") && <><p className="muted">Create, Import, Update, and Repair use your ChatGPT Codex access. No API key is requested.</p>{account.error && <p className="callout review" role="status">{account.error}</p>}<div className="button-row"><button type="button" className="button secondary" onClick={() => void signIn("browser")} disabled={state.codexLoginPending}>{state.codexLoginPending ? "Opening sign-in…" : "Sign in with ChatGPT"}</button><button type="button" className="text-button" onClick={() => void signIn("device")} disabled={state.codexLoginPending}>Use device code</button>{state.codexLoginPending && <button type="button" className="text-button" onClick={() => void cancel()}>Cancel sign-in</button>}</div>{state.codexLogin?.auth_url && <p><button type="button" className="text-button" onClick={() => void openLoginUrl(state.codexLogin?.auth_url ?? "")}>Open the ChatGPT sign-in page</button></p>}{state.codexLogin?.verification_url && <p className="muted"><button type="button" className="text-button" onClick={() => void openLoginUrl(state.codexLogin?.verification_url ?? "")}>Open the device-code page</button> and enter <strong>{state.codexLogin.user_code}</strong>.</p>}<button type="button" className="text-button" onClick={() => void refresh()}>Check again</button></>}
  </div></section>}
  <section><div className="section-label">Already have a project?</div><div className="panel recent-list"><p className="muted">Check or remove a project already set up by this app.</p><button type="button" className="text-button" onClick={() => update({ screen: "identity", mode: "existing", recoveryEntry: true, identity: { ...DEFAULT_IDENTITY }, transaction: undefined, transactionError: undefined })}>Manage an existing project</button></div></section></div>;
}

function Description({ state, updateDescription, updateIdentity }: { state: WizardState; updateDescription: (description: string) => void; updateIdentity: (patch: Partial<ProjectIdentity>) => void }) {
  return <div className="stack narrow"><section className="panel form-panel"><Field label="Mod name" value={state.identity.displayName} onChange={(value) => updateIdentity({ displayName: value })} /><label className="field"><span className="field-label">Description</span><textarea className="brief-input" aria-label="Mod description" value={state.description} onChange={(event) => updateDescription(event.target.value)} /></label><p className="muted">The project ID, script prefix, namespace, tags, and initial folders are filled from these two fields. You can edit any generated value on the next screen.</p></section><details><summary>{aiProviderLabel(state.aiProvider, state.aiProfiles)} input preview</summary><p className="muted">The brief is sent to the selected local planning adapter with Windows or macOS, lowercase project ID, no Workshop identity, no file writes, and the selected provider optimization profile.</p></details><div className="chips"><span>Natural-language brief</span><span>Generated identity</span><span>Editable structure</span></div></div>;
}

const PROPOSAL_LABELS: Record<string, string> = {
  display_name: "Mod name",
  project_id: "Project ID",
  script_prefix: "Script prefix",
  primary_namespace: "Primary namespace",
  project_description: "Description",
  descriptor_tags: "Descriptor tags",
  folder_profile: "Initial folders",
  agents_profile: "Project guidance",
  localisation_convention: "Localisation style",
  documentation_convention: "Documentation style",
};

function formatProposalValue(value: unknown): string {
  if (typeof value === "string") return value;
  if (Array.isArray(value) && value.every((item) => typeof item === "string")) return value.join(", ");
  return JSON.stringify(value);
}

function CodexReview({ state, onConfirmAnalysis }: { state: WizardState; onConfirmAnalysis: () => Promise<void> }) {
  const analysis = state.codexAnalysis;
  if (!analysis) return null;
  const confirmed = (state.codexAnalysisRecord?.confirmed_fields.length ?? 0) > 0;
  const providerLabel = aiProviderLabel(state.aiProvider, state.aiProfiles);
  return <section className="panel proposal-review" aria-label={`${providerLabel} proposal review`}><div className="list-row"><div><strong>{confirmed ? `${providerLabel} suggestions confirmed` : `Suggested by ${providerLabel}`}</strong><span>{analysis.project_summary}</span></div><Status label={confirmed ? "Confirmed" : "Review required"} tone={confirmed ? "pass" : "review"} /></div><details open={!confirmed}><summary>{analysis.proposals.length} suggested values</summary><div className="manifest-details proposal-list">{analysis.proposals.map((proposal) => <div className="proposal-row" key={proposal.key}><div className="proposal-heading"><strong>{PROPOSAL_LABELS[proposal.key] ?? proposal.key.replaceAll("_", " ")}</strong><span>{Math.round(proposal.confidence * 100)}% confidence</span></div><span className="proposal-value">{formatProposalValue(proposal.value)}</span><small>{proposal.reason}</small></div>)}</div></details>{analysis.warnings.length > 0 && <details className="analysis-notes"><summary>Planning notes</summary><p>{analysis.warnings.join(" ")}</p></details>}{!confirmed && <div className="proposal-actions"><button type="button" className="button primary" onClick={() => void onConfirmAnalysis()}>Confirm {providerLabel} suggestions</button></div>}</section>;
}

function RecoveryProjectPicker({ state, updateIdentity, onPickProjectFolder }: { state: WizardState; updateIdentity: (patch: Partial<ProjectIdentity>) => void; onPickProjectFolder: () => Promise<FolderSelection | null> }) {
  const [message, setMessage] = useState<string>();
  const choose = async () => {
    setMessage(undefined);
    const selected = await onPickProjectFolder();
    if (selected?.path) updateIdentity({
      projectRoot: selected.path,
      launcherDescriptorPath: selected.launcher_descriptor_path ?? undefined,
    });
    else if (selected?.error) setMessage(`The selected folder could not be used: ${selected.error}`);
    else setMessage("No folder selected.");
  };
  return <div className="stack narrow"><section className="panel form-panel"><PanelTitle title="Choose an installed project" /><p className="muted">Check an existing setup, repair missing files, add workflows, or remove managed files without connecting an AI provider.</p><Field label="Project folder" value={state.identity.projectRoot} placeholder="Choose an installed project folder" onChange={(value) => updateIdentity({ projectRoot: value })} action="Browse" onAction={() => void choose()} />{message && <p className="muted" role="status">{message}</p>}</section></div>;
}

function ExistingProjectPicker({ state, updateIdentity, onPickProjectFolder }: { state: WizardState; updateIdentity: (patch: Partial<ProjectIdentity>) => void; onPickProjectFolder: () => Promise<FolderSelection | null> }) {
  const [message, setMessage] = useState<string>();
  const choose = async () => {
    setMessage(undefined);
    const selected = await onPickProjectFolder();
    if (selected?.path) updateIdentity({
      projectRoot: selected.path,
      launcherDescriptorPath: selected.launcher_descriptor_path ?? undefined,
    });
    else if (selected?.error) setMessage(`The selected folder could not be used: ${selected.error}`);
    else setMessage("No folder selected.");
  };
  return <div className="stack narrow"><section className="panel form-panel"><p className="muted">Choose the mod project. Its descriptors, identity, structure, and existing setup will be detected during the read-only scan.</p><Field label="Project folder" value={state.identity.projectRoot} placeholder="Choose an existing mod project" onChange={(value) => updateIdentity({ projectRoot: value, launcherDescriptorPath: undefined })} action="Browse" onAction={() => void choose()} />{state.identity.launcherDescriptorPath && <p className="muted path-preview" role="status"><strong>Launcher file:</strong> <code>{state.identity.launcherDescriptorPath}</code></p>}{message && <p className="muted" role="status">{message}</p>}</section></div>;
}

function Identity({ state, update, updateIdentity, onPickProjectFolder, onPickLauncherFolder, onConfirmAnalysis }: { state: WizardState; update: (patch: Partial<WizardState>) => void; updateIdentity: (patch: Partial<ProjectIdentity>) => void; onPickProjectFolder: () => Promise<FolderSelection | null>; onPickLauncherFolder: () => Promise<FolderSelection | null>; onConfirmAnalysis: () => Promise<void> }) {
  const [previews, setPreviews] = useState<GeneratedArtifactPreview[]>([]);
  const [previewMessage, setPreviewMessage] = useState<string>();
  const [folderMessage, setFolderMessage] = useState<string>();
  const showPreviews = async () => {
    setPreviewMessage("Preparing preview…");
    const result = await previewDescriptors(state);
    setPreviews(result);
    setPreviewMessage(result.length ? undefined : "Descriptor preview is available in the desktop core after a project path is selected.");
  };
  const chooseFolder = async () => {
    setFolderMessage(undefined);
    const selected = state.mode === "new" ? await onPickLauncherFolder() : await onPickProjectFolder();
    if (selected?.path) {
      if (state.mode === "new") {
        const separator = selected.path.includes("\\") ? "\\" : "/";
        const modDirectory = selected.path.replace(/[\\/]+$/, "");
        updateIdentity({
          projectRoot: `${modDirectory}${separator}${state.identity.projectId}`,
          launcherDescriptorPath: `${modDirectory}${separator}${state.identity.projectId}.mod`,
        });
        update({
          projectPathsOverridden: true,
          projectPathStatus: "manual",
          projectPathMessage: `The project will be created in ${modDirectory}.`,
        });
      } else {
        updateIdentity({ projectRoot: selected.path });
      }
    } else if (selected?.error) {
      setFolderMessage(`The selected folder could not be used: ${selected.error}`);
    } else {
      setFolderMessage("No folder selected.");
    }
  };
  const chooseLauncherFolder = async () => {
    const selected = await onPickLauncherFolder();
    if (!selected?.path) return;
    const separator = selected.path.includes("\\") ? "\\" : "/";
    const modDirectory = selected.path.replace(/[\\/]+$/, "");
    updateIdentity({
      projectRoot: `${modDirectory}${separator}${state.identity.projectId || "project"}`,
      launcherDescriptorPath: `${modDirectory}${separator}${state.identity.projectId || "project"}.mod`,
    });
    update({
      projectPathsOverridden: true,
      projectPathStatus: "manual",
      projectPathMessage: `The project will be created in ${modDirectory}.`,
    });
  };
  if (state.recoveryEntry) {
    return <RecoveryProjectPicker state={state} updateIdentity={updateIdentity} onPickProjectFolder={onPickProjectFolder} />;
  }
  if (state.mode === "existing") {
    return <ExistingProjectPicker state={state} updateIdentity={updateIdentity} onPickProjectFolder={onPickProjectFolder} />;
  }
  return <div className="stack">{state.codexAnalysis && <CodexReview state={state} onConfirmAnalysis={onConfirmAnalysis} />}<div className="two-column"><section className="panel form-panel"><p className="muted">Generated from the mod name and description. Edit any value when you want a different convention.</p><div className="form-grid">
    <Field label="Mod name" value={state.identity.displayName} onChange={(value) => updateIdentity({ displayName: value })} />
    <Field label="Project ID" value={state.identity.projectId} onChange={(value) => updateIdentity({ projectId: value })} mono />
    <Field label="Script prefix" value={state.identity.scriptPrefix ?? ""} onChange={(value) => updateIdentity({ scriptPrefix: value })} mono />
    <Field label="Primary namespace" value={state.identity.primaryNamespace ?? ""} onChange={(value) => updateIdentity({ primaryNamespace: value })} mono />
    <Field label="Descriptor tags" value={(state.identity.descriptorTags ?? []).join(", ")} onChange={(value) => updateIdentity({ descriptorTags: value.split(",").map((tag) => tag.trim()).filter(Boolean) })} />
    <Field label="Initial folders" value={(state.folderProfile ?? []).join(", ")} placeholder="common, events, gfx, localisation/english, docs" onChange={(value) => update({ folderProfile: value.split(",").map((folder) => folder.trim()).filter(Boolean) })} />
    <details><summary>Advanced project metadata</summary>
    <Field label="Author" value={state.identity.author} onChange={(value) => updateIdentity({ author: value })} />
    <Field label="Version" value={state.identity.version} onChange={(value) => updateIdentity({ version: value })} />
    <Field label="Supported game version" value={state.identity.supportedGameVersion} onChange={(value) => updateIdentity({ supportedGameVersion: value })} />
    <Field label="Default branch" value={state.identity.defaultBranch} onChange={(value) => updateIdentity({ defaultBranch: value })} />
    </details>
  </div><Field label="Project folder" value={state.identity.projectRoot} placeholder="Finding the HOI4 mod folder…" onChange={(value) => { updateIdentity({ projectRoot: value }); update({ projectPathsOverridden: true, projectPathStatus: "manual", projectPathMessage: "Using your project location." }); }} action="Change location" onAction={() => void chooseFolder()} mono />{state.projectPathMessage && <p className={state.projectPathStatus === "collision" || state.projectPathStatus === "unavailable" ? "callout review" : "muted"} role="status">{state.projectPathMessage}</p>}{folderMessage && <p className="muted" role="status">{folderMessage}</p>}<details><summary>Launcher file location</summary><Field label="Launcher descriptor path" value={state.identity.launcherDescriptorPath ?? ""} placeholder="Filled automatically" onChange={(value) => { updateIdentity({ launcherDescriptorPath: value }); update({ projectPathsOverridden: true, projectPathStatus: "manual", projectPathMessage: "Using your project location." }); }} action="Choose mod folder" onAction={() => void chooseLauncherFolder()} mono /></details></section><section className="panel"><PanelTitle title="Generated files" /><div className="list-row"><div><strong>descriptor.mod</strong><span>Inside the project</span></div><button type="button" className="text-button" onClick={() => void showPreviews()}>Preview</button></div><div className="list-row"><div><strong>{state.identity.projectId || "project"}.mod</strong><span>HOI4 launcher file</span></div><button type="button" className="text-button" onClick={() => void showPreviews()}>Preview</button></div><div className="list-row"><div><strong>thumbnail.png</strong><span>Replaceable placeholder</span></div><button type="button" className="text-button" onClick={() => void showPreviews()}>Preview</button></div>{previewMessage && <p className="muted" role="status">{previewMessage}</p>}{previews.map((artifact) => <details key={artifact.destination}><summary>{artifact.destination}</summary><pre className="report-preview">{artifact.content}</pre></details>)}</section></div></div>;
}

const SCAN_STAGE_LABELS: Record<string, string> = {
  discovering_files: "Reading selected project metadata",
  detecting_descriptors: "Checking descriptors",
  detecting_thumbnail: "Checking thumbnail",
  detecting_structure: "Checking project structure",
  detecting_git: "Checking Git state",
  detecting_identifiers: "Checking identifiers and namespaces",
  detecting_localisation: "Checking localisation",
  detecting_documentation: "Checking documentation",
  detecting_agentic_files: "Checking agent files and skills",
  detecting_paths: "Checking project paths",
  detecting_components: "Checking managed components",
  complete: "Read-only scan complete",
  cancelled: "Read-only scan cancelled",
};

function formatScanBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${Math.round(bytes / 1024)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

export function Scan({ state, complete, error, progress, partial, limitsHit, canCancel, cancellationRequested, onCancel }: { state: WizardState; complete: boolean; error?: string; progress: ScanProgress; partial: boolean; limitsHit: string[]; canCancel: boolean; cancellationRequested: boolean; onCancel: () => Promise<void> }) {
  const currentCheck = cancellationRequested ? "Cancellation requested" : error ? "Scan needs attention" : complete && partial ? "Read-only scan reached a safety limit" : SCAN_STAGE_LABELS[progress.stage] ?? "Reading selected project metadata";
  const statusText = complete ? partial ? "Partial scan complete" : "100% complete" : cancellationRequested ? "Stopping scan" : "Scan in progress";
  return (
    <div className="scan-wrap"><section className="panel scan-panel">
      <div className="scan-top"><div aria-live="polite"><span className="muted">Current check</span><strong>{currentCheck}</strong></div><strong className="scan-percent">{complete ? "100%" : "Working"}</strong></div>
      <Progress value={complete ? 100 : undefined} valueText={statusText} label="Project scan progress" />
      {error && <p className="callout block" role="alert">{error}</p>}
      {partial && !error && <p className="callout review" role="status">Some files were not inspected because the scanner reached a safety limit. Review the partial result before continuing.</p>}
      {partial && limitsHit.length > 0 && <details><summary>Show scan limits</summary><p className="muted">{limitsHit.join(", ")}</p></details>}
      <div className="list-row no-border"><div><strong>{complete ? partial ? "Partial scan evidence saved for review" : "Scan evidence saved for review" : "Detected so far"}</strong><span>Current path: <code className="scan-path">{progress.currentPath || "."}</code></span><span>{progress.filesScanned.toLocaleString()} files, {progress.directoriesScanned.toLocaleString()} directories, {formatScanBytes(progress.bytesRead)} read</span></div><span className="muted">{state.mode === "existing" ? "bounded scan" : "new project"}</span></div>
      {canCancel && !complete && <button type="button" className="text-button scan-cancel" onClick={() => void onCancel()} disabled={cancellationRequested}>{cancellationRequested ? "Cancelling scan..." : "Cancel scan"}</button>}
    </section></div>
  );
}

function Findings({ state, findings, selected, setSelected, setFindings, onConfirmAnalysis, onManageExisting }: { state: WizardState; findings: ScanFinding[]; selected: string; setSelected: (id: string) => void; setFindings: Dispatch<SetStateAction<ScanFinding[]>>; onConfirmAnalysis: () => Promise<void>; onManageExisting: () => void }) {
  const active = findings.find((finding) => finding.id === selected) ?? findings[0];
  const managed = managedInstallationDetails(findings);
  return <div className="stack"><details><summary>{aiProviderLabel(state.aiProvider, state.aiProfiles)} input preview</summary><div className="manifest-details">{findings.filter((finding) => finding.status !== "rejected").map((finding) => <div key={finding.id}><strong>{finding.id}</strong><span>{finding.evidencePath ?? "approved finding reference"}</span><small>{finding.evidenceExcerpt ?? finding.value}</small></div>)}</div></details>{managed.present && managed.valid && <section className="callout info existing-setup-callout"><div><strong>Existing setup found</strong><p>This project already has a managed setup. You can repair it now or add the 3D workflow later without starting over.</p></div><button type="button" className="button secondary" onClick={onManageExisting}>Repair or add workflows</button></section>}{state.codexAnalysis && <CodexReview state={state} onConfirmAnalysis={onConfirmAnalysis} />}<div className="two-column"><section className="panel"><PanelTitle title="Project facts" />{findings.length ? <div>{findings.map((finding) => <button type="button" key={finding.id} className={`finding-row ${finding.status === "needs_review" ? "review" : ""}`} aria-pressed={finding.id === active?.id} onClick={() => setSelected(finding.id)}><span className={`state-icon ${finding.status === "needs_review" ? "review" : "pass"}`}>{finding.status === "needs_review" ? "!" : "✓"}</span><span><strong>{finding.label}</strong><small>{finding.value}</small></span><span className="text-button">{finding.status === "needs_review" ? "Review" : "Edit"}</span></button>)}</div> : <p className="muted">No scan findings are available in this runtime. The desktop scanner must return evidence before values can be accepted.</p>}</section><section className="panel selected-finding"><PanelTitle title="Selected finding" />{active ? <div className="selected-body"><label className="field-label" htmlFor="finding-value">Editable project value: {active.label}</label><input id="finding-value" className="text-input focused" value={active.value} onChange={(event) => setFindings((current) => current.map((finding) => finding.id === active.id ? { ...finding, value: event.target.value, status: "edited" } : finding))} /><span className="confidence">{Math.round(active.confidence * 100)}% confidence</span><button type="button" className="text-button" aria-pressed={active.status !== "rejected"} onClick={() => setFindings((current) => current.map((finding) => finding.id === active.id ? { ...finding, status: finding.status === "rejected" ? "accepted" : "rejected" } : finding))}>{active.status === "rejected" ? `Include in ${aiProviderLabel(state.aiProvider, state.aiProfiles)} input` : `Exclude from ${aiProviderLabel(state.aiProvider, state.aiProfiles)} input`}</button><div className="evidence-block"><span>Evidence</span><p>{active.evidence}</p></div><details><summary>Show matching files</summary><p className="muted">Full evidence and hashes stay behind progressive disclosure.</p></details></div> : <p className="muted">Select a finding after the bounded scan returns.</p>}</section></div></div>;
}

function formatManifestSize(component: ManifestComponentPreview): string {
  const bytes = component.expected_files.reduce((total, file) => total + (file.size ?? 0), 0);
  const size = bytes >= 1024 * 1024 ? `${(bytes / (1024 * 1024)).toFixed(1)} MB` : bytes >= 1024 ? `${Math.round(bytes / 1024)} KB` : bytes ? `${bytes} B` : "—";
  return `${component.expected_files.length} files · ${size}`;
}

function manifestRow(component: ManifestComponentPreview, selected: boolean, provider: AiProviderId = "codex", components: ManifestComponentPreview[] = [component]): ComponentRow {
  const platform = component.platforms.length === 1 && (component.platforms[0] === "windows" || component.platforms[0] === "macos") ? component.platforms[0] : "all";
  const providerBlocked = provider !== "codex" && dependsOn(component.id, "codex.config", components);
  return {
    id: component.id,
    title: component.display_name,
    detail: providerBlocked && component.id !== "codex.config"
      ? `${component.description ?? `${component.category} component from the resolved manifest`} Not available for the selected provider because the verified manifest requires Codex.`
      : component.description ?? `${component.category} component from the resolved manifest`,
    size: formatManifestSize(component),
    selected,
    required: component.id === "codex.config" ? provider === "codex" : !component.optional,
    platform,
    state: providerBlocked ? "blocked" : "supported",
  };
}

function dependsOn(componentId: string, targetId: string, components: ManifestComponentPreview[], seen = new Set<string>()): boolean {
  if (componentId === targetId) return true;
  if (!seen.add(componentId)) return false;
  const component = components.find((candidate) => candidate.id === componentId);
  return component?.dependencies.some((dependency) => dependsOn(dependency, targetId, components, seen)) ?? false;
}

function providerSupportsComponent(component: ManifestComponentPreview, components: ManifestComponentPreview[], provider: AiProviderId): boolean {
  return provider === "codex" || !dependsOn(component.id, "codex.config", components);
}

export function Components({ state, update }: { state: WizardState; update: (patch: Partial<WizardState>) => void }) {
  const [manifest, setManifest] = useState<SourceManifestPreview | null>(state.manifestPreview ?? null);
  const [manifestMessage, setManifestMessage] = useState(state.manifestPreview ? "Components loaded." : "Loading setup components…");

  useEffect(() => {
    let active = true;
    setManifest(null);
    setManifestMessage("Loading setup components…");
    void previewSourceManifestResult(state.sourceMode, state.pinnedRef).then((response) => {
      if (!active) return;
      const result = response.value;
      if (!result) {
        setManifest(null);
        setManifestMessage(response.error ? `The setup components could not be loaded: ${response.error}` : "The setup components could not be loaded.");
        return;
      }
      const requiredIds = result.components.filter((component) => providerSupportsComponent(component, result.components, state.aiProvider) && (component.id === "codex.config" ? state.aiProvider === "codex" : !component.optional)).map((component) => component.id);
      const availableIds = new Set(result.components.map((component) => component.id));
      const supportedIds = new Set(result.components.filter((component) => providerSupportsComponent(component, result.components, state.aiProvider)).map((component) => component.id));
      const selectedComponents = Array.from(new Set([
        ...state.selectedComponents.filter((id) => availableIds.has(id) && supportedIds.has(id)),
        ...requiredIds,
      ]));
      const rows = result.components.map((component) => manifestRow(component, selectedComponents.includes(component.id), state.aiProvider, result.components));
      setManifest(result);
      setManifestMessage("Components loaded.");
      update({ manifestPreview: result, components: rows, selectedComponents });
    });
    return () => { active = false; };
  }, [state.sourceMode, state.pinnedRef, state.aiProvider]);

  const rows = manifest?.components.map((component) => manifestRow(component, state.selectedComponents.includes(component.id), state.aiProvider, manifest.components)) ?? [];
  const visibleRows = rows.filter((component) => !component.id.startsWith("workflow."));
  const toggle = (id: string) => {
    const component = rows.find((row) => row.id === id);
    if (!component || component.required || component.state === "blocked") return;
    const selected = component.selected ? state.selectedComponents.filter((value) => value !== id) : [...state.selectedComponents, id];
    update({ selectedComponents: selected, components: rows.map((row) => row.id === id ? { ...row, selected: !row.selected } : row) });
  };
  return <div className="stack narrow"><section className="panel">{manifest ? visibleRows.map((component) => <button type="button" key={component.id} className="component-row" onClick={() => toggle(component.id)} aria-pressed={component.selected} aria-disabled={component.required || undefined}><span className={`checkbox ${component.selected ? "checked" : ""}`}>{component.selected ? "✓" : ""}</span><span><strong>{component.title}</strong><small>{component.detail}</small></span><span className="size">{component.size}</span></button>) : null}<p className="muted" role="status">{manifestMessage}</p><p className="muted">Source: <ExternalLink href="https://github.com/klimPaskov/Agentic-HOI4-Modding">Agentic HOI4 Modding <span aria-hidden="true">↗</span></ExternalLink></p><details><summary>Dependencies and file list</summary>{manifest ? <div className="manifest-details">{manifest.components.filter((component) => !component.id.startsWith("workflow.")).map((component) => <div key={component.id}><strong>{component.display_name}</strong><span>{component.dependencies.length ? `Requires ${component.dependencies.join(", ")}` : "No dependencies"} · {component.platforms.join(" / ")}</span><small>{component.expected_files.length === 1 ? "1 file" : `${component.expected_files.length} files`} · destination: {component.destination.path}</small></div>)}</div> : <p className="muted">Dependencies appear after the components load.</p>}</details><details><summary>Choose source version</summary><label className="field"><span className="field-label">Version</span><select className="text-input" value={state.sourceMode} onChange={(event) => update({ sourceMode: event.target.value as WizardState["sourceMode"], manifestPreview: undefined, components: [] })}><option value="latest">Latest</option><option value="pinned_commit">Specific commit</option><option value="pinned_release">Release</option></select></label>{state.sourceMode !== "latest" && <Field label={state.sourceMode === "pinned_commit" ? "Commit" : "Release"} value={state.pinnedRef} onChange={(value) => update({ pinnedRef: value, manifestPreview: undefined })} mono placeholder={state.sourceMode === "pinned_commit" ? "40-character commit" : "v1.0.0"} />}</details></section><div className="disclosure-note">Download size appears before installation.</div></div>;
}

function Workflows({ state, update }: { state: WizardState; update: (patch: Partial<WizardState>) => void }) {
  const meshComponent = state.manifestPreview?.components.find((component) => component.id === "workflow.3d");
  const meshUnavailable = state.aiProvider !== "codex" && meshComponent && !providerSupportsComponent(meshComponent, state.manifestPreview?.components ?? [], state.aiProvider);
  const superEventsComponent = state.manifestPreview?.components.find((component) => component.id === "workflow.super_events");
  const superEventsUnavailable = Boolean(state.manifestPreview && !superEventsComponent);
  const setWorkflow = (id: string, checked: boolean, patch: Partial<WizardState>) => {
    const selectedComponents = checked
      ? Array.from(new Set([...state.selectedComponents, id]))
      : state.selectedComponents.filter((componentId) => componentId !== id);
    update({ ...patch, selectedComponents });
  };
  return <div className="stack narrow"><section className="panel"><ToggleRow label="Do you want to set up the 3D models workflow?" detail={meshUnavailable ? "This workflow currently requires Codex" : "Adds the available 3D workflow files and checks"} checked={state.meshSelected} disabled={Boolean(meshUnavailable)} onChange={(checked) => setWorkflow("workflow.3d", checked, { meshSelected: checked })} /><ToggleRow label="Do you want to set up the Super Events workflow?" detail={superEventsUnavailable ? "Unavailable in the selected source version" : "Adds a ready-to-use popup, templates, example, and reusable registration workflow"} checked={state.superEventsSelected} disabled={superEventsUnavailable} onChange={(checked) => setWorkflow("workflow.super_events", checked, { superEventsSelected: checked })} /></section></div>;
}

function WorkflowDeclaration({ state }: { state: WizardState }) {
  const component = state.manifestPreview?.components.find((candidate) => candidate.id === "workflow.3d" && state.selectedComponents.includes(candidate.id));
  if (!component) {
    return <details><summary>Verified workflow declarations</summary><p className="muted">Tool names, commands, dependencies, and health checks appear only after the exact manifest revision is resolved. No macOS substitute is invented.</p></details>;
  }
  const tools = component.required_tools.map((tool) => tool.id).join(", ") || "None declared";
  const health = component.required_tools.flatMap((tool) => tool.health_checks).join(", ") || "None declared";
  const validation = component.validation.map((rule) => `${rule.id} · ${rule.severity}`).join(", ") || "None declared";
  return <details><summary>Verified workflow declarations</summary><div className="manifest-details"><div><strong>Source</strong><span>{component.source.path}</span></div><div><strong>Destination</strong><span>{component.destination.path}</span></div><div><strong>Tools</strong><span>{tools}</span></div><div><strong>Health checks</strong><span>{health}</span></div><div><strong>Validation</strong><span>{validation}</span></div></div></details>;
}

function Mesh({ state, update }: { state: WizardState; update: (patch: Partial<WizardState>) => void }) {
  const save = async () => {
    const value = state.meshKeyDraft;
    update({ meshKeyDraft: "" });
    const result = await storeMeshyCredential(value);
    update({ meshKeyStatus: result ? "present" : "missing", meshCredentialReference: result ?? undefined });
  };
  const credentialLabel = state.meshKeyStatus === "present" ? "Stored; health check pending" : state.meshKeyStatus === "verified" ? "Verified" : "Not stored";
  const remove = async () => {
    if (!state.meshCredentialReference) return;
    if (await removeMeshyCredential(state.meshCredentialReference)) {
      update({ meshKeyStatus: "missing", meshCredentialReference: undefined, transactionError: undefined });
    } else {
      update({ transactionError: "The operating-system credential was not removed; no project files were changed." });
    }
  };
  return <div className="stack narrow"><section className="panel form-panel"><label className="field-label" htmlFor="meshy-key">Meshy API key</label><p className="muted">Meshy.ai may charge for provider usage. Review the provider’s current pricing and account limits before running the source-declared workflow.</p><input id="meshy-key" className="text-input focused" type="password" value={state.meshKeyDraft} onChange={(event) => update({ meshKeyDraft: event.target.value })} autoComplete="off" /><div className="button-row"><button type="button" className="button primary" onClick={save}>Store in vault</button><button type="button" className="button secondary" onClick={() => update({ meshKeyDraft: "", meshKeyStatus: "missing" })}>Configure later</button>{state.meshCredentialReference && <button type="button" className="text-button" onClick={() => void remove()}>Delete stored key</button>}</div></section><section className="panel"><div className="list-row"><div><strong>MESHY_API_KEY</strong><span>Process-only environment variable</span></div><Status label={credentialLabel} tone={state.meshKeyStatus === "verified" ? "pass" : "review"} /></div><WorkflowDeclaration state={state} /></section></div>;
}

function Mcp({ state }: { state: WizardState }) {
  const component = state.manifestPreview?.components.find((candidate) => candidate.category === "mcp" && state.selectedComponents.includes(candidate.id));
  const toolText = component?.required_tools.map((tool) => `${tool.id}${tool.version ? ` ${tool.version}` : ""}`).join(", ") || "None declared";
  const environmentText = component?.environment.map((environment) => `${environment.name}${environment.secret ? " · secret" : ""}`).join(", ") || "None declared";
  const capabilityText = component?.capabilities.join(", ") || "None declared";
  const healthText = component?.required_tools.flatMap((tool) => tool.health_checks).join(", ") || "No health checks declared";
  return <div className="two-column"><section className="panel"><PanelTitle title="Integrations" />{component ? <div className="server-row"><img className="server-icon server-icon-image" src={appIcon} alt="" aria-hidden="true" /><span><strong>{component.display_name}</strong><small>{component.description ?? "Selected integration"}</small></span></div> : <p className="muted">No integration is selected.</p>}{component && <details><summary>Requirements</summary><div className="manifest-details"><div><strong>Platforms</strong><span>{component.platforms.join(" / ")}</span></div><div><strong>Tools</strong><span>{toolText}</span></div><div><strong>Environment names</strong><span>{environmentText}</span></div><div><strong>Capabilities</strong><span>{capabilityText}</span></div><div><strong>Health checks</strong><span>{healthText}</span></div><div><strong>Validation</strong><span>{component.validation.map((rule) => `${rule.id} · ${rule.severity}`).join(", ") || "None declared"}</span></div></div></details>}</section><section className="panel"><PanelTitle title="Credentials" /><div className="list-row"><div><strong>Meshy API key</strong><span>Stored securely by the operating system</span></div><Status label={state.meshSelected && state.meshKeyStatus === "present" ? "Stored" : state.meshSelected ? "Not stored" : "Not selected"} tone={state.meshSelected && state.meshKeyStatus === "present" ? "review" : "muted"} /></div><div className="list-row"><div><strong>Project secrets</strong><span>Secret values are never added to the project.</span></div><Status label="Protected" tone="info" /></div></section></div>;
}

export function Git({ state, update }: { state: WizardState; update: (patch: Partial<WizardState>) => void }) {
  const onlineAction = state.gitOnlineAction ?? "none";
  const repository = state.gitHubRepository || state.identity?.projectId || "my-mod";
  const chooseGitMode = (mode: WizardState["gitMode"]) => update({ gitMode: mode, ...(mode === "skip" ? { gitOnlineAction: "none" as const } : {}) });
  const chooseOnlineAction = (action: GitOnlineAction) => update({ gitOnlineAction: action });
  return <div className="stack narrow"><section className="panel">{(["initialize", "preserve", "skip"] as const).map((mode) => <label key={mode} className="radio-row"><input type="radio" name="git-mode" checked={state.gitMode === mode} onChange={() => chooseGitMode(mode)} /><span><strong>{mode === "initialize" ? "Initialize a Git repository" : mode === "preserve" ? "Preserve the existing repository" : "Skip Git setup"}</strong><small>{mode === "initialize" ? "Create .git, merge .gitignore, and prepare an initial commit" : mode === "preserve" ? "Keep remotes, history, and branch state" : "Continue without repository changes"}</small></span>{mode === "initialize" && <Status label="Recommended" tone="info" />}</label>)}</section><section className="panel form-panel"><div className="form-grid"><Field label="Default branch" value={state.gitBranch} onChange={(value) => update({ gitBranch: value })} /><label className="field"><span className="field-label">Initial commit</span><select className="text-input" value={state.initialCommit ? "after-validation" : "none"} onChange={(event) => update({ initialCommit: event.target.value === "after-validation" })}><option value="after-validation">Create after validation</option><option value="none">Do not create</option></select></label></div><details><summary>Remote settings</summary><div className="form-grid"><Field label="Remote name" value={state.gitRemoteName} onChange={(value) => update({ gitRemoteName: value })} placeholder="origin" /><Field label="Remote URL" value={state.gitRemoteUrl} onChange={(value) => update({ gitRemoteUrl: value })} placeholder="https://github.com/owner/repo.git" mono /></div></details></section>{state.gitMode !== "skip" && <section className="panel form-panel"><div className="section-label">Online Git</div>{(["none", "push_remote", "create_public_github"] as const).map((action) => <label key={action} className="radio-row"><input type="radio" name="git-online-action" checked={onlineAction === action} onChange={() => chooseOnlineAction(action)} /><span><strong>{action === "none" ? "Keep this project local" : action === "push_remote" ? "Push to an existing remote" : "Create a public GitHub repository"}</strong><small>{action === "none" ? "No online action will run." : action === "push_remote" ? "Push the validated branch after setup." : "Create the repository as public, then push this project."}</small></span>{action === "create_public_github" && <Status label="Separate approval" tone="review" />}</label>)}{onlineAction === "push_remote" && <p className="muted">The remote must already be configured and signed in through your Git credential helper.</p>}{onlineAction === "create_public_github" && <><Field label="GitHub repository" value={repository} onChange={(value) => update({ gitHubRepository: value })} placeholder="my-hoi4-mod" /><p className="muted">The app uses the GitHub sign-in already set up on this computer. It asks you to approve publication after setup.</p></>}</section>}</div>;
}

export function DryRun({ state, update }: { state: WizardState; update: (patch: Partial<WizardState>) => void }) {
  const plan = state.plan;
  const counts = plan?.operations.reduce((summary, operation) => {
    if (operation.action === "create" || operation.action === "generate") summary.create += 1;
    else if (operation.action === "replace" || operation.action === "merge" || operation.action === "rename") summary.update += 1;
    else if (operation.action === "skip") summary.skip += 1;
    return summary;
  }, { create: 0, update: 0, skip: 0 }) ?? { create: 0, update: 0, skip: 0 };
  const unresolved = plan?.conflicts.filter((conflict) => !conflict.selected).length;
  const planStatus = plan ? `${plan.operations.length} file changes` : "Plan unavailable";
  const onlineActionLabel = state.gitOnlineAction === "push_remote" ? "Push to an existing remote" : state.gitOnlineAction === "create_public_github" ? "Create a public GitHub repository" : "Keep this project local";
  const flattenedFiles = plan?.generated_artifacts?.filter((artifact) => artifact.destination.startsWith("chatgpt_project_sources/")) ?? [];
  const flattenedSize = flattenedFiles.reduce((total, artifact) => total + (artifact.bytes?.length ?? new TextEncoder().encode(artifact.content).length), 0);
  const flattenedSummary = flattenedFiles.length
    ? `${flattenedFiles.length} files · ${formatScanBytes(flattenedSize)}`
    : state.flattenForChat ? "Prepare changes to calculate" : "Optional";
  const chooseFlattenedSources = (selected: boolean) => update({
    flattenForChat: selected,
    plan: undefined,
    conflictChoice: undefined,
    transactionError: undefined,
  });
  return <div className="stack"><div className="metric-grid"><Metric label="Create" value={plan ? String(counts.create) : "—"} tone={plan ? "pass" : "info"} /><Metric label="Update" value={plan ? String(counts.update) : "—"} tone={plan ? "info" : "muted"} /><Metric label="Skip" value={plan ? String(counts.skip) : "—"} tone={plan ? "review" : "muted"} /><Metric label="Conflicts" value={plan ? String(unresolved) : "—"} tone={plan ? unresolved ? "block" : "pass" : "info"} /></div><div className="two-column"><section className="panel"><PanelTitle title="Plan summary" /><ChangeRow title="Add development tools" detail="Skills, instructions, configuration, and offline wiki" value={planStatus} /><ChangeRow title="Update project instructions" detail="Keeps your existing edits" status={plan ? "Review if modified" : "Pending"} /><ChangeRow title="Configure integrations" detail="Selected tools only" value={plan ? "Ready" : "Pending"} /><ChangeRow title="Git setup" detail={`${state.gitBranch} · local changes`} value={plan ? "Ready" : "Pending"} /><ChangeRow title="Online Git" detail="Runs only after setup" value={plan ? onlineActionLabel : "Pending"} />{plan && <details><summary>Open full file plan</summary><p className="muted">Review every file that will be created, updated, or kept.</p></details>}{plan?.external_actions?.length ? <details><summary>External tools requiring approval</summary><div className="manifest-details">{plan.external_actions.map((action) => <div key={action.id}><strong>{action.component_id}</strong><span>{action.display_command ?? action.command_source} · {action.risk} risk · approval required</span><small>Program: {action.executable ?? "Not declared"}; expected changes: {action.expected_writes?.join(", ") || "None declared"}</small></div>)}</div></details> : null}{!plan && <p className="muted">Prepare the changes before installation.</p>}</section><section className="panel"><PanelTitle title="Before setup" />{state.aiProvider === "codex" && <><label className="component-row flatten-package-row"><input className="visually-hidden" type="checkbox" checked={state.flattenForChat} onChange={(event) => chooseFlattenedSources(event.target.checked)} /><span className={`checkbox ${state.flattenForChat ? "checked" : ""}`} aria-hidden="true">{state.flattenForChat ? "✓" : ""}</span><span><strong>Prepare a flattened ChatGPT project-sources folder</strong><small>Project guidance, README, skills, and subagents</small></span><span className="size">{flattenedSummary}</span></label>{state.flattenForChat && flattenedFiles.length > 0 && <details><summary>{flattenedSummary}</summary><div className="manifest-details flattened-file-list">{flattenedFiles.map((artifact) => { const bytes = artifact.bytes?.length ?? new TextEncoder().encode(artifact.content).length; return <div key={artifact.destination}><strong>{artifact.destination.replace("chatgpt_project_sources/", "")}</strong><small>{formatScanBytes(bytes)}</small></div>; })}</div></details>}</>}<CheckRow label="Existing files saved" status={plan ? "Ready" : "Pending"} tone={plan ? "pass" : "info"} /><CheckRow label="External tools" status={plan ? "Review required actions" : "Pending"} tone="review" /><CheckRow label="Unresolved conflicts" status={plan ? String(unresolved) : "Pending"} tone={plan ? unresolved ? "block" : "pass" : "info"} /></section></div></div>;
}

function Install({ state }: { state: WizardState }) {
  const stages = state.transaction?.stages ?? ["preflight", "repository source resolution", "selective download", "checksum verification", "dry-run review", "backup", "staging", "validation", "apply", "post-install checks", "readiness report", "rollback record"].map((id) => ({ id, status: "pending" }));
  const stageLabel = (id: string) => ({
    preflight: "Check the project",
    "repository source resolution": "Find the selected source",
    "selective download": "Download selected files",
    "checksum verification": "Verify downloaded files",
    "dry-run review": "Confirm the changes",
    backup: "Save existing files",
    staging: "Prepare the setup",
    validation: "Validate the project",
    apply: "Apply the setup",
    "post-install checks": "Check the result",
    "readiness report": "Check readiness",
    "rollback record": "Save recovery information",
  } as Record<string, string>)[id] ?? id;
  const completed = state.transaction?.stages.filter((stage) => stage.status === "completed").length ?? 0;
  const progress = state.transaction ? Math.round((completed / stages.length) * 100) : state.installProgress;
  return <div className="center-column"><section className="panel install-panel"><div className="install-progress"><Progress value={progress} label="Installation progress" /><Status label={state.transaction ? `${progress}%` : "Awaiting transaction"} tone={state.transaction ? "info" : "muted"} /></div>{stages.map((stage) => { const done = stage.status === "completed"; const active = !done && state.transaction?.last_checkpoint === stage.id; return <div key={stage.id} className={`timeline-row ${done ? "done" : active ? "active" : ""}`}><span className="timeline-icon">{done ? "✓" : active ? "●" : ""}</span><strong>{stageLabel(stage.id)}</strong><span>{done ? "Done" : active ? "In progress" : "Next"}</span></div>; })}<details><summary>Show setup details</summary><p className="muted">The app keeps a protected record of each step so an interrupted setup can continue safely.</p></details></section></div>;
}

function OnlineGitAction({ state, update }: { state: WizardState; update: (patch: Partial<WizardState>) => void }) {
  const action = state.gitOnlineAction ?? "none";
  const [approvalRequested, setApprovalRequested] = useState(false);
  const [review, setReview] = useState<GitOnlinePlan | null>(null);
  const [result, setResult] = useState<GitOnlineResult | null>(null);
  const [error, setError] = useState<string>();
  const [pushAfterCreation, setPushAfterCreation] = useState(false);
  if (action === "none") return null;
  const repository = state.gitHubRepository || state.identity?.projectId || "my-mod";
  const remoteName = state.gitRemoteName || "origin";
  const isCreating = action === "create_public_github" && !pushAfterCreation;
  const reviewAction: Exclude<GitOnlineAction, "none"> = pushAfterCreation ? "push_remote" : action;
  const prepareReview = async () => {
    setError(undefined);
    const response = await prepareGitOnlineAction({
      projectRoot: state.identity.projectRoot,
      action: reviewAction,
      remoteName,
      repository,
      branch: state.gitBranch || "main",
    });
    if (response.value) {
      setReview(response.value);
      setApprovalRequested(true);
    } else {
      setError(response.error ?? "The online Git review could not be prepared.");
    }
  };
  const approveAndRun = async () => {
    if (!review) return;
    setError(undefined);
    const response = await runGitOnlineAction({
      projectRoot: state.identity.projectRoot,
      planId: review.plan_id,
      confirmed: true,
      transactionId: null,
    });
    if (response.value) {
      setResult(response.value);
      setReview(null);
      setApprovalRequested(false);
      if (response.value.action === "create_public_github") setPushAfterCreation(true);
      else setPushAfterCreation(false);
      update({ transactionError: undefined });
    } else {
      const message = response.error ?? "The online Git action could not be completed.";
      setError(message);
      update({ transactionError: message });
    }
  };
  return <section className="panel online-git-panel"><PanelTitle title={isCreating ? "Publish on GitHub" : "Push changes online"} /><p className="muted">{isCreating ? "Create " + repository + " as a public GitHub repository." : "Push the " + (state.gitBranch || "main") + " branch to " + remoteName + "."}</p>{result && <div className="callout pass" role="status"><strong>{result.message}</strong>{result.repository_url && <a href={result.repository_url} target="_blank" rel="noreferrer">Open GitHub repository ↗</a>}</div>}{result?.action === "create_public_github" && pushAfterCreation && !approvalRequested && <button type="button" className="button secondary" disabled={!state.readiness?.coreReady} onClick={() => void prepareReview()}>Review push</button>}{!result && !approvalRequested && <button type="button" className="button secondary" disabled={!state.readiness?.coreReady} onClick={() => void prepareReview()}>{isCreating ? "Review publication" : "Review push"}</button>}{approvalRequested && review && <div className="callout review"><strong>{reviewAction === "create_public_github" ? "Make this project public on GitHub?" : "Push this reviewed commit now?"}</strong><p>Commit {review.head_sha.slice(0, 12)} on {review.branch} is ready for your separate approval.</p><div className="button-row"><button type="button" className="button primary" onClick={() => void approveAndRun()}>{reviewAction === "create_public_github" ? "Approve and create" : "Approve and push"}</button><button type="button" className="button secondary" onClick={() => { setApprovalRequested(false); setReview(null); }}>Cancel</button></div></div>}{error && <p className="callout block" role="alert">{error}</p>}</section>;
}

export function Ready({ state, update, onMaintenance }: { state: WizardState; update: (patch: Partial<WizardState>) => void; onMaintenance: (screen: "update" | "conflict" | "recovery") => void }) {
  const [mcpCheck, setMcpCheck] = useState<WorkflowHealthResult>();
  const [openMessage, setOpenMessage] = useState<string>();
  const report = state.readiness;
  const selectedProvider = state.aiProvider ?? "codex";
  const providerLabel = aiProviderLabel(selectedProvider, state.aiProfiles);
  const coreReady = report?.coreReady === true;
  const open = selectedProvider === "codex" && report?.openInCodex === true;
  const readinessPending = report === null;
  const project = readinessRow(report, ["descriptor.project", "structure.core"]);
  const codex = readinessRow(report, ["codex.agents", "skills.core", "subagents.core", "codex.config"]);
  const mcpWiki = readinessRow(report, ["mcp.hoi4", "wiki.coverage"]);
  const mcpStatus = report?.checks.find((check) => check.id === "mcp.hoi4")?.status;
  const mcpRouteUnavailable = mcpStatus === "planned_unavailable" || mcpStatus === "unsupported_platform";
  const gitHashes = readinessRow(report, ["git.project", "hashes.managed", "conflicts.resolved", "dependencies.core"]);
  const mesh = readinessRow(report, ["workflow.3d"]);
  const superEvents = readinessRow(report, ["workflow.super_events"]);
  const run3dCheck = async () => {
    const result = await run3DHealthCheck(state.identity.projectRoot);
    if (!result) {
      update({ transactionError: "The verified 3D bootstrap could not be started. The workflow remains incomplete." });
      return;
    }
    if (result.status === "ready") {
      update({ meshKeyStatus: "verified", readiness: null, transactionError: undefined });
    } else {
      update({ readiness: null, transactionError: `The source-declared 3D check did not pass (exit ${result.exit_code ?? "unknown"}). The workflow remains incomplete.` });
    }
  };
  const runMcpCheck = async () => {
    const result = await runMcpHealthCheck(state.identity.projectRoot);
    if (!result) {
      update({ transactionError: "The source-declared MCP health check could not be started." });
      return;
    }
    setMcpCheck(result);
    update({ readiness: null, transactionError: result.status === "ready" ? undefined : "The source-declared MCP initialize check did not pass." });
  };
  const handleOpen = async () => {
    setOpenMessage(undefined);
    const result = await openInCodex(state.identity.projectRoot);
    if (!result) {
      update({ transactionError: "Codex could not be opened. Check the Codex installation or open the project folder manually." });
      return;
    }
    update({ transactionError: undefined });
    setOpenMessage(result.message);
  };
  return (
    <div className="stack">
      <section className={`ready-banner ${coreReady ? "pass" : "block"}`} aria-live="polite">
        <span className="ready-icon">{readinessPending ? "…" : coreReady ? "✓" : "!"}</span>
        <div>
          <h2>{readinessPending ? "Checking readiness" : `${state.identity.displayName || "Project"} ${coreReady ? "is ready" : "needs review"}`}</h2>
          <p>{readinessPending ? "Core checks are still being evaluated." : coreReady ? `Optional workflow status does not block ${providerLabel}.` : "Resolve blocking checks before continuing."}</p>
        </div>
        <div className="ready-action">
          {selectedProvider === "codex" && <button type="button" className="button primary" disabled={!open} aria-describedby="open-in-codex-help" onClick={() => void handleOpen()}>Open in Codex ↗</button>}
          {openMessage && <p className="ready-action-message" role="status">{openMessage}</p>}
        </div>
        <span id="open-in-codex-help" className="visually-hidden">
          {readinessPending ? "Readiness checks are still running." : open ? "Opens the project in Codex, or shows a manual folder-opening instruction if no verified opener is installed." : selectedProvider === "codex" ? "Resolve blocking checks before opening in Codex." : `The project is ready for ${providerLabel}; no Codex opener is offered for this provider.`}
        </span>
      </section>
      <div className="two-column">
        <section className="panel">
          <CheckRow label="Project and descriptors" status={project.status} tone={project.tone} />
          <CheckRow label={`${providerLabel} instructions and skills`} status={codex.status} tone={codex.tone} />
          <CheckRow label="MCP and offline wiki" status={mcpWiki.status} tone={mcpWiki.tone} />
          {state.selectedComponents.includes("mcp.hoi4_agent_tools") && <>
            {mcpRouteUnavailable ? <p className="muted" role="status">This optional integration is unavailable on this computer.</p> : <button type="button" className="button secondary" onClick={() => void runMcpCheck()}>Check integration</button>}
            {mcpCheck && <p className="muted" role="status">{mcpCheck.status === "ready" ? "Integration checked" : "Integration needs review"}</p>}
          </>}
          <CheckRow label="Git and managed files" status={gitHashes.status} tone={gitHashes.tone} />
        </section>
        <section className="panel">
          <CheckRow label="3D model workflow" status={mesh.status} tone={mesh.tone} />
          <CheckRow label="Super Events workflow" status={superEvents.status} tone={superEvents.tone} />
          {state.flattenForChat && selectedProvider === "codex" && <div className="callout info"><strong>ChatGPT Chat sources prepared</strong><p>After setup, start planning using ChatGPT &quot;Chat&quot;. No upload or planning action starts automatically.</p></div>}
          {state.meshSelected && <button type="button" className="button secondary" onClick={() => void run3dCheck()}>Check 3D setup</button>}
          {coreReady && <div className="portrait-workflow-note"><ExternalLink href="https://github.com/klimPaskov/comfyui-hoi4-portraits">Optional portrait workflow: Explore ComfyUI HOI4 Portraits on GitHub <span aria-hidden="true">↗</span></ExternalLink></div>}
          <details>
            <summary>Open readiness report</summary>
            <pre className="report-preview">{report ? JSON.stringify(report.checks, null, 2) : "Report will be stored after post-install verification."}</pre>
          </details>
        </section>
      </div>
      <OnlineGitAction state={state} update={update} />
      <div className="button-row end">
        <button type="button" className="text-button" onClick={() => onMaintenance("update")}>Manage installation</button>
        <button type="button" className="text-button" onClick={() => update({ readiness: null })}>Refresh checks</button>
      </div>
    </div>
  );
}

function readinessRow(report: ReadinessReport | null, ids: string[]): { status: string; tone: StatusTone } {
  if (!report) return { status: "Checking", tone: "info" };
  const checks = report.checks.filter((check) => ids.includes(check.id));
  if (!checks.length) return { status: "Not reported", tone: "review" };
  if (checks.some((check) => check.status === "block" && check.blocking)) return { status: "Blocked", tone: "block" };
  if (checks.some((check) => ["block", "warn", "unsupported_platform"].includes(check.status))) return { status: "Review", tone: "review" };
  if (checks.some((check) => check.status === "planned_unavailable")) return { status: "Planned", tone: "review" };
  if (checks.every((check) => check.status === "not_selected")) return { status: "Not selected", tone: "muted" };
  return { status: "Pass", tone: "pass" };
}

function MaintenanceWorkflowOptions({ state, update }: { state: WizardState; update: (patch: Partial<WizardState>) => void }) {
  if (!state.existingInstallationDetected) return null;
  const meshInstalled = state.installedWorkflow3dState !== undefined
    && state.installedWorkflow3dState !== "not_selected"
    && state.installedWorkflow3dState !== "unsupported_platform";
  const superEventsInstalled = state.installedSuperEventsState !== undefined
    && state.installedSuperEventsState !== "not_selected"
    && state.installedSuperEventsState !== "unsupported_platform";
  const keyNeedsConfiguration = meshInstalled && state.meshKeyStatus === "missing";
  const storeKey = async () => {
    const credential = state.meshKeyDraft;
    update({ meshKeyDraft: "" });
    const reference = await storeMeshyCredential(credential);
    if (reference) {
      update({ meshKeyStatus: "present", meshCredentialReference: reference, transactionError: undefined });
    } else {
      update({ transactionError: "The Meshy key could not be stored. Nothing in the project was changed." });
    }
  };
  return <section className="panel maintenance-workflow-panel"><PanelTitle title="Optional workflows" /><ToggleRow label="Do you want to set up the 3D models workflow?" detail={meshInstalled ? "Already part of this project" : "Add it during the next update or repair"} checked={meshInstalled || state.meshSelected} disabled={meshInstalled} onChange={(checked) => update({ meshSelected: checked })} /><ToggleRow label="Do you want to set up the Super Events workflow?" detail={superEventsInstalled ? "Already part of this project" : "Add it during the next update or repair"} checked={superEventsInstalled || state.superEventsSelected} disabled={superEventsInstalled} onChange={(checked) => update({ superEventsSelected: checked })} />{state.meshSelected && (!meshInstalled || keyNeedsConfiguration) && <div className="maintenance-key"><p className="muted">A Meshy key is optional for the file repair. Store it now if you want the workflow ready to run.</p><label className="field-label" htmlFor="maintenance-meshy-key">Meshy API key</label><div className="input-with-action"><input id="maintenance-meshy-key" className="text-input" type="password" autoComplete="off" value={state.meshKeyDraft} onChange={(event) => update({ meshKeyDraft: event.target.value })} /><button type="button" className="input-action" onClick={() => void storeKey()} disabled={!state.meshKeyDraft}>Store</button></div><span className="muted">Stored in the operating-system credential vault.</span></div>}</section>;
}

export function Update({ state, update, findings, setFindings, onMaintenance, onStartMaintenance, onReanalyze }: { state: WizardState; update: (patch: Partial<WizardState>) => void; findings: ScanFinding[]; setFindings: Dispatch<SetStateAction<ScanFinding[]>>; onMaintenance: (screen: "update" | "conflict" | "recovery") => void; onStartMaintenance: (mode: "update" | "repair" | "reinstall" | "remove") => void; onReanalyze: () => Promise<boolean> }) {
  const plan = state.plan;
  const optional3d = state.meshSelected ? state.meshKeyStatus === "present" ? "Stored; health check pending" : "Selected; key not stored" : "Not selected";
  const superEvents = state.superEventsSelected ? state.installedSuperEventsState === "not_selected" ? "Selected for the next change" : "Installed" : "Not selected";
  const providerLabel = aiProviderLabel(state.aiProvider, state.aiProfiles);
  const reanalysisLabel = state.maintenanceEvidenceReady ? state.maintenanceCodexAnalysisRecord ? "Run again" : `Run ${providerLabel} reanalysis` : "Prepare read-only evidence";
  return <div className="stack"><div className="action-grid"><ActionTile title="Check for updates" detail="Compare this project with a newer setup." onClick={() => onStartMaintenance("update")} /><ActionTile title="Repair installation" detail="Restore missing or damaged setup files." onClick={() => onStartMaintenance("repair")} /><ActionTile title="Remove components" detail="Review the files before removing app-managed setup." onClick={() => onStartMaintenance("remove")} /><ActionTile title="Recover interrupted setup" detail="Continue or undo an interrupted change." onClick={() => onMaintenance("recovery")} /></div><MaintenanceWorkflowOptions state={state} update={update} /><section className="panel"><PanelTitle title={`${providerLabel} review`} /><p className="muted">Review the project before updating its setup.</p><button type="button" className="button secondary" onClick={() => void onReanalyze()}>{reanalysisLabel}</button>{state.maintenanceEvidenceReady && <details open><summary>{findings.filter((finding) => finding.status !== "rejected").length} approved findings</summary><div className="manifest-details">{findings.filter((finding) => finding.status !== "rejected").map((finding) => <div key={finding.id}><strong>{finding.id}</strong><span>{finding.evidencePath ?? "approved finding reference"}</span><small>{finding.evidenceExcerpt ?? finding.value}</small><button type="button" className="text-button" aria-pressed="true" onClick={() => setFindings((current) => current.map((candidate) => candidate.id === finding.id ? { ...candidate, status: "rejected" } : candidate))}>Exclude</button></div>)}</div></details>}{state.maintenanceCodexAnalysisRecord && <p className="muted" role="status">Review returned. Confirm the {providerLabel} suggestions before checking for updates.</p>}</section><section className="panel"><PanelTitle title="Installed state" /><CheckRow label="Core setup" status={plan ? `${plan.operations.length} planned changes` : "No plan loaded"} tone={plan ? "info" : "muted"} /><CheckRow label="Optional 3D workflow" status={optional3d} tone={state.meshSelected ? "review" : "muted"} /><CheckRow label="Super Events workflow" status={superEvents} tone={state.superEventsSelected ? "review" : "muted"} /><CheckRow label="Modified files" status={plan ? String(plan.conflicts.length) : "Not evaluated"} tone={plan?.conflicts.length ? "review" : "muted"} />{plan && <details open><summary>Reviewed changes</summary><p className="muted">Modified files remain visible until resolved.</p></details>}</section></div>;
}

function Conflict({ state, update, onChoice }: { state: WizardState; update: (patch: Partial<WizardState>) => void; onChoice: (choice: ConflictChoice) => void }) {
  const conflict = state.plan?.conflicts.find((candidate) => !candidate.selected) ?? state.plan?.conflicts[0];
  const choices = (conflict?.options ?? ["keep", "replace", "merge", "rename", "skip"]).filter((choice): choice is ConflictChoice => ["keep", "replace", "merge", "rename", "skip"].includes(choice));
  const operation = conflict && state.plan?.operations.find((candidate) => candidate.destination === conflict.path);
  const [preview, setPreview] = useState<ConflictPreview>();
  const [previewMessage, setPreviewMessage] = useState<string>();
  const planId = state.plan?.plan_id;
  const conflictPath = conflict?.path;
  useEffect(() => {
    let active = true;
    setPreview(undefined);
    if (!planId || !conflictPath) {
      setPreviewMessage(undefined);
      return () => { active = false; };
    }
    setPreviewMessage("Loading the file comparison…");
    void previewInstallationConflict(planId, conflictPath).then((result) => {
      if (!active) return;
      if (!result) {
        setPreviewMessage("The file preview is unavailable. You can keep the current file or skip this change.");
        return;
      }
      setPreview(result);
      setPreviewMessage(result.truncated && result.redacted
        ? "Long content was shortened and sensitive-looking values were hidden."
        : result.truncated
          ? "Long content was shortened for this preview."
          : result.redacted
            ? "Sensitive-looking values were hidden from this preview."
            : undefined);
    });
    return () => { active = false; };
  }, [planId, conflictPath]);
  const content = (value: string | null | undefined, fallback: string) => value ?? fallback;
  const baseText = content(preview?.base, preview ? "No original content is available." : "Loading preview…");
  const localText = content(preview?.local, preview ? "No current content is present." : "Loading preview…");
  const incomingText = content(preview?.incoming, preview ? "No new text preview is available." : "Loading preview…");
  return <div className="stack"><div className="diff-grid"><DiffPane title="Original" badge={preview?.base ? "Previously installed" : "Reference"} text={`${conflict?.path ?? "No conflict selected"}\n\n${baseText}`} tone="neutral" /><DiffPane title="Current" badge={operation?.local_state === "modified" ? "Your changes" : "Not present"} text={`${conflict?.path ?? "No conflict selected"}\n\n${localText}`} tone="minus" /><DiffPane title="New" badge={preview?.kind ? "Available update" : "Incoming file"} text={`${conflict?.path ?? "No conflict selected"}\n\n${incomingText}`} tone="plus" /></div>{previewMessage && <p className="muted" role="status">{previewMessage}</p>}<section className="panel conflict-actions"><p className="muted">{conflict ? `Choose what to do with ${conflict.path}. Any merged result is checked before it is saved.` : "All conflicts are resolved."}</p><div className="button-row">{choices.map((choice) => <button type="button" key={choice} className={`button ${state.conflictChoice === choice ? "primary" : "secondary"}`} aria-pressed={state.conflictChoice === choice} onClick={() => { update({ conflictChoice: choice }); void onChoice(choice); }}>{choice === "keep" ? "Keep current" : choice === "replace" ? "Use new" : choice === "merge" ? "Merge" : choice === "rename" ? "Keep both" : "Skip"}</button>)}</div></section></div>;
}

function Recovery({ state, update, onPickProjectFolder, onStartMaintenance }: { state: WizardState; update: (patch: Partial<WizardState>) => void; onPickProjectFolder: () => Promise<FolderSelection | null>; onStartMaintenance: (mode: "update" | "repair" | "reinstall" | "remove") => Promise<void> | void }) {
  const transaction = state.transaction;
  if (!transaction) {
    const choose = async () => {
      const selected = await onPickProjectFolder();
      if (selected?.path) update({ identity: { ...state.identity, projectRoot: selected.path }, transactionError: undefined });
      else if (selected?.error) update({ transactionError: `The selected folder could not be used: ${selected.error}` });
    };
    return <div className="stack narrow"><section className="panel"><PanelTitle title="Recover setup" /><p className="muted">Choose an installed project to continue an interrupted setup or remove files added by the app.</p><div className="button-row"><button type="button" className="button secondary" onClick={() => void choose()}>Choose another project</button><button type="button" className="button secondary" disabled={!state.identity.projectRoot.trim()} onClick={() => void onStartMaintenance("remove")}>Remove managed components</button></div></section></div>;
  }
  const options: Array<{ id: RecoveryChoice; title: string; detail: string; allowed: boolean }> = [
    { id: "resume", title: "Continue setup", detail: "Check the prepared files again and continue where setup stopped.", allowed: transaction.recovery.resume_allowed },
    { id: "rollback", title: "Undo changes", detail: "Return the project to the state it had before this setup began.", allowed: transaction.recovery.rollback_allowed },
    { id: "discard", title: "Discard prepared files", detail: "Remove temporary setup files without changing the project.", allowed: transaction.recovery.discard_staging_allowed },
  ];
  const checkpoint = transaction.state === "finalizing"
    ? "The files are installed. Continue setup to finish the final checks."
    : transaction.state === "rolling_back"
      ? "The app is restoring the previous project state. Choose Undo changes to continue or inspect the project."
      : transaction.recovery.project_apply_started
        ? "Some project files were already changed, so continuing automatically is unavailable."
    : "Setup stopped before any project files were changed.";
  return <div className="stack"><div className="callout review">{checkpoint}</div><div className="recovery-grid">{options.map((item) => <button type="button" key={item.id} className={`recovery-card ${state.recoveryChoice === item.id ? "selected" : ""}`} aria-pressed={state.recoveryChoice === item.id} disabled={!item.allowed} onClick={() => update({ recoveryChoice: item.id })}><span className="choice-radio" aria-hidden="true" /><strong>{item.title}</strong><p>{item.detail}</p></button>)}</div><section className="panel recovery-actions"><button type="button" className="text-button" onClick={() => void onStartMaintenance("remove")}>Manage installed components</button></section></div>;
}

function Field({ label, value, onChange, placeholder, action, onAction, mono }: { label: string; value: string; onChange: (value: string) => void; placeholder?: string; action?: string; onAction?: () => void; mono?: boolean }) {
  const id = `field-${label.toLowerCase().replace(/[^a-z0-9]+/g, "-")}`;
  return <div className="field"><label className="field-label" htmlFor={id}>{label}</label><span className="input-with-action"><input id={id} className={`text-input ${mono ? "mono" : ""}`} value={value} placeholder={placeholder} onChange={(event) => onChange(event.target.value)} />{action && onAction && <button type="button" className="input-action" aria-label={`${action} ${label.toLowerCase()}`} onClick={onAction}>{action}</button>}</span></div>;
}
function PanelTitle({ title }: { title: string }) { return <h2 className="panel-title">{title}</h2>; }
function Status({ label, tone }: { label: string; tone: StatusTone }) { return <span className={`status status-${tone}`}><span className="status-symbol" aria-hidden="true">{tone === "pass" ? "✓" : tone === "block" ? "!" : tone === "review" ? "!" : tone === "info" ? "i" : "–"}</span>{label}</span>; }
function Progress({ value, label, valueText }: { value?: number; label: string; valueText?: string }) {
  const bounded = value === undefined ? undefined : Math.min(100, Math.max(0, value));
  return <div className={`progress ${bounded === undefined ? "indeterminate" : ""}`} role="progressbar" aria-label={label} aria-valuemin={0} aria-valuemax={100} aria-valuetext={valueText ?? (bounded === undefined ? "In progress" : `${bounded}% complete`)} {...(bounded === undefined ? {} : { "aria-valuenow": bounded })}><span style={bounded === undefined ? undefined : { width: `${bounded}%` }} /></div>;
}
function ToggleRow({ label, detail, checked, disabled = false, onChange }: { label: string; detail: string; checked: boolean; disabled?: boolean; onChange: (checked: boolean) => void }) { return <button type="button" className="toggle-row" role="switch" aria-checked={checked} aria-disabled={disabled || undefined} disabled={disabled} onClick={() => onChange(!checked)}><span className="toggle-copy"><strong>{label}</strong><small>{detail}</small></span><span className={`toggle ${checked ? "on" : ""}`} aria-hidden="true"><i /></span></button>; }
function Metric({ label, value, tone }: { label: string; value: string; tone: StatusTone }) { return <div className={`metric metric-${tone}`}><span>{label}</span><strong>{value}</strong></div>; }
function ChangeRow({ title, detail, value, status }: { title: string; detail: string; value?: string; status?: string }) { return <div className="change-row"><span className="change-icon">◆</span><span><strong>{title}</strong><small className="mono">{detail}</small></span>{value && <span className="size">{value}</span>}{status && <Status label={status} tone="review" />}</div>; }
function CheckRow({ label, status, tone }: { label: string; status: string; tone: StatusTone }) { return <div className="check-row"><strong>{label}</strong><Status label={status} tone={tone} /></div>; }
function ActionTile({ title, detail, onClick }: { title: string; detail: string; onClick: () => void }) { return <button className="action-tile" onClick={onClick}><ChoiceIcon kind="sparkle" /><strong>{title}</strong><p>{detail}</p></button>; }
function DiffPane({ title, badge, text, tone }: { title: string; badge: string; text: string; tone: "plus" | "minus" | "neutral" }) {
  const titleId = `diff-${tone}-${title.toLowerCase()}`;
  return <section className="diff-pane" aria-labelledby={titleId}><h2 className="diff-title"><strong id={titleId}>{title}</strong><span>{badge}</span></h2><pre className={`diff-code ${tone}`} aria-label={`${title} code preview, ${badge}`}>{text}</pre></section>;
}
