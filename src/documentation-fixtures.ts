import type { InstallationPlan, ManifestComponentPreview, ReadinessReport, WizardState } from "./types";

export const DOCUMENTATION_SCENARIOS = [
  "welcome",
  "provider",
  "existing",
  "description",
  "identity",
  "components",
  "workflows",
  "mcp",
  "git",
  "dry-run",
  "ready",
  "maintenance",
] as const;

export type DocumentationScenario = typeof DOCUMENTATION_SCENARIOS[number];

function selectedScenario(): DocumentationScenario | undefined {
  if (!import.meta.env.DEV || typeof window === "undefined") return undefined;
  const value = new URLSearchParams(window.location.search).get("screenshot");
  return DOCUMENTATION_SCENARIOS.find((scenario) => scenario === value);
}

export function isDocumentationScreenshot(): boolean {
  return selectedScenario() !== undefined;
}

function component(
  id: string,
  displayName: string,
  description: string,
  category: string,
  fileCount: number,
  optional = false,
): ManifestComponentPreview {
  return {
    id,
    display_name: displayName,
    description,
    category,
    optional,
    platforms: ["all"],
    source: { kind: "repository_tree", path: id },
    destination: { path: ".", ownership: "managed" },
    dependencies: [],
    required_tools: [],
    environment: [],
    expected_files: Array.from({ length: fileCount }, (_, index) => ({
      path: `${id}/file-${index + 1}.md`,
      size: 2048 + index * 256,
    })),
    capabilities: [],
    validation: [],
    update: { strategy: "replace_unmodified", remove_obsolete: true, preserve_local_additions: true },
  };
}

const manifestComponents = [
  component("core.agents", "Project instructions", "AGENTS.md adapted to the mod", "instructions", 1),
  component("core.skills", "HOI4 skills", "Current workflows for scripting, art, research, and review", "skills", 18),
  component("core.subagents", "Focused subagents", "Narrow helpers for common HOI4 tasks", "subagents", 14),
  component("codex.config", "Codex and MCP configuration", "Project configuration for Codex", "configuration", 2),
  component("mcp.hoi4_agent_tools", "HOI4 Agent Tools MCP", "HOI4-aware validation tools", "mcp", 2, true),
  component("wiki.snapshot", "Offline Paradox wiki", "A local reference installed under paradox_wiki/", "documentation", 942),
  component("workflow.3d", "3D models workflow", "Optional Meshy and Blender workflow", "workflow", 12, true),
  component("workflow.super_events", "Super Events workflow", "Reusable event popup, templates, examples, and assets", "workflow", 23, true),
  component("workflow.portraits.core", "Portrait production contract", "Provider-neutral source, prompt, archive, validation, and runtime handoff contract.", "skill", 1, true),
  component("workflow.portraits.cloud", "Comfy Cloud portrait production", "Selected provider route", "workflow", 1, true),
  component("workflow.portraits.local", "Loopback ComfyUI portrait production", "Selected provider route", "workflow", 1, true),
  component("workflow.portraits.runpod", "RunPod ComfyUI portrait production", "Selected provider route", "workflow", 1, true),
  component("workflow.portraits.subagent", "Portrait production subagent", "Bounded provider execution and portrait QA", "subagent", 1, true),
  component("workflow.portraits.config", "Portrait provider configuration", "Non-secret provider and pinned upstream configuration", "configuration", 1, true),
  component("workflow.portraits.docs", "Portrait upstream lock", "Canonical portrait repository and workflow hash evidence", "documentation", 1, true),
];
const portraitComponentIds = [
  "workflow.portraits.core",
  "workflow.portraits.runpod",
  "workflow.portraits.subagent",
  "workflow.portraits.config",
  "workflow.portraits.docs",
];
const allPortraitComponentIds = [
  "workflow.portraits.core",
  "workflow.portraits.cloud",
  "workflow.portraits.local",
  "workflow.portraits.runpod",
  "workflow.portraits.subagent",
  "workflow.portraits.config",
  "workflow.portraits.docs",
];

const source = {
  repository: "klimPaskov/Agentic-HOI4-Modding",
  mode: "latest" as const,
  resolved_revision: "documentation-preview",
  manifest_sha256: "documentation-preview",
};

