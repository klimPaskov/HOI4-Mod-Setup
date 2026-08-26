import { useEffect, useRef, useState } from "react";
import type { Dispatch, KeyboardEvent as ReactKeyboardEvent, ReactNode, SetStateAction } from "react";
import { applyInstallationResult, approveInstallation, approveScanEvidence, buildInstallationPlan, buildInstallationPlanResult, buildMaintenancePlan, cancelCodexLogin, cancelScan, checkForAppUpdate, confirmCodexAnalysis, discardInstallationStaging, evaluateReadiness, findInterruptedTransaction, installAppUpdate, installLocalPortraitWorkflows, inspectLocalPortraitProvider, isTauriRuntime, logoutCodexResult, openCodexLoginUrlResult, openExternalUrlResult, openInCodex, packageChatSources, pickChatSourcesFolder, pickLauncherFolder, pickProjectFolder, prepareGitOnlineAction, previewChatSources, previewDescriptorsResult, previewInstallationConflict, previewSourceManifestResult, readAiAccount, readAiModels, readAiProviderProfiles, readCodexAccount, readMeshyCredential, readTransactionJournal, removeAiProviderCredential, removeMeshyCredential, resolveInstallationConflict, resumeInstallation, rollbackInstallationResult, runAiAnalysisResult, runCodexAnalysisResult, runGitOnlineAction, runMcpHealthCheck, scanProject, startCodexLogin, storeAiProviderCredential, storeMeshyCredential, suggestProjectPaths, waitForCodexLoginResult } from "./lib/tauri";
import { deriveGeneratedIdentity, HOI4_DESCRIPTOR_TAGS } from "./identity";
import type { AiModelOption, AiProviderId, AiProviderProfile, AppUpdateStatus, ChatSourcesPreview, CodexAnalysisRequest, ComponentRow, ConflictChoice, ConflictPreview, FolderSelection, GeneratedArtifactPreview, GitOnlineAction, GitOnlinePlan, GitOnlineResult, InstallationPlan, LocalPortraitDiscovery, ManifestComponentPreview, PhaseId, PortraitPipelineState, PortraitProviderId, PortraitProviderStatus, ProjectIdentity, ReadinessReport, RecoveryChoice, ScanFinding, ScanProgress, ScreenId, SourceManifestPreview, StatusTone, TransactionJournal, WizardState, WorkflowHealthResult, WorkflowState } from "./types";
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

const DEFAULT_DESCRIPTION = "An Atlantis total conversion with a new Atlantic island, naval expansion, custom units, national focuses, and original mechanics.";
const DEFAULT_GENERATED_IDENTITY = deriveGeneratedIdentity("Atlantis Rising", DEFAULT_DESCRIPTION);

