// The HOI4 Mod Setup web edition. A dropped folder stays in this browser
// tab; the setup core runs as WebAssembly in a worker, downloads only the
// selected files from GitHub at one exact commit, verifies each against the
// manifest, and returns a ZIP for the user to unpack.

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { ChangeEvent, DragEvent, ReactNode, RefObject } from "react";
import { deriveGeneratedIdentity, HOI4_DESCRIPTOR_TAGS } from "../identity";
import { renderCoverPng, renderReferenceJpeg } from "../lib/images";
import { startCoreWorker } from "./core-client";
import type { CoreClient } from "./core-client";
import { BROWSER_PROVIDERS, isLoopbackEndpoint, MODEL_PATTERN, modelChoices, proposal, provenanceFor, routeFor, runAssistant } from "./assistant";
import { folderFromDrop, folderFromInput, readFileBytes } from "./folder";
import type { FolderSelection } from "./folder";
import { fetchRaw, MANIFEST_PATH, networkLog, onNetworkLog, resolveSource, SOURCE_NAME, SOURCE_OWNER } from "./github";
import type { NetworkRecord, ResolvedSource, SourceMode } from "./github";
import { availableImageProviders, defaultImageChoice, isLoopbackUrl, liveTestingProblem, NO_LIVE_TESTING, NO_PORTRAITS, normalizeImageChoice, portraitProblem } from "./integrations";
import ContextPanel from "./parts/ContextPanel";
import type { ContextDocumentEntry, ContextImageEntry } from "./parts/ContextPanel";
import FindingsPanel from "./parts/FindingsPanel";
import MaintenancePanel from "./parts/MaintenancePanel";
import type { MaintenanceChoice } from "./parts/MaintenancePanel";
import DoneStep from "./steps/DoneStep";
import IntegrationsStep from "./steps/IntegrationsStep";
import ReviewStep from "./steps/ReviewStep";
import type { Analysis, AnalysisRequest, ApprovedEvidence, AssistantProvenance, Comparison, Decision, ExportEntry, ExportRequest, ImageChoice, LiveTestingChoice, Platform, PortraitChoice, ReadinessCheck, ScanSummary, SourcePreview } from "./protocol";

type Step = "start" | "describe" | "details" | "components" | "integrations" | "review" | "done";
const STEP_LABELS: Record<Step, string> = {
  start: "Project",
  describe: "Describe",
  details: "Details",
  components: "Components",
  integrations: "Integrations",
  review: "Review",
  done: "Download",
};
/** Need a desktop installer or launcher: the 3D workflow runs Meshy through
 * the app, and Live Tools installs its MCP server. */
const DESKTOP_ONLY_COMPONENTS = new Set(["workflow.3d", "workflow.live_tools"]);
/** Chosen on the Integrations step, not as separate components. */
const HIDDEN_COMPONENT_PREFIXES = ["workflow.portraits", "imagegen.", "workflow.live_testing"];
/** Shown although another optional component depends on it. */
const ALWAYS_LISTED = new Set(["workflow.super_events"]);
const READ_BATCH_BYTES = 32 * 1024 * 1024;
const PROJECT_ID = /^[a-z][a-z0-9_]{1,63}$/;
const MAX_ATTACHMENTS = 12;
const MAX_IMAGE_BYTES = 12 * 1024 * 1024;
const EXISTING_PROJECT_BRIEF = "Review this existing HOI4 mod using only the approved scan evidence. Keep detected facts unchanged, infer conventions only from that evidence, and do not invent the mod's theme or content.";

interface Identity {
  displayName: string;
  projectId: string;
  description: string;
  author: string;
  primaryNamespace: string;
  scriptPrefix: string;
  descriptorTags: string[];
}

interface Progress {
  label: string;
  done: number;
  total: number;
}

function guessPlatform(): Platform {
  const agent = `${navigator.userAgent} ${(navigator as Navigator & { userAgentData?: { platform?: string } }).userAgentData?.platform ?? ""}`.toLowerCase();
  if (agent.includes("mac")) return "macos";
  if (agent.includes("linux") || agent.includes("x11")) return "linux";
  return "windows";
}

function detectedString(scan: ScanSummary | null, key: string): string | undefined {
  const value = scan?.detected.find((item) => item.key === key)?.value;
  return typeof value === "string" ? value : undefined;
}

function detectedList(scan: ScanSummary | null, key: string): string[] | undefined {
  const value = scan?.detected.find((item) => item.key === key)?.value;
  return Array.isArray(value) ? value.filter((item): item is string => typeof item === "string") : undefined;
}

function saveBlob(bytes: ArrayBuffer | string, name: string, type = "application/zip"): void {
  const url = URL.createObjectURL(new Blob([bytes], { type }));
  const link = document.createElement("a");
  link.href = url;
  link.download = name;
  link.click();
  setTimeout(() => URL.revokeObjectURL(url), 60_000);
}

function toBase64(bytes: ArrayBuffer): string {
  const view = new Uint8Array(bytes);
  let text = "";
  for (let index = 0; index < view.length; index += 0x8000) {
    text += String.fromCharCode(...view.subarray(index, index + 0x8000));
  }
  return btoa(text);
}

function readDataUrl(file: File): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => resolve(String(reader.result));
    reader.onerror = () => reject(new Error("The image could not be read."));
    reader.readAsDataURL(file);
  });
}

