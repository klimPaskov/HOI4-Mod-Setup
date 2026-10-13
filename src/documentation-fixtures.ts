import sourceManifest from "../docs/source-manifest/hoi4-mod-setup.manifest.json";
import type { ChatSourcesPreview, CodexAnalysis, CodexAnalysisRecord, InstallationPlan, ManifestComponentPreview, ReadinessReport, SemanticConventions, SourceManifestPreview, WizardState } from "./types";

import { DOCUMENTATION_SCENARIOS, type DocumentationScenario } from "./documentation-scenarios";
export { DOCUMENTATION_SCENARIOS } from "./documentation-scenarios";

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
  platforms: string[] = ["all"],
): ManifestComponentPreview {
  return {
    id,
    display_name: displayName,
    description,
    category,
    optional,
    platforms,
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
  component("core.claude.instructions", "Project instructions", "Shared instructions adapted to the mod and selected clients", "core", 1),
  component("core.agents", "Project instructions (AGENTS.md)", "The same rules for Codex, Cursor, Qoder, OpenCode, and DSH", "core", 1),
  component("core.skills", "HOI4 workflow skills", "Workflow skills for the clients you selected", "skill", 512),
  component("core.subagents", "Subagents", "Specialist agents for the clients you selected", "subagent", 19),
  component("runtime.agent_sync", "Runtime synchronizers", "Regenerate AGENTS.md, other runtimes' agents, and skill copies", "script", 8),
  component("mcp.hoi4_agent_tools", "HOI4 Agent Tools MCP", "HOI4 modding tools", "mcp", 0, true, ["windows"]),
  component("docs.mcp_integration", "HOI4 Agent Tools integration guide", "Capability, evidence, recovery, and troubleshooting contract", "documentation", 1),
  component("wiki.snapshot", "Offline Paradox wiki", "A local reference installed under paradox_wiki/", "documentation", 586),
  component("workflow.3d", "3D models workflow", "Optional Meshy and bounded Blender adapter workflow", "workflow", 10, true, ["windows"]),
  component("workflow.super_events", "Super Events workflow", "Reusable event popup, templates, examples, and assets", "workflow", 9, true),
  component("workflow.live_testing", "Live testing", "Choose a provider for testing the running game", "workflow", 2, true),
  component("workflow.portraits.core", "Complete portrait production contract", "Grounded source, generated portraits, processing, wiring, manifest, and handoff contract.", "skill", 1, true),
  component("workflow.portraits.router", "ComfyUI portrait-provider router", "Explicit Cloud, Local, or user-run RunPod selection contract", "workflow", 1, true),
  component("workflow.portraits.cloud", "Comfy Cloud portrait production", "Selected provider route", "workflow", 1, true),
  component("workflow.portraits.local", "Loopback ComfyUI portrait production", "Selected provider route", "workflow", 1, true),
  component("workflow.portraits.runpod", "RunPod ComfyUI portrait production", "Selected provider route", "workflow", 1, true),
  component("workflow.portraits.subagent", "Portrait production subagent", "Bounded provider execution and portrait QA", "subagent", 1, true),
  component("workflow.portraits.config", "Portrait provider configuration", "Non-secret provider and pinned upstream configuration", "configuration", 1, true),
];
const portraitComponentIds = [
  "workflow.portraits.core",
  "workflow.portraits.runpod",
  "workflow.portraits.subagent",
  "workflow.portraits.config",
];
const allPortraitComponentIds = [
  "workflow.portraits.core",
  "workflow.portraits.router",
  "workflow.portraits.cloud",
  "workflow.portraits.local",
  "workflow.portraits.runpod",
  "workflow.portraits.subagent",
  "workflow.portraits.config",
];

const source = {
  repository: "klimPaskov/Agentic-HOI4-Modding",
  mode: "latest" as const,
  resolved_revision: "documentation-preview",
  manifest_sha256: "documentation-preview",
};

const manifest = {
  schema_version: "3.0.0",
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
  profiles: [{
    id: "core",
    display_name: "Core agentic setup",
    components: ["core.claude.instructions", "core.agents", "core.skills", "core.subagents", "runtime.agent_sync", "mcp.hoi4_agent_tools", "docs.mcp_integration", "wiki.snapshot"],
    default: true,
  }],
  coding_environments: [
    { id: "claude_code", display_name: "Claude Code", description: "Project instructions, skills, agents, and MCP", default_primary: true },
    { id: "codex", display_name: "Codex", description: "Project instructions, skills, agents, and MCP" },
    { id: "cursor", display_name: "Cursor", description: "Project instructions, skills, agents, and MCP" },
    { id: "qoder", display_name: "Qoder", description: "Project instructions, skills, agents, and MCP" },
    { id: "opencode", display_name: "OpenCode", description: "Project instructions, skills, agents, and MCP" },
  ],
  live_testing: (sourceManifest as unknown as SourceManifestPreview).live_testing ?? {
    default_provider: "clausewitz_mcp",
    providers: [
      { id: "clausewitz_mcp", display_name: "Clausewitz MCP", description: "Linux game and server; configure now or later.", requirements: ["Native Linux HOI4 1.19.2 and a separately prepared server."], supported_platforms: ["windows", "macos", "linux"], supported_coding_environments: ["claude_code", "codex", "cursor", "qoder", "opencode"], experimental: false, route: { kind: "linux_stdio", server_id: "clausewitz_mcp" }, guidance: ["WSL on Windows; Linux and SSH on macOS."], supported_game_version: "1.19.2", attribution: { author: "smerss", url: "https://github.com/smerss/Clausewitz-MCP", license: "See upstream license", license_url: "https://github.com/smerss/Clausewitz-MCP/blob/main/LICENSE" } },
      { id: "hoi4_live_tools", display_name: "HOI4 Live Tools (experimental)", description: "Slower and not fully tested; verified Windows package only.", requirements: ["Windows; Python and the public Live Tools package."], supported_platforms: ["windows"], supported_coding_environments: ["claude_code", "codex", "cursor", "qoder", "opencode"], experimental: true, route: { kind: "local_package", component_id: "workflow.live_tools" }, guidance: ["The macOS scaffold is incomplete."] },
      { id: "none", display_name: "None", description: "Disable live testing.", requirements: [], supported_platforms: ["all"], supported_coding_environments: [], experimental: false, route: { kind: "none" }, guidance: [] },
    ],
  },
  image_generation: (sourceManifest as unknown as SourceManifestPreview).image_generation ?? {
    default_provider: "gptimage",
    providers: [
      { id: "gptimage", display_name: "ChatGPT subscription (GPTImage)", description: "Use the local GPTImage server with your ChatGPT plan.", availability: "supported", requirements: ["ChatGPT plan and browser sign-in."], good_for: ["Concept art", "Reference images"], cost_summary: "No separate image API bill; plan limits apply.", models: ["GPT Image"], supported_coding_environments: ["claude_code", "codex", "cursor", "qoder", "opencode"], route: { kind: "local_package", auth_method: "chatgpt_subscription", component_id: "imagegen.gptimage", tool_names: { generate: "generate_image", auth_status: "image_auth_status" } }, capabilities: { reference_images: { supported: true, max_count: 5, notes: "Pass the image paths and explain what to copy." }, image_edits: false, transparent_background: { native: false, notes: "PNG output may have an opaque background." }, size_limits: { notes: "Follow the selected route's size limits." }, one_image_per_call: true, fallback_if_no_alpha: "Use background removal and check the saved alpha.", fallback_if_no_edits: "Make a new version from references." } },
      { id: "openai_api", display_name: "OpenAI Images API", description: "Use image models with a key saved on this computer.", availability: "supported", requirements: ["OpenAI API key."], good_for: ["Image edits", "Transparent output"], cost_summary: "Pay per image; quality and size change the price.", models: ["GPT Image 2.5", "GPT Image 2", "GPT Image 1.5"], supported_coding_environments: ["claude_code", "codex", "cursor", "qoder", "opencode"], route: { kind: "stdio_adapter", auth_method: "api_key_vault", component_id: "imagegen.runtime", api_protocol: "openai_images", api_base_url: "https://api.openai.com/v1", default_model: "gpt-image-2.5-sunburst", supported_models: ["gpt-image-2.5-sunburst", "gpt-image-2", "gpt-image-1.5"], credential_fields: [{ id: "api_key", display_name: "OpenAI API key", environment_name: "OPENAI_API_KEY" }] }, capabilities: { reference_images: { supported: true, max_count: 16, notes: "Reference limits vary by model." }, image_edits: true, transparent_background: { native: true, notes: "Supported models can return transparent images; check the alpha." }, size_limits: { accepts_dimensions: true, maximum_width_px: 3840, maximum_height_px: 3840, notes: "Use model-supported dimensions." }, one_image_per_call: true, fallback_if_no_alpha: "Use a configured background-removal route.", fallback_if_no_edits: "Make a new version from references." } },
      { id: "gemini_api", display_name: "Google Gemini images (Nano Banana)", description: "Use Google's image models through AI Studio.", availability: "supported", requirements: ["Google AI Studio API key."], good_for: ["Reference-guided edits", "Several visual styles"], cost_summary: "Pay per image; size and model affect the price.", models: ["Nano Banana 2.1", "Nano Banana Pro"], supported_coding_environments: ["claude_code", "codex", "cursor", "qoder", "opencode"], route: { kind: "stdio_adapter", auth_method: "api_key_vault", component_id: "imagegen.runtime", api_protocol: "gemini_generate_content", default_model: "gemini-3.1-flash-image", credential_fields: [{ id: "api_key", display_name: "Google AI Studio API key", environment_name: "GEMINI_API_KEY" }] }, capabilities: { reference_images: { supported: true, max_count: 14, notes: "Limits depend on the model." }, image_edits: true, transparent_background: { native: false, notes: "Native alpha is not documented." }, size_limits: { accepts_dimensions: true, maximum_width_px: 4096, maximum_height_px: 4096, notes: "Limits depend on the model." }, one_image_per_call: true, fallback_if_no_alpha: "Use a configured background-removal route.", fallback_if_no_edits: "Make a new version from references." } },
      { id: "fal_ai", display_name: "fal.ai", description: "Choose among many hosted image models.", availability: "supported", requirements: ["fal API key."], good_for: ["FLUX", "Recraft", "Ideogram", "Seedream"], cost_summary: "Pay per model run; prices vary.", models: ["FLUX", "Recraft", "Ideogram", "Seedream", "Nano Banana"], supported_coding_environments: ["claude_code", "codex", "cursor", "qoder", "opencode"], route: { kind: "stdio_adapter", auth_method: "api_key_vault", component_id: "imagegen.runtime", api_protocol: "fal_queue", default_model: "fal-ai/flux-2-pro", credential_fields: [{ id: "api_key", display_name: "fal API key", environment_name: "FAL_KEY" }] }, capabilities: { reference_images: { supported: true, max_count: 1, notes: "Limits depend on model." }, image_edits: true, transparent_background: { native: false, notes: "Some models support transparency." }, size_limits: { notes: "Limits depend on model." }, one_image_per_call: true, fallback_if_no_alpha: "Use a configured background-removal route.", fallback_if_no_edits: "Make a new version from references." } },
      { id: "none", display_name: "None — I will make art myself", description: "Use artwork you create or source yourself.", availability: "supported", requirements: ["No account or install."], good_for: ["Your own artwork"], cost_summary: "No provider charge.", models: [], supported_coding_environments: ["claude_code", "codex", "cursor", "qoder", "opencode"], route: { kind: "manual", auth_method: "none" }, capabilities: { reference_images: { supported: false, notes: "Add your own files to the project." }, image_edits: false, transparent_background: { native: false, notes: "Check alpha in the files you add." }, size_limits: { notes: "Use HOI4 asset size rules." }, one_image_per_call: true, fallback_if_no_alpha: "Edit the image yourself and check its alpha.", fallback_if_no_edits: "Edit the image yourself." } },
    ],
  },
};

const semanticAnalysis: CodexAnalysis = {
  schema_version: "1.0.0",
  analysis_id: "documentation-semantic-analysis",
  mode: "new_project_identity",
  input_sha256: "documentation-input",
  project_summary: "An island-focused total conversion with naval and political systems.",
  proposals: [
    ["display_name", "Atlantis Rising"],
    ["project_id", "atlantis_rising"],
    ["script_prefix", "atr"],
    ["primary_namespace", "atr"],
    ["project_description", "An island-focused total conversion with naval and political systems."],
    ["descriptor_tags", ["Alternative History", "National Focuses", "Gameplay"]],
    ["folder_profile", ["common", "events", "gfx", "interface", "localisation/english"]],
    ["agents_profile", "default"],
    ["localisation_convention", "english"],
    ["documentation_convention", "markdown"],
  ].map(([key, value]) => ({ key: key as string, value, confidence: 0.9, reason: "Matches the approved development brief.", evidence_refs: ["brief:1"] })),
  component_recommendations: [
    { component_id: "core.skills", recommendation: "recommended", reason: "The brief calls for repeated HOI4 scripting and asset workflows." },
    { component_id: "workflow.3d", recommendation: "not_recommended", reason: "The brief does not require custom models at project creation." },
  ],
  warnings: ["Check the project ID and namespace before continuing."],
};

const semanticAnalysisRecord: CodexAnalysisRecord = {
  engine: "codex_app_server",
  auth_mode: "chatgpt",
  provider: "codex",
  model: "gpt-5.6-luna",
  reasoning_effort: "xhigh",
  optimization_profile: "Codex setup analysis",
  analysis_id: semanticAnalysis.analysis_id,
  schema_version: "1.0.0",
  input_sha256: semanticAnalysis.input_sha256,
  output_sha256: "documentation-output",
  confirmed_fields: [],
  confirmed_at: "pending",
  source_revision: "documentation-preview",
  source_manifest_sha256: "documentation-preview",
};

const semanticConventions: SemanticConventions = {
  agents_profile: "default",
  localisation_convention: "english",
  documentation_convention: "markdown",
};

const operations: InstallationPlan["operations"] = [
  ["claude", "core.claude.instructions", "CLAUDE.md"],
  ["agents", "core.agents", "AGENTS.md"],
  ["skills", "core.skills", ".claude/skills/"],
  ["subagents", "core.subagents", ".claude/agents/"],
  ["settings", "runtime.claude", ".claude/settings.json"],
  ["mcp", "runtime.claude.mcp", ".mcp.json"],
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
  schema_version: "1.1.0",
  plan_id: "documentation-preview",
  project_id: "atlantis_rising",
  source,
  ai_provider: "claude_account",
  ai_model: "claude-haiku-5-5",
  flatten_chat_sources: true,
  selected_components: ["core.claude.instructions", "core.agents", "core.skills", "core.subagents", "runtime.agent_sync", "runtime.claude", "runtime.claude.mcp", "mcp.hoi4_agent_tools", "docs.mcp_integration", "wiki.snapshot", "workflow.super_events", "workflow.live_testing", "workflow.portraits.router", ...portraitComponentIds],
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
    project_root_identity: "windows-v2:0000000000000001:00000000000000000000000000000001",
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
    ["coding.environments", "Coding environments", "pass"],
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
    aiAccount: { available: true, authenticated: true, provider: "claude_account", model: "claude-haiku-5-5", auth_mode: "claude_account", usage_limited: false },
    manifestPreview: manifest,
    components: manifestComponents.map((item) => ({
      id: item.id,
      title: item.display_name,
      detail: item.description ?? "Setup component",
      size: item.expected_files.length === 1 ? "1 file" : `${item.expected_files.length} files`,
      selected: !["workflow.3d", "workflow.super_events", "workflow.live_testing", ...allPortraitComponentIds].includes(item.id),
      required: !item.optional,
    })),
    selectedComponents: ["core.claude.instructions", "core.agents", "core.skills", "core.subagents", "runtime.agent_sync", "mcp.hoi4_agent_tools", "docs.mcp_integration", "wiki.snapshot"],
    primaryCodingEnvironment: "codex",
    additionalCodingEnvironments: ["claude_code"],
    folderProfile: ["common", "events", "gfx", "interface", "localisation/english"],
    codexAnalysis: semanticAnalysis,
    codexAnalysisRecord: semanticAnalysisRecord,
    conventions: semanticConventions,
    semanticComponentRecommendations: semanticAnalysis.component_recommendations,
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
      aiModel: "claude-haiku-5-5",
      aiEndpoint: "https://api.anthropic.com/v1/messages",
      aiAccount: { available: true, authenticated: true, provider: "claude", model: "claude-haiku-5-5", auth_mode: "api_key", usage_limited: false },
    };
  }
  if (scenario === "description") return { ...common, screen: "description", mode: "new", codexAnalysis: undefined, codexAnalysisRecord: undefined };
  if (scenario === "identity") return { ...common, screen: "identity", mode: "new", codexAnalysis: semanticAnalysis, codexAnalysisRecord: semanticAnalysisRecord, conventions: semanticConventions, semanticComponentRecommendations: semanticAnalysis.component_recommendations };
  if (scenario === "existing") return { ...common, screen: "identity", mode: "existing", recoveryEntry: true };
  if (scenario === "components") return { ...common, screen: "components", flattenForChat: true };
  if (scenario === "environments") return { ...common, screen: "environments", primaryCodingEnvironment: "codex", additionalCodingEnvironments: ["claude_code"] };
  if (scenario === "live-testing") return { ...common, screen: "live-testing", selectedComponents: [...common.selectedComponents, "workflow.live_testing"] };
  if (scenario === "image-generation") return { ...common, screen: "image-generation", primaryCodingEnvironment: "codex", additionalCodingEnvironments: ["claude_code"], imageGeneration: { primary_provider: "gptimage", additional_providers: ["openai_api"], route_settings: {}, credential_references: {} } };
  if (scenario === "workflows") return { ...common, screen: "workflows", meshSelected: true, superEventsSelected: true, portraitPipeline: runpodPortrait, selectedComponents: [...common.selectedComponents, "workflow.3d", "workflow.super_events", "workflow.live_testing", ...portraitComponentIds] };
  if (scenario === "mcp") return { ...common, screen: "mcp", meshSelected: true, meshKeyStatus: "present", selectedComponents: [...common.selectedComponents, "workflow.3d"] };
  if (scenario === "git") return { ...common, screen: "git", gitOnlineAction: "create_public_github", gitHubRepository: "atlantis-rising" };
  if (scenario === "dry-run") return { ...common, screen: "dry-run", plan, flattenForChat: true, superEventsSelected: true };
  if (scenario === "ready") return { ...common, screen: "ready", plan, readiness, flattenForChat: true, superEventsSelected: true, portraitPipeline: runpodPortrait, installedSuperEventsState: "ready", selectedComponents: [...common.selectedComponents, ...portraitComponentIds] };
  if (scenario === "maintenance") return { ...common, screen: "update", mode: "existing", plan, readiness, selectedComponents: [...common.selectedComponents, "workflow.live_testing"], existingInstallationDetected: true, installedWorkflow3dState: "not_selected", installedSuperEventsState: "ready", superEventsSelected: true };
  if (scenario === "recovery") {
    const stageIds = ["preflight", "repository source resolution", "selective download", "checksum verification", "dry-run review", "backup", "staging", "validation", "apply", "post-install checks", "readiness report", "rollback record"];
    return {
      ...common,
      screen: "recovery",
      mode: "existing",
      recoveryChoice: "resume",
      transaction: {
        schema_version: "1.0.0",
        transaction_id: "00000000-0000-4000-8000-000000000011",
        transaction_kind: "installation",
        project_id: identity.projectId,
        project_root: identity.projectRoot,
        state: "validating",
        created_at: "2026-08-09T12:00:00Z",
        updated_at: "2026-08-09T12:03:00Z",
        last_checkpoint: "validation",
        stages: stageIds.map((id, index) => ({ id, status: index < 7 ? "complete" : index === 7 ? "active" : "pending" })),
        operations: Array.from({ length: 1146 }, (_, index) => ({ id: `op-${index + 1}`, status: "staged", destination: index === 1145 ? identity.launcherDescriptorPath : `prepared/file-${index + 1}.txt`, external: index === 1145 })),
        recovery: { resume_allowed: true, rollback_allowed: false, discard_staging_allowed: true, project_apply_started: false, recommended_action: "resume" },
        error: { code: "transaction_error", message: "Launcher descriptor path did not match the selected project root.", stage: "validation" },
      },
    };
  }
  if (scenario === "chat-sources") {
    const chatSourcesPreview: ChatSourcesPreview = {
      eligible: true,
      projectRoot: identity.projectRoot,
      destinationDirectory: "C:\\Users\\Player\\Downloads",
      archiveName: "atlantis_rising-chatgpt-project-sources.zip",
      files: [
        { id: "AGENTS.md", sourcePath: "AGENTS.md", archivePath: "AGENTS.md", category: "instructions", size: 18432, required: true, selectedByDefault: true },
        { id: "README.md", sourcePath: "README.md", archivePath: "README.md", category: "readme", size: 9216, required: true, selectedByDefault: true },
        { id: ".claude/skills/hoi4-events/SKILL.md", sourcePath: ".claude/skills/hoi4-events/SKILL.md", archivePath: "hoi4-events.md", category: "skill", size: 14336, required: true, selectedByDefault: true },
        { id: ".claude/agents/hoi4-repo-explorer.md", sourcePath: ".claude/agents/hoi4-repo-explorer.md", archivePath: "hoi4-repo-explorer.md", category: "subagent", size: 5632, required: true, selectedByDefault: true },
        { id: "DESIGN_NOTES.md", sourcePath: "DESIGN_NOTES.md", archivePath: "DESIGN_NOTES.md", category: "root_markdown", size: 4096, required: false, selectedByDefault: false },
        { id: "TODO.md", sourcePath: "TODO.md", archivePath: "TODO.md", category: "root_markdown", size: 3072, required: false, selectedByDefault: false },
      ],
    };
    return { ...common, screen: "chat-sources", mode: "existing", existingInstallationDetected: true, chatSourcesAvailable: true, chatSourcesPreview, chatSourcesDestination: chatSourcesPreview.destinationDirectory, chatSourcesSelectedIds: chatSourcesPreview.files.filter((file) => file.selectedByDefault).map((file) => file.id) };
  }
  return { ...common, screen: scenario };
}