const DEFAULT_IDENTITY: ProjectIdentity = {
  displayName: "Atlantis Rising",
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

const PORTRAIT_REPOSITORY = "https://github.com/klimPaskov/comfyui-hoi4-portraits";
const PORTRAIT_BRANCH = "codex/portrait-pipeline";
const PORTRAIT_COMMIT = "b47222a77f2f6454704530865aa1441fad48bdd3";
const PORTRAIT_RESOURCE_REQUIREMENT = "Minimum recommended to run this workflow: 16 GB VRAM and 25 GB storage.";

const DEFAULT_PORTRAIT_PIPELINE: PortraitPipelineState = {
  enabled: false,
  provider: "disabled",
  providerStatus: "not_selected",
  workflowRepository: PORTRAIT_REPOSITORY,
  workflowBranch: PORTRAIT_BRANCH,
  workflowCommit: PORTRAIT_COMMIT,
  preferredWorkflow: "source",
  localComfyuiRoot: "",
  localServerUrl: "http://127.0.0.1:8188",
  runpodUrl: "",
  runpodWorkspace: "/workspace/comfyui-hoi4-portraits",
  mcpRegistered: false,
};

function portraitComponentIdsFor(provider: PortraitProviderId): string[] {
  if (provider === "disabled") return [];
  return [
    "workflow.portraits.core",
    `workflow.portraits.${provider}`,
    "workflow.portraits.subagent",
    "workflow.portraits.config",
  ];
}

const FALLBACK_AI_PROFILES: AiProviderProfile[] = [
  { id: "codex", display_name: "Codex", protocol: "codex_app_server", requires_credential: false, optimization_profile: "Codex project and ChatGPT Chat", default_model: "gpt-5.6-luna", default_reasoning_effort: "xhigh" },
  { id: "claude", display_name: "Claude", protocol: "anthropic_messages", requires_credential: true, optimization_profile: "Claude Code / Anthropic conventions", default_model: "claude-sonnet-5", default_endpoint: "https://api.anthropic.com/v1/messages", account_url: "https://platform.claude.com/settings/keys" },
  { id: "kimi", display_name: "Kimi", protocol: "openai_compatible", requires_credential: true, optimization_profile: "Kimi coding conventions", default_model: "kimi-k2.6", default_endpoint: "https://api.moonshot.ai/v1/chat/completions", account_url: "https://platform.kimi.ai/console/api-keys" },
  { id: "glm", display_name: "GLM", protocol: "openai_compatible", requires_credential: true, optimization_profile: "GLM coding conventions", default_model: "glm-5.2", default_endpoint: "https://open.bigmodel.cn/api/paas/v4/chat/completions", account_url: "https://bigmodel.cn/usercenter/proj-mgmt/apikeys" },
  { id: "deepseek", display_name: "DeepSeek", protocol: "openai_compatible", requires_credential: true, optimization_profile: "DeepSeek coding conventions", default_model: "deepseek-v4-flash", default_reasoning_effort: "high", default_endpoint: "https://api.deepseek.com/chat/completions", account_url: "https://platform.deepseek.com/api_keys" },
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

export const initialState: WizardState = {
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
  aiModel: "gpt-5.6-luna",
  aiReasoningEffort: "xhigh",
  aiEndpoint: "",
  aiAccount: null,
  aiProfiles: undefined,
  selectedComponents: DEFAULT_COMPONENTS.filter((component) => component.selected).map((component) => component.id),
  components: DEFAULT_COMPONENTS,
  folderProfile: DEFAULT_GENERATED_IDENTITY.folderProfile,
  meshSelected: false,
  superEventsSelected: false,
  portraitPipeline: DEFAULT_PORTRAIT_PIPELINE,
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
  installedComponentIds: [],
  maintenanceOptionalSelections: [],
  installedWorkflow3dState: "not_selected",
  installedSuperEventsState: "not_selected",
  installedPortraitState: "not_selected",
  installedPortraitProvider: undefined,
  chatSourcesAvailable: false,
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
  "chat-sources": "install",
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
  "chat-sources": { title: "Package ChatGPT project sources", supporting: "Choose detected project files and a download folder.", status: { label: "Export", tone: "info" } },
};

function phaseIndex(screen: ScreenId): number {
  return PHASES.findIndex((phase) => phase.id === SCREEN_PHASE[screen]);
}

const GENERATED_IDENTITY_FIELDS = ["projectId", "scriptPrefix", "primaryNamespace", "descriptorTags", "folderProfile"] as const;

export function detectedChatSourcesAvailable(findings: ScanFinding[]): boolean {
  const agents = findings.find((finding) => finding.id === "codex.agents")?.value.trim().toLowerCase() === "true";
  const count = (id: string) => {
    const finding = findings.find((candidate) => candidate.id === id);
    if (!finding) return 0;
    try {
      const value = JSON.parse(finding.value) as { count?: number };
      return typeof value.count === "number" ? value.count : 0;
    } catch {
      return 0;
    }
  };
  return agents || count("skill.inventory") > 0 || count("subagent.inventory") > 0;
}

function managedInstallationDetails(findings: ScanFinding[]): { present: boolean; valid: boolean; componentIds: string[]; workflow3d: WorkflowState; superEvents: WorkflowState; meshKeyConfigured: boolean; chatSourcesAvailable: boolean; portraitPipeline?: Partial<PortraitPipelineState> & { provider_status?: PortraitProviderStatus } } {
  const detectedSources = detectedChatSourcesAvailable(findings);
  const finding = findings.find((candidate) => candidate.id === "installation.managed");
  if (!finding) return { present: false, valid: false, componentIds: [], workflow3d: "not_selected", superEvents: "not_selected", meshKeyConfigured: false, chatSourcesAvailable: detectedSources };
  try {
    const value = JSON.parse(finding.value) as { present?: boolean; valid?: boolean; component_ids?: string[]; workflow_3d_state?: WorkflowState; workflow_super_events_state?: WorkflowState; workflow_3d_key_configured?: boolean; portrait_provider?: PortraitProviderId; portrait_provider_status?: PortraitProviderStatus; portrait_enabled?: boolean; portrait_workflow_commit?: string; portrait_preferred_workflow?: PortraitPipelineState["preferredWorkflow"]; portrait_mcp_registered?: boolean; portrait_local_root?: string; portrait_local_server_url?: string; portrait_runpod_url?: string; portrait_runpod_workspace?: string };
    const installedComponents = new Set(value.component_ids ?? []);
    return {
      present: value.present === true,
      valid: value.valid === true,
      componentIds: Array.from(installedComponents),
      workflow3d: value.workflow_3d_state ?? "not_selected",
      superEvents: value.workflow_super_events_state ?? "not_selected",
      meshKeyConfigured: value.workflow_3d_key_configured === true,
      chatSourcesAvailable: detectedSources || ["core.agents", "core.skills", "core.subagents"].every((id) => installedComponents.has(id)),
      portraitPipeline: value.portrait_provider ? {
        enabled: value.portrait_enabled !== false,
        provider: value.portrait_provider,
        providerStatus: value.portrait_provider_status ?? "not_selected",
        workflowCommit: value.portrait_workflow_commit ?? PORTRAIT_COMMIT,
        preferredWorkflow: value.portrait_preferred_workflow ?? "source",
        mcpRegistered: value.portrait_mcp_registered === true,
        localComfyuiRoot: value.portrait_local_root ?? "",
        localServerUrl: value.portrait_local_server_url ?? "http://127.0.0.1:8188",
        runpodUrl: value.portrait_runpod_url ?? "",
        runpodWorkspace: value.portrait_runpod_workspace ?? "/workspace/comfyui-hoi4-portraits",
      } : undefined,
    };
  } catch {
    return { present: false, valid: false, componentIds: [], workflow3d: "not_selected", superEvents: "not_selected", meshKeyConfigured: false, chatSourcesAvailable: detectedSources };
  }
}

export function dynamicMaintenanceOptionalComponentIds(state: WizardState): string[] {
  const components = state.manifestPreview?.components ?? [];
  const dependencyOnlyIds = new Set(components
    .filter((component) => component.optional)
    .flatMap((component) => component.dependencies));
  const installedIds = new Set(state.installedComponentIds ?? []);
  const requestedIds = new Set(state.maintenanceOptionalSelections ?? []);
  return components
    .filter((component) => component.optional)
    .filter((component) => !dependencyOnlyIds.has(component.id))
    .filter((component) => requestedIds.has(component.id))
    .filter((component) => !installedIds.has(component.id))
    .filter((component) => component.id !== "workflow.3d")
    .filter((component) => component.id !== "workflow.super_events")
    .filter((component) => !component.id.startsWith("workflow.portraits"))
    .filter((component) => providerSupportsComponent(component, components, state.aiProvider))
    .map((component) => component.id);
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
    case "workflows": return state.meshSelected && meshWorkflowAvailable(state) ? "mesh" : "mcp";
    case "mesh": return "mcp";
    case "mcp": return "git";
    case "git": return "dry-run";
    case "dry-run": return "install";
    case "install": return "ready";
    case "ready": return "ready";
    case "conflict": return "dry-run";
    case "recovery": return "install";
    case "chat-sources": return "update";
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
    case "chat-sources": return "update";
    default: return "welcome";
  }
}

export function maintenanceReviewScreen(plan: Pick<InstallationPlan, "conflicts">): ScreenId {
  return plan.conflicts.some((conflict) => !conflict.selected) ? "conflict" : "dry-run";
}

export default function App() {
  const [state, setState] = useState<WizardState>(() => import.meta.env.DEV && window.__HOI4_DOCUMENTATION_STATE__ ? window.__HOI4_DOCUMENTATION_STATE__ : initialState);
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
  const [semanticProgressStage, setSemanticProgressStage] = useState<"preparing" | "analyzing" | "validating">("preparing");
  const [semanticProgressStartedAt, setSemanticProgressStartedAt] = useState<number>();
  const [semanticProgressNow, setSemanticProgressNow] = useState(Date.now());
  const [planPreparationPending, setPlanPreparationPending] = useState(false);
  const [planPreparationStartedAt, setPlanPreparationStartedAt] = useState<number>();
  const [planPreparationNow, setPlanPreparationNow] = useState(Date.now());
  const [maintenancePending, setMaintenancePending] = useState(false);
  const [installationPending, setInstallationPending] = useState(false);
  const [recoveryPending, setRecoveryPending] = useState(false);
  const [activeTransactionId, setActiveTransactionId] = useState<string>();
  const [appUpdate, setAppUpdate] = useState<AppUpdateStatus | null>(null);
  const [appUpdateState, setAppUpdateState] = useState<"idle" | "installing" | "error">("idle");
  const appUpdateInstallStarted = useRef(false);
  const [chatSourcesPending, setChatSourcesPending] = useState(false);
  const codexAccountReadPending = useRef(false);

  useEffect(() => {
    if (!semanticAnalysisPending) return;
    setSemanticProgressNow(Date.now());
    const timer = window.setInterval(() => setSemanticProgressNow(Date.now()), 1_000);
    return () => window.clearInterval(timer);
  }, [semanticAnalysisPending]);

  useEffect(() => {
    if (!planPreparationPending) return;
    setPlanPreparationNow(Date.now());
    const timer = window.setInterval(() => setPlanPreparationNow(Date.now()), 500);
    return () => window.clearInterval(timer);
  }, [planPreparationPending]);

  const installAvailableAppUpdate = async () => {
    setAppUpdateState("installing");
    try {
      const result = await installAppUpdate();
      if (!result.error) return;
    } catch {
      // Keep the current installation usable and expose the same retry path.
    }
    appUpdateInstallStarted.current = false;
    setAppUpdateState("error");
  };

  useEffect(() => {
    if (!isTauriRuntime()) return;
    let active = true;
    void checkForAppUpdate().then((result) => {
      if (!active || !result.value?.available) return;
      setAppUpdate(result.value);
      if (!appUpdateInstallStarted.current) {
        appUpdateInstallStarted.current = true;
        void installAvailableAppUpdate();
      }
    });
    return () => { active = false; };
  }, []);

  useEffect(() => {
    if (!isTauriRuntime()) return;
    let active = true;
    void readMeshyCredential().then((reference) => {
      if (active && reference) {
        setState((current) => ({
          ...current,
          meshKeyStatus: "present",
          meshCredentialReference: reference,
        }));
      }
    });
    return () => { active = false; };
  }, []);

  const applyAppUpdate = async () => {
    if (appUpdateInstallStarted.current) return;
    appUpdateInstallStarted.current = true;
    await installAvailableAppUpdate();
  };

  useEffect(() => {
    if (state.screen !== "welcome" && state.screen !== "description" && state.screen !== "identity" && state.screen !== "findings") return;
    if (state.aiProvider === "codex") {
      if (state.codexAccount !== null || codexAccountReadPending.current) return;
      codexAccountReadPending.current = true;
      void readCodexAccount().then((account) => {
        if (account) {
          setState((current) => ({ ...current, codexAccount: account }));
        } else if (isTauriRuntime()) {
          setState((current) => ({ ...current, codexAccount: { available: false, authenticated: false, auth_mode: "", usage_limited: false, error: "Codex could not be reached." } }));
        }
      }).finally(() => { codexAccountReadPending.current = false; });
      return;
    }
    if (!state.aiProfiles) {
      void readAiProviderProfiles().then((profiles) => setState((current) => ({ ...current, aiProfiles: profiles.length ? profiles : FALLBACK_AI_PROFILES })));
    }
    if (state.aiAccount !== null) return;
    void readAiAccount(state.aiProvider, state.aiModel, state.aiReasoningEffort, state.aiEndpoint).then((account) => {
      if (account) setState((current) => ({ ...current, aiAccount: account }));
    });
  }, [state.screen, state.codexAccount, state.aiProvider, state.aiModel, state.aiReasoningEffort, state.aiEndpoint, state.aiAccount, state.aiProfiles]);

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
          installedComponentIds: managed.componentIds,
          maintenanceOptionalSelections: [],
          chatSourcesAvailable: managed.chatSourcesAvailable,
          installedWorkflow3dState: managed.workflow3d,
          installedSuperEventsState: managed.superEvents,
          installedPortraitState: managed.portraitPipeline?.enabled && managed.portraitPipeline.provider !== "disabled"
            ? managed.portraitPipeline.providerStatus === "ready" ? "ready" : "incomplete"
            : "not_selected",
          installedPortraitProvider: managed.portraitPipeline?.provider,
          meshSelected: managed.workflow3d !== "not_selected" && managed.workflow3d !== "unsupported_platform",
          superEventsSelected: managed.superEvents !== "not_selected" && managed.superEvents !== "unsupported_platform",
          portraitPipeline: managed.portraitPipeline
            ? { ...current.portraitPipeline, ...managed.portraitPipeline, workflowRepository: current.portraitPipeline.workflowRepository, workflowBranch: current.portraitPipeline.workflowBranch }
            : current.portraitPipeline,
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
    // A new-project destination is often filled automatically before the user
    // has chosen to create anything. Do not treat that suggested path as an
    // existing-project recovery target; the apply boundary still performs the
    // exact transaction check when the user starts installation.
    if (state.mode !== "existing" || !state.identity.projectRoot || installationPending || state.screen === "install" || state.screen === "recovery") return;
    let active = true;
    void findInterruptedTransaction(state.identity.projectRoot).then((journal) => {
      if (!active || !journal) return;
      setState((current) => current.screen === "install" ? current : ({
        ...current,
        transaction: journal,
        recoveryChoice: preferredRecoveryChoice(journal),
        screen: "recovery",
        transactionError: "A previous setup needs one recovery choice before installation can continue.",
      }));
    });
    return () => { active = false; };
  }, [state.identity.projectRoot, state.screen, installationPending]);

  useEffect(() => {
    const transactionId = activeTransactionId ?? state.plan?.plan_id;
    if (!installationPending || !transactionId || !state.identity.projectRoot) return;
    let active = true;
    let timer: number | undefined;
    const refresh = async () => {
      const journal = await readTransactionJournal(state.identity.projectRoot, transactionId);
      if (!active) return;
      if (journal) {
        setState((current) => current.screen !== "install" ? current : ({
          ...current,
          transaction: journal,
          installStage: journal.last_checkpoint,
        }));
      }
      timer = window.setTimeout(() => void refresh(), 500);
    };
    void refresh();
    return () => {
      active = false;
      if (timer !== undefined) window.clearTimeout(timer);
    };
  }, [installationPending, activeTransactionId, state.identity.projectRoot, state.plan?.plan_id]);

  useEffect(() => {
    if (state.screen === "ready" && !state.readiness) {
      void evaluateReadiness(state.identity.projectRoot || "<selected project>", state.identity.projectId, state.meshSelected ? state.meshKeyStatus === "verified" ? "ready" : "incomplete" : "not_selected", state.portraitPipeline).then((result) => {
        if (result) setState((current) => ({ ...current, readiness: result }));
      });
    }
  }, [state.screen, state.readiness, state.identity.projectId, state.identity.projectRoot, state.meshSelected, state.meshKeyStatus, state.portraitPipeline]);

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
      providerAccount = providerAccount ?? await readAiAccount(provider, state.aiModel, state.aiReasoningEffort, state.aiEndpoint);
      if (!providerAccount?.available) {
        if (isTauriRuntime()) update({ aiAccount: providerAccount, transactionError: `${aiProviderLabel(provider, state.aiProfiles)} is not connected. Return to the first step and connect it before planning the setup.` });
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
    setSemanticProgressStage("analyzing");
    const request: CodexAnalysisRequest = {
      mode,
      brief: state.description,
      evidence: mode === "existing_project_semantics" ? evidence : [],
      constraints: {
        project_id_pattern: "^[a-z][a-z0-9_]{1,63}$",
        source: { mode: state.sourceMode, selected_ref: state.sourceMode === "latest" ? undefined : state.pinnedRef },
      },
      analysis_purpose: mode === "existing_project_semantics" ? "existing_project_import" : undefined,
      project_root: mode === "existing_project_semantics" ? state.scanContext?.projectRoot : undefined,
      scan_id: mode === "existing_project_semantics" ? state.scanContext?.scanId : undefined,
    };
    const response = provider === "codex"
      ? await runCodexAnalysisResult(request, state.aiModel, state.aiReasoningEffort)
      : await runAiAnalysisResult({
        ...request,
        provider,
        model: state.aiModel,
        reasoning_effort: state.aiReasoningEffort,
        endpoint: state.aiEndpoint,
      });
    setSemanticProgressStage("validating");
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
      installedComponentIds: managed.componentIds,
      maintenanceOptionalSelections: [],
      chatSourcesAvailable: managed.chatSourcesAvailable,
      installedWorkflow3dState: managed.workflow3d,
      installedPortraitState: managed.portraitPipeline?.enabled && managed.portraitPipeline.provider !== "disabled"
        ? managed.portraitPipeline.providerStatus === "ready" ? "ready" : "incomplete"
        : "not_selected",
      installedPortraitProvider: managed.portraitPipeline?.provider,
      meshSelected: managed.workflow3d !== "not_selected" && managed.workflow3d !== "unsupported_platform",
      portraitPipeline: managed.portraitPipeline
        ? { ...state.portraitPipeline, ...managed.portraitPipeline, workflowRepository: state.portraitPipeline.workflowRepository, workflowBranch: state.portraitPipeline.workflowBranch }
        : state.portraitPipeline,
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
      aiAccount = aiAccount ?? await readAiAccount(provider, state.aiModel, state.aiReasoningEffort, state.aiEndpoint);
      if (!aiAccount || !aiAccount.available) {
        update({ aiAccount, transactionError: `${providerLabel} is not connected for reanalysis. Return to the first step and check the connection.` });
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
        analysis_purpose: "maintenance_reanalysis",
        project_id_pattern: "^[a-z][a-z0-9_]{1,63}$",
      },
      analysis_purpose: "maintenance_reanalysis",
      project_root: state.scanContext.projectRoot,
      scan_id: state.scanContext.scanId,
    };
    const response = provider === "codex"
      ? await runCodexAnalysisResult(request, state.aiModel, state.aiReasoningEffort)
      : await runAiAnalysisResult({
        ...request,
        provider,
        model: state.aiModel,
        reasoning_effort: state.aiReasoningEffort,
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
    const pendingTransaction = state.identity.projectRoot.trim()
      ? await findInterruptedTransaction(state.identity.projectRoot)
      : null;
    if (pendingTransaction) {
      update({
        transaction: pendingTransaction,
        recoveryChoice: preferredRecoveryChoice(pendingTransaction),
        transactionError: "Finish recovering the previous setup before starting another installation.",
        screen: "recovery",
      });
      return;
    }
    if (plan.conflicts.some((conflict) => !conflict.selected)) {
      update({ plan, screen: "conflict", transactionError: undefined });
      return;
    }
    const originScreen = state.screen;
    setInstallationPending(true);
    update({ screen: "install", transaction: undefined, installProgress: 2, installStage: "Starting setup", transactionError: undefined });
    try {
      if (!(await approveInstallation(plan.plan_id))) {
        update({ screen: originScreen, installProgress: 0, transactionError: "Setup could not start. Prepare the changes again." });
        return;
      }
      setActiveTransactionId(plan.plan_id);
      const installation = await applyInstallationResult(plan.plan_id, state.identity.projectRoot);
      const journal = installation.value;
      if (!journal) {
        const interrupted = await findInterruptedTransaction(state.identity.projectRoot);
        if (interrupted) {
          update({
            transaction: interrupted,
            recoveryChoice: preferredRecoveryChoice(interrupted),
            transactionError: "Setup stopped. Choose the available recovery action before trying again.",
            screen: "recovery",
          });
        } else {
          update({ screen: originScreen, installProgress: 0, transactionError: installation.error ? `Installation could not start: ${safeRecoveryFailureMessage(installation.error)}` : "Installation could not start. Prepare the changes again." });
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
    } finally {
      setActiveTransactionId(undefined);
      setInstallationPending(false);
    }
  };
  const prepareSetupPlan = async () => {
    if (planPreparationPending) return;
    const startedAt = Date.now();
    setPlanPreparationStartedAt(startedAt);
    setPlanPreparationNow(startedAt);
    setPlanPreparationPending(true);
    update({ transactionError: undefined });
    try {
      const result = await buildInstallationPlanResult({ ...state, conflictChoice: undefined });
      const plan = result.value;
      if (!plan) {
        update({ transactionError: result.error ?? "The installation plan is unavailable. Nothing was changed." });
        return;
      }
      update({ plan, conflictChoice: undefined, transactionError: undefined, screen: plan.conflicts.some((conflict) => !conflict.selected) ? "conflict" : "dry-run" });
    } catch {
      update({ transactionError: "Preparing the installation stopped unexpectedly. Nothing was changed; try again." });
    } finally {
      setPlanPreparationPending(false);
    }
  };
  const startMaintenance = async (mode: "update" | "repair" | "reinstall" | "remove") => {
    if (maintenancePending) return;
    setMaintenancePending(true);
    update({ transactionError: "Preparing the review…" });
    try {
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
        ...dynamicMaintenanceOptionalComponentIds(state),
        ...(state.meshSelected && state.installedWorkflow3dState === "not_selected" ? ["workflow.3d"] : []),
        ...(state.superEventsSelected && state.installedSuperEventsState === "not_selected" ? ["workflow.super_events"] : []),
        ...(state.portraitPipeline.enabled && state.portraitPipeline.provider !== "disabled" && (state.installedPortraitState === "not_selected" || state.installedPortraitProvider !== state.portraitPipeline.provider)
          ? portraitComponentIdsFor(state.portraitPipeline.provider)
          : []),
      ]
      : [];
    const plan = await buildMaintenancePlan(mode, state.identity.projectRoot, state.maintenanceCodexAnalysisRecord, addOptionalComponents, state.portraitPipeline);
    if (!plan) {
      update({ transactionError: "The maintenance plan is unavailable. Nothing was changed." });
      return;
    }
    update({ plan, maintenanceMode: mode, transactionError: undefined, screen: maintenanceReviewScreen(plan) });
    } finally {
      setMaintenancePending(false);
    }
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
    if (recoveryPending) return;
    const transaction = state.transaction;
    if (!transaction) {
      update({ transactionError: "No interrupted transaction is available for this project." });
      return;
    }
    setRecoveryPending(true);
    update({ transactionError: undefined });
    try {
    const transactionId = transaction.transaction_id;
    if (state.recoveryChoice === "resume") {
      if (!transaction.recovery.resume_allowed) {
        update({ transactionError: "This setup cannot continue safely. Choose Undo changes or inspect the project." });
        return;
      }
      const resumed = await resumeInstallation(state.identity.projectRoot, transactionId);
      if (!resumed) {
        const refreshed = await findInterruptedTransaction(state.identity.projectRoot);
        update(refreshed ? {
          transaction: refreshed,
          recoveryChoice: preferredRecoveryChoice(refreshed),
          transactionError: refreshed.recovery.rollback_allowed
            ? "Some files had already changed, so this setup must be undone before trying again."
            : "Setup could not continue safely. Choose another available recovery action.",
        } : { transactionError: "Setup could not continue safely. Review the project or choose Undo changes." });
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
      const result = await rollbackInstallationResult(state.identity.projectRoot, transactionId);
      if (!result.value) {
        const refreshed = await findInterruptedTransaction(state.identity.projectRoot);
        update({
          ...(refreshed ? { transaction: refreshed, recoveryChoice: preferredRecoveryChoice(refreshed) } : {}),
          transactionError: result.error
            ? `Undo could not continue: ${safeRecoveryFailureMessage(result.error)}`
            : "Undo changes was refused because the project needs review.",
        });
        return;
      }
      update(state.plan ? {
        transaction: result.value,
        plan: undefined,
        transactionError: undefined,
        readiness: null,
        installProgress: 0,
        screen: "dry-run",
      } : {
        transaction: result.value,
        transactionError: "The partial setup was undone. You can start again.",
        readiness: null,
        recoveryEntry: false,
        screen: "welcome",
      });
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
    } finally {
      setRecoveryPending(false);
    }
  };
  const goNext = async () => {
    if (state.screen === "chat-sources") {
      await packageChatSourcesNow();
      return;
    }
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
      setSemanticProgressStage("preparing");
      setSemanticProgressStartedAt(Date.now());
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
    if (state.screen === "components" && isTauriRuntime() && !state.codexAnalysisRecord) {
      update({
        screen: state.mode === "existing" ? "findings" : "description",
        transactionError: "The source version changed. Run and confirm analysis for that exact source before choosing components.",
      });
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
    if (state.screen === "ready") {
      update({ finished: true, transactionError: undefined });
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
  const openMaintenance = async (screen: "update" | "conflict" | "recovery") => {
    const transaction = screen === "recovery" && state.identity.projectRoot.trim()
      ? await findInterruptedTransaction(state.identity.projectRoot)
      : undefined;
    update({
      screen,
      transaction: screen === "recovery" ? transaction ?? undefined : state.transaction,
      recoveryChoice: transaction ? preferredRecoveryChoice(transaction) : state.recoveryChoice,
      plan: undefined,
      maintenanceMode: undefined,
      maintenanceCodexAnalysisRecord: undefined,
      maintenanceEvidenceReady: undefined,
      transactionError: screen === "recovery" && !transaction ? "There is no interrupted setup for this project." : undefined,
    });
  };

  const openChatSources = async () => {
    if (chatSourcesPending) return;
    if (!state.identity.projectRoot.trim()) {
      update({ transactionError: "Choose the project folder before packaging ChatGPT sources." });
      return;
    }
    setChatSourcesPending(true);
    const result = await previewChatSources(state.identity.projectRoot);
    setChatSourcesPending(false);
    if (!result.value) {
      update({ transactionError: result.error ?? "The ChatGPT source list could not be prepared. Nothing was changed." });
      return;
    }
    const preview: ChatSourcesPreview = result.value;
    update({
      screen: "chat-sources",
      chatSourcesPreview: preview,
      chatSourcesDestination: preview.destinationDirectory,
      chatSourcesSelectedIds: preview.files.filter((file) => file.selectedByDefault).map((file) => file.id),
      chatSourcesResult: undefined,
      transactionError: undefined,
    });
  };

  const chooseChatSourcesFolder = async () => {
    const selected = await pickChatSourcesFolder();
    if (selected?.path) {
      update({ chatSourcesDestination: selected.path, chatSourcesResult: undefined, transactionError: undefined });
    } else if (selected?.error) {
      update({ transactionError: selected.error });
    }
  };

  const packageChatSourcesNow = async () => {
    if (chatSourcesPending) return;
    const preview = state.chatSourcesPreview;
    const destination = state.chatSourcesDestination?.trim() ?? "";
    const selected = state.chatSourcesSelectedIds ?? [];
    if (!preview?.eligible || !destination || selected.length === 0) {
      update({ transactionError: "Choose an accessible download folder and keep the required source files selected." });
      return;
    }
    setChatSourcesPending(true);
    const result = await packageChatSources(state.identity.projectRoot, destination, selected);
    setChatSourcesPending(false);
    if (!result.value) {
      update({ transactionError: result.error ?? "The ChatGPT source package could not be created. Nothing in the project was changed." });
      return;
    }
    update({ chatSourcesResult: result.value, transactionError: undefined });
  };

  const recoveryStage = state.screen === "recovery" && state.transaction
    ? recoveryStagePosition(state.transaction)
    : undefined;
  const copy = state.screen === "ready" && state.finished
    ? {
      title: "Congratulations, you are all set!",
      supporting: `${state.identity.displayName || "Your mod"} is ready for agentic development.`,
      status: { label: "Complete", tone: "pass" as const },
    }
    : state.screen === "ready"
    ? {
      title: "Project ready",
      supporting: state.readiness ? state.readiness.coreReady ? `Core requirements passed for ${aiProviderLabel(state.aiProvider, state.aiProfiles)}.` : "Resolve blocking checks before continuing." : "Checking core requirements.",
      status: state.readiness ? state.readiness.coreReady ? { label: `Ready for ${aiProviderLabel(state.aiProvider, state.aiProfiles)}`, tone: "pass" as const } : { label: "Needs review", tone: "block" as const } : { label: "Checking readiness", tone: "info" as const },
    }
    : state.screen === "recovery" && state.transaction
      ? {
        title: "Installation was interrupted",
        supporting: state.transaction.recovery.project_apply_started
          ? "Undo the partial setup before installing again."
          : state.transaction.recovery.resume_allowed
            ? "Continue from the last safe checkpoint."
            : "Clear the prepared files before installing again.",
        status: recoveryStage
          ? {
            label: `Stage ${recoveryStage.index + 1} of ${recoveryStage.total}`,
            tone: "review" as const,
          }
          : undefined,
      }
      : screenCopy[state.screen];
  const appUpdateMessage = appUpdateState === "error"
    ? "Update failed. Try again."
    : appUpdateState === "installing"
      ? `Installing version ${appUpdate?.availableVersion}...`
      : `Version ${appUpdate?.availableVersion} is available`;
  return (
    <div className="app-shell">
      <header className="titlebar">
        <img className="brand-mark" src={appIcon} alt="" aria-hidden="true" />
        <span className="brand-name">HOI4 Mod Setup</span>
        {appUpdate && <div className="app-update" role="status" aria-live="polite">
          <span className="app-update-message">{appUpdateMessage}</span>
          {appUpdateState === "error" && <button className="button update-button" type="button" onClick={() => void applyAppUpdate()}>
            Retry update
          </button>}
        </div>}
      </header>
      <div className="workspace">
        <PhaseRail screen={state.screen} />
        <main className="main-viewport" aria-labelledby="screen-title" aria-describedby="screen-supporting" onKeyDown={closeDisclosureOnEscape}>
          <div className="visually-hidden" role="status" aria-live="polite" aria-atomic="true">{copy.title}</div>
          <ScreenFrame screen={state.screen} copy={copy} state={state} canAdvance={!semanticAnalysisPending && !planPreparationPending && !installationPending && !recoveryPending && !chatSourcesPending && canAdvanceFromScreen(state, { scanComplete, scanError, scanPartial, findings })} pending={semanticAnalysisPending || recoveryPending || chatSourcesPending} chatSourcesPending={chatSourcesPending} semanticProgressStage={semanticProgressStage} semanticProgressStartedAt={semanticProgressStartedAt} semanticProgressNow={semanticProgressNow} preparingPlan={planPreparationPending} planPreparationStartedAt={planPreparationStartedAt} planPreparationNow={planPreparationNow} headingRef={headingRef} onBack={goBack} onNext={goNext} onMaintenance={openMaintenance} onPrepareConflicts={prepareSetupPlan}>
            {renderScreen(state, update, updateIdentity, updateDescription, findings, selectedFinding, setSelectedFinding, setFindings, scanComplete, scanError, scanProgress, scanPartial, scanLimitsHit, scanRequestId, scanCancellationRequested, cancelActiveScan, openMaintenance, startMaintenance, runMaintenanceReanalysis, chooseConflict, chooseProjectFolder, chooseLauncherFolder, chooseChatSourcesFolder, confirmAnalysis, recoveryPending, maintenancePending, openChatSources)}
          </ScreenFrame>
        </main>
      </div>
    </div>
  );
}

function PhaseRail({ screen }: { screen: ScreenId }) {
  const maintenance = screen === "update" || screen === "conflict" || screen === "recovery" || screen === "chat-sources";
  const currentMaintenance = maintenance ? screen === "update" || screen === "chat-sources" ? 1 : screen === "conflict" ? 2 : 3 : -1;
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
    <ExternalLink className="rail-repo" href="https://github.com/klimPaskov/HOI4-Mod-Setup">HOI4 Mod Setup <span aria-hidden="true">↗</span></ExternalLink>
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

function ScreenFrame({ screen, copy, state, canAdvance, pending, chatSourcesPending, semanticProgressStage, semanticProgressStartedAt, semanticProgressNow, preparingPlan, planPreparationStartedAt, planPreparationNow, headingRef, onBack, onNext, onMaintenance, onPrepareConflicts, children }: { screen: ScreenId; copy: { title: string; supporting?: string; status?: { label: string; tone: StatusTone } }; state: WizardState; canAdvance: boolean; pending: boolean; chatSourcesPending: boolean; semanticProgressStage: "preparing" | "analyzing" | "validating"; semanticProgressStartedAt?: number; semanticProgressNow: number; preparingPlan: boolean; planPreparationStartedAt?: number; planPreparationNow: number; headingRef: { current: HTMLHeadingElement | null }; onBack: () => void; onNext: () => void; onMaintenance: (screen: "update" | "conflict" | "recovery") => void; onPrepareConflicts: () => Promise<void>; children: ReactNode }) {
  const installDone = screen === "install" && state.installProgress >= 100;
  const unresolvedConflicts = state.plan?.conflicts.some((conflict) => !conflict.selected) === true;
  const recoveryLabel = state.recoveryChoice === "rollback" ? "Undo changes" : state.recoveryChoice === "discard" ? "Discard prepared files" : "Continue setup";
  const primaryLabel = pending && screen === "description" ? "Preparing details…" : pending && screen === "recovery" ? state.recoveryChoice === "rollback" ? "Undoing changes…" : state.recoveryChoice === "discard" ? "Discarding files…" : "Continuing setup…" : screen === "welcome" ? "Continue" : screen === "findings" && !state.codexAnalysis ? `Review with ${aiProviderLabel(state.aiProvider, state.aiProfiles)}` : screen === "dry-run" ? "Start installation" : screen === "install" ? (installDone ? "Continue" : "") : screen === "ready" ? state.finished ? "" : "Finish" : screen === "recovery" ? recoveryLabel : screen === "conflict" ? "Apply" : screen === "update" ? (state.plan ? "Apply reviewed plan" : "") : "Next";
  const displayedPrimaryLabel = screen === "chat-sources" ? (chatSourcesPending ? "Packaging..." : "Package sources") : primaryLabel;
  const showBack = !["welcome", "install"].includes(screen) && !(screen === "ready" && state.finished);
  return <>
    <div className="content-scroll">
      <div className="screen-heading">
        <div><div className="eyebrow">{(screen === "update" || screen === "chat-sources" ? "Update" : screen === "conflict" ? "Conflicts" : screen === "recovery" ? "Recovery" : SCREEN_PHASE[screen]).toUpperCase()}</div><h1 id="screen-title" ref={headingRef} tabIndex={-1}>{copy.title}</h1>{copy.supporting && <p id="screen-supporting">{copy.supporting}</p>}</div>
        {copy.status && <Status label={copy.status.label} tone={copy.status.tone} />}
      </div>
      {pending && screen === "description" && <SemanticPlanningProgress stage={semanticProgressStage} startedAt={semanticProgressStartedAt} now={semanticProgressNow} />}
      {preparingPlan && screen === "dry-run" && <PlanPreparationProgress startedAt={planPreparationStartedAt} now={planPreparationNow} />}
      {children}
    </div>
    <footer className="footer-bar">
      <span className="footer-note" role={state.transactionError ? "alert" : undefined}>{footerNote(screen, state)}</span>
      <div className="footer-actions">
        {screen === "ready" && !state.finished && <button className="button secondary" onClick={() => onMaintenance("update")}>Update and repair</button>}
        {screen === "dry-run" && (!state.plan || unresolvedConflicts) && <button className="button secondary" onClick={() => void onPrepareConflicts()} disabled={preparingPlan} aria-busy={preparingPlan || undefined}>{preparingPlan ? "Preparing changes…" : state.plan ? "Resolve conflicts" : "Prepare changes"}</button>}
        {showBack && <button className="button secondary" onClick={onBack}>Back</button>}
        {displayedPrimaryLabel && <button className="button primary" onClick={onNext} disabled={!canAdvance} aria-busy={pending || undefined}>{displayedPrimaryLabel}</button>}
      </div>
    </footer>
  </>;
}

const SEMANTIC_PROGRESS_COPY = {
  preparing: "Preparing your description",
  analyzing: "Generating project details",
  validating: "Checking the generated details",
} as const;

export function estimateSemanticPlanningProgress(stage: keyof typeof SEMANTIC_PROGRESS_COPY, startedAt = Date.now(), now = Date.now()): { percent: number; remaining: string } {
  const elapsedSeconds = Math.max(0, (now - startedAt) / 1_000);
  const bounds = stage === "preparing" ? { minimum: 5, maximum: 18 }
    : stage === "analyzing" ? { minimum: 20, maximum: 88 }
      : { minimum: 90, maximum: 98 };
  const estimatedTotalSeconds = 90;
  const timedProgress = bounds.minimum + (elapsedSeconds / estimatedTotalSeconds) * (bounds.maximum - bounds.minimum);
  const percent = Math.min(bounds.maximum, Math.max(bounds.minimum, Math.floor(timedProgress)));
  const remainingSeconds = Math.max(1, Math.ceil(estimatedTotalSeconds - elapsedSeconds));
  const remaining = remainingSeconds <= 10
    ? "Less than 10 seconds remaining"
    : remainingSeconds < 60
      ? `About ${Math.ceil(remainingSeconds / 10) * 10} seconds remaining`
      : `About ${Math.ceil(remainingSeconds / 60)} minutes remaining`;
  return { percent, remaining };
}

function SemanticPlanningProgress({ stage, startedAt, now }: { stage: keyof typeof SEMANTIC_PROGRESS_COPY; startedAt?: number; now: number }) {
  const label = SEMANTIC_PROGRESS_COPY[stage];
  const progress = estimateSemanticPlanningProgress(stage, startedAt, now);
  return <section className="panel semantic-planning-progress" role="status" aria-live="polite" aria-busy="true">
    <div><strong>Preparing your mod details</strong><span>{label}</span></div>
    <Progress value={progress.percent} label="Mod detail preparation progress" valueText={`${progress.percent}% complete. ${label}. Estimated time: ${progress.remaining}`} />
    <div className="semantic-progress-meta" aria-hidden="true"><span>{progress.percent}%</span><span>Estimated time: {progress.remaining}</span></div>
  </section>;
}

export function estimatePlanPreparationProgress(startedAt = Date.now(), now = Date.now()): { percent: number; remaining: string } {
  const elapsedSeconds = Math.max(0, (now - startedAt) / 1_000);
  const estimatedTotalSeconds = 30;
  const percent = Math.min(96, Math.max(4, Math.floor(4 + (elapsedSeconds / estimatedTotalSeconds) * 92)));
  const remainingSeconds = Math.max(1, Math.ceil(estimatedTotalSeconds - elapsedSeconds));
  return {
    percent,
    remaining: remainingSeconds < 60 ? `About ${remainingSeconds} seconds remaining` : `About ${Math.ceil(remainingSeconds / 60)} minutes remaining`,
  };
}

function PlanPreparationProgress({ startedAt, now }: { startedAt?: number; now: number }) {
  const progress = estimatePlanPreparationProgress(startedAt, now);
  return <section className="panel plan-preparation" role="status" aria-label="Plan preparation status" aria-live="polite" aria-busy="true">
    <div><strong>Preparing changes</strong><span>Checking the selected files and building the review.</span></div>
    <Progress value={progress.percent} label="Preparing changes" valueText={`${progress.percent}% complete. Estimated time: ${progress.remaining}`} />
    <div className="plan-preparation-meta" aria-hidden="true"><span>{progress.percent}%</span><span>Estimated time: {progress.remaining}</span></div>
  </section>;
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
  if (screen === "install") return state.installProgress >= 100 ? "Setup saved. Readiness is next." : "Installing selected files…";
  if (screen === "ready") return state.transactionError ?? "Readiness checks saved.";
  if (screen === "update") return state.transactionError ?? "User-modified files are never overwritten silently.";
  if (screen === "conflict") return "A preview and validation run follow the selected resolution.";
  if (screen === "recovery") return "Recovery actions are reversible until apply begins.";
  if (screen === "chat-sources") return "Detected instructions, README, skills, and subagents are included by default; root Markdown files are optional.";
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

function preferredRecoveryChoice(transaction: NonNullable<WizardState["transaction"]>): RecoveryChoice {
  if (transaction.recovery.recommended_action === "resume" && transaction.recovery.resume_allowed) return "resume";
  if (transaction.recovery.recommended_action === "rollback" && transaction.recovery.rollback_allowed) return "rollback";
  if (transaction.recovery.recommended_action === "discard_staging" && transaction.recovery.discard_staging_allowed) return "discard";
  if (transaction.recovery.rollback_allowed) return "rollback";
  if (transaction.recovery.resume_allowed) return "resume";
  return "discard";
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
    case "chat-sources":
      return Boolean(state.chatSourcesPreview?.eligible && state.chatSourcesDestination?.trim() && state.chatSourcesSelectedIds?.length);
    case "conflict":
      return Boolean(state.plan) && !state.plan?.conflicts.some((conflict) => !conflict.selected);
    case "update":
      return Boolean(state.plan);
    default:
      return true;
  }
}

function renderScreen(state: WizardState, update: (patch: Partial<WizardState>) => void, updateIdentity: (patch: Partial<ProjectIdentity>) => void, updateDescription: (description: string) => void, findings: ScanFinding[], selectedFinding: string, setSelectedFinding: (id: string) => void, setFindings: Dispatch<SetStateAction<ScanFinding[]>>, scanComplete: boolean, scanError: string | undefined, scanProgress: ScanProgress, scanPartial: boolean, scanLimitsHit: string[], scanRequestId: string | undefined, scanCancellationRequested: boolean, onCancelScan: () => Promise<void>, onMaintenance: (screen: "update" | "conflict" | "recovery") => void, startMaintenance: (mode: "update" | "repair" | "reinstall" | "remove") => void, onReanalyze: () => Promise<boolean>, chooseConflict: (choice: ConflictChoice) => void, onPickProjectFolder: () => Promise<FolderSelection | null>, onPickLauncherFolder: () => Promise<FolderSelection | null>, onPickChatSourcesFolder: () => Promise<void>, onConfirmAnalysis: () => Promise<void>, recoveryPending: boolean, maintenancePending: boolean, onPackageChatSources: () => Promise<void>) {
  switch (state.screen) {
    case "welcome": return <Welcome state={state} update={update} />;
    case "description": return <Description state={state} updateDescription={updateDescription} updateIdentity={updateIdentity} />;
    case "identity": return <Identity state={state} update={update} updateIdentity={updateIdentity} onPickProjectFolder={onPickProjectFolder} onPickLauncherFolder={onPickLauncherFolder} onConfirmAnalysis={onConfirmAnalysis} />;
    case "scan": return <Scan state={state} complete={scanComplete} error={scanError} progress={scanProgress} partial={scanPartial} limitsHit={scanLimitsHit} canCancel={Boolean(scanRequestId)} cancellationRequested={scanCancellationRequested} onCancel={onCancelScan} />;
    case "findings": return <Findings state={state} findings={findings} selected={selectedFinding} setSelected={setSelectedFinding} setFindings={setFindings} onConfirmAnalysis={onConfirmAnalysis} onManageExisting={() => onMaintenance("update")} onPackageChatSources={onPackageChatSources} />;
    case "components": return <Components state={state} update={update} />;
    case "workflows": return <Workflows state={state} update={update} />;
    case "mesh": return <Mesh state={state} update={update} />;
    case "mcp": return <Mcp state={state} />;
    case "git": return <Git state={state} update={update} />;
    case "dry-run": return <DryRun state={state} update={update} />;
    case "install": return <Install state={state} />;
    case "ready": return <Ready state={state} update={update} onMaintenance={onMaintenance} />;
    case "update": return <Update state={state} update={update} findings={findings} setFindings={setFindings} onMaintenance={onMaintenance} onStartMaintenance={startMaintenance} onReanalyze={onReanalyze} onPackageChatSources={onPackageChatSources} pending={maintenancePending} />;
    case "conflict": return <Conflict state={state} update={update} onChoice={chooseConflict} />;
    case "recovery": return <Recovery state={state} update={update} onPickProjectFolder={onPickProjectFolder} onStartMaintenance={startMaintenance} pending={recoveryPending} />;
    case "chat-sources": return <ChatSources state={state} update={update} onPickFolder={onPickChatSourcesFolder} />;
  }
}

export function Welcome({ state, update }: { state: WizardState; update: (patch: Partial<WizardState>) => void }) {
  const [aiKeyDraft, setAiKeyDraft] = useState("");
  const [aiModels, setAiModels] = useState<AiModelOption[]>([]);
  const [modelListStatus, setModelListStatus] = useState<"idle" | "loading" | "unavailable">("idle");
  const activeCodexLoginId = useRef<string | undefined>(undefined);
  const selectedProvider = state.aiProvider ?? "codex";
  const desktopRuntime = isTauriRuntime() || Boolean(import.meta.env.DEV && window.__HOI4_DOCUMENTATION_STATE__);
  const profiles = state.aiProfiles?.length ? state.aiProfiles : FALLBACK_AI_PROFILES;
  const profile = profiles.find((candidate) => candidate.id === selectedProvider) ?? profiles[0];
  const selectedLabel = aiProviderLabel(selectedProvider, profiles);
  const providerNeedsManualDetails = selectedProvider === "local" || selectedProvider === "custom";
  useEffect(() => {
    const canLoad = selectedProvider === "codex" || state.aiAccount?.authenticated || (selectedProvider === "local" && Boolean(state.aiEndpoint.trim()));
    if (!desktopRuntime || !canLoad) { setAiModels([]); setModelListStatus("idle"); return; }
    let active = true;
    setModelListStatus("loading");
    void readAiModels(selectedProvider, state.aiEndpoint).then((models) => {
      if (!active) return;
      setAiModels(models);
      setModelListStatus(models.length ? "idle" : "unavailable");
      const selected = models.find((model) => model.id === state.aiModel);
      if (selected && !selected.supported_reasoning_efforts.includes(state.aiReasoningEffort)) {
        update({ aiReasoningEffort: selected.default_reasoning_effort });
      }
    }).catch(() => { if (active) { setAiModels([]); setModelListStatus("unavailable"); } });
    return () => { active = false; };
  }, [desktopRuntime, selectedProvider, state.aiEndpoint, state.aiAccount?.authenticated]);
  const selectedModel = aiModels.find((model) => model.id === state.aiModel);
  const reasoningEfforts = selectedModel?.supported_reasoning_efforts
    ?? (selectedProvider === "codex" || selectedProvider === "deepseek" || selectedProvider === "claude"
      ? (["low", "medium", "high", "xhigh", "max"] as const)
      : (["high"] as const));
  const effortLabel = (effort: string) => ({ low: "Light", medium: "Medium", high: "High", xhigh: "Extra high", max: "Max" }[effort] ?? effort);
  const selectProvider = (provider: AiProviderId) => {
    const selectedProfile = profiles.find((candidate) => candidate.id === provider);
    update({
      aiProvider: provider,
      aiModel: selectedProfile?.default_model ?? "",
      aiReasoningEffort: selectedProfile?.default_reasoning_effort ?? (provider === "codex" ? "xhigh" : "high"),
      aiEndpoint: selectedProfile?.default_endpoint ?? "",
      aiAccount: null,
      codexAnalysis: undefined,
      codexAnalysisRecord: undefined,
      flattenForChat: provider === "codex" ? state.flattenForChat : false,
      meshSelected: state.meshSelected,
      selectedComponents: provider === "codex"
        ? Array.from(new Set([...state.selectedComponents, "codex.config"]))
        : state.selectedComponents.filter((id) => id !== "codex.config"),
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
    const next = await readAiAccount(selectedProvider, state.aiModel, state.aiReasoningEffort, state.aiEndpoint);
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
  const openProviderAccount = async () => {
    if (!profile?.account_url) return;
    const result = await openExternalUrlResult(profile.account_url);
    update({ transactionError: result.error });
  };
  const providerStatus = state.aiAccount?.authenticated
    ? `${selectedLabel} is connected.`
    : state.aiAccount?.error && !state.aiAccount.error.toLowerCase().includes("credential is not available")
      ? state.aiAccount.error
      : providerNeedsManualDetails
        ? `Enter the ${selectedLabel} details and connect.`
        : `Paste your ${selectedLabel} API key to connect.`;
  const chooseMode = (mode: "new" | "existing") => {
    if (state.mode === mode) return;
    update({
      mode,
      recoveryEntry: false,
      identity: { ...DEFAULT_IDENTITY },
      transaction: undefined,
      transactionError: undefined,
      projectPathsOverridden: false,
      projectPathStatus: mode === "new" ? "resolving" : "manual",
    });
  };
  return <div className="stack wide welcome-screen"><div className="choice-grid">
    <button type="button" className={`choice-card ${state.mode === "new" ? "selected" : ""}`} aria-pressed={state.mode === "new"} onClick={() => chooseMode("new")}>
      <ChoiceIcon kind="plus" /><span className="choice-radio" aria-hidden="true" /><h2>Create new mod</h2><p>Start from a short description.</p>
    </button>
    <button type="button" className={`choice-card ${state.mode === "existing" ? "selected" : ""}`} aria-pressed={state.mode === "existing"} onClick={() => chooseMode("existing")}>
      <ChoiceIcon kind="search" /><span className="choice-radio" aria-hidden="true" /><h2>Import existing mod</h2><p>Scan the project without changing it.</p>
    </button>
  </div><section><div className="section-label">Planning provider</div><div className="panel recent-list provider-panel"><label className="field"><span className="field-label">AI provider</span><select className="text-input" value={state.aiProvider} onChange={(event) => selectProvider(event.target.value as AiProviderId)}>{profiles.map((candidate) => <option key={candidate.id} value={candidate.id}>{candidate.display_name}</option>)}</select></label><p className="muted provider-help">Codex is the default. The project follows the provider you choose.</p><div className="provider-manual-details"><label className="field"><span className="field-label">Model</span>{aiModels.length ? <select className="text-input" value={state.aiModel} onChange={(event) => { const model = aiModels.find((candidate) => candidate.id === event.target.value); update({ aiModel: event.target.value, aiReasoningEffort: model?.default_reasoning_effort ?? state.aiReasoningEffort, aiAccount: null, codexAnalysis: undefined, codexAnalysisRecord: undefined }); }}>{aiModels.map((model) => <option key={model.id} value={model.id}>{model.display_name}</option>)}</select> : <input className="text-input mono" value={state.aiModel} onChange={(event) => update({ aiModel: event.target.value, aiAccount: null, codexAnalysis: undefined, codexAnalysisRecord: undefined })} placeholder={modelListStatus === "loading" ? "Loading models…" : "Model name"} />}</label><label className="field"><span className="field-label">Reasoning effort</span><select className="text-input" value={state.aiReasoningEffort} onChange={(event) => update({ aiReasoningEffort: event.target.value as WizardState["aiReasoningEffort"], aiAccount: null, codexAnalysis: undefined, codexAnalysisRecord: undefined })}>{reasoningEfforts.map((effort) => <option key={effort} value={effort}>{effortLabel(effort)}</option>)}</select></label></div>{modelListStatus === "unavailable" && <p className="muted provider-help">The live model catalog is unavailable; the verified default remains editable.</p>}{state.aiProvider !== "codex" && <>
    {profile?.account_url && <div className="provider-connect-intro"><span>Use an API key from your {selectedLabel} account.</span>{desktopRuntime ? <button type="button" className="text-button" onClick={() => void openProviderAccount()}>Get {selectedLabel} API key</button> : <a href={profile.account_url} target="_blank" rel="noreferrer">Get {selectedLabel} API key</a>}</div>}
    {profile?.requires_credential && <label className="field"><span className="field-label">{selectedLabel} API key</span><input className="text-input" type="password" value={aiKeyDraft} onChange={(event) => setAiKeyDraft(event.target.value)} autoComplete="off" /></label>}
    {providerNeedsManualDetails ? <div className="provider-manual-details"><Field label={state.aiProvider === "local" ? "Local model address" : "Provider address"} value={state.aiEndpoint} onChange={(value) => update({ aiEndpoint: value, aiAccount: null, codexAnalysis: undefined, codexAnalysisRecord: undefined })} placeholder={state.aiProvider === "local" ? "http://127.0.0.1:…" : "https://…"} mono /></div> : <details><summary>Advanced</summary><div className="provider-advanced"><Field label="Provider address" value={state.aiEndpoint} onChange={(value) => update({ aiEndpoint: value, aiAccount: null, codexAnalysis: undefined, codexAnalysisRecord: undefined })} placeholder="Filled automatically" mono /></div></details>}
    {state.aiProvider === "local" && <p className="muted provider-help">Enter the address shown by your local model app. No online account is needed.</p>}
    <div className="button-row"><button type="button" className="button secondary" onClick={() => void connectProvider()} disabled={!state.aiModel.trim() || !state.aiEndpoint.trim() || (profile?.requires_credential === true && !aiKeyDraft.trim() && !state.aiAccount?.authenticated)}>{state.aiAccount?.authenticated ? `Check ${selectedLabel}` : `Connect ${selectedLabel}`}</button>{state.aiAccount?.authenticated && profile?.requires_credential && <button type="button" className="text-button" onClick={() => void removeProviderCredential()}>Disconnect</button>}</div>
    {state.aiAccount && <p className="callout" role="status">{providerStatus}</p>}
  </>}</div></section>{state.aiProvider === "codex" && <section><div className="section-label">Codex access</div><div className="panel recent-list">
    {!desktopRuntime && <p className="muted">Open the installed desktop app to sign in with ChatGPT.</p>}
    {desktopRuntime && !account && <p className="muted" role="status">Checking Codex…</p>}
    {desktopRuntime && account && account.available && account.authenticated && account.auth_mode === "chatgpt" && <><p><strong>Signed in with ChatGPT</strong>{account.email ? ` · ${account.email}` : ""}</p>{account.usage_limited && <p className="callout review" role="status">Codex usage is currently limited. Planning is paused until usage is available again; recovery remains available.</p>}<button type="button" className="text-button" onClick={() => void refresh()}>Refresh account status</button><button type="button" className="text-button" onClick={() => void signOut()}>Sign out</button></>}
    {desktopRuntime && account && (!account.available || !account.authenticated || account.auth_mode !== "chatgpt") && <><p className="muted">Create, Import, Update, and Repair use your ChatGPT Codex access. No API key is requested.</p>{account.error && <p className="callout review" role="status">{account.error}</p>}<div className="button-row"><button type="button" className="button secondary" onClick={() => void signIn("browser")} disabled={state.codexLoginPending}>{state.codexLoginPending ? "Opening sign-in…" : "Sign in with ChatGPT"}</button><button type="button" className="text-button" onClick={() => void signIn("device")} disabled={state.codexLoginPending}>Use device code</button>{state.codexLoginPending && <button type="button" className="text-button" onClick={() => void cancel()}>Cancel sign-in</button>}</div>{state.codexLogin?.auth_url && <p><button type="button" className="text-button" onClick={() => void openLoginUrl(state.codexLogin?.auth_url ?? "")}>Open the ChatGPT sign-in page</button></p>}{state.codexLogin?.verification_url && <p className="muted"><button type="button" className="text-button" onClick={() => void openLoginUrl(state.codexLogin?.verification_url ?? "")}>Open the device-code page</button> and enter <strong>{state.codexLogin.user_code}</strong>.</p>}<button type="button" className="text-button" onClick={() => void refresh()}>Check again</button></>}
  </div></section>}
  <section><div className="section-label">Already have a project?</div><div className="panel recent-list"><p className="muted">Inspect its files, package ChatGPT sources, or manage setup if this app has installed it.</p><button type="button" className="text-button" onClick={() => update({ screen: "identity", mode: "existing", recoveryEntry: false, identity: { ...DEFAULT_IDENTITY }, transaction: undefined, transactionError: undefined })}>Manage an existing project</button></div></section></div>;
}

function Description({ state, updateDescription, updateIdentity }: { state: WizardState; updateDescription: (description: string) => void; updateIdentity: (patch: Partial<ProjectIdentity>) => void }) {
  return <div className="stack narrow"><section className="panel form-panel"><Field label="Mod name" value={state.identity.displayName} onChange={(value) => updateIdentity({ displayName: value })} /><label className="field"><span className="field-label">Description</span><textarea className="brief-input" aria-label="Mod description" value={state.description} onChange={(event) => updateDescription(event.target.value)} /></label><p className="muted">The project ID, script prefix, namespace, tags, and initial folders are filled from these two fields. You can edit any generated value on the next screen.</p></section><details><summary>{aiProviderLabel(state.aiProvider, state.aiProfiles)} input preview</summary><p className="muted">Your brief and setup preferences are used to prepare editable project details. No files are changed.</p></details><div className="chips"><span>Natural-language brief</span><span>Generated identity</span><span>Editable structure</span></div></div>;
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
  return <section className="panel proposal-review" aria-label={`${providerLabel} proposal review`}><div className="list-row"><div><strong>{confirmed ? `${providerLabel} suggestions confirmed` : `Suggested by ${providerLabel}`}</strong><span>{analysis.project_summary}</span></div><Status label={confirmed ? "Confirmed" : "Review required"} tone={confirmed ? "pass" : "review"} /></div><details open={!confirmed}><summary>{analysis.proposals.length} suggested values</summary><div className="manifest-details proposal-list">{analysis.proposals.map((proposal) => <div className="proposal-row" key={proposal.key}><div className="proposal-heading"><strong>{PROPOSAL_LABELS[proposal.key] ?? proposal.key.replaceAll("_", " ")}</strong><span>{Math.round(proposal.confidence * 100)}% confidence</span></div><span className="proposal-value">{formatProposalValue(proposal.value)}</span><small>{proposal.reason}</small></div>)}</div></details>{analysis.warnings.length > 0 && <details className="analysis-notes"><summary>Before you continue</summary><ul>{analysis.warnings.map((warning) => <li key={warning}>{warning}</li>)}</ul></details>}{!confirmed && <div className="proposal-actions"><button type="button" className="button primary" onClick={() => void onConfirmAnalysis()}>Confirm {providerLabel} suggestions</button></div>}</section>;
}

function RecoveryProjectPicker({ state, updateIdentity, onPickProjectFolder }: { state: WizardState; updateIdentity: (patch: Partial<ProjectIdentity>) => void; onPickProjectFolder: () => Promise<FolderSelection | null> }) {
  const [message, setMessage] = useState<string>();
  const [pendingSelection, setPendingSelection] = useState<FolderSelection>();
  const choose = async () => {
    setMessage(undefined);
    setPendingSelection(undefined);
    const selected = await onPickProjectFolder();
    if (selected?.path && selected.error) {
      setPendingSelection(selected);
      setMessage(`Launcher discovery needs review: ${selected.error}`);
    } else if (selected?.path) updateIdentity({
        projectRoot: selected.path,
        launcherDescriptorPath: selected.launcher_descriptor_path ?? undefined,
      });
    else if (selected?.error) setMessage(`The selected folder could not be used: ${selected.error}`);
    else setMessage("No folder selected.");
  };
  return <div className="stack narrow"><section className="panel form-panel"><PanelTitle title="Choose an installed project" /><p className="muted">Check an existing setup, repair missing files, add workflows, or remove managed files without connecting an AI provider.</p><Field label="Project folder" value={state.identity.projectRoot} placeholder="Choose an installed project folder" onChange={(value) => { setPendingSelection(undefined); updateIdentity({ projectRoot: value, launcherDescriptorPath: undefined }); }} action="Browse" onAction={() => void choose()} />{pendingSelection?.path && <button type="button" className="button secondary" onClick={() => { updateIdentity({ projectRoot: pendingSelection.path ?? "", launcherDescriptorPath: undefined }); setPendingSelection(undefined); setMessage("Maintenance will continue without an external launcher file."); }}>Continue without launcher file</button>}{state.identity.launcherDescriptorPath && <div className="path-preview" role="status"><p className="muted"><strong>Launcher candidate:</strong> <code>{state.identity.launcherDescriptorPath}</code></p><p className="muted">Its declared path matches the selected project. Continue to use it, or exclude it from maintenance scans.</p><button type="button" className="button secondary" onClick={() => { updateIdentity({ launcherDescriptorPath: undefined }); setMessage("Maintenance will continue without an external launcher file."); }}>Continue without launcher file</button></div>}{message && <p className="muted" role="status">{message}</p>}</section></div>;
}

function ExistingProjectPicker({ state, updateIdentity, onPickProjectFolder }: { state: WizardState; updateIdentity: (patch: Partial<ProjectIdentity>) => void; onPickProjectFolder: () => Promise<FolderSelection | null> }) {
  const [message, setMessage] = useState<string>();
  const [pendingSelection, setPendingSelection] = useState<FolderSelection>();
  const choose = async () => {
    setMessage(undefined);
    setPendingSelection(undefined);
    const selected = await onPickProjectFolder();
    if (selected?.path && selected.error) {
      setPendingSelection(selected);
      setMessage(`Launcher discovery needs review: ${selected.error}`);
    } else if (selected?.path) {
      updateIdentity({
        projectRoot: selected.path,
        launcherDescriptorPath: selected.launcher_descriptor_path ?? undefined,
      });
    }
    else if (selected?.error) setMessage(`The selected folder could not be used: ${selected.error}`);
    else setMessage("No folder selected.");
  };
  return <div className="stack narrow"><section className="panel form-panel"><p className="muted">Choose the mod project. Its descriptors, agentic setup, and existing managed state will be detected during the read-only scan.</p><Field label="Project folder" value={state.identity.projectRoot} placeholder="Choose an existing mod project" onChange={(value) => { setPendingSelection(undefined); updateIdentity({ projectRoot: value, launcherDescriptorPath: undefined }); }} action="Browse" onAction={() => void choose()} />{pendingSelection?.path && <button type="button" className="button secondary" onClick={() => { updateIdentity({ projectRoot: pendingSelection.path ?? "", launcherDescriptorPath: undefined }); setPendingSelection(undefined); setMessage("The scan will continue without an external launcher file."); }}>Scan without launcher file</button>}{state.identity.launcherDescriptorPath && <div className="path-preview" role="status"><p className="muted"><strong>Launcher candidate:</strong> <code>{state.identity.launcherDescriptorPath}</code></p><p className="muted">Its declared path matches the selected project. Continue to use it, or exclude it from the scan.</p><button type="button" className="button secondary" onClick={() => { updateIdentity({ launcherDescriptorPath: undefined }); setMessage("The scan will continue without an external launcher file."); }}>Scan without launcher file</button></div>}{message && <p className="muted" role="status">{message}</p>}</section></div>;
}

export function Identity({ state, update, updateIdentity, onPickProjectFolder, onPickLauncherFolder, onConfirmAnalysis }: { state: WizardState; update: (patch: Partial<WizardState>) => void; updateIdentity: (patch: Partial<ProjectIdentity>) => void; onPickProjectFolder: () => Promise<FolderSelection | null>; onPickLauncherFolder: () => Promise<FolderSelection | null>; onConfirmAnalysis: () => Promise<void> }) {
  const [selectedPreview, setSelectedPreview] = useState<GeneratedArtifactPreview>();
  const [previewMessage, setPreviewMessage] = useState<string>();
  const [previewPending, setPreviewPending] = useState(false);
  const [folderMessage, setFolderMessage] = useState<string>();
  const previewReady = Boolean(state.identity.projectRoot.trim())
    && (!isTauriRuntime() || Boolean(state.codexAnalysisRecord?.confirmed_fields.length));
  const showPreview = async (componentId: string) => {
    if (!previewReady || previewPending) return;
    setPreviewPending(true);
    setPreviewMessage("Preparing preview…");
    try {
      const result = await previewDescriptorsResult(state);
      if (!result.value) {
        setSelectedPreview(undefined);
        setPreviewMessage(result.error ?? "The preview could not be prepared. Nothing was changed.");
        return;
      }
      const artifact = result.value.find((candidate) => candidate.component_id === componentId);
      setSelectedPreview(artifact);
      setPreviewMessage(artifact ? undefined : "That generated file is not available in this setup.");
    } catch {
      setSelectedPreview(undefined);
      setPreviewMessage("The preview stopped unexpectedly. Nothing was changed; try again.");
    } finally {
      setPreviewPending(false);
    }
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
    <fieldset className="field tag-picker"><legend className="field-label">Descriptor tags</legend><div className="tag-options">{HOI4_DESCRIPTOR_TAGS.map((tag) => { const checked = state.identity.descriptorTags?.includes(tag) ?? false; return <label key={tag}><input type="checkbox" checked={checked} onChange={(event) => updateIdentity({ descriptorTags: event.target.checked ? [...(state.identity.descriptorTags ?? []), tag] : (state.identity.descriptorTags ?? []).filter((value) => value !== tag) })} /><span>{tag}</span></label>; })}</div></fieldset>
    <Field label="Initial folders" value={(state.folderProfile ?? []).join(", ")} placeholder="common, events, gfx, localisation/english, docs" onChange={(value) => update({ folderProfile: value.split(",").map((folder) => folder.trim()).filter(Boolean) })} />
    <details><summary>Advanced project metadata</summary>
    <Field label="Author" value={state.identity.author} onChange={(value) => updateIdentity({ author: value })} />
    <Field label="Version" value={state.identity.version} onChange={(value) => updateIdentity({ version: value })} />
    <Field label="Supported game version" value={state.identity.supportedGameVersion} onChange={(value) => updateIdentity({ supportedGameVersion: value })} />
    <Field label="Default branch" value={state.identity.defaultBranch} onChange={(value) => updateIdentity({ defaultBranch: value })} />
    </details>
  </div><Field label="Project folder" value={state.identity.projectRoot} placeholder="Finding the HOI4 mod folder…" onChange={(value) => { updateIdentity({ projectRoot: value }); update({ projectPathsOverridden: true, projectPathStatus: "manual", projectPathMessage: "Using your project location." }); }} action="Change location" onAction={() => void chooseFolder()} mono />{state.projectPathMessage && <p className={state.projectPathStatus === "collision" || state.projectPathStatus === "unavailable" ? "callout review" : "muted"} role="status">{state.projectPathMessage}</p>}{folderMessage && <p className="muted" role="status">{folderMessage}</p>}<details><summary>Launcher file location</summary><Field label="Launcher descriptor path" value={state.identity.launcherDescriptorPath ?? ""} placeholder="Filled automatically" onChange={(value) => { updateIdentity({ launcherDescriptorPath: value }); update({ projectPathsOverridden: true, projectPathStatus: "manual", projectPathMessage: "Using your project location." }); }} action="Choose mod folder" onAction={() => void chooseLauncherFolder()} mono /></details></section><GeneratedFilesPreview state={state} ready={previewReady} pending={previewPending} selected={selectedPreview} message={previewMessage} onPreview={showPreview} /></div></div>;
}

function GeneratedFilesPreview({ state, ready, pending, selected, message, onPreview }: { state: WizardState; ready: boolean; pending: boolean; selected?: GeneratedArtifactPreview; message?: string; onPreview: (componentId: string) => Promise<void> }) {
  const files = [
    { componentId: "project.descriptor", name: "descriptor.mod", detail: "Inside the project" },
    { componentId: "project.launcher_descriptor", name: `${state.identity.projectId || "project"}.mod`, detail: "HOI4 launcher file" },
    { componentId: "project.thumbnail", name: "thumbnail.png", detail: "Replaceable placeholder" },
  ];
  const imageUrl = selected?.bytes?.length ? pngDataUrl(selected.bytes) : undefined;
  return <section className="panel generated-files-panel"><PanelTitle title="Generated files" />{files.map((file) => <div className="list-row" key={file.componentId}><div><strong>{file.name}</strong><span>{file.detail}</span></div><button type="button" className="text-button" disabled={!ready || pending} title={!ready ? "Confirm the generated details and project location first." : undefined} aria-expanded={selected?.component_id === file.componentId} onClick={() => void onPreview(file.componentId)}>{pending ? "Preparing…" : "Preview"}</button></div>)}{!ready && <p className="muted" role="status">Confirm the generated details and project location to enable previews.</p>}{message && <p className="muted" role="status">{message}</p>}{selected && <div className="artifact-preview" aria-live="polite"><div className="artifact-preview-heading"><strong>{selected.destination}</strong><button type="button" className="text-button" onClick={() => void onPreview(selected.component_id)}>Refresh</button></div>{imageUrl ? <><div className="thumbnail-preview"><img src={imageUrl} alt="Generated thumbnail placeholder preview" /></div><p className="muted">{selected.content}</p></> : <pre className="report-preview">{selected.content}</pre>}</div>}</section>;
}

function pngDataUrl(bytes: number[]): string {
  let binary = "";
  for (let index = 0; index < bytes.length; index += 0x8000) {
    binary += String.fromCharCode(...bytes.slice(index, index + 0x8000));
  }
  return `data:image/png;base64,${btoa(binary)}`;
}

const SCAN_STAGE_LABELS: Record<string, string> = {
  discovering_files: "Reading selected mod metadata",
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
  const currentCheck = cancellationRequested ? "Cancellation requested" : error ? "Scan needs attention" : complete && partial ? "Read-only scan reached a safety limit" : SCAN_STAGE_LABELS[progress.stage] ?? "Reading selected mod metadata";
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

export function Findings({ state, findings, selected, setSelected, setFindings, onConfirmAnalysis, onManageExisting, onPackageChatSources }: { state: WizardState; findings: ScanFinding[]; selected: string; setSelected: (id: string) => void; setFindings: Dispatch<SetStateAction<ScanFinding[]>>; onConfirmAnalysis: () => Promise<void>; onManageExisting: () => void; onPackageChatSources: () => Promise<void> }) {
  const active = findings.find((finding) => finding.id === selected) ?? findings[0];
  const managed = managedInstallationDetails(findings);
  return <div className="stack"><details><summary>{aiProviderLabel(state.aiProvider, state.aiProfiles)} input preview</summary><div className="manifest-details">{findings.filter((finding) => finding.status !== "rejected").map((finding) => <div key={finding.id}><strong>{finding.id}</strong><span>{finding.evidencePath ?? "approved finding reference"}</span><small>{finding.evidenceExcerpt ?? finding.value}</small></div>)}</div></details>{managed.present && managed.valid && <section className="callout info existing-setup-callout"><div><strong>Existing setup found</strong><p>This project already has a managed setup. You can repair it now or add the 3D workflow later without starting over.</p></div><button type="button" className="button secondary" onClick={onManageExisting}>Repair or add workflows</button></section>}{managed.chatSourcesAvailable && <section className="callout info existing-setup-callout"><div><strong>ChatGPT project sources found</strong><p>Package the detected instructions, skills, subagents, and optional root Markdown outside the mod project.</p></div><button type="button" className="button secondary" onClick={() => void onPackageChatSources()}>Package ChatGPT project sources</button></section>}{state.codexAnalysis && <CodexReview state={state} onConfirmAnalysis={onConfirmAnalysis} />}<div className="two-column"><section className="panel"><PanelTitle title="Project facts" />{findings.length ? <div>{findings.map((finding) => <button type="button" key={finding.id} className={`finding-row ${finding.status === "needs_review" ? "review" : ""}`} aria-pressed={finding.id === active?.id} onClick={() => setSelected(finding.id)}><span className={`state-icon ${finding.status === "needs_review" ? "review" : "pass"}`}>{finding.status === "needs_review" ? "!" : "✓"}</span><span><strong>{finding.label}</strong><small>{finding.value}</small></span><span className="text-button">{finding.status === "needs_review" ? "Review" : "Edit"}</span></button>)}</div> : <p className="muted">No scan findings are available in this runtime. The desktop scanner must return evidence before values can be accepted.</p>}</section><section className="panel selected-finding"><PanelTitle title="Selected finding" />{active ? <div className="selected-body"><label className="field-label" htmlFor="finding-value">Editable project value: {active.label}</label><input id="finding-value" className="text-input focused" value={active.value} onChange={(event) => setFindings((current) => current.map((finding) => finding.id === active.id ? { ...finding, value: event.target.value, status: "edited" } : finding))} /><span className="confidence">{Math.round(active.confidence * 100)}% confidence</span><button type="button" className="text-button" aria-pressed={active.status !== "rejected"} onClick={() => setFindings((current) => current.map((finding) => finding.id === active.id ? { ...finding, status: finding.status === "rejected" ? "accepted" : "rejected" } : finding))}>{active.status === "rejected" ? `Include in ${aiProviderLabel(state.aiProvider, state.aiProfiles)} input` : `Exclude from ${aiProviderLabel(state.aiProvider, state.aiProfiles)} input`}</button><div className="evidence-block"><span>Evidence</span><p>{active.evidence}</p></div><details><summary>Show matching files</summary><p className="muted">Full evidence and hashes stay behind progressive disclosure.</p></details></div> : <p className="muted">Select a finding after the bounded scan returns.</p>}</section></div></div>;
}

function formatManifestSize(component: ManifestComponentPreview): string {
  const bytes = component.expected_files.reduce((total, file) => total + (file.size ?? 0), 0);
  const size = bytes >= 1024 * 1024 ? `${(bytes / (1024 * 1024)).toFixed(1)} MB` : bytes >= 1024 ? `${Math.round(bytes / 1024)} KB` : bytes ? `${bytes} B` : "—";
  return `${component.expected_files.length} files · ${size}`;
}

function currentUiPlatform(): "windows" | "macos" {
  return typeof navigator !== "undefined" && /macintosh|mac os x/i.test(navigator.userAgent)
    ? "macos"
    : "windows";
}

export function manifestComponentSupportsPlatform(
  component: ManifestComponentPreview,
  platform: "windows" | "macos" = currentUiPlatform(),
): boolean {
  const platforms = component.platforms ?? ["all"];
  return platforms.includes("all") || platforms.includes(platform);
}

function manifestRow(component: ManifestComponentPreview, selected: boolean, provider: AiProviderId = "codex", components: ManifestComponentPreview[] = [component]): ComponentRow {
  const platform = component.platforms.length === 1 && (component.platforms[0] === "windows" || component.platforms[0] === "macos") ? component.platforms[0] : "all";
  const providerBlocked = provider !== "codex" && dependsOn(component.id, "codex.config", components);
  const platformBlocked = !manifestComponentSupportsPlatform(component);
  return {
    id: component.id,
    title: component.display_name,
    detail: platformBlocked
      ? `${component.description ?? `${component.category} component from the resolved manifest`} Not available on this computer.`
      : providerBlocked && component.id !== "codex.config"
      ? `${component.description ?? `${component.category} component from the resolved manifest`} Not available for the selected provider because the verified manifest requires Codex.`
      : component.description ?? `${component.category} component from the resolved manifest`,
    size: formatManifestSize(component),
    selected,
    required: component.id === "codex.config" ? provider === "codex" : !component.optional,
    platform,
    state: providerBlocked || platformBlocked ? "blocked" : "supported",
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

function meshWorkflowAvailable(state: WizardState): boolean {
  const component = state.manifestPreview?.components.find((candidate) => candidate.id === "workflow.3d");
  return Boolean(component && manifestComponentSupportsPlatform(component) && providerSupportsComponent(component, state.manifestPreview?.components ?? [], state.aiProvider));
}

function superEventsComponentUnavailableOnComputer(state: WizardState): boolean {
  const component = state.manifestPreview?.components.find((candidate) => candidate.id === "workflow.super_events");
  return Boolean(component && !manifestComponentSupportsPlatform(component));
}

interface FlattenedChatFile {
  name: string;
  size?: number;
}

function flattenedChatFiles(state: WizardState, manifest: SourceManifestPreview | null): FlattenedChatFile[] {
  const planned = state.plan?.generated_artifacts?.filter((artifact) => artifact.destination.startsWith("chatgpt_project_sources/")) ?? [];
  if (planned.length > 0) {
    return planned.map((artifact) => ({
      name: artifact.destination.replace("chatgpt_project_sources/", ""),
      size: artifact.bytes?.length ?? new TextEncoder().encode(artifact.content).length,
    }));
  }

  const files = new Map<string, FlattenedChatFile>([
    ["agents.md", { name: "AGENTS.md" }],
    ["readme.md", { name: "README.md" }],
  ]);
  for (const component of manifest?.components ?? []) {
    if (!state.selectedComponents.includes(component.id)) continue;
    for (const file of component.expected_files) {
      const path = file.path.replaceAll("\\", "/");
      const skill = path.match(/(?:^|\/)\.agents\/skills\/([^/]+)\/SKILL\.md$/i);
      const subagent = path.match(/(?:^|\/)\.codex\/agents\/([^/]+\.toml)$/i);
      const name = skill ? `${skill[1]}.md` : subagent?.[1];
      if (name && !files.has(name.toLocaleLowerCase())) files.set(name.toLocaleLowerCase(), { name, size: file.size ?? undefined });
    }
  }
  return Array.from(files.values());
}

export function Components({ state, update }: { state: WizardState; update: (patch: Partial<WizardState>) => void }) {
  const [manifest, setManifest] = useState<SourceManifestPreview | null>(state.manifestPreview ?? null);
  const [manifestMessage, setManifestMessage] = useState(state.manifestPreview ? "Components loaded." : "Loading setup components…");
  const [manifestFailed, setManifestFailed] = useState(false);
  const [manifestRetrying, setManifestRetrying] = useState(false);
  const [manifestRequest, setManifestRequest] = useState(0);
  const manifestStatusRef = useRef<HTMLParagraphElement | null>(null);

  useEffect(() => {
    if (import.meta.env.DEV && window.__HOI4_DOCUMENTATION_STATE__ && state.manifestPreview) {
      setManifest(state.manifestPreview);
      setManifestMessage("Components loaded.");
      return;
    }
    let active = true;
    setManifest(null);
    if (!manifestRetrying) setManifestFailed(false);
    setManifestMessage("Loading setup components…");
    const returningFromRetry = manifestRetrying;
    void previewSourceManifestResult(state.sourceMode, state.pinnedRef).then((response) => {
      if (!active) return;
      const result = response.value;
      if (!result) {
        setManifest(null);
        setManifestFailed(true);
        setManifestRetrying(false);
        setManifestMessage(response.error ? `The setup components could not be loaded: ${response.error}` : "The setup components could not be loaded.");
        return;
      }
      const requiredIds = result.components.filter((component) => manifestComponentSupportsPlatform(component) && providerSupportsComponent(component, result.components, state.aiProvider) && (component.id === "codex.config" ? state.aiProvider === "codex" : !component.optional)).map((component) => component.id);
      const defaultProfileIds = ((result.profiles ?? []).find((profile) => profile.default)?.components ?? [])
        .filter((id) => result.components.some((component) => component.id === id && manifestComponentSupportsPlatform(component) && providerSupportsComponent(component, result.components, state.aiProvider)));
      const availableIds = new Set(result.components.map((component) => component.id));
      const supportedIds = new Set(result.components.filter((component) => manifestComponentSupportsPlatform(component) && providerSupportsComponent(component, result.components, state.aiProvider)).map((component) => component.id));
      const installedIds = new Set(state.installedComponentIds ?? []);
      const explicitlyRequestedIds = new Set(state.maintenanceOptionalSelections ?? []);
      const retainedSelections = state.selectedComponents.filter((id) => {
        const component = result.components.find((candidate) => candidate.id === id);
        return availableIds.has(id)
          && supportedIds.has(id)
          && (!state.existingInstallationDetected
            || component?.optional !== true
            || installedIds.has(id)
            || explicitlyRequestedIds.has(id));
      });
      const profileSelections = state.existingInstallationDetected
        ? defaultProfileIds.filter((id) => result.components.find((component) => component.id === id)?.optional !== true)
        : defaultProfileIds;
      const selectedComponents = Array.from(new Set([
        ...retainedSelections,
        ...profileSelections,
        ...Array.from(installedIds).filter((id) => availableIds.has(id) && supportedIds.has(id)),
        ...Array.from(explicitlyRequestedIds).filter((id) => availableIds.has(id) && supportedIds.has(id)),
        ...requiredIds,
      ]));
      const rows = result.components.map((component) => manifestRow(component, selectedComponents.includes(component.id), state.aiProvider, result.components));
      setManifest(result);
      setManifestMessage("Components loaded.");
      setManifestFailed(false);
      setManifestRetrying(false);
      update({ manifestPreview: result, components: rows, selectedComponents });
      if (returningFromRetry) window.setTimeout(() => manifestStatusRef.current?.focus(), 0);
    });
    return () => { active = false; };
  }, [state.sourceMode, state.pinnedRef, state.aiProvider, manifestRequest]);

  const rows = manifest?.components.map((component) => manifestRow(component, state.selectedComponents.includes(component.id), state.aiProvider, manifest.components)) ?? [];
  const dependencyOnlyOptionalIds = new Set((manifest?.components ?? [])
    .filter((component) => component.optional)
    .flatMap((component) => component.dependencies));
  const visibleRows = rows.filter((component) => !component.id.startsWith("workflow.") && !dependencyOnlyOptionalIds.has(component.id));
  const chatFiles = flattenedChatFiles(state, manifest);
  const chatSize = chatFiles.reduce((total, file) => total + (file.size ?? 0), 0);
  const allChatSizesKnown = chatFiles.length > 0 && chatFiles.every((file) => file.size !== undefined);
  const chatSummary = `${chatFiles.length} files${allChatSizesKnown ? ` · ${formatScanBytes(chatSize)}` : ""}`;
  const toggle = (id: string) => {
    const component = rows.find((row) => row.id === id);
    if (!component || component.required || component.state === "blocked") return;
    const selected = component.selected ? state.selectedComponents.filter((value) => value !== id) : [...state.selectedComponents, id];
    const manifestComponent = manifest?.components.find((candidate) => candidate.id === id);
    const maintenanceOptionalSelections = state.existingInstallationDetected && manifestComponent?.optional
      ? component.selected
        ? (state.maintenanceOptionalSelections ?? []).filter((componentId) => componentId !== id)
        : Array.from(new Set([...(state.maintenanceOptionalSelections ?? []), id]))
      : state.maintenanceOptionalSelections;
    update({ selectedComponents: selected, maintenanceOptionalSelections, components: rows.map((row) => row.id === id ? { ...row, selected: !row.selected } : row) });
  };
  const chooseFlattenedSources = (selected: boolean) => update({ flattenForChat: selected, plan: undefined, conflictChoice: undefined, transactionError: undefined });
  const retryManifest = () => {
    if (manifestRetrying) return;
    setManifestRetrying(true);
    setManifestRequest((request) => request + 1);
  };
  return <div className="stack narrow"><section className="panel">{manifest ? visibleRows.map((component) => <button type="button" key={component.id} className="component-row" onClick={() => toggle(component.id)} aria-pressed={component.selected} aria-disabled={component.required || component.state === "blocked" || undefined} disabled={component.required || component.state === "blocked"}><span className={`checkbox ${component.selected ? "checked" : ""}`}>{component.selected ? "✓" : ""}</span><span><strong>{component.title}</strong><small>{component.detail}</small></span><span className="size">{component.size}</span></button>) : null}{state.aiProvider === "codex" && <><label className="component-row flatten-package-row"><input className="visually-hidden" type="checkbox" checked={state.flattenForChat} onChange={(event) => chooseFlattenedSources(event.target.checked)} /><span className={`checkbox ${state.flattenForChat ? "checked" : ""}`} aria-hidden="true">{state.flattenForChat ? "✓" : ""}</span><span><strong>Prepare a flattened ChatGPT project-sources folder</strong><small>Project guidance, README, skills, and subagents in one folder</small></span><span className="size">{chatSummary}</span></label>{state.flattenForChat && <details><summary>Files in the ChatGPT folder</summary><div className="manifest-details flattened-file-list">{chatFiles.map((file) => <div key={file.name}><strong>{file.name}</strong><small>{file.size === undefined ? "Size calculated during review" : formatScanBytes(file.size)}</small></div>)}</div></details>}</>}{manifestFailed || manifestRetrying ? <div className="callout block" role={manifestRetrying ? "status" : "alert"}><span>{manifestMessage}</span><button type="button" className="button secondary" aria-disabled={manifestRetrying || undefined} aria-busy={manifestRetrying || undefined} onClick={retryManifest}>{manifestRetrying ? "Loading components…" : "Retry loading components"}</button></div> : <p className="muted" role="status" tabIndex={-1} ref={manifestStatusRef}>{manifestMessage}</p>}<p className="muted">Source: <ExternalLink href="https://github.com/klimPaskov/Agentic-HOI4-Modding">Agentic HOI4 Modding <span aria-hidden="true">↗</span></ExternalLink></p><details><summary>Dependencies and file list</summary>{manifest ? <div className="manifest-details">{manifest.components.map((component) => <div key={component.id}><strong>{component.display_name}</strong><span>{component.dependencies.length ? `Requires ${component.dependencies.join(", ")}` : "No additional components required"}</span><small>Platforms: {component.platforms.join(", ")}</small><small>{component.expected_files.length === 1 ? "1 file" : `${component.expected_files.length} files`} · destination: {component.destination.path}</small>{component.expected_files.map((file) => <small className="manifest-file-path" key={`${component.id}-${file.path}`}>{file.path}</small>)}</div>)}</div> : <p className="muted">Dependencies appear after the components load.</p>}</details><details><summary>Choose source version</summary><label className="field"><span className="field-label">Version</span><select className="text-input" value={state.sourceMode} onChange={(event) => update({ sourceMode: event.target.value as WizardState["sourceMode"], pinnedRef: "", manifestPreview: undefined, components: [], codexAnalysis: undefined, codexAnalysisRecord: undefined, plan: undefined, transactionError: "Changing the source requires a new analysis before setup can continue." })}><option value="latest">Latest</option><option value="pinned_commit">Specific commit</option><option value="pinned_release">Release</option></select></label>{state.sourceMode !== "latest" && <Field label={state.sourceMode === "pinned_commit" ? "Commit" : "Release"} value={state.pinnedRef} onChange={(value) => update({ pinnedRef: value, manifestPreview: undefined, components: [], codexAnalysis: undefined, codexAnalysisRecord: undefined, plan: undefined, transactionError: "Changing the source requires a new analysis before setup can continue." })} mono placeholder={state.sourceMode === "pinned_commit" ? "40-character commit" : "v1.0.0"} />}</details></section><div className="disclosure-note">Download size appears before installation.</div></div>;
}

export function Workflows({ state, update }: { state: WizardState; update: (patch: Partial<WizardState>) => void }) {
  const [localDiscovery, setLocalDiscovery] = useState<LocalPortraitDiscovery>();
  const [localDiscoveryPending, setLocalDiscoveryPending] = useState(false);
  const [localInstallPending, setLocalInstallPending] = useState(false);
  const [localInstallMessage, setLocalInstallMessage] = useState<string>();
  const meshUnavailable = !meshWorkflowAvailable(state);
  const superEventsComponent = state.manifestPreview?.components.find((component) => component.id === "workflow.super_events");
  const superEventsUnavailable = Boolean(state.manifestPreview && (!superEventsComponent || !manifestComponentSupportsPlatform(superEventsComponent)));
  const manifestComponents = state.manifestPreview?.components ?? [];
  const dependencyOnlyWorkflowIds = new Set(manifestComponents
    .filter((component) => component.optional && component.id.startsWith("workflow."))
    .flatMap((component) => component.dependencies.filter((dependency) => dependency.startsWith("workflow."))));
  const genericWorkflowComponents = manifestComponents.filter((component) =>
    component.optional
    && component.id.startsWith("workflow.")
    && !dependencyOnlyWorkflowIds.has(component.id)
    && component.id !== "workflow.3d"
    && component.id !== "workflow.super_events"
    && !component.id.startsWith("workflow.portraits")
  );
  const setWorkflow = (id: string, checked: boolean, patch: Partial<WizardState>) => {
    const selectedComponents = checked
      ? Array.from(new Set([...state.selectedComponents, id]))
      : state.selectedComponents.filter((componentId) => componentId !== id);
    const genericMaintenanceChoice = state.existingInstallationDetected
      && id !== "workflow.3d"
      && id !== "workflow.super_events"
      && !id.startsWith("workflow.portraits");
    const maintenanceOptionalSelections = genericMaintenanceChoice
      ? checked
        ? Array.from(new Set([...(state.maintenanceOptionalSelections ?? []), id]))
        : (state.maintenanceOptionalSelections ?? []).filter((componentId) => componentId !== id)
      : state.maintenanceOptionalSelections;
    update({ ...patch, selectedComponents, maintenanceOptionalSelections });
  };
  const portrait = state.portraitPipeline ?? DEFAULT_PORTRAIT_PIPELINE;
  const portraitComponents = manifestComponents.filter((component) => ["workflow.portraits.cloud", "workflow.portraits.local", "workflow.portraits.runpod"].includes(component.id));
  const portraitAvailable = portraitComponents.some((component) => manifestComponentSupportsPlatform(component));
  const portraitPlatformUnavailable = portraitComponents.length > 0 && !portraitAvailable;
  const choosePortraitProvider = (provider: PortraitProviderId) => {
    const enabled = provider !== "disabled";
    const providerStatus: PortraitProviderStatus = provider === "disabled"
      ? "not_selected"
      : provider === "cloud"
        ? "needs_authorization"
        : provider === "local"
          ? "needs_hardware"
          : "needs_runpod";
    const selectedComponents = enabled
      ? Array.from(new Set([...state.selectedComponents.filter((id) => !id.startsWith("workflow.portraits")), ...portraitComponentIdsFor(provider)]))
      : state.selectedComponents.filter((id) => !id.startsWith("workflow.portraits"));
    update({
      portraitPipeline: { ...portrait, enabled, provider, providerStatus, mcpRegistered: provider === "cloud" && enabled },
      selectedComponents,
      transactionError: undefined,
    });
  };
  const inspectLocal = async () => {
    if (localDiscoveryPending) return;
    setLocalDiscoveryPending(true);
    const result = await inspectLocalPortraitProvider(portrait.localComfyuiRoot, portrait.localServerUrl);
    setLocalDiscoveryPending(false);
    if (!result) {
      update({ portraitPipeline: { ...portrait, providerStatus: "temporarily_unavailable" }, transactionError: "The local ComfyUI check could not be started." });
      return;
    }
    setLocalDiscovery(result);
    const knownStatuses: PortraitProviderStatus[] = ["not_selected", "ready", "needs_authorization", "needs_subscription", "needs_huggingface_access", "needs_models", "needs_workflow_install", "needs_hardware", "needs_runpod", "unreachable", "temporarily_unavailable"];
    const providerStatus = knownStatuses.includes(result.status as PortraitProviderStatus) ? result.status as PortraitProviderStatus : "temporarily_unavailable";
    update({
      portraitPipeline: {
        ...portrait,
        providerStatus,
        localComfyuiRoot: result.detectedRoot || portrait.localComfyuiRoot,
      },
      transactionError: undefined,
    });
  };
  const installLocal = async () => {
    if (localInstallPending) return;
    const root = localDiscovery?.detectedRoot || portrait.localComfyuiRoot;
    if (!root) {
      setLocalInstallMessage("Choose or discover a verified ComfyUI root first.");
      return;
    }
    setLocalInstallPending(true);
    const result = await installLocalPortraitWorkflows(root);
    setLocalInstallPending(false);
    if (result.error || !result.value) {
      setLocalInstallMessage(result.error ?? "The pinned portrait workflows could not be installed.");
      return;
    }
    setLocalInstallMessage(result.value.message);
    if (result.value.status === "ready") {
      update({ portraitPipeline: { ...portrait, localComfyuiRoot: root, providerStatus: "needs_models" }, transactionError: undefined });
    }
  };
  /*
  return <div className="stack narrow"><section className="panel"><ToggleRow label="3D models workflow" detail={meshUnavailable ? "This workflow currently requires Codex" : "Adds the available 3D workflow files and checks"} checked={state.meshSelected} disabled={Boolean(meshUnavailable)} onChange={(checked) => setWorkflow("workflow.3d", checked, { meshSelected: checked })} /><ToggleRow label="Super Events workflow" detail={superEventsUnavailable ? "Unavailable in the selected source version" : "Adds a ready-to-use popup, templates, example, and reusable registration workflow"} checked={state.superEventsSelected} disabled={superEventsUnavailable} onChange={(checked) => setWorkflow("workflow.super_events", checked, { superEventsSelected: checked })} /><ToggleRow label="ComfyUI portrait production" detail={!portraitAvailable ? "Unavailable in the selected source version" : "Optional provider-backed portraits with a source-based fallback"} checked={portrait.enabled} disabled={!portraitAvailable} onChange={(checked) => choosePortraitProvider(checked ? "cloud" : "disabled")} />{portraitAvailable && portrait.enabled && <div className="workflow-provider-panel"><span className="field-label" id="portrait-provider-label">Portrait provider</span><div className="radio-row" role="radiogroup" aria-labelledby="portrait-provider-label">{(["cloud", "local", "runpod"] as PortraitProviderId[]).map((provider) => <label className="radio-option" key={provider}><input type="radio" name="portrait-provider" checked={portrait.provider === provider} onChange={() => choosePortraitProvider(provider)} /><span><strong>{provider === "cloud" ? "Comfy Cloud" : provider === "local" ? "Local ComfyUI" : "RunPod"}</strong><small>{provider === "cloud" ? "MCP registration and provider-owned authorization" : provider === "local" ? "Configured loopback server and local models" : "Existing pod, URL, and browser guidance"}</small></span></label>)}</div>}{portrait.enabled && portrait.provider === "local" && <div className="workflow-provider-fields"><Field label="Local ComfyUI folder" value={portrait.localComfyuiRoot} onChange={(value) => update({ portraitPipeline: { ...portrait, localComfyuiRoot: value } })} placeholder="Optional configured folder" mono /><Field label="Local server" value={portrait.localServerUrl} onChange={(value) => update({ portraitPipeline: { ...portrait, localServerUrl: value } })} mono /><button type="button" className="button secondary" disabled={localDiscoveryPending} onClick={() => void inspectLocal()}>{localDiscoveryPending ? "Checking local setup…" : "Check local setup"}</button>{localDiscovery && <div className="workflow-provider-summary"><Status label={localDiscovery.status === "ready" ? "Local provider ready" : "Local setup needs review"} tone={localDiscovery.status === "ready" ? "pass" : "review"} /><span>{localDiscovery.detectedRoot ? `Detected root: ${localDiscovery.detectedRoot}` : "No ComfyUI root detected"}</span><span>GPU: {localDiscovery.gpuName ? `${localDiscovery.gpuName}${localDiscovery.vramGb ? ` (${localDiscovery.vramGb.toFixed(1)} GiB)` : ""}` : "Not verified"}</span><span>Workflows: {localDiscovery.workflowStatus}; models: {localDiscovery.modelStatus}; server: {localDiscovery.serverStatus}</span><small>{localDiscovery.message}</small><small>Workflow install: <code>{localDiscovery.installCommand}</code></small></div>}</div>}{portrait.enabled && portrait.provider === "runpod" && <div className="workflow-provider-fields"><Field label="RunPod URL" value={portrait.runpodUrl} onChange={(value) => update({ portraitPipeline: { ...portrait, runpodUrl: value } })} placeholder="Optional HTTPS URL" mono /><div className="workflow-provider-summary"><Status label="RunPod setup deferred" tone="review" /><span>Open the pod URL only after the current workflow package is installed and the ComfyUI workflow is visible.</span><small>Canonical install: <code>export HF_TOKEN="hf_…"; P=/workspace/comfyui-hoi4-portraits; COMFY_ROOT=/workspace/runpod-slim/ComfyUI; test -f "$COMFY_ROOT/main.py"; test -d "$P/.git" || git clone --depth 1 {PORTRAIT_REPOSITORY} "$P"; "$P/scripts/install_runpod.sh" "$COMFY_ROOT"</code></small></div></div>}{portrait.enabled && portrait.provider === "cloud" && <div className="workflow-provider-summary"><Status label="Cloud MCP registered" tone="pass" /><span>{providerName} uses the official Comfy Cloud MCP endpoint; authorization and Builder subscription remain provider-owned setup.</span><code>https://cloud.comfy.org/mcp</code></div>}<div className="workflow-provider-summary"><Status label={portrait.providerStatus === "ready" ? "Provider ready" : portrait.providerStatus === "not_selected" ? "Disabled" : "Setup deferred"} tone={portrait.providerStatus === "ready" ? "pass" : "review"} /><span>Current portrait workflow commit: <code>{portrait.workflowCommit.slice(0, 12)}</code></span></div></div>}</section></div>;
  */
  return <PortraitProviderSetup state={state} portrait={portrait} portraitAvailable={portraitAvailable} portraitPlatformUnavailable={portraitPlatformUnavailable} update={update} localDiscovery={localDiscovery} localDiscoveryPending={localDiscoveryPending} localInstallPending={localInstallPending} localInstallMessage={localInstallMessage} inspectLocal={inspectLocal} installLocal={installLocal} choosePortraitProvider={choosePortraitProvider} meshUnavailable={Boolean(meshUnavailable)} superEventsUnavailable={superEventsUnavailable} genericWorkflowComponents={genericWorkflowComponents} setWorkflow={setWorkflow} />;
}

function PortraitProviderSetup({
  state,
  portrait,
  portraitAvailable,
  portraitPlatformUnavailable,
  update,
  localDiscovery,
  localDiscoveryPending,
  localInstallPending,
  localInstallMessage,
  inspectLocal,
  installLocal,
  choosePortraitProvider,
  meshUnavailable,
  superEventsUnavailable,
  genericWorkflowComponents,
  setWorkflow,
}: {
  state: WizardState;
  portrait: PortraitPipelineState;
  portraitAvailable: boolean;
  portraitPlatformUnavailable: boolean;
  update: (patch: Partial<WizardState>) => void;
  localDiscovery?: LocalPortraitDiscovery;
  localDiscoveryPending: boolean;
  localInstallPending: boolean;
  localInstallMessage?: string;
  inspectLocal: () => Promise<void>;
  installLocal: () => Promise<void>;
  choosePortraitProvider: (provider: PortraitProviderId) => void;
  meshUnavailable?: boolean;
  superEventsUnavailable: boolean;
  genericWorkflowComponents: ManifestComponentPreview[];
  setWorkflow: (id: string, checked: boolean, patch: Partial<WizardState>) => void;
}) {
  const localStatus = localDiscovery?.status === "ready" ? "Local provider ready" : "Local setup needs review";
  const localTone: StatusTone = localDiscovery?.status === "ready" ? "pass" : "review";
  const runpodInstall = "scripts/install_runpod.sh /workspace/runpod-slim/ComfyUI";
  const meshComponent = state.manifestPreview?.components.find((component) => component.id === "workflow.3d");
  const meshPlatformUnavailable = Boolean(meshComponent && !manifestComponentSupportsPlatform(meshComponent));
  const meshProviderUnavailable = Boolean(meshComponent && !providerSupportsComponent(meshComponent, state.manifestPreview?.components ?? [], state.aiProvider));
  const meshUnavailableReason = meshPlatformUnavailable
    ? "Not available on this computer"
    : meshProviderUnavailable
      ? "Not available for the selected provider because the verified manifest requires Codex."
      : "Unavailable in the selected source version";
  return <div className="stack narrow">
    <section className="panel">
      <ToggleRow label="3D models workflow" detail={meshUnavailable ? meshUnavailableReason : "Installs and prepares the verified Meshy and Blender workflow automatically"} checked={state.meshSelected && !meshUnavailable} disabled={Boolean(meshUnavailable)} onChange={(checked) => setWorkflow("workflow.3d", checked, { meshSelected: checked })} />
      <ToggleRow label="Super Events workflow" detail={superEventsUnavailable ? (superEventsComponentUnavailableOnComputer(state) ? "Not available on this computer" : "Unavailable in the selected source version") : "Adds a ready-to-use popup, templates, example, and reusable registration workflow"} checked={state.superEventsSelected && !superEventsUnavailable} disabled={superEventsUnavailable} onChange={(checked) => setWorkflow("workflow.super_events", checked, { superEventsSelected: checked })} />
      <ToggleRow label="ComfyUI portrait production" detail={!portraitAvailable ? (portraitPlatformUnavailable ? "Not available on this computer" : "Unavailable in the selected source version") : "Optional provider-backed portraits with a source-based fallback"} checked={portrait.enabled && portraitAvailable} disabled={!portraitAvailable} onChange={(checked) => choosePortraitProvider(checked ? "cloud" : "disabled")} />
      {genericWorkflowComponents.map((component) => {
        const platformUnavailable = !manifestComponentSupportsPlatform(component);
        const providerUnavailable = !providerSupportsComponent(component, state.manifestPreview?.components ?? [], state.aiProvider);
        const unavailable = platformUnavailable || providerUnavailable;
        const detail = platformUnavailable
          ? "Not available on this computer"
          : providerUnavailable
            ? "Not available for the selected provider because the verified manifest requires Codex."
            : component.description ?? "Optional workflow declared by the selected source version";
        return <ToggleRow key={component.id} label={component.display_name} detail={detail} checked={state.selectedComponents.includes(component.id) && !unavailable} disabled={unavailable} onChange={(checked) => setWorkflow(component.id, checked, {})} />;
      })}
      {portraitAvailable && portrait.enabled && <div className="workflow-provider-panel">
        <p className="workflow-resource-note">{PORTRAIT_RESOURCE_REQUIREMENT}</p>
        <span className="field-label" id="portrait-provider-label">Portrait provider</span>
        <div className="radio-row" role="radiogroup" aria-labelledby="portrait-provider-label">
          {(["cloud", "local", "runpod"] as PortraitProviderId[]).map((provider) => <label className="radio-option" key={provider}>
            <input type="radio" name="portrait-provider" checked={portrait.provider === provider} onChange={() => choosePortraitProvider(provider)} />
            <span><strong>{provider === "cloud" ? "Comfy Cloud" : provider === "local" ? "Local ComfyUI" : "RunPod"}</strong><small>{provider === "cloud" ? "MCP registration and provider-owned authorization" : provider === "local" ? "Configured loopback server and local models" : "Existing pod, URL, and browser guidance"}</small></span>
          </label>)}
        </div>
        {portrait.provider === "local" && <div className="workflow-provider-fields">
          <Field label="Local ComfyUI folder" value={portrait.localComfyuiRoot} onChange={(value) => update({ portraitPipeline: { ...portrait, localComfyuiRoot: value } })} placeholder="Optional configured folder" mono />
          <Field label="Local server" value={portrait.localServerUrl} onChange={(value) => update({ portraitPipeline: { ...portrait, localServerUrl: value } })} mono />
          <button type="button" className="button secondary" disabled={localDiscoveryPending} onClick={() => void inspectLocal()}>{localDiscoveryPending ? "Checking local setup…" : "Check local setup"}</button>
          <button type="button" className="button secondary" disabled={localInstallPending || !(localDiscovery?.detectedRoot || portrait.localComfyuiRoot)} onClick={() => void installLocal()}>{localInstallPending ? "Installing pinned workflows…" : "Install pinned workflows"}</button>
          {localInstallMessage && <small className="muted" role="status">{localInstallMessage}</small>}
          {localDiscovery && <div className="workflow-provider-summary">
            <Status label={localStatus} tone={localTone} />
            <span>{localDiscovery.detectedRoot ? `Detected root: ${localDiscovery.detectedRoot}` : "No ComfyUI root detected"}</span>
            <span>GPU: {localDiscovery.gpuName ? `${localDiscovery.gpuName}${localDiscovery.vramGb ? ` (${localDiscovery.vramGb.toFixed(1)} GiB)` : ""}` : "Not verified"}</span>
            <span>Workflows: {localDiscovery.workflowStatus}; models: {localDiscovery.modelStatus}; server: {localDiscovery.serverStatus}</span>
            <small>{localDiscovery.message}</small>
            <small>Workflow install: <code>{localDiscovery.installCommand}</code></small>
          </div>}
        </div>}
        {portrait.provider === "runpod" && <div className="workflow-provider-fields">
          <Field label="RunPod URL" value={portrait.runpodUrl} onChange={(value) => update({ portraitPipeline: { ...portrait, runpodUrl: value } })} placeholder="Optional HTTPS URL" mono />
          <div className="workflow-provider-summary"><Status label="RunPod setup deferred" tone="review" /><span>Open the pod URL after the pinned workflow package is installed and visible.</span><small>Run the pinned install script: <code>{runpodInstall}</code></small></div>
        </div>}
        {portrait.provider === "cloud" && <div className="workflow-provider-summary"><Status label="Cloud MCP registered" tone="pass" /><span>Authorization and Builder subscription remain provider-owned setup.</span><code>https://cloud.comfy.org/mcp</code></div>}
        {portrait.provider !== "runpod" && <div className="workflow-provider-summary"><Status label={portrait.providerStatus === "ready" ? "Provider ready" : "Setup deferred"} tone={portrait.providerStatus === "ready" ? "pass" : "review"} /><span>Current portrait workflow commit: <code>{portrait.workflowCommit.slice(0, 12)}</code></span></div>}
      </div>}
    </section>
  </div>;
}

function WorkflowDeclaration({ state }: { state: WizardState }) {
  const component = state.manifestPreview?.components.find((candidate) => candidate.id === "workflow.3d" && state.selectedComponents.includes(candidate.id));
  if (!component) {
    return <details><summary>Workflow details</summary><p className="muted">Tools, requirements, and checks appear after the selected source version is loaded.</p></details>;
  }
  const tools = component.required_tools.map((tool) => tool.id).join(", ") || "None declared";
  const health = component.required_tools.flatMap((tool) => tool.health_checks).join(", ") || "None declared";
  const validation = component.validation.map((rule) => `${rule.id} · ${rule.severity}`).join(", ") || "None declared";
  return <details><summary>Verified workflow declarations</summary><div className="manifest-details"><div><strong>Source</strong><span>{component.source.path}</span></div><div><strong>Destination</strong><span>{component.destination.path}</span></div><div><strong>Tools</strong><span>{tools}</span></div><div><strong>Health checks</strong><span>{health}</span></div><div><strong>Validation</strong><span>{validation}</span></div></div></details>;
}

export function Mesh({ state, update }: { state: WizardState; update: (patch: Partial<WizardState>) => void }) {
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
  return <div className="stack narrow"><section className="panel form-panel"><label className="field-label" htmlFor="meshy-key">Meshy API key</label><p className="muted">Meshy.ai may charge for provider usage. Review the provider’s current pricing and account limits before running the source-declared workflow.</p><input id="meshy-key" className="text-input focused" type="password" value={state.meshKeyDraft} onChange={(event) => update({ meshKeyDraft: event.target.value })} autoComplete="off" /><div className="button-row mesh-actions"><button type="button" className="button primary" onClick={save}>Store in vault</button><button type="button" className="button secondary" onClick={() => update({ meshKeyDraft: "", meshKeyStatus: state.meshCredentialReference ? "present" : "missing", screen: "mcp" })}>Configure later</button>{state.meshCredentialReference && <button type="button" className="text-button" onClick={() => void remove()}>Delete stored key</button>}</div></section><section className="panel"><div className="list-row"><div><strong>MESHY_API_KEY</strong><span>Process-only environment variable</span></div><Status label={credentialLabel} tone={state.meshKeyStatus === "verified" ? "pass" : "review"} /></div><WorkflowDeclaration state={state} /></section></div>;
}

export function Mcp({ state }: { state: WizardState }) {
  const component = state.manifestPreview?.components.find((candidate) => candidate.category === "mcp" && state.selectedComponents.includes(candidate.id) && manifestComponentSupportsPlatform(candidate));
  const toolText = component?.required_tools.map((tool) => `${tool.id}${tool.version ? ` ${tool.version}` : ""}`).join(", ") || "None declared";
  const environmentText = component?.environment.map((environment) => `${environment.name}${environment.secret ? " · secret" : ""}`).join(", ") || "None declared";
  const capabilityText = component?.capabilities.join(", ") || "None declared";
  const healthText = component?.required_tools.flatMap((tool) => tool.health_checks).join(", ") || "No health checks declared";
  return <div className="two-column"><section className="panel"><PanelTitle title="Integrations" />{component ? <div className="server-row"><img className="server-icon server-icon-image" src={appIcon} alt="" aria-hidden="true" /><span><strong>{component.display_name}</strong><small>{component.description ?? "Selected integration"}</small></span></div> : <p className="muted">No integration is selected.</p>}{component && <details open><summary>Requirements</summary><div className="manifest-details"><div><strong>Tools</strong><span>{toolText}</span></div><div><strong>Environment names</strong><span>{environmentText}</span></div><div><strong>Capabilities</strong><span>{capabilityText}</span></div><div><strong>Health checks</strong><span>{healthText}</span></div><div><strong>Validation</strong><span>{component.validation.map((rule) => `${rule.id} · ${rule.severity}`).join(", ") || "None declared"}</span></div></div></details>}</section><section className="panel"><PanelTitle title="Credentials" /><div className="list-row"><div><strong>Meshy API key</strong><span>Stored securely by this computer</span></div><Status label={state.meshSelected && state.meshKeyStatus === "present" ? "Stored" : state.meshSelected ? "Not stored" : "Not selected"} tone={state.meshSelected && state.meshKeyStatus === "present" ? "review" : "muted"} /></div><div className="list-row"><div><strong>Project secrets</strong><span>Secret values are never added to the project.</span></div><Status label="Protected" tone="info" /></div></section></div>;
}

export function Git({ state, update }: { state: WizardState; update: (patch: Partial<WizardState>) => void }) {
  const onlineAction = state.gitOnlineAction ?? "none";
  const repository = state.gitHubRepository || state.identity?.projectId || "my-mod";
  const chooseGitMode = (mode: WizardState["gitMode"]) => update({ gitMode: mode, ...(mode === "skip" ? { gitOnlineAction: "none" as const } : {}) });
  const chooseOnlineAction = (action: GitOnlineAction) => update({ gitOnlineAction: action });
  return <div className="stack narrow"><section className="panel">{(["initialize", "preserve", "skip"] as const).map((mode) => <label key={mode} className="radio-row"><input type="radio" name="git-mode" checked={state.gitMode === mode} onChange={() => chooseGitMode(mode)} /><span><strong>{mode === "initialize" ? "Initialize a Git repository" : mode === "preserve" ? "Preserve the existing repository" : "Skip Git setup"}</strong><small>{mode === "initialize" ? "Create .git, merge .gitignore, and prepare an initial commit" : mode === "preserve" ? "Keep remotes, history, and branch state" : "Continue without creating a Git repository"}</small></span>{mode === "initialize" && <Status label="Recommended" tone="info" />}</label>)}</section><section className="panel form-panel"><div className="form-grid"><Field label="Default branch" value={state.gitBranch} onChange={(value) => update({ gitBranch: value })} /><label className="field"><span className="field-label">Initial commit</span><select className="text-input" value={state.initialCommit ? "after-validation" : "none"} onChange={(event) => update({ initialCommit: event.target.value === "after-validation" })}><option value="after-validation">Create after validation</option><option value="none">Do not create</option></select></label></div><details><summary>Remote settings</summary><div className="form-grid"><Field label="Remote name" value={state.gitRemoteName} onChange={(value) => update({ gitRemoteName: value })} placeholder="origin" /><Field label="Remote URL" value={state.gitRemoteUrl} onChange={(value) => update({ gitRemoteUrl: value })} placeholder="https://github.com/owner/repo.git" mono /></div></details></section>{state.gitMode !== "skip" && <section className="panel form-panel"><div className="section-label">Online Git</div>{(["none", "push_remote", "create_public_github"] as const).map((action) => <label key={action} className="radio-row"><input type="radio" name="git-online-action" checked={onlineAction === action} onChange={() => chooseOnlineAction(action)} /><span><strong>{action === "none" ? "Keep this project local" : action === "push_remote" ? "Push to an existing remote" : "Create a public GitHub repository"}</strong><small>{action === "none" ? "No online action will run." : action === "push_remote" ? "Push the validated branch after setup." : "Create the repository as public, then push this project."}</small></span>{action === "create_public_github" && <Status label="Separate approval" tone="review" />}</label>)}{onlineAction === "push_remote" && <p className="muted">The remote must already be configured and signed in through your Git credential helper.</p>}{onlineAction === "create_public_github" && <><Field label="GitHub repository" value={repository} onChange={(value) => update({ gitHubRepository: value })} placeholder="my-hoi4-mod" /><p className="muted">The app uses the GitHub sign-in already set up on this computer. It asks you to approve publication after setup.</p></>}</section>}</div>;
}

export function DryRun({ state, update }: { state: WizardState; update: (patch: Partial<WizardState>) => void }) {
  const [showFilePlan, setShowFilePlan] = useState(false);
  const plan = state.plan;
  const counts = plan?.operations.reduce((summary, operation) => {
    if (operation.action === "create" || operation.action === "generate") summary.create += 1;
    else if (operation.action === "replace" || operation.action === "merge" || operation.action === "rename") summary.update += 1;
    else if (operation.action === "skip") summary.skip += 1;
    return summary;
  }, { create: 0, update: 0, skip: 0 }) ?? { create: 0, update: 0, skip: 0 };
  const profileDirectories = (plan?.transaction?.directories ?? []).filter((directory) => directory !== ".tmp");
  counts.create += profileDirectories.length;
  const unresolved = plan?.conflicts.filter((conflict) => !conflict.selected).length;
  const planStatus = plan ? `${plan.operations.length} files · ${profileDirectories.length} folders` : "Plan unavailable";
  const onlineActionLabel = state.gitOnlineAction === "push_remote" ? "Push to an existing remote" : state.gitOnlineAction === "create_public_github" ? "Create a public GitHub repository" : "Keep this project local";
  const flattenedFiles = plan?.generated_artifacts?.filter((artifact) => artifact.destination.startsWith("chatgpt_project_sources/")) ?? [];
  const flattenedSize = flattenedFiles.reduce((total, artifact) => total + (artifact.bytes?.length ?? new TextEncoder().encode(artifact.content).length), 0);
  const flattenedSummary = flattenedFiles.length
    ? `${flattenedFiles.length} files · ${formatScanBytes(flattenedSize)}`
    : state.flattenForChat ? "Prepare changes to calculate" : "Optional";
  const setupChecks = plan?.external_actions ?? [];
  const setupCheckLabel = (componentId: string) => componentId === "mcp.hoi4_agent_tools"
    ? "Check the installed AI tools"
    : componentId === "workflow.3d"
      ? "Check the 3D workflow"
      : "Check an installed integration";
  return <div className="stack"><div className="metric-grid"><Metric label="Create" value={plan ? String(counts.create) : "—"} tone={plan ? "pass" : "info"} /><Metric label="Update" value={plan ? String(counts.update) : "—"} tone={plan ? "info" : "muted"} /><Metric label="Skip" value={plan ? String(counts.skip) : "—"} tone={plan ? "review" : "muted"} /><Metric label="Conflicts" value={plan ? String(unresolved) : "—"} tone={plan ? unresolved ? "block" : "pass" : "info"} /></div><div className="two-column"><section className="panel"><PanelTitle title="Plan summary" /><ChangeRow title="Add development tools" detail="Skills, instructions, configuration, and offline wiki" value={planStatus} /><ChangeRow title="Update project instructions" detail="Keeps your existing edits" status={plan ? "Review if modified" : "Pending"} /><ChangeRow title="Configure integrations" detail="Selected tools only" value={plan ? "Ready" : "Pending"} /><ChangeRow title="Git setup" detail={`${state.gitBranch} · local changes`} value={plan ? "Ready" : "Pending"} /><ChangeRow title="Online Git" detail="Runs only after setup" value={plan ? onlineActionLabel : "Pending"} />{state.aiProvider === "codex" && state.flattenForChat && <ChangeRow title="ChatGPT project files" detail="Selected in Components" value={flattenedSummary} />}{plan && <details open={showFilePlan} onToggle={(event) => setShowFilePlan(event.currentTarget.open)}><summary>Files and folders to install · {plan.operations.length + profileDirectories.length}</summary>{showFilePlan && <div className="manifest-details file-plan-list">{profileDirectories.map((directory) => <div key={`directory-${directory}`}><strong>{directory}/</strong><small>Create folder</small></div>)}{plan.operations.map((operation) => <div key={operation.id}><strong>{operation.destination}</strong><small>{operation.action === "create" || operation.action === "generate" ? "Add" : operation.action === "replace" || operation.action === "merge" ? "Update" : operation.action === "rename" ? "Keep both" : operation.action === "skip" ? "Keep current" : operation.action === "delete_managed" ? "Remove" : "Check"}</small></div>)}</div>}</details>}{setupChecks.length > 0 && <details><summary>Setup checks</summary><div className="manifest-details">{setupChecks.map((action) => <details key={action.id}><summary>{setupCheckLabel(action.component_id)}</summary><small>Runs automatically after the files are installed.</small><small>Command: {action.display_command ?? action.command_source}</small><small>Folder: {action.working_directory ?? "Project folder"}</small><small>Environment: {action.environment_names?.join(", ") || "None"}</small><small>Expected changes: {action.expected_writes?.join(", ") || "None"}</small></details>)}</div></details>}{!plan && <p className="muted">Prepare the changes before installation.</p>}</section><section className="panel"><PanelTitle title="Before setup" />{state.flattenForChat && flattenedFiles.length > 0 && <details><summary>ChatGPT folder · {flattenedSummary}</summary><div className="manifest-details flattened-file-list">{flattenedFiles.map((artifact) => { const bytes = artifact.bytes?.length ?? new TextEncoder().encode(artifact.content).length; return <div key={artifact.destination}><strong>{artifact.destination.replace("chatgpt_project_sources/", "")}</strong><small>{formatScanBytes(bytes)}</small></div>; })}</div></details>}<CheckRow label="Existing files saved" status={plan ? "Ready" : "Pending"} tone={plan ? "pass" : "info"} /><CheckRow label="Setup checks" status={plan ? setupChecks.length ? `${setupChecks.length} included` : "None needed" : "Pending"} tone={plan ? "pass" : "info"} /><CheckRow label="Unresolved conflicts" status={plan ? String(unresolved) : "Pending"} tone={plan ? unresolved ? "block" : "pass" : "info"} /></section></div></div>;
}

function Install({ state }: { state: WizardState }) {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    if (!state.transaction || ["completed", "rolled_back", "staging_discarded"].includes(state.transaction.state)) return;
    const timer = window.setInterval(() => setNow(Date.now()), 1000);
    return () => window.clearInterval(timer);
  }, [state.transaction?.transaction_id, state.transaction?.state]);
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
  const isDone = (status: string) => status === "complete" || status === "completed";
  const completed = stages.filter((stage) => isDone(stage.status)).length;
  const activeStage = stages.find((stage) => stage.status === "active");
  const operations = state.transaction?.operations ?? [];
  const stagingOperations = operations.filter((operation) => !operation.external && !["skip", "external", "delete_managed"].includes(operation.action ?? ""));
  const applyingOperations = operations.filter((operation) => !operation.external && operation.action !== "external");
  const stageMeasurement = (stageId: string) => {
    if (stageId === "staging" && stagingOperations.length > 0) {
      const complete = stagingOperations.filter((operation) => ["staged", "applying", "applied", "verified"].includes(operation.status)).length;
      const current = stagingOperations.find((operation) => !["staged", "applying", "applied", "verified"].includes(operation.status));
      return { complete, total: stagingOperations.length, current: current?.destination };
    }
    if (stageId === "apply" && applyingOperations.length > 0) {
      const complete = applyingOperations.filter((operation) => ["applied", "verified"].includes(operation.status)).length;
      const current = applyingOperations.find((operation) => !["applied", "verified"].includes(operation.status));
      return { complete, total: applyingOperations.length, current: current?.destination };
    }
    return undefined;
  };
  const activeMeasurement = activeStage ? stageMeasurement(activeStage.id) : undefined;
  const operationFraction = activeMeasurement ? activeMeasurement.complete / activeMeasurement.total : 0;
  const progress = state.transaction
    ? Math.min(100, Math.round(((completed + operationFraction) / stages.length) * 100))
    : state.installProgress;
  const progressLabel = activeMeasurement && activeStage?.id === "staging"
    ? `Preparing file ${activeMeasurement.complete} of ${activeMeasurement.total}`
    : activeMeasurement && activeStage?.id === "apply"
      ? `Installing file ${activeMeasurement.complete} of ${activeMeasurement.total}`
      : state.transaction
        ? `${progress}% complete`
        : "Starting setup…";
  const remainingTime = estimateRemainingTime(progress, state.transaction?.created_at, now);
  const fileProgressLabel = activeMeasurement
    ? `${activeMeasurement.complete.toLocaleString()} of ${activeMeasurement.total.toLocaleString()} files`
    : applyingOperations.length
      ? `${applyingOperations.filter((operation) => ["applied", "verified"].includes(operation.status)).length.toLocaleString()} of ${applyingOperations.length.toLocaleString()} files`
      : progressLabel;
  return <div className="center-column"><section className="panel install-panel"><div className="install-progress"><div className="install-progress-track"><Progress value={progress} label="Installation progress" valueText={`${fileProgressLabel}. ${progress}% complete. ${remainingTime}`} /><div className="install-progress-meta"><span className="install-progress-label">{fileProgressLabel}</span><span className="install-eta">{remainingTime}</span></div></div><span className="install-percent" aria-hidden="true">{progress}%</span></div><div className="timeline" aria-live="polite">{stages.map((stage) => { const done = isDone(stage.status); const active = !done && stage.status === "active"; const measurement = active ? stageMeasurement(stage.id) : undefined; const stagePercent = done ? 100 : measurement ? Math.round((measurement.complete / measurement.total) * 100) : 0; const valueText = done ? `${stageLabel(stage.id)} complete` : active && measurement ? `${stageLabel(stage.id)} ${stagePercent}% complete` : active ? `${stageLabel(stage.id)} in progress` : `${stageLabel(stage.id)} not started`; return <div key={stage.id} className={`timeline-row ${done ? "done" : active ? "active" : ""}`}><span className="timeline-icon" aria-hidden="true">{done ? "✓" : active ? "●" : ""}</span><div className="timeline-copy"><strong>{stageLabel(stage.id)}</strong>{active && measurement?.current && <small title={measurement.current}>{measurement.current}</small>}</div><span className="timeline-percent">{done ? "100%" : active && measurement ? `${stagePercent}%` : active ? "Working…" : "0%"}</span>{active && <div className="stage-progress"><Progress value={measurement ? stagePercent : undefined} label={`${stageLabel(stage.id)} progress`} valueText={valueText} /></div>}</div>; })}</div><details><summary>Show setup details</summary><p className="muted">The app keeps a protected record of each step so an interrupted setup can continue safely.</p></details></section></div>;
}

export function estimateRemainingTime(progress: number, startedAt?: string, now = Date.now()): string {
  if (progress >= 100) return "Complete";
  const started = startedAt ? Date.parse(startedAt) : Number.NaN;
  if (!Number.isFinite(started) || progress < 2 || now <= started) return "Calculating time remaining";
  const elapsedSeconds = (now - started) / 1000;
  const remainingSeconds = elapsedSeconds * ((100 - progress) / progress);
  if (!Number.isFinite(remainingSeconds) || remainingSeconds <= 0 || elapsedSeconds > 6 * 60 * 60) return "Calculating time remaining";
  if (remainingSeconds < 60) return "Less than a minute remaining";
  const minutes = Math.max(1, Math.round(remainingSeconds / 60));
  if (minutes < 60) return `About ${minutes} ${minutes === 1 ? "minute" : "minutes"} remaining`;
  const hours = Math.max(1, Math.round(minutes / 60));
  return `About ${hours} ${hours === 1 ? "hour" : "hours"} remaining`;
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
  const [openPending, setOpenPending] = useState(false);
  if (state.finished) {
    return <section className="panel completion-panel" role="status">
      <span className="ready-icon" aria-hidden="true">✓</span>
      <div><h2>Setup complete</h2><p>Your project files and selected workflows are ready.</p></div>
    </section>;
  }
  const report = state.readiness;
  const selectedProvider = state.aiProvider ?? "codex";
  const providerLabel = aiProviderLabel(selectedProvider, state.aiProfiles);
  const coreReady = report?.coreReady === true;
  const open = selectedProvider === "codex" && report?.openInCodex === true;
  const readinessPending = report === null;
  const portraitPipeline = state.portraitPipeline ?? DEFAULT_PORTRAIT_PIPELINE;
  const project = readinessRow(report, ["descriptor.project", "structure.core"]);
  const codex = readinessRow(report, ["codex.agents", "skills.core", "subagents.core", "codex.config"]);
  const mcpWiki = readinessRow(report, ["mcp.hoi4", "wiki.coverage"]);
  const mcpStatus = report?.checks.find((check) => check.id === "mcp.hoi4")?.status;
  const canCheckMcp = mcpStatus === "health_not_run";
  const gitHashes = readinessRow(report, ["git.project", "hashes.managed", "conflicts.resolved", "dependencies.core"]);
  const mesh = readinessRow(report, ["workflow.3d"]);
  const superEvents = readinessRow(report, ["workflow.super_events"]);
  const portraits = readinessRow(report, ["workflow.portraits"]);
  const portraitProviderLabel = portraitPipeline.provider === "cloud" ? "Comfy Cloud" : portraitPipeline.provider === "local" ? "Local ComfyUI" : portraitPipeline.provider === "runpod" ? "RunPod" : "Disabled";
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
    if (openPending) return;
    setOpenMessage(undefined);
    setOpenPending(true);
    try {
      const result = await openInCodex(state.identity.projectRoot);
      if (!result) {
        update({ transactionError: "Codex could not be opened. Check the Codex installation or open the project folder manually." });
        return;
      }
      update({ transactionError: undefined });
      setOpenMessage(result.opened ? "Codex opened successfully." : result.message);
    } finally {
      setOpenPending(false);
    }
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
          {selectedProvider === "codex" && <button type="button" className="button primary" disabled={!open || openPending} aria-busy={openPending || undefined} aria-describedby="open-in-codex-help" onClick={() => void handleOpen()}>{openPending ? "Opening Codex…" : openMessage === "Codex opened successfully." ? "Codex opened ✓" : "Open in Codex ↗"}</button>}
          {openMessage && <p className="ready-action-message" role="status">{openMessage}</p>}
        </div>
        <span id="open-in-codex-help" className="visually-hidden">
          {readinessPending ? "Readiness checks are still running." : open ? "Opens the project in Codex, or shows a manual folder-opening instruction if no verified opener is installed." : selectedProvider === "codex" ? "Resolve blocking checks before opening in Codex." : `The project is ready for ${providerLabel}; no Codex opener is offered for this provider.`}
        </span>
      </section>
      <div className="two-column">
        <section className="panel ready-options-panel">
          <div className="ready-check-list">
            <CheckRow label="Project and descriptors" status={project.status} tone={project.tone} />
            <CheckRow label={`${providerLabel} instructions and skills`} status={codex.status} tone={codex.tone} />
            <CheckRow label="MCP and offline wiki" status={mcpWiki.status} tone={mcpWiki.tone} />
            <CheckRow label="Git and managed files" status={gitHashes.status} tone={gitHashes.tone} />
          </div>
          {state.selectedComponents.includes("mcp.hoi4_agent_tools") && canCheckMcp && <button type="button" className="button secondary ready-panel-action" onClick={() => void runMcpCheck()}>Check integration</button>}
          {mcpCheck && <p className="ready-panel-note muted" role="status">{mcpCheck.status === "ready" ? "Integration checked" : "Integration needs review"}</p>}
        </section>
        <section className="panel ready-options-panel">
          <div className="ready-check-list">
            <CheckRow label="3D model workflow" status={mesh.status} tone={mesh.tone} />
            <CheckRow label="Super Events workflow" status={superEvents.status} tone={superEvents.tone} />
            <CheckRow label="Portrait production" status={portraitPipeline.enabled ? `${portraitProviderLabel}: ${portraits.status}` : "Disabled"} tone={portraitPipeline.enabled ? portraits.tone : "muted"} />
          </div>
          {state.flattenForChat && selectedProvider === "codex" && <div className="callout info chatgpt-ready-callout"><ExternalLink href="https://chatgpt.com"><strong>ChatGPT Chat sources prepared</strong><span>Start planning in ChatGPT Chat <span aria-hidden="true">↗</span></span></ExternalLink></div>}
          {coreReady && portraitPipeline.enabled && <div className="portrait-workflow-note"><ExternalLink href={PORTRAIT_REPOSITORY}>Portrait production source and setup guidance <span aria-hidden="true">↗</span></ExternalLink></div>}
          <details>
            <summary>Readiness details</summary>
            {report ? <div className="manifest-details">{report.checks.map((check) => <div key={check.id}><strong>{check.label}</strong><span>{check.status === "configured" || check.status === "pass" ? "Ready" : check.status === "unsupported_platform" ? "Not available on this computer" : check.status === "planned_unavailable" ? "Optional tool not installed" : check.status === "not_selected" ? "Not selected" : check.blocking ? "Action needed" : "Optional setup incomplete"}</span></div>)}</div> : <p className="muted">Details appear after the checks finish.</p>}
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
  if (checks.some((check) => check.status === "planned_unavailable")) return { status: "Optional", tone: "muted" };
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
  const portraitInstalled = state.installedPortraitState !== undefined
    && state.installedPortraitState !== "not_selected"
    && state.installedPortraitState !== "unsupported_platform";
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
  const choosePortraitProvider = (provider: PortraitProviderId) => {
    const portrait = state.portraitPipeline ?? DEFAULT_PORTRAIT_PIPELINE;
    const enabled = provider !== "disabled";
    const providerStatus: PortraitProviderStatus = provider === "disabled"
      ? "not_selected"
      : provider === "cloud"
        ? "needs_authorization"
        : provider === "local"
          ? "needs_hardware"
          : "needs_runpod";
    const selectedComponents = enabled
      ? Array.from(new Set([...state.selectedComponents.filter((id) => !id.startsWith("workflow.portraits")), ...portraitComponentIdsFor(provider)]))
      : state.selectedComponents.filter((id) => !id.startsWith("workflow.portraits"));
    update({ portraitPipeline: { ...portrait, enabled, provider, providerStatus, mcpRegistered: provider === "cloud" && enabled }, selectedComponents });
  };
  /*
  const portraitLabel = state.portraitPipeline.provider === "cloud" ? "Comfy Cloud" : state.portraitPipeline.provider === "local" ? "Local ComfyUI" : state.portraitPipeline.provider === "runpod" ? "RunPod" : "Disabled";
  return <section className="panel maintenance-workflow-panel"><PanelTitle title="Optional workflows" /><ToggleRow label="3D models workflow" detail={meshInstalled ? "Already part of this project" : "Add it during the next update or repair"} checked={meshInstalled || state.meshSelected} disabled={meshInstalled} onChange={(checked) => update({ meshSelected: checked })} /><ToggleRow label="Super Events workflow" detail={superEventsInstalled ? "Already part of this project" : "Add it during the next update or repair"} checked={superEventsInstalled || state.superEventsSelected} disabled={superEventsInstalled} onChange={(checked) => update({ superEventsSelected: checked })} /><ToggleRow label="ComfyUI portrait production" detail={portraitInstalled ? `Installed with ${portraitLabel}; change the provider below or disable it for the next update` : "Add it during the next update or repair"} checked={state.portraitPipeline.enabled} onChange={(checked) => choosePortraitProvider(checked ? "cloud" : "disabled")} />{state.portraitPipeline.enabled && <div className="workflow-provider-panel"><span className="field-label" id="maintenance-portrait-provider-label">Portrait provider</span><div className="radio-row" role="radiogroup" aria-labelledby="maintenance-portrait-provider-label">{(["cloud", "local", "runpod"] as PortraitProviderId[]).map((provider) => <label className="radio-option" key={provider}><input type="radio" name="maintenance-portrait-provider" checked={state.portraitPipeline.provider === provider} onChange={() => choosePortraitProvider(provider)} /><span><strong>{provider === "cloud" ? "Comfy Cloud" : provider === "local" ? "Local ComfyUI" : "RunPod"}</strong><small>{provider === "cloud" ? "Official MCP route" : provider === "local" ? "Loopback ComfyUI route" : "Existing pod and browser route"}</small></span></label>)}</div>}{state.portraitPipeline.provider === "local" && state.portraitPipeline.enabled && <div className="workflow-provider-fields"><Field label="Local ComfyUI folder" value={state.portraitPipeline.localComfyuiRoot} onChange={(value) => update({ portraitPipeline: { ...state.portraitPipeline, localComfyuiRoot: value } })} placeholder="Optional configured folder" mono /><Field label="Local server" value={state.portraitPipeline.localServerUrl} onChange={(value) => update({ portraitPipeline: { ...state.portraitPipeline, localServerUrl: value } })} mono /></div>}{state.portraitPipeline.provider === "runpod" && state.portraitPipeline.enabled && <div className="workflow-provider-fields"><Field label="RunPod URL" value={state.portraitPipeline.runpodUrl} onChange={(value) => update({ portraitPipeline: { ...state.portraitPipeline, runpodUrl: value } })} placeholder="Optional HTTPS URL" mono /></div>}<div className="workflow-provider-summary"><Status label={state.portraitPipeline.providerStatus === "ready" ? "Provider ready" : "Setup deferred"} tone={state.portraitPipeline.providerStatus === "ready" ? "pass" : "review"} /><span>Changes are persisted by the next reviewed update or repair plan.</span></div></div>}{state.meshSelected && (!meshInstalled || keyNeedsConfiguration) && <div className="maintenance-key"><p className="muted">A Meshy key is optional for the file repair. Store it now if you want the workflow ready to run.</p><label className="field-label" htmlFor="maintenance-meshy-key">Meshy API key</label><div className="input-with-action"><input id="maintenance-meshy-key" className="text-input" type="password" autoComplete="off" value={state.meshKeyDraft} onChange={(event) => update({ meshKeyDraft: event.target.value })} /><button type="button" className="input-action" onClick={() => void storeKey()} disabled={!state.meshKeyDraft}>Store</button></div><span className="muted">Stored in the operating-system credential vault.</span></div>}</section>;
}

  */
  return <PortraitMaintenanceSetup state={state} update={update} portraitInstalled={portraitInstalled} choosePortraitProvider={choosePortraitProvider} meshInstalled={meshInstalled} superEventsInstalled={superEventsInstalled} keyNeedsConfiguration={keyNeedsConfiguration} storeKey={storeKey} />;
}

function PortraitMaintenanceSetup({
  state,
  update,
  portraitInstalled,
  choosePortraitProvider,
  meshInstalled,
  superEventsInstalled,
  keyNeedsConfiguration,
  storeKey,
}: {
  state: WizardState;
  update: (patch: Partial<WizardState>) => void;
  portraitInstalled: boolean;
  choosePortraitProvider: (provider: PortraitProviderId) => void;
  meshInstalled: boolean;
  superEventsInstalled: boolean;
  keyNeedsConfiguration: boolean;
  storeKey: () => Promise<void>;
}) {
  const portrait = state.portraitPipeline ?? DEFAULT_PORTRAIT_PIPELINE;
  const portraitLabel = portrait.provider === "cloud" ? "Comfy Cloud" : portrait.provider === "local" ? "Local ComfyUI" : portrait.provider === "runpod" ? "RunPod" : "Disabled";
  return <section className="panel maintenance-workflow-panel">
    <PanelTitle title="Optional workflows" />
    <ToggleRow label="3D models workflow" detail={meshInstalled ? "Already part of this project" : "Add it during the next update or repair"} checked={meshInstalled || state.meshSelected} disabled={meshInstalled} onChange={(checked) => update({ meshSelected: checked })} />
    <ToggleRow label="Super Events workflow" detail={superEventsInstalled ? "Already part of this project" : "Add it during the next update or repair"} checked={superEventsInstalled || state.superEventsSelected} disabled={superEventsInstalled} onChange={(checked) => update({ superEventsSelected: checked })} />
    <ToggleRow label="ComfyUI portrait production" detail={portraitInstalled ? `Installed with ${portraitLabel}; change the provider below or disable it for the next update` : "Add it during the next update or repair"} checked={portrait.enabled} onChange={(checked) => choosePortraitProvider(checked ? "cloud" : "disabled")} />
    {portrait.enabled && <div className="workflow-provider-panel">
      <p className="workflow-resource-note">{PORTRAIT_RESOURCE_REQUIREMENT}</p>
      <span className="field-label" id="maintenance-portrait-provider-label">Portrait provider</span>
      <div className="radio-row" role="radiogroup" aria-labelledby="maintenance-portrait-provider-label">
        {(["cloud", "local", "runpod"] as PortraitProviderId[]).map((provider) => <label className="radio-option" key={provider}>
          <input type="radio" name="maintenance-portrait-provider" checked={portrait.provider === provider} onChange={() => choosePortraitProvider(provider)} />
          <span><strong>{provider === "cloud" ? "Comfy Cloud" : provider === "local" ? "Local ComfyUI" : "RunPod"}</strong><small>{provider === "cloud" ? "Official MCP route" : provider === "local" ? "Loopback ComfyUI route" : "Existing pod and browser route"}</small></span>
        </label>)}
      </div>
      {portrait.provider === "local" && <div className="workflow-provider-fields">
        <Field label="Local ComfyUI folder" value={portrait.localComfyuiRoot} onChange={(value) => update({ portraitPipeline: { ...portrait, localComfyuiRoot: value } })} placeholder="Optional configured folder" mono />
        <Field label="Local server" value={portrait.localServerUrl} onChange={(value) => update({ portraitPipeline: { ...portrait, localServerUrl: value } })} mono />
      </div>}
      {portrait.provider === "runpod" && <div className="workflow-provider-fields"><Field label="RunPod URL" value={portrait.runpodUrl} onChange={(value) => update({ portraitPipeline: { ...portrait, runpodUrl: value } })} placeholder="Optional HTTPS URL" mono /></div>}
      <div className="workflow-provider-summary"><Status label={portrait.providerStatus === "ready" ? "Provider ready" : "Setup deferred"} tone={portrait.providerStatus === "ready" ? "pass" : "review"} /><span>Changes are persisted by the next reviewed update or repair plan.</span></div>
    </div>}
    {state.meshSelected && (!meshInstalled || keyNeedsConfiguration) && <div className="maintenance-key"><p className="muted">A Meshy key is optional for the file repair. Store it now if you want the workflow ready to run.</p><label className="field-label" htmlFor="maintenance-meshy-key">Meshy API key</label><div className="input-with-action"><input id="maintenance-meshy-key" className="text-input" type="password" autoComplete="off" value={state.meshKeyDraft} onChange={(event) => update({ meshKeyDraft: event.target.value })} /><button type="button" className="input-action" onClick={() => void storeKey()} disabled={!state.meshKeyDraft}>Store</button></div><span className="muted">Stored in the operating-system credential vault.</span></div>}
  </section>;
}

export function Update({ state, update, findings, setFindings, onMaintenance, onStartMaintenance, onReanalyze, onPackageChatSources, pending = false }: { state: WizardState; update: (patch: Partial<WizardState>) => void; findings: ScanFinding[]; setFindings: Dispatch<SetStateAction<ScanFinding[]>>; onMaintenance: (screen: "update" | "conflict" | "recovery") => void; onStartMaintenance: (mode: "update" | "repair" | "reinstall" | "remove") => void; onReanalyze: () => Promise<boolean>; onPackageChatSources?: () => Promise<void>; pending?: boolean }) {
  const plan = state.plan;
  const optional3d = state.meshSelected ? state.meshKeyStatus === "present" ? "Stored; health check pending" : "Selected; key not stored" : "Not selected";
  const superEvents = state.superEventsSelected ? state.installedSuperEventsState === "not_selected" ? "Selected for the next change" : "Installed" : "Not selected";
  const portraitPipeline = state.portraitPipeline ?? DEFAULT_PORTRAIT_PIPELINE;
  const portraits = portraitPipeline.enabled ? `${portraitPipeline.provider} · ${portraitPipeline.providerStatus}` : "Disabled";
  const providerLabel = aiProviderLabel(state.aiProvider, state.aiProfiles);
  const reanalysisLabel = state.maintenanceEvidenceReady ? state.maintenanceCodexAnalysisRecord ? "Run again" : `Run ${providerLabel} reanalysis` : "Prepare read-only evidence";
  return <div className="stack"><div className="action-grid"><ActionTile title="Check for updates" detail="Compare this project with a newer setup." onClick={() => onStartMaintenance("update")} /><ActionTile title="Repair installation" detail="Restore missing or damaged setup files." onClick={() => onStartMaintenance("repair")} /><ActionTile title="Remove components" detail="Review the files before removing app-managed setup." onClick={() => onStartMaintenance("remove")} /><ActionTile title="Recover interrupted setup" detail="Continue or undo an interrupted change." onClick={() => onMaintenance("recovery")} />{state.chatSourcesAvailable && onPackageChatSources && <ActionTile title="Package ChatGPT project sources" detail="Choose detected files and download a source ZIP." onClick={() => void onPackageChatSources()} />}</div><MaintenanceWorkflowOptions state={state} update={update} /><section className="panel"><PanelTitle title={`${providerLabel} review`} /><p className="muted">Review the project before updating its setup.</p><button type="button" className="button secondary" onClick={() => void onReanalyze()}>{reanalysisLabel}</button>{state.maintenanceEvidenceReady && <details open><summary>{findings.filter((finding) => finding.status !== "rejected").length} approved findings</summary><div className="manifest-details">{findings.filter((finding) => finding.status !== "rejected").map((finding) => <div key={finding.id}><strong>{finding.id}</strong><span>{finding.evidencePath ?? "approved finding reference"}</span><small>{finding.evidenceExcerpt ?? finding.value}</small><button type="button" className="text-button" aria-pressed="true" onClick={() => setFindings((current) => current.map((candidate) => candidate.id === finding.id ? { ...candidate, status: "rejected" } : candidate))}>Exclude</button></div>)}</div></details>}{state.maintenanceCodexAnalysisRecord && <p className="muted" role="status">Review returned. Confirm the {providerLabel} suggestions before checking for updates.</p>}</section><section className="panel"><PanelTitle title="Installed state" /><CheckRow label="Core setup" status={plan ? `${plan.operations.length} planned changes` : "No plan loaded"} tone={plan ? "info" : "muted"} /><CheckRow label="Optional 3D workflow" status={optional3d} tone={state.meshSelected ? "review" : "muted"} /><CheckRow label="Super Events workflow" status={superEvents} tone={state.superEventsSelected ? "review" : "muted"} /><CheckRow label="Portrait production" status={portraits} tone={portraitPipeline.enabled ? "review" : "muted"} /><CheckRow label="Modified files" status={plan ? String(plan.conflicts.length) : "Not evaluated"} tone={plan?.conflicts.length ? "review" : "muted"} />{plan && <details open><summary>Reviewed changes</summary><p className="muted">Modified files remain visible until resolved.</p></details>}</section></div>;
}

export function ChatSources({ state, update, onPickFolder }: { state: WizardState; update: (patch: Partial<WizardState>) => void; onPickFolder: () => Promise<void> }) {
  const preview = state.chatSourcesPreview;
  const selectedIds = state.chatSourcesSelectedIds ?? preview?.files.filter((file) => file.selectedByDefault).map((file) => file.id) ?? [];
  const selected = new Set(selectedIds);
  const toggle = (file: NonNullable<ChatSourcesPreview["files"]>[number], checked: boolean) => {
    if (file.required) return;
    const next = checked
      ? Array.from(new Set([...selectedIds, file.id]))
      : selectedIds.filter((id) => id !== file.id);
    update({ chatSourcesSelectedIds: next, chatSourcesResult: undefined, transactionError: undefined });
  };
  if (!preview) {
    return <section className="panel chat-sources-empty"><PanelTitle title="Preparing source list" /><p className="muted">The detected project files are being checked.</p></section>;
  }
  return <div className="stack chat-sources-page">
    {!preview.eligible && <div className="callout review" role="status">{preview.message ?? "No ChatGPT project source was found in this project."}</div>}
    <section className="panel form-panel chat-sources-destination">
      <PanelTitle title="Download location" />
      <p className="muted">The package is created outside the mod project. It defaults to the Downloads folder.</p>
      <label className="field" htmlFor="chat-sources-destination"><span className="field-label">Download folder</span><div className="input-with-action"><input id="chat-sources-destination" className="text-input" value={state.chatSourcesDestination ?? preview.destinationDirectory} onChange={(event) => update({ chatSourcesDestination: event.target.value, chatSourcesResult: undefined, transactionError: undefined })} /><button type="button" className="input-action" onClick={() => void onPickFolder()}>Choose folder</button></div></label>
      <div className="chat-sources-output"><span>Package name</span><code>{preview.archiveName}</code></div>
    </section>
    <section className="panel chat-sources-files">
      <PanelTitle title="Files included" />
      <p className="muted">{selectedIds.length} of {preview.files.length} files selected. Detected guidance, skills, and subagents are included by default; root Markdown files are optional.</p>
      <div className="chat-source-file-list" role="group" aria-label="ChatGPT project source files">
        {preview.files.map((file) => <label className="chat-source-file" key={file.id}>
          <input type="checkbox" checked={selected.has(file.id)} disabled={file.required} onChange={(event) => toggle(file, event.target.checked)} />
          <span><strong>{file.archivePath}</strong><small>From {file.sourcePath} · {file.required ? "Included by default" : "Optional root Markdown"}</small></span>
          <span className="size">{formatScanBytes(file.size)}</span>
        </label>)}
      </div>
    </section>
    {state.chatSourcesResult && <section className="callout pass chat-sources-result" role="status"><strong>Package downloaded</strong><span>{state.chatSourcesResult.includedFiles.length} files were saved to <code>{state.chatSourcesResult.archivePath}</code>.</span></section>}
  </div>;
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

export function recoveryProgress(journal: TransactionJournal | null | undefined) {
  if (!journal) return undefined;
  const operations = journal.operations.filter((operation) => !["skip", "external"].includes(operation.action ?? ""));
  if (operations.length === 0) return undefined;
  const saving = journal.transaction_kind === "rollback" && journal.state === "preflight";
  const complete = saving
    ? operations.filter((operation) => operation.backup_path || operation.after_exists !== null && operation.after_exists !== undefined).length
    : operations.filter((operation) => operation.status === "rolled_back").length;
  const current = saving
    ? operations.find((operation) => !operation.backup_path && (operation.after_exists === null || operation.after_exists === undefined))
    : operations.find((operation) => operation.status !== "rolled_back");
  return {
    complete,
    total: operations.length,
    percent: Math.round((complete / operations.length) * 100),
    current: current?.destination,
    label: saving ? `Saving file ${complete} of ${operations.length}` : `Restoring file ${complete} of ${operations.length}`,
  };
}

function recoveryStagePosition(journal: TransactionJournal) {
  const stages = journal.stages ?? [];
  if (stages.length === 0) return undefined;
  const errorStage = journal.error?.stage;
  let index = errorStage ? stages.findIndex((stage) => stage.id === errorStage) : -1;
  if (index < 0) index = stages.findIndex((stage) => stage.status === "active");
  if (index < 0) {
    const lastComplete = stages.reduce((last, stage, stageIndex) => stage.status === "complete" || stage.status === "completed" ? stageIndex : last, -1);
    index = Math.min(stages.length - 1, lastComplete + 1);
  }
  return { index, total: stages.length, id: stages[index]?.id ?? errorStage ?? journal.state };
}

function readableRecoveryStage(value: string) {
  const words = value.replace(/[-_]+/g, " ").trim();
  return words ? `${words.charAt(0).toUpperCase()}${words.slice(1)}` : "Not recorded";
}

function readableRecoveryAction(value: string) {
  return ({
    resume: "Continue setup",
    rollback: "Undo changes",
    discard_staging: "Discard prepared files",
    inspect: "Inspect project",
  } as Record<string, string>)[value] ?? "Review recovery options";
}

function safeRecoveryFailureMessage(value: string) {
  const redacted = value
    .replace(/(bearer\s+)[A-Za-z0-9._~+/=-]+/gi, "$1[REDACTED]")
    .replace(/(["']?(?:authorization|api[_-]?key|client[_-]?secret|private[_-]?key|access[_-]?token|refresh[_-]?token|device[_-]?code|user[_-]?code|login[_-]?token)["']?\s*:\s*["'])[^"']+(["'])/gi, "$1[REDACTED]$2")
    .replace(/((?:authorization|api[_-]?key|client[_-]?secret|private[_-]?key|access[_-]?token|refresh[_-]?token|device[_-]?code|user[_-]?code|login[_-]?token)\s*[:=]\s*)[^\s,;&]+/gi, "$1[REDACTED]")
    .replace(/([?&](?:code|token|access_token|refresh_token|device_code|user_code)=)[^&#\s]+/gi, "$1[REDACTED]")
    .replace(/\b(?:msy_|sk-ant-|sk-(?:proj-)?|AIza|xai-)[A-Za-z0-9_-]{8,}\b/g, "[REDACTED]");
  const encoder = new TextEncoder();
  if (encoder.encode(redacted).length <= 2048) return redacted;
  const characters: string[] = [];
  let bytes = encoder.encode("...").length;
  for (const character of redacted) {
    const characterBytes = encoder.encode(character).length;
    if (bytes + characterBytes > 2048) break;
    characters.push(character);
    bytes += characterBytes;
  }
  return `${characters.join("")}...`;
}

function Recovery({ state, update, onPickProjectFolder, onStartMaintenance, pending }: { state: WizardState; update: (patch: Partial<WizardState>) => void; onPickProjectFolder: () => Promise<FolderSelection | null>; onStartMaintenance: (mode: "update" | "repair" | "reinstall" | "remove") => Promise<void> | void; pending: boolean }) {
  const transaction = state.transaction;
  const [progressJournal, setProgressJournal] = useState<TransactionJournal | null>(null);
  useEffect(() => {
    if (!pending || !transaction || !state.identity.projectRoot) {
      setProgressJournal(null);
      return;
    }
    let active = true;
    let timer: number | undefined;
    const refresh = async () => {
      const parent = await readTransactionJournal(state.identity.projectRoot, transaction.transaction_id);
      if (!active) return;
      const progressId = parent?.rollback_transaction_id ?? transaction.rollback_transaction_id;
      const progress = progressId
        ? await readTransactionJournal(state.identity.projectRoot, progressId)
        : parent;
      if (!active) return;
      if (progress) setProgressJournal(progress);
      timer = window.setTimeout(() => void refresh(), 500);
    };
    void refresh();
    return () => {
      active = false;
      if (timer !== undefined) window.clearTimeout(timer);
    };
  }, [pending, state.identity.projectRoot, transaction?.transaction_id, transaction?.rollback_transaction_id]);
  if (!transaction) {
    const choose = async () => {
      const selected = await onPickProjectFolder();
      if (selected?.path) update({ identity: { ...state.identity, projectRoot: selected.path }, transactionError: undefined });
      else if (selected?.error) update({ transactionError: `The selected folder could not be used: ${selected.error}` });
    };
    return <div className="stack narrow"><section className="panel"><PanelTitle title="Recover setup" /><p className="muted">Choose an installed project to continue an interrupted setup or remove files added by the app.</p><div className="button-row"><button type="button" className="button secondary" onClick={() => void choose()}>Choose another project</button><button type="button" className="button secondary" disabled={!state.identity.projectRoot.trim()} onClick={() => void onStartMaintenance("remove")}>Remove managed components</button></div></section></div>;
  }
  if (pending) {
    const progress = recoveryProgress(progressJournal);
    const label = state.recoveryChoice === "rollback" ? progress?.label ?? "Starting undo…" : state.recoveryChoice === "discard" ? "Discarding prepared files…" : "Checking the prepared files…";
    return <div className="stack narrow"><section className="panel recovery-progress" role="status" aria-live="polite" aria-busy="true"><strong>{state.recoveryChoice === "rollback" ? "Undoing changes" : state.recoveryChoice === "discard" ? "Discarding prepared files" : "Continuing setup"}</strong><span>{label}</span><Progress value={progress?.percent} label="Recovery progress" valueText={progress ? `${progress.percent}% complete. ${label}` : label} />{progress?.current && <small className="mono" title={progress.current}>{progress.current}</small>}</section></div>;
  }
  const options: Array<{ id: RecoveryChoice; title: string; detail: string; allowed: boolean }> = [
    { id: "resume", title: "Continue setup", detail: "Check the prepared files again and continue where setup stopped.", allowed: transaction.recovery.resume_allowed },
    { id: "rollback", title: "Undo changes", detail: "Return the project to the state it had before this setup began.", allowed: transaction.recovery.rollback_allowed },
    { id: "discard", title: "Discard prepared files", detail: "Remove temporary setup files without changing the project.", allowed: transaction.recovery.discard_staging_allowed },
  ];
  const availableOptions = options.filter((item) => item.allowed);
  const checkpoint = transaction.state === "finalizing"
    ? "The files are installed. Continue setup to finish the final checks."
    : transaction.state === "rolling_back"
      ? "The app is restoring the previous project state. Choose Undo changes to continue or inspect the project."
      : transaction.recovery.project_apply_started
        ? "Some project files were already changed, so continuing automatically is unavailable."
    : "Setup stopped before any project files were changed.";
  const stages = transaction.stages ?? [];
  const stagedFiles = (transaction.operations ?? []).filter((operation) => ["staged", "applying", "applied", "verified"].includes(operation.status)).length;
  const checkpointFacts = [
    stages.some((stage) => stage.id === "backup" && ["complete", "completed"].includes(stage.status)) ? "Backup verified" : undefined,
    transaction.recovery.project_apply_started ? "Project apply started" : "Project apply had not started",
    stagedFiles > 0 ? `${stagedFiles.toLocaleString("en-US")} ${stagedFiles === 1 ? "file" : "files"} staged` : undefined,
  ].filter((fact): fact is string => Boolean(fact));
  const stage = recoveryStagePosition(transaction);
  return <div className="stack"><section className="panel recovery-overview" aria-labelledby="recovery-checkpoint-title"><div className="recovery-state" role="status"><strong>{checkpoint}</strong><span>{checkpointFacts.join(" \u00b7 ")}</span></div><div className="recovery-checkpoint"><div><strong id="recovery-checkpoint-title">Last checkpoint</strong><code>{transaction.last_checkpoint || "Not recorded"}</code></div><details><summary>Details</summary><div className="recovery-checkpoint-details"><span><small>Current stage</small><strong>{readableRecoveryStage(stage?.id ?? transaction.error?.stage ?? transaction.state)}</strong></span><span><small>Recommended action</small><strong>{readableRecoveryAction(transaction.recovery.recommended_action)}</strong></span>{transaction.error?.message && <span><small>Why setup stopped</small><strong>{safeRecoveryFailureMessage(transaction.error.message)}</strong></span>}</div></details></div></section><div className={`recovery-grid recovery-grid-${availableOptions.length}`}>{availableOptions.map((item) => <button type="button" key={item.id} className={`recovery-card ${state.recoveryChoice === item.id ? "selected" : ""}`} aria-pressed={state.recoveryChoice === item.id} onClick={() => update({ recoveryChoice: item.id })}><span className="choice-radio" aria-hidden="true" /><strong>{item.title}</strong><p>{item.detail}</p></button>)}</div></div>;
}

function Field({ label, value, onChange, placeholder, action, onAction, mono }: { label: string; value: string; onChange: (value: string) => void; placeholder?: string; action?: string; onAction?: () => void; mono?: boolean }) {
  const id = `field-${label.toLowerCase().replace(/[^a-z0-9]+/g, "-")}`;
  return <div className="field"><label className="field-label" htmlFor={id}>{label}</label><span className="input-with-action"><input id={id} className={`text-input ${mono ? "mono" : ""}`} value={value} placeholder={placeholder} onChange={(event) => onChange(event.target.value)} />{action && onAction && <button type="button" className="input-action" aria-label={`${action} ${label.toLowerCase()}`} onClick={onAction}>{action}</button>}</span></div>;
}
function PanelTitle({ title }: { title: string }) { return <h2 className="panel-title">{title}</h2>; }
function Status({ label, tone }: { label: string; tone: StatusTone }) { const symbol = tone === "pass" ? "✓" : tone === "block" || tone === "review" ? "!" : tone === "muted" ? "–" : ""; return <span className={`status status-${tone}`}>{symbol && <span className="status-symbol" aria-hidden="true">{symbol}</span>}{label}</span>; }
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