const manifest = {
  schema_version: "1.0.0",
  manifest_id: "agentic-hoi4-modding",
  source,
  repository: {
    provider: "github",
    owner: "klimPaskov",
    name: "Agentic-HOI4-Modding",
    default_branch: "main",
    web_url: "https://github.com/klimPaskov/Agentic-HOI4-Modding",
  },
  components: manifestComponents,
};

const operations: InstallationPlan["operations"] = [
  ["agents", "core.agents", "AGENTS.md"],
  ["skills", "core.skills", ".agents/skills/"],
  ["subagents", "core.subagents", ".codex/agents/"],
  ["config", "codex.config", ".codex/config.toml"],
  ["wiki", "wiki.snapshot", "paradox_wiki/"],
  ["descriptor", "core.agents", "descriptor.mod"],
  ["launcher", "core.agents", "atlantis_rising.mod"],
  ["thumbnail", "core.agents", "thumbnail.png"],
].map(([id, componentId, destination]) => ({
  id,
  component_id: componentId,
  action: "create" as const,
  destination,
  local_state: "absent" as const,
  rollback: "remove_created" as const,
}));

const plan: InstallationPlan = {
  schema_version: "1.0.0",
  plan_id: "documentation-preview",
  project_id: "atlantis_rising",
  source,
  ai_provider: "codex",
  ai_model: "default",
  flatten_chat_sources: true,
  selected_components: ["core.agents", "core.skills", "core.subagents", "codex.config", "mcp.hoi4_agent_tools", "wiki.snapshot", "workflow.super_events", ...portraitComponentIds],
  wiki_required_pages: ["Hearts of Iron 4 Wiki", "Modding"],
  generated_artifacts: [
    { component_id: "chat.flattened", destination: "chatgpt_project_sources/AGENTS.md", content: "Project instructions", expected_sha256: "documentation-preview" },
    { component_id: "chat.flattened", destination: "chatgpt_project_sources/README.md", content: "Project overview", expected_sha256: "documentation-preview" },
    { component_id: "chat.flattened", destination: "chatgpt_project_sources/hoi4-events.md", content: "Events skill", expected_sha256: "documentation-preview" },
  ],
  git_setup: { mode: "initialize", branch: "main", initial_commit: true, remote_name: "origin", push_approved: false },
    optional_workflows: { "workflow.3d": "not_selected", "workflow.super_events": "selected_pending", "workflow.portraits": "selected_pending" },
    portrait_pipeline: {
      enabled: true,
      provider: "runpod",
      provider_status: "needs_runpod",
      workflow_repository: "https://github.com/klimPaskov/comfyui-hoi4-portraits",
      workflow_branch: "codex/portrait-pipeline",
      workflow_commit: "b47222a77f2f6454704530865aa1441fad48bdd3",
      preferred_workflow: "source",
      runpod_workspace: "/workspace/comfyui-hoi4-portraits",
      mcp_registered: false,
    },
  operations,
  conflicts: [],
  external_actions: [],
  transaction: {
    stages: ["preflight", "repository source resolution", "selective download", "checksum verification", "dry-run review", "backup", "staging", "validation", "apply", "post-install checks", "readiness report", "rollback record"],
    backup_root: ".hoi4-mod-setup/backups/documentation-preview",
    staging_root: ".hoi4-mod-setup/staging/documentation-preview",
    directories: ["common", "events", "gfx", "interface", "localisation/english"],
    project_root_mode: "create_leaf",
    project_root_parent: "C:\\Users\\Player\\Documents\\Paradox Interactive\\Hearts of Iron IV\\mod",
    project_root_leaf: "atlantis_rising",
  },
  approvals: { dry_run_reviewed: true, external_actions_reviewed: true, git_remote_approved: false, push_approved: false },
};

const readiness: ReadinessReport = {
  openInCodex: true,
  coreReady: true,
  blockingCheckIds: [],
  checks: [
    ["descriptor.project", "Project descriptors", "pass"],
    ["structure.core", "Project folders", "pass"],
    ["codex.agents", "Project instructions", "pass"],
    ["skills.core", "Skills", "pass"],
    ["subagents.core", "Subagents", "pass"],
    ["codex.config", "Codex configuration", "pass"],
    ["mcp.hoi4", "HOI4 Agent Tools", "pass"],
    ["wiki.coverage", "Offline wiki", "pass"],
    ["git.project", "Git", "pass"],
    ["hashes.managed", "Installed files", "pass"],
    ["conflicts.resolved", "File conflicts", "pass"],
    ["dependencies.core", "Required tools", "pass"],
    ["workflow.3d", "3D model workflow", "not_selected"],
    ["workflow.super_events", "Super Events workflow", "pass"],
    ["workflow.portraits", "Portrait production", "warn"],
  ].map(([id, label, status]) => ({ id, label, status, blocking: false, message: status === "pass" ? "Ready" : status === "warn" ? "RunPod setup still required" : "Not selected" })),
  codex: { authenticated_during_setup: true, analysis_status: "confirmed", confirmed_field_count: 9 },
};