export default function WebApp({ client: providedClient }: { client?: CoreClient }) {
  const clientRef = useRef<CoreClient | null>(providedClient ?? null);
  const client = useCallback((): CoreClient => {
    clientRef.current ??= startCoreWorker();
    return clientRef.current;
  }, []);
  useEffect(() => () => {
    if (!providedClient) clientRef.current?.terminate();
  }, [providedClient]);

  const [step, setStep] = useState<Step>("start");
  const [kind, setKind] = useState<"new" | "existing" | null>(null);
  const [folder, setFolder] = useState<FolderSelection | null>(null);
  const [scan, setScan] = useState<ScanSummary | null>(null);
  const [busy, setBusy] = useState<Progress | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [sourceMode, setSourceMode] = useState<SourceMode>("latest");
  const [sourceRef, setSourceRef] = useState("");
  const [source, setSource] = useState<ResolvedSource | null>(null);
  // The revision the worker opened, read synchronously when downloading.
  const resolvedSource = useRef<ResolvedSource | null>(null);
  const [preview, setPreview] = useState<SourcePreview | null>(null);
  const [platform, setPlatform] = useState<Platform>(guessPlatform);
  const [identity, setIdentity] = useState<Identity>({ displayName: "", projectId: "", description: "", author: "", primaryNamespace: "", scriptPrefix: "", descriptorTags: [] });
  const [edited, setEdited] = useState<Set<keyof Identity>>(new Set());
  const [thumbnail, setThumbnail] = useState<string | null>(null);
  const [assistantId, setAssistantId] = useState(BROWSER_PROVIDERS[0].id);
  const assistantProvider = BROWSER_PROVIDERS.find((item) => item.id === assistantId) ?? BROWSER_PROVIDERS[0];
  const [assistantModel, setAssistantModel] = useState(BROWSER_PROVIDERS[0].defaultModel);
  const [assistantEffort, setAssistantEffort] = useState(BROWSER_PROVIDERS[0].defaultEffort);
  const [assistantEndpoint, setAssistantEndpoint] = useState(BROWSER_PROVIDERS[0].endpoint);
  // The key lives only in this component's memory and is never stored.
  const [apiKey, setApiKey] = useState("");
  const [analysis, setAnalysis] = useState<Analysis | null>(null);
  const [provenance, setProvenance] = useState<AssistantProvenance | null>(null);
  const [suggested, setSuggested] = useState<Set<keyof Identity>>(new Set());
  const [evidencePreview, setEvidencePreview] = useState<ApprovedEvidence[] | null>(null);
  // Suggestions arrive after a long request; apply them against what the user
  // has typed by then, not when the request started.
  const identityRef = useRef(identity);
  identityRef.current = identity;
  const editedRef = useRef(edited);
  editedRef.current = edited;
  const suggestedRef = useRef(suggested);
  suggestedRef.current = suggested;
  // A new mod's brief, kept apart from the description the assistant writes.
  const [brief, setBrief] = useState("");
  const [excluded, setExcluded] = useState<Set<string>>(new Set());
  const [documents, setDocuments] = useState<ContextDocumentEntry[]>([]);
  const [images, setImages] = useState<ContextImageEntry[]>([]);
  const [refused, setRefused] = useState<string[]>([]);
  const attachmentCounter = useRef(0);
  const [profileId, setProfileId] = useState<string>("");
  const [optional, setOptional] = useState<Set<string>>(new Set());
  const [primary, setPrimary] = useState("");
  const [additional, setAdditional] = useState<Set<string>>(new Set());
  const [imageChoice, setImageChoice] = useState<ImageChoice | null>(null);
  const [portrait, setPortrait] = useState<PortraitChoice>(NO_PORTRAITS);
  const [live, setLive] = useState<LiveTestingChoice>(NO_LIVE_TESTING);
  const [maintenance, setMaintenance] = useState<MaintenanceChoice | null>(null);
  const [entries, setEntries] = useState<ExportEntry[] | null>(null);
  const [decisions, setDecisions] = useState<Record<string, Decision>>({});
  const [comparison, setComparison] = useState<Comparison | null>(null);
  const [merged, setMerged] = useState<Record<string, string>>({});
  const [mergeErrors, setMergeErrors] = useState<Record<string, string>>({});
  const [basesReady, setBasesReady] = useState(false);
  const [readiness, setReadiness] = useState<ReadinessCheck[]>([]);
  const [layout, setLayout] = useState<"overlay" | "full_copy">("overlay");
  const [network, setNetwork] = useState<NetworkRecord[]>(networkLog);
  const [dragging, setDragging] = useState(false);
  const headingRef = useRef<HTMLHeadingElement>(null);
  const seededAnalysis = useRef<string | null>(null);

  useEffect(() => onNetworkLog(setNetwork), []);
  useEffect(() => headingRef.current?.focus(), [step]);

  const existingSetup = scan?.existing_setup?.valid ? scan.existing_setup : null;

  const run = useCallback(async (label: string, task: () => Promise<void>) => {
    setError(null);
    setBusy({ label, done: 0, total: 0 });
    try {
      await task();
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      setBusy(null);
    }
  }, []);

  /** Resolve the source to an exact commit and read its manifest. */
  const loadSource = useCallback(async (mode: SourceMode = sourceMode, reference: string = sourceRef): Promise<SourcePreview> => {
    const resolved = await resolveSource(mode, reference);
    const manifest = await fetchRaw(resolved.revision, MANIFEST_PATH);
    const buffer = manifest.buffer.slice(manifest.byteOffset, manifest.byteOffset + manifest.byteLength) as ArrayBuffer;
    await client().call("openSource", { manifest: buffer, revision: resolved.revision, mode: resolved.mode, requestedRef: resolved.requestedRef }, [buffer]);
    const next = await client().call("preview", { platform });
    resolvedSource.current = resolved;
    setSource(resolved);
    setPreview(next);
    const defaultProfile = next.profiles.find((profile) => profile.default) ?? next.profiles[0];
    setProfileId((current) => current || defaultProfile?.id || "");
    const defaultClient = next.coding_environments.find((environment) => environment.default_primary) ?? next.coding_environments[0];
    setPrimary((current) => (current && next.coding_environments.some((environment) => environment.id === current) ? current : defaultClient?.id ?? ""));
    setImageChoice((current) => current ?? defaultImageChoice(next));
    return next;
  }, [client, platform, sourceMode, sourceRef]);

  const resetAnalysis = () => {
    setAnalysis(null);
    setProvenance(null);
    setSuggested(new Set());
  };

  const importFolder = useCallback((selection: FolderSelection) => run("Reading the folder", async () => {
    if (selection.files.length === 0) throw new Error("The folder is empty.");
    await client().call("reset", {});
    let batch: Array<{ path: string; buffer: ArrayBuffer }> = [];
    let batchBytes = 0;
    let read = 0;
    const flush = async () => {
      if (batch.length === 0) return;
      await client().call("addFiles", { files: batch }, batch.map((file) => file.buffer));
      batch = [];
      batchBytes = 0;
    };
    for (const entry of selection.files) {
      const buffer = await readFileBytes(entry.file);
      batch.push({ path: entry.path, buffer });
      batchBytes += buffer.byteLength;
      read += 1;
      if (batchBytes >= READ_BATCH_BYTES) {
        await flush();
        setBusy({ label: "Reading the folder", done: read, total: selection.files.length });
      }
    }
    await flush();
    setBusy({ label: "Scanning", done: 0, total: 0 });
    const result = await client().call("scan", {});
    setFolder(selection);
    setScan(result);
    setKind("existing");
    const name = detectedString(result, "display_name") ?? result.descriptor?.name ?? selection.name;
    const defaults = deriveGeneratedIdentity(name, "");
    const namespace = detectedString(result, "primary_namespace") ?? defaults.primaryNamespace;
    setIdentity({
      displayName: name,
      projectId: PROJECT_ID.test(selection.name) ? selection.name : defaults.projectId,
      description: "",
      author: "",
      primaryNamespace: namespace,
      scriptPrefix: detectedString(result, "script_prefix") ?? namespace,
      descriptorTags: detectedList(result, "descriptor_tags") ?? defaults.descriptorTags,
    });
    setEdited(new Set());
    setExcluded(new Set());
    setMaintenance(result.existing_setup?.valid ? "update" : null);
    resetAnalysis();
  }), [client, run]);

  const onDrop = useCallback(async (event: DragEvent<HTMLElement>) => {
    event.preventDefault();
    setDragging(false);
    try {
      importFolder(await folderFromDrop(event.dataTransfer.items));
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    }
  }, [importFolder]);

  const onPick = useCallback((event: ChangeEvent<HTMLInputElement>) => {
    const files = event.target.files;
    if (!files || files.length === 0) return;
    try {
      importFolder(folderFromInput(files));
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    }
    event.target.value = "";
  }, [importFolder]);

  const chooseNew = () => {
    if (kind === "new") return;
    setKind("new");
    setFolder(null);
    setScan(null);
    setMaintenance(null);
    setIdentity({ displayName: "", projectId: "", description: "", author: "", primaryNamespace: "", scriptPrefix: "", descriptorTags: [] });
    setEdited(new Set());
    setBrief("");
    resetAnalysis();
  };

  const updateIdentity = (key: keyof Identity, value: string | string[]) => {
    setEdited((current) => new Set(current).add(key));
    setSuggested((current) => {
      const next = new Set(current);
      next.delete(key);
      return next;
    });
    setIdentity((current) => {
      const next = { ...current, [key]: value } as Identity;
      if (kind === "new" && key === "displayName") {
        const defaults = deriveGeneratedIdentity(value as string, current.description);
        if (!edited.has("projectId")) next.projectId = defaults.projectId;
        if (!edited.has("primaryNamespace")) next.primaryNamespace = defaults.primaryNamespace;
        if (!edited.has("scriptPrefix")) next.scriptPrefix = defaults.scriptPrefix;
        if (!edited.has("descriptorTags")) next.descriptorTags = defaults.descriptorTags;
      }
      return next;
    });
  };

  const chooseThumbnail = async (event: ChangeEvent<HTMLInputElement>) => {
    const file = event.target.files?.[0];
    event.target.value = "";
    if (!file) return;
    try {
      setThumbnail(await renderCoverPng(await readDataUrl(file), 512));
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    }
  };

  const addDocuments = (files: File[]) => run("Reading documents", async () => {
    const added: ContextDocumentEntry[] = [];
    const problems: string[] = [];
    for (const file of files) {
      if (documents.length + added.length >= MAX_ATTACHMENTS) {
        problems.push(`${file.name}: at most ${MAX_ATTACHMENTS} documents can be added`);
        continue;
      }
      try {
        const buffer = await readFileBytes(file);
        attachmentCounter.current += 1;
        const document = await client().call("contextDocument", { number: attachmentCounter.current, name: file.name, buffer: buffer.slice(0) }, []);
        added.push({ document, data: toBase64(buffer), include: true, keepCopy: false });
      } catch (cause) {
        problems.push(`${file.name}: ${cause instanceof Error ? cause.message : String(cause)}`);
      }
    }
    setDocuments((current) => [...current, ...added]);
    setRefused(problems);
    resetAnalysis();
  });

  const addImages = (files: File[]) => run("Reading images", async () => {
    const added: ContextImageEntry[] = [];
    const problems: string[] = [];
    for (const file of files) {
      if (images.length + added.length >= MAX_ATTACHMENTS) {
        problems.push(`${file.name}: at most ${MAX_ATTACHMENTS} images can be added`);
        continue;
      }
      if (!["image/png", "image/jpeg", "image/webp"].includes(file.type) || file.size > MAX_IMAGE_BYTES) {
        problems.push(`${file.name}: only PNG, JPEG, or WebP images up to 12 MB can be added`);
        continue;
      }
      try {
        const dataUrl = await readDataUrl(file);
        let reference: string | null = null;
        try {
          reference = await renderReferenceJpeg(dataUrl, 1600, 1024 * 1024);
        } catch {
          reference = null;
        }
        attachmentCounter.current += 1;
        added.push({ id: `img-${attachmentCounter.current}`, name: file.name, dataUrl, reference, keep: false });
      } catch (cause) {
        problems.push(`${file.name}: ${cause instanceof Error ? cause.message : String(cause)}`);
      }
    }
    setImages((current) => [...current, ...added]);
    setRefused(problems);
  });

  const takeThumbnail = (image: ContextImageEntry) => run("Making the thumbnail", async () => {
    setThumbnail(await renderCoverPng(image.dataUrl, 512));
  });

  const assistantName = provenance ? (BROWSER_PROVIDERS.find((item) => item.id === provenance.provider)?.label ?? provenance.provider).replace(/ API key$/, "") : "";
  // An existing mod's folder name is its ID when it is a valid one.
  const folderDecidesId = kind === "existing" && folder !== null && PROJECT_ID.test(folder.name);
  const badge = (key: keyof Identity, detectedKey?: string): string | undefined => {
    if (key === "projectId" && folderDecidesId && !edited.has(key)) return "Detected";
    if (kind === "existing" && detectedKey && !edited.has(key) && detectedString(scan, detectedKey) !== undefined) return "Detected";
    if (suggested.has(key)) return `Suggested by ${assistantName}`;
    return undefined;
  };

  const identityProblem = useMemo(() => {
    if (!identity.displayName.trim()) return "Enter the mod name.";
    if (!PROJECT_ID.test(identity.projectId)) return "The mod ID must start with a lowercase letter and use only lowercase letters, digits, and underscores.";
    return null;
  }, [identity]);

  const chooseAssistant = (id: string) => {
    const next = BROWSER_PROVIDERS.find((item) => item.id === id) ?? BROWSER_PROVIDERS[0];
    setAssistantId(next.id);
    setAssistantModel(next.defaultModel);
    setAssistantEffort(next.defaultEffort);
    setAssistantEndpoint(next.endpoint);
    setApiKey("");
    resetAnalysis();
  };
  const assistantLabel = assistantProvider.label.replace(/ API key$/, "");
  const assistantProblem = assistantProvider.local
    ? !isLoopbackEndpoint(assistantEndpoint.trim()) ? "Enter the local model's address on this computer." : !MODEL_PATTERN.test(assistantModel.trim()) ? "Enter the local model's ID." : null
    : !apiKey.trim() ? `Enter your ${assistantLabel} API key.` : !MODEL_PATTERN.test(assistantModel.trim()) ? "Enter a model ID without spaces." : null;
  const usesAssistant = !(kind === "existing" && existingSetup && maintenance);
  const startProblem = kind === null ? "Choose a new mod or an existing mod folder." : kind === "existing" && !scan ? "Choose the mod folder." : usesAssistant ? assistantProblem : null;
  const briefProblem = !identity.displayName.trim() ? "Enter the mod name." : !brief.trim() ? "Describe the mod in a sentence or two." : null;

  const contextEvidence = (): ApprovedEvidence[] => documents.filter((entry) => entry.include).map(({ document }) => ({
    reference: document.reference,
    path: document.evidence_path,
    excerpt: document.excerpt,
    excerpt_sha256: document.excerpt_sha256,
    confidence: null,
  }));

  /** Ask the assistant for every value, then show them for review. */
  const analyze = () => run(`Asking ${assistantLabel}`, async () => {
    if (!preview) await loadSource();
    const registry = await client().call("registry", {});
    const scanEvidence = kind === "existing" ? await client().call("evidence", { excluded: [...excluded] }) : [];
    if (kind === "existing") setEvidencePreview(scanEvidence);
    const name = identity.displayName.trim();
    const request: AnalysisRequest = {
      mode: kind === "existing" ? "existing_project_semantics" : "new_project_identity",
      brief: kind === "existing" ? EXISTING_PROJECT_BRIEF : brief.trim(),
      evidence: [...scanEvidence, ...contextEvidence()],
      constraints: {
        project_id_pattern: "^[a-z][a-z0-9_]{1,63}$",
        source: { mode: sourceMode, selected_ref: sourceMode === "latest" ? undefined : sourceRef.trim() },
        component_registry: registry,
        ...(kind === "new" && name ? { requested_mod_name: name } : {}),
      },
    };
    if (kind === "existing" && request.evidence.length === 0) throw new Error(`Include at least one scan finding or document for ${assistantLabel} to read.`);
    const route = routeFor(assistantProvider, assistantModel.trim(), assistantEffort, assistantProvider.local ? assistantEndpoint.trim() : assistantProvider.endpoint);
    const result = await runAssistant(client(), route, apiKey, request);
    const text = (key: string) => {
      const value = proposal(result, key);
      return typeof value === "string" ? value : undefined;
    };
    const tags = proposal(result, "descriptor_tags");
    const values: Partial<Record<keyof Identity, string | string[]>> = {
      displayName: text("display_name"),
      projectId: text("project_id"),
      primaryNamespace: text("primary_namespace"),
      scriptPrefix: text("script_prefix"),
      description: text("project_description"),
      descriptorTags: Array.isArray(tags) ? tags.filter((tag): tag is string => typeof tag === "string" && (HOI4_DESCRIPTOR_TAGS as readonly string[]).includes(tag)) : undefined,
    };
    // A value the user typed, or one the project files decide, always wins
    // over a suggestion; an earlier suggestion is refreshed.
    const detectedKeys = new Set<keyof Identity>(kind === "existing" ? ([
      ["displayName", "display_name"],
      ["primaryNamespace", "primary_namespace"],
      ["scriptPrefix", "script_prefix"],
      ["descriptorTags", "descriptor_tags"],
    ] as Array<[keyof Identity, string]>).filter(([, key]) => scan?.detected.some((item) => item.key === key)).map(([field]) => field) : []);
    if (folderDecidesId) detectedKeys.add("projectId");
    const applied = new Set<keyof Identity>();
    const next = { ...identityRef.current };
    for (const [key, value] of Object.entries(values) as Array<[keyof Identity, string | string[] | undefined]>) {
      const userEdited = editedRef.current.has(key) && !suggestedRef.current.has(key);
      if (value === undefined || userEdited || detectedKeys.has(key)) continue;
      (next as Record<keyof Identity, string | string[]>)[key] = value;
      applied.add(key);
    }
    setIdentity(next);
    setSuggested(applied);
    setEdited((current) => new Set([...current, ...applied]));
    setAnalysis(result);
    setProvenance(provenanceFor(route));
    setStep("details");
  });

  const profileFor = (current: SourcePreview, components: string[]) =>
    // The richest profile whose components the setup has, else the default.
    [...current.profiles].sort((left, right) => right.components.length - left.components.length).find((profile) => profile.components.every((id) => components.includes(id)))
    ?? current.profiles.find((profile) => profile.default) ?? current.profiles[0];

  /** Start a maintenance run with the setup's recorded choices. */
  const startMaintenance = (choice: MaintenanceChoice) => run(choice === "remove" ? "Listing the setup files" : "Reading the setup source", async () => {
    if (!existingSetup) throw new Error("This folder has no setup to maintain.");
    if (choice === "remove") {
      const result = await client().call("exportRemoval", { layout });
      setEntries(result);
      setDecisions({});
      setReadiness(await client().call("readiness", {}));
      setStep("review");
      return;
    }
    const revision = existingSetup.source?.revision ?? "";
    // Update follows the chosen source; repair and change keep the installed revision.
    const current = choice === "update" ? await loadSource() : await loadSource("pinned_commit", revision);
    const installed = existingSetup.components ?? [];
    const profile = profileFor(current, installed);
    setProfileId(profile?.id ?? "");
    setOptional(new Set(optionalComponentsFor(current, profile).map((component) => component.id).filter((id) => installed.includes(id))));
    const clients = current.coding_environments.map((environment) => environment.id);
    const recordedPrimary = existingSetup.primary_coding_environment ?? "";
    setPrimary(clients.includes(recordedPrimary) ? recordedPrimary : current.coding_environments.find((environment) => environment.default_primary)?.id ?? clients[0] ?? "");
    setAdditional(new Set((existingSetup.additional_coding_environments ?? []).filter((id) => clients.includes(id) && id !== recordedPrimary)));
    setImageChoice(existingSetup.image_generation ? { primary_provider: existingSetup.image_generation.primary_provider, additional_providers: existingSetup.image_generation.additional_providers ?? [], route_settings: existingSetup.image_generation.route_settings ?? {} } : defaultImageChoice(current));
    const recordedPortrait = existingSetup.portrait_pipeline;
    setPortrait(recordedPortrait?.enabled ? { provider: recordedPortrait.provider as PortraitChoice["provider"], localComfyuiRoot: recordedPortrait.local_comfyui_root, localServerUrl: recordedPortrait.local_server_url, runpodUrl: recordedPortrait.runpod_url, runpodWorkspace: recordedPortrait.runpod_workspace } : NO_PORTRAITS);
    const recordedLive = existingSetup.live_testing;
    setLive(recordedLive && recordedLive.provider === "clausewitz_mcp" ? recordedLive : NO_LIVE_TESTING);
    if (choice === "repair") {
      await prepareWith(current, {
        profileComponents: profile?.components ?? [],
        optionalIds: optionalComponentsFor(current, profile).map((component) => component.id).filter((id) => installed.includes(id)),
        primaryId: clients.includes(recordedPrimary) ? recordedPrimary : clients[0] ?? "",
        additionalIds: (existingSetup.additional_coding_environments ?? []).filter((id) => clients.includes(id) && id !== recordedPrimary),
        image: existingSetup.image_generation ? { primary_provider: existingSetup.image_generation.primary_provider, additional_providers: existingSetup.image_generation.additional_providers ?? [], route_settings: existingSetup.image_generation.route_settings ?? {} } : defaultImageChoice(current),
        portraitChoice: recordedPortrait?.enabled ? { provider: recordedPortrait.provider as PortraitChoice["provider"], localComfyuiRoot: recordedPortrait.local_comfyui_root, localServerUrl: recordedPortrait.local_server_url, runpodUrl: recordedPortrait.runpod_url, runpodWorkspace: recordedPortrait.runpod_workspace } : NO_PORTRAITS,
        liveChoice: recordedLive && recordedLive.provider === "clausewitz_mcp" ? recordedLive : NO_LIVE_TESTING,
        mode: "repair",
      });
      return;
    }
    setStep("components");
  });

  const goToComponents = () => run("Reading the setup source", async () => {
    if (!analysis || !provenance) throw new Error(`Ask ${assistantLabel} for the project details first.`);
    const current = preview ?? await loadSource();
    // The assistant's recommendations are preselected once per analysis;
    // later changes are the user's.
    if (seededAnalysis.current !== analysis.analysis_id) {
      seededAnalysis.current = analysis.analysis_id;
      const recommendedIds = new Set(analysis.component_recommendations.filter((item) => item.recommendation !== "not_recommended").map((item) => item.component_id));
      const currentProfile = current.profiles.find((item) => item.id === profileId) ?? current.profiles.find((item) => item.default) ?? current.profiles[0];
      setOptional(new Set(optionalComponentsFor(current, currentProfile).filter((component) => recommendedIds.has(component.id) && !DESKTOP_ONLY_COMPONENTS.has(component.id)).map((component) => component.id)));
    }
    setStep("components");
  });

  const changePlatform = (next: Platform) => run("Checking clients for this computer", async () => {
    setPlatform(next);
    const updated = await client().call("preview", { platform: next });
    setPreview(updated);
    const available = new Set(updated.coding_environments.map((environment) => environment.id));
    if (!available.has(primary)) setPrimary(updated.coding_environments.find((environment) => environment.default_primary)?.id ?? updated.coding_environments[0]?.id ?? "");
    setAdditional((current) => new Set([...current].filter((id) => available.has(id))));
  });

  const profile = preview?.profiles.find((item) => item.id === profileId);
  const optionalComponents = useMemo(() => optionalComponentsFor(preview, profile), [preview, profile]);
  const clients = useMemo(() => [primary, ...additional].filter(Boolean), [primary, additional]);
  const imageProviders = useMemo(() => availableImageProviders(preview, clients, primary), [preview, clients, primary]);
  const effectiveImage = useMemo(() => normalizeImageChoice(imageChoice ?? defaultImageChoice(preview), imageProviders), [imageChoice, imageProviders, preview]);
  const portraitsAvailable = Boolean(preview?.components.some((component) => component.id === "workflow.portraits.core"));
  const liveProviders = preview?.live_testing?.providers ?? [];
  const hasIntegrations = imageProviders.length > 0 || portraitsAvailable || liveProviders.some((provider) => provider.id === "clausewitz_mcp");
  const integrationsProblem = liveTestingProblem(live, platform) ?? portraitProblem(portrait) ?? (Object.entries(effectiveImage.route_settings).some(([, settings]) => settings.server_url && !isLoopbackUrl(settings.server_url)) ? "A local image server must use a loopback address such as http://127.0.0.1:8188." : null);

  const conventionSuggestions = useMemo(() => {
    if (!analysis) return [] as Array<[string, string]>;
    const labels: Array<[string, string]> = [["agents_profile", "Project instructions"], ["localisation_convention", "Localisation"], ["documentation_convention", "Documentation"], ["folder_profile", "Starter folders"]];
    return labels.flatMap(([key, label]) => {
      const value = proposal(analysis, key);
      const text = Array.isArray(value) ? value.join(", ") : typeof value === "string" ? value : "";
      return text ? [[label, text] as [string, string]] : [];
    });
  }, [analysis]);

  const recommended = useMemo(() => new Set((analysis?.component_recommendations ?? []).filter((item) => item.recommendation !== "not_recommended").map((item) => item.component_id)), [analysis]);

  interface Plan {
    profileComponents: string[];
    optionalIds: string[];
    primaryId: string;
    additionalIds: string[];
    image: ImageChoice;
    portraitChoice: PortraitChoice;
    liveChoice: LiveTestingChoice;
    mode: "setup" | MaintenanceChoice;
  }

  /** Select, download and verify, adapt, and compare: the reviewed result. */
  const prepareWith = async (current: SourcePreview, plan: Plan) => {
    const selected = [...plan.profileComponents, ...plan.optionalIds].filter((id) => !DESKTOP_ONLY_COMPONENTS.has(id));
    const hasImageCatalog = Boolean(current.image_generation);
    const selection = await client().call("select", {
      request: {
        selectedComponents: selected,
        primaryCodingEnvironment: plan.primaryId,
        additionalCodingEnvironments: plan.additionalIds,
        platform,
        ...(hasImageCatalog ? { imageGeneration: plan.image } : {}),
        liveTesting: plan.liveChoice,
        portrait: plan.portraitChoice,
      },
    });
    const stop = client().onProgress((done, total) => setBusy({ label: "Downloading verified files", done, total }));
    try {
      setBusy({ label: "Downloading verified files", done: 0, total: new Set(selection.files.map((file) => file.source_path)).size });
      const expected = Object.fromEntries(selection.files.filter((file) => file.expected_sha256).map((file) => [file.source_path, file.expected_sha256 as string]));
      const resolved = resolvedSource.current;
      if (!resolved) throw new Error("Choose the setup source first.");
      await client().call("download", { revision: resolved.revision, expected });
    } finally {
      stop();
    }
    setBusy({ label: "Comparing", done: 0, total: 0 });
    const maintaining = plan.mode !== "setup";
    const request: ExportRequest = maintaining
      ? { maintenance: plan.mode as "update" | "repair" | "change", layout, installedAt: new Date().toISOString() }
      : {
        identity: {
          displayName: identity.displayName.trim(),
          projectId: identity.projectId,
          author: identity.author.trim(),
          scriptPrefix: identity.scriptPrefix.trim() || null,
          primaryNamespace: identity.primaryNamespace.trim() || null,
          descriptorTags: identity.descriptorTags,
        },
        description: identity.description.trim(),
        conventions: {
          agentsProfile: (analysis && typeof proposal(analysis, "agents_profile") === "string" ? proposal(analysis, "agents_profile") as string : profile?.display_name) ?? "",
          localisationConvention: detectedString(scan, "localisation_convention") ?? (analysis && typeof proposal(analysis, "localisation_convention") === "string" ? proposal(analysis, "localisation_convention") as string : ""),
          documentationConvention: detectedString(scan, "documentation_convention") ?? (analysis && typeof proposal(analysis, "documentation_convention") === "string" ? proposal(analysis, "documentation_convention") as string : ""),
        },
        layout,
        installedAt: new Date().toISOString(),
        thumbnailPng: thumbnail,
        assistant: provenance,
        contextDocuments: documents.filter((entry) => entry.keepCopy).map((entry) => ({ name: entry.document.name, data: entry.data })),
        contextImages: images.filter((image) => image.keep && image.reference).map((image) => ({ name: image.name, data: image.reference as string })),
      };
    const result = await client().call("export", { existing: kind === "existing", request });
    setEntries(result);
    setDecisions(Object.fromEntries(result.filter((entry) => entry.action === "conflict").map((entry) => [entry.path, "keep" as Decision])));
    setComparison(null);
    setMerged({});
    setMergeErrors({});
    setBasesReady(false);
    setReadiness(await client().call("readiness", {}));
    if (result.some((entry) => entry.mergeable)) {
      setBusy({ label: "Fetching the installed versions for merging", done: 0, total: 0 });
      const bases = await client().call("fetchBases", {});
      setMergeErrors(Object.fromEntries(bases.failed.map((item) => [item.path, `Merging is unavailable: ${item.reason}`])));
      setBasesReady(true);
    }
    setStep("review");
  };

  const prepare = () => run("Downloading verified files", async () => {
    const current = preview ?? await loadSource();
    await prepareWith(current, {
      profileComponents: profile?.components ?? [],
      optionalIds: [...optional],
      primaryId: primary,
      additionalIds: [...additional],
      image: effectiveImage,
      portraitChoice: portrait,
      liveChoice: live,
      mode: kind === "existing" && existingSetup && maintenance ? maintenance : "setup",
    });
  });

  const compare = (path: string) => run("Opening the comparison", async () => {
    setComparison(await client().call("compare", { path }));
  });

  const previewMerge = (path: string) => run("Merging", async () => {
    try {
      const text = await client().call("merge", { path });
      setMerged((current) => ({ ...current, [path]: text }));
      setMergeErrors((current) => {
        const next = { ...current };
        delete next[path];
        return next;
      });
      setComparison(await client().call("compare", { path }));
    } catch (cause) {
      setMergeErrors((current) => ({ ...current, [path]: `The files cannot be merged automatically: ${cause instanceof Error ? cause.message : String(cause)}` }));
    }
  });

  const download = () => run("Building the ZIP", async () => {
    const bytes = await client().call("zip", { decisions, layout });
    const name = identity.projectId || existingSetup?.project_id || folder?.name || "mod";
    const suffix = kind === "existing" ? (maintenance === "remove" ? "-without-setup" : layout === "overlay" ? "-setup" : "-with-setup") : "";
    saveBlob(bytes, `${name}${suffix}.zip`);
    setStep("done");
  });

  const downloadLauncher = () => run("Writing the launcher file", async () => {
    if (!folder) return;
    const text = await client().call("launcher", { folder: folder.name });
    saveBlob(text, `${existingSetup?.project_id ?? identity.projectId}.mod`, "text/plain");
  });

  const removal = kind === "existing" && maintenance === "remove";
  const maintaining = kind === "existing" && Boolean(existingSetup) && maintenance !== null;
  const steps: Step[] = maintaining
    ? maintenance === "update" || maintenance === "change"
      ? ["start", "components", ...(hasIntegrations ? ["integrations" as Step] : []), "review", "done"]
      : ["start", "review", "done"]
    : kind === "existing"
      ? ["start", "details", "components", ...(hasIntegrations ? ["integrations" as Step] : []), "review", "done"]
      : ["start", "describe", "details", "components", ...(hasIntegrations ? ["integrations" as Step] : []), "review", "done"];
  const stepIndex = steps.indexOf(step);
  const previous = stepIndex > 0 ? steps[stepIndex - 1] : null;

  const startContinue = () => {
    if (maintaining && maintenance) startMaintenance(maintenance);
    else if (kind === "existing") analyze();
    else setStep("describe");
  };

  return (
    <div className="app-shell web-shell">
      <header className="titlebar">
        <img className="brand-mark" src={`${import.meta.env.BASE_URL}icon.png`} alt="" />
        <span className="brand-name">HOI4 Mod Setup</span>
        <span className="web-badge">Web edition</span>
        <a className="web-header-link external-link" href="https://github.com/klimPaskov/HOI4-Mod-Setup/releases/latest">Desktop app</a>
      </header>
      <div className="workspace">
        <nav className="phase-rail" aria-label="Setup steps">
          <div className="rail-label">Setup</div>
          <ol className="web-steps">
            {steps.map((item, index) => (
              <li key={item} className={`phase-item${index === stepIndex ? " active" : ""}${index < stepIndex ? " complete" : ""}`} aria-current={index === stepIndex ? "step" : undefined}>
                <span className="phase-number" aria-hidden="true">{index < stepIndex ? "✓" : index + 1}</span>
                {STEP_LABELS[item]}
              </li>
            ))}
          </ol>
          <NetworkPanel records={network} />
        </nav>
        <main className="main-viewport">
          <div className="content-scroll">
            {step === "start" && (
              <Screen title="Prepare a mod in your browser" sentence="Your files stay on this computer. Nothing is uploaded." headingRef={headingRef}>
                <div className="choice-grid">
                  <button type="button" className={`choice-card${kind === "new" ? " selected" : ""}`} aria-pressed={kind === "new"} onClick={chooseNew} disabled={busy !== null}>
                    <span className="choice-icon" aria-hidden="true">+</span>
                    <strong className="web-card-title">New mod</strong>
                    <span className="muted">Start an empty mod with both descriptors and a thumbnail.</span>
                  </button>
                  <div
                    className={`choice-card web-dropzone${dragging ? " dragging" : ""}${kind === "existing" ? " selected" : ""}`}
                    onDragOver={(event) => { event.preventDefault(); setDragging(true); }}
                    onDragLeave={() => setDragging(false)}
                    onDrop={onDrop}
                  >
                    <span className="choice-icon" aria-hidden="true">⇩</span>
                    <strong className="web-card-title">Existing mod</strong>
                    <span className="muted">Drop the mod folder here, or</span>
                    <label className="button web-pick">
                      Choose folder
                      <input type="file" className="visually-hidden" onChange={onPick} disabled={busy !== null} {...{ webkitdirectory: "", directory: "" }} />
                    </label>
                  </div>
                </div>
                {folder && scan && (
                  <section className="panel web-scan" aria-label="Scan result">
                    <h2 className="panel-title">{folder.name}</h2>
                    <p className="web-panel-text muted">
                      {scan.stats ? `${scan.stats.files_indexed.toLocaleString()} files indexed, ${scan.stats.samples_read} read for detection.` : `${folder.files.length} files.`}
                      {folder.skipped.length > 0 && ` Skipped ${folder.skipped.join(", ")}.`}
                    </p>
                    {scan.detected.length > 0 && (
                      <ul className="web-detected">
                        {scan.detected.map((item) => (
                          <li key={item.key}><span className="status status-pass"><span className="status-symbol" aria-hidden="true">●</span>Detected</span> <strong>{item.key.replace(/_/g, " ")}</strong>: <span className="mono">{Array.isArray(item.value) ? item.value.join(", ") : String(item.value)}</span></li>
                        ))}
                      </ul>
                    )}
                    {scan.issues.length > 0 && <p className="web-panel-text status-review">{scan.issues.join(" ")}</p>}
                    {usesAssistant && <div className="web-panel-text"><FindingsPanel findings={scan.findings} excluded={excluded} onToggle={(id, include) => { setExcluded((current) => { const next = new Set(current); if (include) next.delete(id); else next.add(id); return next; }); resetAnalysis(); }} assistantName={assistantLabel} /></div>}
                  </section>
                )}
                {scan?.existing_setup && <MaintenancePanel setup={scan.existing_setup} choice={existingSetup ? maintenance : null} onChoose={(choice) => { setMaintenance(choice); resetAnalysis(); }} />}
                {maintaining && maintenance === "update" && <UpdateSource mode={sourceMode} reference={sourceRef} onMode={(mode) => { setSourceMode(mode); setPreview(null); }} onReference={(value) => { setSourceRef(value); setPreview(null); }} />}
                {usesAssistant && (
                  <section className="panel form-panel web-assistant" aria-labelledby="assistant-title">
                    <h2 id="assistant-title" className="panel-title">Setup assistant</h2>
                    <p className="muted">{assistantProvider.local
                      ? "It fills in the mod's details and recommends components; you review everything before the ZIP is made. Requests go only to the model server on this computer."
                      : `It fills in the mod's details and recommends components; you review everything before the ZIP is made. Your API key stays in this tab's memory, is sent only to ${new URL(assistantProvider.endpoint).host}, and is forgotten when you close or reload the page.`}</p>
                    <div className="form-grid">
                      <label className="field">
                        <span className="field-label">AI provider</span>
                        <select className="text-input" value={assistantId} onChange={(event) => chooseAssistant(event.target.value)}>
                          {BROWSER_PROVIDERS.map((item) => <option key={item.id} value={item.id}>{item.label}</option>)}
                        </select>
                      </label>
                      {assistantProvider.local ? (
                        <label className="field">
                          <span className="field-label">Server address</span>
                          <input className="text-input mono" value={assistantEndpoint} onChange={(event) => { setAssistantEndpoint(event.target.value); resetAnalysis(); }} />
                        </label>
                      ) : (
                        <label className="field">
                          <span className="field-label">API key</span>
                          <input className="text-input mono" type="password" autoComplete="off" spellCheck={false} value={apiKey} onChange={(event) => { setApiKey(event.target.value); resetAnalysis(); }} />
                        </label>
                      )}
                      <label className="field">
                        <span className="field-label">Model</span>
                        <input className="text-input mono" list="assistant-models" value={assistantModel} onChange={(event) => { setAssistantModel(event.target.value); resetAnalysis(); }} />
                        <datalist id="assistant-models">{modelChoices(assistantProvider).map((model) => <option key={model} value={model} />)}</datalist>
                      </label>
                      {assistantProvider.efforts.length > 1 && (
                        <label className="field">
                          <span className="field-label">Reasoning effort</span>
                          <select className="text-input" value={assistantEffort} onChange={(event) => { setAssistantEffort(event.target.value); resetAnalysis(); }}>
                            {assistantProvider.efforts.map((effort) => <option key={effort} value={effort}>{effort}</option>)}
                          </select>
                        </label>
                      )}
                    </div>
                    <a className="external-link" href={assistantProvider.keyUrl} target="_blank" rel="noreferrer noopener">{assistantProvider.local ? "Local model setup" : `Get a ${assistantLabel} API key`}</a>
                  </section>
                )}
                {kind === "existing" && usesAssistant && scan && (
                  <ContextPanel documents={documents} images={images} refused={refused} onAddDocuments={addDocuments} onAddImages={addImages}
                    onDocument={(id, change) => { setDocuments((current) => current.map((entry) => entry.document.id === id ? { ...entry, ...change } : entry)); if ("include" in change) resetAnalysis(); }}
                    onImage={(id, change) => setImages((current) => current.map((image) => image.id === id ? { ...image, ...change } : image))}
                    onRemoveDocument={(id) => { setDocuments((current) => current.filter((entry) => entry.document.id !== id)); resetAnalysis(); }}
                    onRemoveImage={(id) => setImages((current) => current.filter((image) => image.id !== id))}
                    onThumbnail={takeThumbnail} assistantName={assistantLabel} disabled={busy !== null} />
                )}
                {!maintaining && (
                  <details className="web-source">
                    <summary className="details">Setup source</summary>
                    <SourceOptions mode={sourceMode} reference={sourceRef} onMode={(mode) => { setSourceMode(mode); setPreview(null); }} onReference={(value) => { setSourceRef(value); setPreview(null); }} />
                  </details>
                )}
              </Screen>
            )}

            {step === "describe" && (
              <Screen title="Describe the mod" sentence={`${assistantLabel} fills in the ID, namespace, prefix, tags, and starter folders from this.`} headingRef={headingRef}>
                <div className="panel form-panel">
                  <TextField label="Mod name" value={identity.displayName} onChange={(value) => updateIdentity("displayName", value)} />
                  <label className="field">
                    <span className="field-label">What the mod is about</span>
                    <textarea className="brief-input web-brief" value={brief} onChange={(event) => { setBrief(event.target.value); resetAnalysis(); }} />
                  </label>
                </div>
                <ContextPanel documents={documents} images={images} refused={refused} onAddDocuments={addDocuments} onAddImages={addImages}
                  onDocument={(id, change) => { setDocuments((current) => current.map((entry) => entry.document.id === id ? { ...entry, ...change } : entry)); if ("include" in change) resetAnalysis(); }}
                  onImage={(id, change) => setImages((current) => current.map((image) => image.id === id ? { ...image, ...change } : image))}
                  onRemoveDocument={(id) => { setDocuments((current) => current.filter((entry) => entry.document.id !== id)); resetAnalysis(); }}
                  onRemoveImage={(id) => setImages((current) => current.filter((image) => image.id !== id))}
                  onThumbnail={takeThumbnail} assistantName={assistantLabel} disabled={busy !== null} />
              </Screen>
            )}

            {step === "details" && analysis && (
              <Screen title="Review the details" sentence={kind === "existing" ? `Values found in the folder are kept; ${assistantName} filled in the rest. Change anything that is wrong.` : `${assistantName} filled these in. Change anything that is wrong.`} headingRef={headingRef}>
                <section className="panel web-assistant-result" aria-label={`${assistantName} summary`}>
                  <p>{analysis.project_summary}</p>
                  {analysis.warnings.map((warning) => <p key={warning} className="status-review">{warning}</p>)}
                  {conventionSuggestions.length > 0 && (
                    <dl className="web-conventions">
                      {conventionSuggestions.map(([label, value]) => <div key={label}><dt className="field-label">{label}</dt><dd>{value}</dd></div>)}
                    </dl>
                  )}
                  {kind === "existing" && evidencePreview && (
                    <details className="web-evidence">
                      <summary className="details">What {assistantName} received ({evidencePreview.length} scan findings)</summary>
                      <ul className="web-file-list">
                        {evidencePreview.map((item) => <li key={item.reference}><span className="mono">{item.reference}</span> <span className="muted">{item.excerpt.length > 160 ? `${item.excerpt.slice(0, 160)}…` : item.excerpt}</span></li>)}
                      </ul>
                    </details>
                  )}
                  <button type="button" className="text-button" disabled={busy !== null} onClick={analyze}>Ask {assistantName} again</button>
                </section>
                <div className="panel form-panel">
                  <div className="form-grid">
                    <TextField label="Mod name" value={identity.displayName} onChange={(value) => updateIdentity("displayName", value)} badge={badge("displayName", "display_name")} />
                    <TextField label="Mod ID" value={identity.projectId} onChange={(value) => updateIdentity("projectId", value.toLowerCase())} mono badge={badge("projectId")} />
                    <TextField label="Event namespace" value={identity.primaryNamespace} onChange={(value) => updateIdentity("primaryNamespace", value)} mono badge={badge("primaryNamespace", "primary_namespace")} />
                    <TextField label="Script prefix" value={identity.scriptPrefix} onChange={(value) => updateIdentity("scriptPrefix", value)} mono badge={badge("scriptPrefix", "script_prefix")} />
                    <TextField label="Author (optional)" value={identity.author} onChange={(value) => updateIdentity("author", value)} />
                  </div>
                  <label className="field">
                    <span className="field-label">Description{badge("description") && <span className="status status-info web-detected-label"> · {badge("description")}</span>}</span>
                    <textarea className="brief-input web-brief" value={identity.description} onChange={(event) => updateIdentity("description", event.target.value)} />
                  </label>
                  <fieldset className="tag-picker">
                    <legend className="field-label">Launcher tags{badge("descriptorTags", "descriptor_tags") && <span className="status status-info web-detected-label"> · {badge("descriptorTags", "descriptor_tags")}</span>}</legend>
                    <div className="tag-options">
                      {HOI4_DESCRIPTOR_TAGS.map((tag) => (
                        <label key={tag} className="check-row web-tag">
                          <input type="checkbox" checked={identity.descriptorTags.includes(tag)} onChange={(event) => updateIdentity("descriptorTags", event.target.checked ? [...identity.descriptorTags, tag] : identity.descriptorTags.filter((item) => item !== tag))} />
                          {tag}
                        </label>
                      ))}
                    </div>
                  </fieldset>
                  <div className="web-thumbnail">
                    {thumbnail ? <img src={`data:image/png;base64,${thumbnail}`} alt="Thumbnail preview" width={96} height={96} /> : <span className="muted">{kind === "existing" ? "The current thumbnail is kept." : "A placeholder thumbnail is used."}</span>}
                    <label className="button">
                      {thumbnail ? "Change image" : "Use an image as the thumbnail"}
                      <input type="file" accept="image/png,image/jpeg,image/webp" className="visually-hidden" onChange={chooseThumbnail} />
                    </label>
                    {thumbnail && <button type="button" className="text-button" onClick={() => setThumbnail(null)}>Remove</button>}
                  </div>
                </div>
              </Screen>
            )}

            {step === "components" && preview && (
              <Screen title="Choose components" sentence={`Agentic HOI4 Modding at ${preview.revision.slice(0, 12)}.`} headingRef={headingRef}>
                <div className="two-column">
                  <section className="panel">
                    <h2 className="panel-title">Workflow</h2>
                    {preview.profiles.map((item) => (
                      <label key={item.id} className="radio-row">
                        <input type="radio" name="profile" checked={profileId === item.id} onChange={() => setProfileId(item.id)} />
                        {item.display_name}
                      </label>
                    ))}
                    {optionalComponents.map((component) => {
                      const desktopOnly = DESKTOP_ONLY_COMPONENTS.has(component.id);
                      return (
                        <label key={component.id} className="check-row">
                          <input type="checkbox" disabled={desktopOnly} checked={optional.has(component.id)} onChange={(event) => setOptional((current) => { const next = new Set(current); if (event.target.checked) next.add(component.id); else next.delete(component.id); return next; })} />
                          <span>{component.display_name}{desktopOnly && <span className="muted"> — desktop app only</span>}{!desktopOnly && recommended.has(component.id) && <span className="status-info"> · Recommended by {assistantName}</span>}</span>
                        </label>
                      );
                    })}
                  </section>
                  <section className="panel">
                    <h2 className="panel-title">Coding clients</h2>
                    <label className="field web-field">
                      <span className="field-label">Primary</span>
                      <select className="text-input" value={primary} onChange={(event) => { setPrimary(event.target.value); setAdditional((current) => { const next = new Set(current); next.delete(event.target.value); return next; }); }}>
                        {preview.coding_environments.map((environment) => <option key={environment.id} value={environment.id}>{environment.display_name}</option>)}
                      </select>
                    </label>
                    <details className="web-additional">
                      <summary className="details">Additional clients{additional.size > 0 ? ` (${additional.size})` : ""}</summary>
                      {preview.coding_environments.filter((environment) => environment.id !== primary).map((environment) => (
                        <label key={environment.id} className="check-row">
                          <input type="checkbox" checked={additional.has(environment.id)} onChange={(event) => setAdditional((current) => { const next = new Set(current); if (event.target.checked) next.add(environment.id); else next.delete(environment.id); return next; })} />
                          {environment.display_name}
                        </label>
                      ))}
                      {preview.unavailable_coding_environments.length > 0 && (
                        <ul className="web-unavailable">
                          {preview.unavailable_coding_environments.map((environment) => <li key={environment.id} className="muted">{environment.display_name}: {environment.reason}</li>)}
                        </ul>
                      )}
                    </details>
                    <label className="field web-field">
                      <span className="field-label">Your computer</span>
                      <select className="text-input" value={platform} disabled={busy !== null} onChange={(event) => changePlatform(event.target.value as Platform)}>
                        <option value="windows">Windows</option>
                        <option value="macos">macOS</option>
                        <option value="linux">Linux</option>
                      </select>
                    </label>
                  </section>
                </div>
              </Screen>
            )}

            {step === "integrations" && preview && (
              <Screen title="Choose integrations" sentence="Image generation, portraits, and live testing. Each one is optional." headingRef={headingRef}>
                <IntegrationsStep providers={imageProviders} image={effectiveImage} onImage={setImageChoice} portraitsAvailable={portraitsAvailable} portrait={portrait} onPortrait={setPortrait} liveProviders={liveProviders} live={live} onLive={setLive} platform={platform} />
              </Screen>
            )}

            {step === "review" && entries && (
              <Screen title="Review the changes" sentence={entries.some((entry) => entry.action === "conflict") ? "Some files in your folder differ from the incoming version. Choose what to keep." : undefined} headingRef={headingRef}>
                <ReviewStep entries={entries} decisions={decisions} onDecision={(path, decision) => setDecisions((current) => ({ ...current, [path]: decision }))} merged={merged} mergeErrors={mergeErrors} basesReady={basesReady} onMerge={previewMerge} onCompare={compare} comparison={comparison} onCloseComparison={() => setComparison(null)} existing={kind === "existing"} removal={removal} layout={layout} onLayout={setLayout} busy={busy !== null} />
              </Screen>
            )}

            {step === "done" && (
              <Screen title="Your ZIP is ready" headingRef={headingRef}>
                <DoneStep kind={kind === "new" ? "new" : "existing"} removal={removal} layout={layout} projectId={identity.projectId || existingSetup?.project_id || ""} folderName={kind === "existing" ? folder?.name ?? null : null} checks={readiness} onDownloadAgain={download} onLauncher={kind === "existing" && !removal ? downloadLauncher : null} />
              </Screen>
            )}

            {busy && (
              <div className="web-progress" role="status" aria-live="polite">
                <span>{busy.label}{busy.total > 0 ? ` (${busy.done} of ${busy.total})` : "…"}</span>
                <progress max={busy.total || undefined} value={busy.total ? busy.done : undefined} />
              </div>
            )}
            {error && (
              <div className="web-error-row">
                <p className="web-error status-block" role="alert"><span className="status-symbol" aria-hidden="true">!</span> {error}</p>
                <CopyDetails error={error} step={STEP_LABELS[step]} revision={source?.revision ?? null} network={network} />
              </div>
            )}
          </div>
          <footer className="footer-bar">
            <span className="footer-note">Source: <a className="external-link" href={`https://github.com/${SOURCE_OWNER}/${SOURCE_NAME}`}>{SOURCE_OWNER}/{SOURCE_NAME}</a>{source ? <> at <span className="mono">{source.revision.slice(0, 12)}</span></> : null}</span>
            <div className="footer-actions">
              {previous && step !== "done" && <button type="button" className="button" disabled={busy !== null} onClick={() => setStep(previous)}>Back</button>}
              {step === "start" && <button type="button" className="button primary" disabled={busy !== null || startProblem !== null} title={startProblem ?? undefined} onClick={startContinue}>Continue</button>}
              {step === "describe" && <button type="button" className="button primary" disabled={busy !== null || briefProblem !== null} title={briefProblem ?? undefined} onClick={analyze}>Continue</button>}
              {step === "details" && <button type="button" className="button primary" disabled={busy !== null || identityProblem !== null} title={identityProblem ?? undefined} onClick={goToComponents}>Continue</button>}
              {step === "components" && (hasIntegrations
                ? <button type="button" className="button primary" disabled={busy !== null || !primary} onClick={() => setStep("integrations")}>Continue</button>
                : <button type="button" className="button primary" disabled={busy !== null || !primary} onClick={prepare}>Download and review</button>)}
              {step === "integrations" && <button type="button" className="button primary" disabled={busy !== null || integrationsProblem !== null} title={integrationsProblem ?? undefined} onClick={prepare}>Download and review</button>}
              {step === "review" && <button type="button" className="button primary" disabled={busy !== null} onClick={download}>Download ZIP</button>}
              {step === "done" && <button type="button" className="button" onClick={() => { setStep("start"); setEntries(null); }}>Prepare another mod</button>}
            </div>
          </footer>
          {(step === "start" ? startProblem : step === "describe" ? briefProblem : step === "details" ? identityProblem : step === "integrations" ? integrationsProblem : null) && <p className="visually-hidden" aria-live="polite">{step === "start" ? startProblem : step === "describe" ? briefProblem : step === "details" ? identityProblem : integrationsProblem}</p>}
        </main>
      </div>
    </div>
  );
}

