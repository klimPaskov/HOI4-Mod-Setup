// The HOI4 Mod Setup web edition. A dropped folder stays in this browser
// tab; the setup core runs as WebAssembly in a worker, downloads only the
// selected files from GitHub at one exact commit, verifies each against the
// manifest, and returns a ZIP for the user to unpack.

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { ChangeEvent, DragEvent, ReactNode, RefObject } from "react";
import { deriveGeneratedIdentity, HOI4_DESCRIPTOR_TAGS } from "../identity";
import { renderCoverPng } from "../lib/images";
import { startCoreWorker } from "./core-client";
import type { CoreClient } from "./core-client";
import { folderFromDrop, folderFromInput } from "./folder";
import type { FolderSelection } from "./folder";
import { fetchRaw, MANIFEST_PATH, networkLog, onNetworkLog, resolveSource, SOURCE_NAME, SOURCE_OWNER } from "./github";
import type { NetworkRecord, ResolvedSource, SourceMode } from "./github";
import type { Decision, ExportEntry, Comparison, Platform, ScanSummary, SourcePreview } from "./protocol";

type Step = "start" | "describe" | "components" | "review" | "done";
const STEPS: Array<{ id: Step; label: string }> = [
  { id: "start", label: "Project" },
  { id: "describe", label: "Describe" },
  { id: "components", label: "Components" },
  { id: "review", label: "Review" },
  { id: "done", label: "Download" },
];
/** Needs the desktop app's local Meshy launcher. */
const DESKTOP_ONLY_COMPONENTS = new Set(["workflow.3d"]);
const READ_BATCH_BYTES = 32 * 1024 * 1024;
const PROJECT_ID = /^[a-z][a-z0-9_]{1,63}$/;

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

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
}