export function documentationFixture(base: WizardState): WizardState {
  const scenario = selectedScenario();
  if (!scenario) return base;

  const identity = {
    ...base.identity,
    displayName: "Atlantis Rising",
    projectId: "atlantis_rising",
    projectRoot: "C:\\Users\\Player\\Documents\\Paradox Interactive\\Hearts of Iron IV\\mod\\atlantis_rising",
    launcherDescriptorPath: "C:\\Users\\Player\\Documents\\Paradox Interactive\\Hearts of Iron IV\\mod\\atlantis_rising.mod",
    scriptPrefix: "atr",
    primaryNamespace: "atr",
    descriptorTags: ["Alternative History", "National Focuses", "Gameplay"],
  };
  const common: WizardState = {
    ...base,
    identity,
    description: "An Atlantis total conversion with a new Atlantic island, naval expansion, custom units, national focuses, and original mechanics.",
    projectPathStatus: "ready",
    projectPathMessage: "The project folder and launcher file were found automatically.",
    codexAccount: { available: true, authenticated: true, auth_mode: "chatgpt", usage_limited: false },
    manifestPreview: manifest,
    components: manifestComponents.map((item) => ({
      id: item.id,
      title: item.display_name,
      detail: item.description ?? "Setup component",
      size: item.expected_files.length === 1 ? "1 file" : `${item.expected_files.length} files`,
      selected: !["workflow.3d", "workflow.super_events", ...allPortraitComponentIds].includes(item.id),
      required: !item.optional,
    })),
    selectedComponents: ["core.agents", "core.skills", "core.subagents", "codex.config", "mcp.hoi4_agent_tools", "wiki.snapshot"],
    folderProfile: ["common", "events", "gfx", "interface", "localisation/english"],
    draftSaved: true,
  };
  const runpodPortrait: WizardState["portraitPipeline"] = {
    ...common.portraitPipeline,
    enabled: true,
    provider: "runpod",
    providerStatus: "needs_runpod",
  };

  if (scenario === "provider") {
    return {
      ...common,
      screen: "welcome",
      aiProvider: "claude",
      aiModel: "claude-sonnet-5",
      aiEndpoint: "https://api.anthropic.com/v1/messages",
      aiAccount: { available: true, authenticated: true, provider: "claude", model: "claude-sonnet-5", auth_mode: "api_key", usage_limited: false },
    };
  }
  if (scenario === "existing") return { ...common, screen: "identity", mode: "existing", recoveryEntry: true };
  if (scenario === "components") return { ...common, screen: "components", flattenForChat: true };
  if (scenario === "workflows") return { ...common, screen: "workflows", meshSelected: true, superEventsSelected: true, portraitPipeline: runpodPortrait, selectedComponents: [...common.selectedComponents, "workflow.3d", "workflow.super_events", ...portraitComponentIds] };
  if (scenario === "mcp") return { ...common, screen: "mcp", meshSelected: true, meshKeyStatus: "present", selectedComponents: [...common.selectedComponents, "workflow.3d"] };
  if (scenario === "git") return { ...common, screen: "git", gitOnlineAction: "create_public_github", gitHubRepository: "atlantis-rising" };
  if (scenario === "dry-run") return { ...common, screen: "dry-run", plan, flattenForChat: true, superEventsSelected: true };
  if (scenario === "ready") return { ...common, screen: "ready", plan, readiness, flattenForChat: true, superEventsSelected: true, portraitPipeline: runpodPortrait, installedSuperEventsState: "ready", selectedComponents: [...common.selectedComponents, ...portraitComponentIds] };
  if (scenario === "maintenance") return { ...common, screen: "update", mode: "existing", plan, readiness, existingInstallationDetected: true, installedWorkflow3dState: "not_selected", installedSuperEventsState: "ready", superEventsSelected: true };
  return { ...common, screen: scenario };
}