/** Optional components offered beside the chosen workflow profile. */
function optionalComponentsFor(preview: SourcePreview | null, profile: SourcePreview["profiles"][number] | undefined): SourcePreview["components"] {
  const components = preview?.components ?? [];
  const optionalIds = new Set(components.filter((component) => component.optional).map((component) => component.id));
  // Optional components that exist only to support another choice are
  // installed through that choice, as on the desktop.
  const dependencyOnly = new Set(components.filter((component) => component.optional).flatMap((component) => component.dependencies).filter((id) => optionalIds.has(id)));
  return components.filter((component) =>
    component.optional
    && !component.coding_environment
    && !(profile?.components.includes(component.id))
    && (ALWAYS_LISTED.has(component.id) || !dependencyOnly.has(component.id))
    && !HIDDEN_COMPONENT_PREFIXES.some((prefix) => component.id.startsWith(prefix)));
}

function SourceOptions({ mode, reference, onMode, onReference }: { mode: SourceMode; reference: string; onMode: (mode: SourceMode) => void; onReference: (value: string) => void }) {
  return (
    <fieldset className="update-source-options">
      <legend className="field-label">Agentic HOI4 Modding version</legend>
      {(["latest", "pinned_commit", "pinned_release"] as SourceMode[]).map((value) => (
        <label key={value} className="radio-row">
          <input type="radio" name="source" checked={mode === value} onChange={() => onMode(value)} />
          {value === "latest" ? "Latest commit on the default branch" : value === "pinned_commit" ? "A specific commit" : "A release"}
        </label>
      ))}
      {mode !== "latest" && (
        <label className="field">
          <span className="field-label">{mode === "pinned_commit" ? "Commit ID" : "Release tag"}</span>
          <input className="text-input mono" value={reference} onChange={(event) => onReference(event.target.value)} />
        </label>
      )}
    </fieldset>
  );
}