function saveBlob(bytes: ArrayBuffer, name: string): void {
  const url = URL.createObjectURL(new Blob([bytes], { type: "application/zip" }));
  const link = document.createElement("a");
  link.href = url;
  link.download = name;
  link.click();
  setTimeout(() => URL.revokeObjectURL(url), 60_000);
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
  const [preview, setPreview] = useState<SourcePreview | null>(null);
  const [platform, setPlatform] = useState<Platform>(guessPlatform);
  const [identity, setIdentity] = useState<Identity>({ displayName: "", projectId: "", description: "", author: "", primaryNamespace: "", scriptPrefix: "", descriptorTags: [] });
  const [edited, setEdited] = useState<Set<keyof Identity>>(new Set());
  const [thumbnail, setThumbnail] = useState<string | null>(null);
  const [profileId, setProfileId] = useState<string>("");
  const [optional, setOptional] = useState<Set<string>>(new Set());
  const [primary, setPrimary] = useState("");
  const [additional, setAdditional] = useState<Set<string>>(new Set());
  const [entries, setEntries] = useState<ExportEntry[] | null>(null);
  const [decisions, setDecisions] = useState<Record<string, Decision>>({});
  const [comparison, setComparison] = useState<Comparison | null>(null);
  const [layout, setLayout] = useState<"overlay" | "full_copy">("overlay");
  const [network, setNetwork] = useState<NetworkRecord[]>(networkLog);
  const [dragging, setDragging] = useState(false);
  const headingRef = useRef<HTMLHeadingElement>(null);

  useEffect(() => onNetworkLog(setNetwork), []);
  useEffect(() => headingRef.current?.focus(), [step]);

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

  const loadSource = useCallback(async (): Promise<SourcePreview> => {
    const resolved = await resolveSource(sourceMode, sourceRef);
    const manifest = await fetchRaw(resolved.revision, MANIFEST_PATH);
    const buffer = manifest.buffer.slice(manifest.byteOffset, manifest.byteOffset + manifest.byteLength) as ArrayBuffer;
    await client().call("openSource", { manifest: buffer, revision: resolved.revision, mode: resolved.mode, requestedRef: resolved.requestedRef }, [buffer]);
    const next = await client().call("preview", { platform });
    setSource(resolved);
    setPreview(next);
    const defaultProfile = next.profiles.find((profile) => profile.default) ?? next.profiles[0];
    setProfileId((current) => current || defaultProfile?.id || "");
    const defaultClient = next.coding_environments.find((environment) => environment.default_primary) ?? next.coding_environments[0];
    setPrimary((current) => (current && next.coding_environments.some((environment) => environment.id === current) ? current : defaultClient?.id ?? ""));
    return next;
  }, [client, platform, sourceMode, sourceRef]);

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
      const buffer = await entry.file.arrayBuffer();
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

  const startNew = () => {
    setKind("new");
    setFolder(null);
    setScan(null);
    setIdentity({ displayName: "", projectId: "", description: "", author: "", primaryNamespace: "", scriptPrefix: "", descriptorTags: [] });
    setEdited(new Set());
    setStep("describe");
  };

  const updateIdentity = (key: keyof Identity, value: string | string[]) => {
    setEdited((current) => new Set(current).add(key));
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
      const dataUrl = await new Promise<string>((resolve, reject) => {
        const reader = new FileReader();
        reader.onload = () => resolve(String(reader.result));
        reader.onerror = () => reject(new Error("The image could not be read."));
        reader.readAsDataURL(file);
      });
      setThumbnail(await renderCoverPng(dataUrl, 512));
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    }
  };

  const identityProblem = useMemo(() => {
    if (!identity.displayName.trim()) return "Enter the mod name.";
    if (!PROJECT_ID.test(identity.projectId)) return "The mod ID must start with a lowercase letter and use only lowercase letters, digits, and underscores.";
    return null;
  }, [identity]);

  const goToComponents = () => run("Reading the setup source", async () => {
    if (!preview) await loadSource();
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
  const optionalComponents = useMemo(() => (preview?.components ?? []).filter((component) =>
    component.optional && !component.coding_environment && !(profile?.components.includes(component.id))), [preview, profile]);

  const prepare = () => run("Downloading verified files", async () => {
    const selected = [...(profile?.components ?? []), ...optional].filter((id) => !DESKTOP_ONLY_COMPONENTS.has(id));
    const selection = await client().call("select", {
      request: { selectedComponents: selected, primaryCodingEnvironment: primary, additionalCodingEnvironments: [...additional], platform },
    });
    const stop = client().onProgress((done, total) => setBusy({ label: "Downloading verified files", done, total }));
    try {
      setBusy({ label: "Downloading verified files", done: 0, total: new Set(selection.files.map((file) => file.source_path)).size });
      await client().call("download", { revision: source!.revision });
    } finally {
      stop();
    }
    setBusy({ label: "Comparing", done: 0, total: 0 });
    const result = await client().call("export", {
      existing: kind === "existing",
      request: {
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
          agentsProfile: profile?.display_name ?? "",
          localisationConvention: detectedString(scan, "localisation_convention") ?? "",
          documentationConvention: detectedString(scan, "documentation_convention") ?? "",
        },
        layout,
        installedAt: new Date().toISOString(),
        thumbnailPng: thumbnail,
      },
    });
    setEntries(result);
    setDecisions(Object.fromEntries(result.filter((entry) => entry.action === "conflict").map((entry) => [entry.path, "keep" as Decision])));
    setComparison(null);
    setStep("review");
  });

  const compare = (path: string) => run("Opening the comparison", async () => {
    setComparison(await client().call("compare", { path }));
  });

  const download = () => run("Building the ZIP", async () => {
    const bytes = await client().call("zip", { decisions });
    const suffix = kind === "existing" ? (layout === "overlay" ? "-setup" : "-with-setup") : "";
    saveBlob(bytes, `${identity.projectId}${suffix}.zip`);
    setStep("done");
  });

  const counts = useMemo(() => ({
    create: entries?.filter((entry) => entry.action === "create").length ?? 0,
    unchanged: entries?.filter((entry) => entry.action === "unchanged").length ?? 0,
    conflict: entries?.filter((entry) => entry.action === "conflict").length ?? 0,
  }), [entries]);
  const stepIndex = STEPS.findIndex((item) => item.id === step);

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
            {STEPS.map((item, index) => (
              <li key={item.id} className={`phase-item${index === stepIndex ? " active" : ""}${index < stepIndex ? " complete" : ""}`} aria-current={index === stepIndex ? "step" : undefined}>
                <span className="phase-number" aria-hidden="true">{index < stepIndex ? "✓" : index + 1}</span>
                {item.label}
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
                  <button type="button" className="choice-card" onClick={startNew} disabled={busy !== null}>
                    <span className="choice-icon" aria-hidden="true">+</span>
                    <strong className="web-card-title">New mod</strong>
                    <span className="muted">Start an empty mod with both descriptors and a thumbnail.</span>
                  </button>
                  <div
                    className={`choice-card web-dropzone${dragging ? " dragging" : ""}`}
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
                    <div className="proposal-actions">
                      <button type="button" className="button primary" onClick={() => setStep("describe")}>Continue</button>
                    </div>
                  </section>
                )}
                <details className="web-source">
                  <summary className="details">Setup source</summary>
                  <fieldset className="update-source-options">
                    <legend className="field-label">Agentic HOI4 Modding version</legend>
                    {(["latest", "pinned_commit", "pinned_release"] as SourceMode[]).map((mode) => (
                      <label key={mode} className="radio-row">
                        <input type="radio" name="source" checked={sourceMode === mode} onChange={() => { setSourceMode(mode); setPreview(null); }} />
                        {mode === "latest" ? "Latest commit on the default branch" : mode === "pinned_commit" ? "A specific commit" : "A release"}
                      </label>
                    ))}
                    {sourceMode !== "latest" && (
                      <label className="field">
                        <span className="field-label">{sourceMode === "pinned_commit" ? "Commit ID" : "Release tag"}</span>
                        <input className="text-input mono" value={sourceRef} onChange={(event) => { setSourceRef(event.target.value); setPreview(null); }} />
                      </label>
                    )}
                  </fieldset>
                </details>
              </Screen>
            )}

            {step === "describe" && (
              <Screen title="Describe the mod" sentence={kind === "existing" ? "Values found in the folder are filled in. Change anything that is wrong." : undefined} headingRef={headingRef}>
                <div className="panel form-panel">
                  <div className="form-grid">
                    <TextField label="Mod name" value={identity.displayName} onChange={(value) => updateIdentity("displayName", value)} detected={kind === "existing" && !edited.has("displayName") && detectedString(scan, "display_name") !== undefined} />
                    <TextField label="Mod ID" value={identity.projectId} onChange={(value) => updateIdentity("projectId", value.toLowerCase())} mono />
                    <TextField label="Event namespace" value={identity.primaryNamespace} onChange={(value) => updateIdentity("primaryNamespace", value)} mono detected={kind === "existing" && !edited.has("primaryNamespace") && detectedString(scan, "primary_namespace") !== undefined} />
                    <TextField label="Script prefix" value={identity.scriptPrefix} onChange={(value) => updateIdentity("scriptPrefix", value)} mono detected={kind === "existing" && !edited.has("scriptPrefix") && detectedString(scan, "script_prefix") !== undefined} />
                    <TextField label="Author (optional)" value={identity.author} onChange={(value) => updateIdentity("author", value)} />
                  </div>
                  <label className="field">
                    <span className="field-label">What the mod is about (optional)</span>
                    <textarea className="brief-input web-brief" value={identity.description} onChange={(event) => updateIdentity("description", event.target.value)} />
                  </label>
                  <fieldset className="tag-picker">
                    <legend className="field-label">Launcher tags</legend>
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
                          <span>{component.display_name}{desktopOnly && <span className="muted"> — desktop app only</span>}</span>
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

            {step === "review" && entries && (
              <Screen title="Review the changes" sentence={counts.conflict > 0 ? "Some files in your folder differ from the incoming version. Choose what to keep." : undefined} headingRef={headingRef}>
                <section className="panel">
                  <ul className="web-counts">
                    <li><strong>{counts.create}</strong> new</li>
                    {kind === "existing" && <li><strong>{counts.unchanged}</strong> already up to date</li>}
                    {counts.conflict > 0 && <li><strong>{counts.conflict}</strong> need a decision</li>}
                  </ul>
                  {entries.filter((entry) => entry.action === "conflict").map((entry) => (
                    <div key={entry.path} className="change-row web-conflict">
                      <span className="mono web-path">{entry.path}</span>
                      <fieldset className="web-decision">
                        <legend className="visually-hidden">{entry.path}</legend>
                        <label><input type="radio" name={`decision-${entry.path}`} checked={decisions[entry.path] === "keep"} onChange={() => setDecisions((current) => ({ ...current, [entry.path]: "keep" }))} /> Keep mine</label>
                        <label><input type="radio" name={`decision-${entry.path}`} checked={decisions[entry.path] === "replace"} onChange={() => setDecisions((current) => ({ ...current, [entry.path]: "replace" }))} /> Use incoming</label>
                        <button type="button" className="text-button" onClick={() => compare(entry.path)}>Compare</button>
                      </fieldset>
                    </div>
                  ))}
                  {comparison && <ComparisonView comparison={comparison} onClose={() => setComparison(null)} />}
                  <details className="manifest-details">
                    <summary className="details">All files</summary>
                    <ul className="web-file-list">
                      {entries.map((entry) => <li key={entry.path}><span className="mono">{entry.path}</span> <span className="muted">{entry.action} · {formatBytes(entry.size)}</span></li>)}
                    </ul>
                  </details>
                </section>
                {kind === "existing" && (
                  <fieldset className="panel update-source-options">
                    <legend className="field-label">ZIP contents</legend>
                    <label className="radio-row"><input type="radio" name="layout" checked={layout === "overlay"} onChange={() => setLayout("overlay")} /> Only new and changed files, to copy over your folder</label>
                    <label className="radio-row"><input type="radio" name="layout" checked={layout === "full_copy"} onChange={() => setLayout("full_copy")} /> A full copy of the mod with the setup included</label>
                  </fieldset>
                )}
              </Screen>
            )}

            {step === "done" && (
              <Screen title="Your ZIP is ready" headingRef={headingRef}>
                <section className="panel form-panel web-done">
                  {kind === "new" ? (
                    <ol>
                      <li>Unpack the ZIP into <span className="mono">Documents/Paradox Interactive/Hearts of Iron IV/mod/</span>.</li>
                      <li>It contains the <span className="mono">{identity.projectId}</span> folder and <span className="mono">{identity.projectId}.mod</span>; keep both there.</li>
                      <li>Open the folder in your coding client.</li>
                    </ol>
                  ) : layout === "overlay" ? (
                    <ol>
                      <li>Back up your mod folder.</li>
                      <li>Unpack the ZIP over it. <span className="mono">HOI4_MOD_SETUP_CHANGES.md</span> lists every file.</li>
                      <li>Open the folder in your coding client.</li>
                    </ol>
                  ) : (
                    <ol>
                      <li>Unpack the ZIP next to your mod folder and compare before replacing it.</li>
                      <li>Open the folder in your coding client.</li>
                    </ol>
                  )}
                  <p className="muted">The installation lock in <span className="mono">.hoi4-mod-setup/</span> records the exact source commit and every file hash, so the desktop app can update or repair this setup later.</p>
                  <div className="button-row">
                    <button type="button" className="button" onClick={download}>Download again</button>
                  </div>
                </section>
              </Screen>
            )}

            {busy && (
              <div className="web-progress" role="status" aria-live="polite">
                <span>{busy.label}{busy.total > 0 ? ` (${busy.done} of ${busy.total})` : "…"}</span>
                <progress max={busy.total || undefined} value={busy.total ? busy.done : undefined} />
              </div>
            )}
            {error && <p className="web-error status-block" role="alert"><span className="status-symbol" aria-hidden="true">!</span> {error}</p>}
          </div>
          <footer className="footer-bar">
            <span className="footer-note">Source: <a className="external-link" href={`https://github.com/${SOURCE_OWNER}/${SOURCE_NAME}`}>{SOURCE_OWNER}/{SOURCE_NAME}</a>{source ? <> at <span className="mono">{source.revision.slice(0, 12)}</span></> : null}</span>
            <div className="footer-actions">
              {step !== "start" && step !== "done" && <button type="button" className="button" disabled={busy !== null} onClick={() => setStep(STEPS[stepIndex - 1].id)}>Back</button>}
              {step === "describe" && <button type="button" className="button primary" disabled={busy !== null || identityProblem !== null} title={identityProblem ?? undefined} onClick={goToComponents}>Continue</button>}
              {step === "components" && <button type="button" className="button primary" disabled={busy !== null || !primary} onClick={prepare}>Download and review</button>}
              {step === "review" && <button type="button" className="button primary" disabled={busy !== null} onClick={download}>Download ZIP</button>}
              {step === "done" && <button type="button" className="button" onClick={() => { setStep("start"); setEntries(null); }}>Prepare another mod</button>}
            </div>
          </footer>
          {step === "describe" && identityProblem && <p className="visually-hidden" aria-live="polite">{identityProblem}</p>}
        </main>
      </div>
    </div>
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

function TextField({ label, value, onChange, mono, detected }: { label: string; value: string; onChange: (value: string) => void; mono?: boolean; detected?: boolean }) {
  return (
    <label className="field">
      <span className="field-label">{label}{detected && <span className="status status-pass web-detected-label"> · Detected</span>}</span>
      <input className={`text-input${mono ? " mono" : ""}`} value={value} onChange={(event) => onChange(event.target.value)} />
    </label>
  );
}

function ComparisonView({ comparison, onClose }: { comparison: Comparison; onClose: () => void }) {
  return (
    <section className="web-compare" aria-label={`Comparison of ${comparison.path}`}>
      <div className="proposal-heading">
        <strong className="mono">{comparison.path}</strong>
        <button type="button" className="text-button" onClick={onClose}>Close</button>
      </div>
      {comparison.binary ? <p className="muted">This file is binary and cannot be shown.</p> : (
        <div className="web-compare-grid">
          <div><div className="field-label">Yours</div><pre>{comparison.local ?? ""}</pre></div>
          <div><div className="field-label">Incoming</div><pre>{comparison.incoming ?? ""}</pre></div>
        </div>
      )}
    </section>
  );
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