function UpdateSource(props: { mode: SourceMode; reference: string; onMode: (mode: SourceMode) => void; onReference: (value: string) => void }) {
  return (
    <details className="panel web-source">
      <summary className="details">Update to</summary>
      <SourceOptions {...props} />
    </details>
  );
}

function Screen({ title, sentence, children, headingRef }: { title: string; sentence?: string; children: ReactNode; headingRef: RefObject<HTMLHeadingElement | null> }) {
  return (
    <>
      <div className="screen-heading">
        <div>
          <h1 ref={headingRef} tabIndex={-1} className="web-title">{title}</h1>
          {sentence && <p className="muted web-sentence">{sentence}</p>}
        </div>
      </div>
      <div className="stack">{children}</div>
    </>
  );
}

function TextField({ label, value, onChange, mono, badge }: { label: string; value: string; onChange: (value: string) => void; mono?: boolean; badge?: string }) {
  return (
    <label className="field">
      <span className="field-label">{label}{badge && <span className={`status ${badge === "Detected" ? "status-pass" : "status-info"} web-detected-label`}> · {badge}</span>}</span>
      <input className={`text-input${mono ? " mono" : ""}`} value={value} onChange={(event) => onChange(event.target.value)} />
    </label>
  );
}

/** Copy what went wrong for a report: the message, the step, the source
 * revision, and recent request addresses. Never a key or file contents. */
function CopyDetails({ error, step, revision, network }: { error: string; step: string; revision: string | null; network: NetworkRecord[] }) {
  const [copied, setCopied] = useState(false);
  const copy = async () => {
    const lines = [
      `HOI4 Mod Setup web edition`,
      `Step: ${step}`,
      `Source revision: ${revision ?? "not resolved"}`,
      `Error: ${error}`,
      "Recent requests:",
      ...network.slice(-15).map((record) => {
        const url = new URL(record.url);
        return `  ${record.status} ${url.host}${url.pathname}`;
      }),
    ];
    try {
      await navigator.clipboard.writeText(lines.join("\n"));
      setCopied(true);
    } catch {
      setCopied(false);
    }
  };
  return <button type="button" className="text-button" onClick={copy}>{copied ? "Copied" : "Copy details"}</button>;
}

function NetworkPanel({ records }: { records: NetworkRecord[] }) {
  return (
    <details className="web-network">
      <summary className="details">Network ({records.length})</summary>
      <p className="muted">Every request this page made. Your mod files are never sent.</p>
      <ul>
        {records.slice(-200).map((record, index) => {
          const url = new URL(record.url);
          return <li key={`${record.at}-${index}`} className="mono"><span className={typeof record.status === "number" && record.status < 400 ? "status-pass" : "status-block"}>{record.status}</span> {url.host}{url.pathname.length > 48 ? `…${url.pathname.slice(-48)}` : url.pathname}</li>;
        })}
      </ul>
    </details>
  );
}
