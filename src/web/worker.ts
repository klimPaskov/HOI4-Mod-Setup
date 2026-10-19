// The web edition's core worker: it loads the WebAssembly setup core, holds
// the dropped folder and the verified source files, downloads selected files
// from GitHub at the exact commit, and builds the ZIP. Keeping this off the
// page thread keeps the interface responsive on large mods.

import { cachedFile, storeFile } from "./cache";
import { rawUrl } from "./github";
import type { CoreOperations, Operation, Request, WorkerMessage } from "./protocol";

interface WasmProject {
  addFile(path: string, bytes: Uint8Array): void;
  fileCount(): number;
  scan(): string;
  analysisEvidence(excluded?: string): string;
  exportRemoval(request: string): WasmExport;
  free(): void;
}

interface WasmExport {
  entries(): string;
  compare(path: string): string;
  mergeBases(): string;
  addBase(path: string, bytes: Uint8Array): void;
  merge(path: string): string;
  readiness(): string;
  launcherFile(folder: string): string;
  zip(decisions: string, layout?: string): Uint8Array;
  free(): void;
}

interface WasmSource {
  preview(platform: string): string;
  select(request: string): string;
  accept(sourcePath: string, bytes: Uint8Array): void;
  missing(): string;
  componentRegistry(): string;
  exportNew(request: string): WasmExport;
  exportExisting(request: string, project: WasmProject): WasmExport;
  free(): void;
}

interface WasmModule {
  default(): Promise<unknown>;
  Project: new () => WasmProject;
  Source: new (manifest: Uint8Array, revision: string, mode: string, requestedRef?: string) => WasmSource;
  analysisTurn(request: string, route: string, correction?: string): string;
  readAnalysisReply(envelope: string, request: string, inputSha256: string, analysisId: string): string;
  contextDocument(number: number, name: string, bytes: Uint8Array): string;
  descriptionTurn(route: string, note: string): string;
  readDescriptionReply(envelope: string): string;
  openRouterSignIn(callback: string, random: Uint8Array): string;
  openRouterKeyRequest(code: string, verifier: string): string;
  openRouterKey(response: string): string;
}

const scope = self as unknown as {
  postMessage(message: WorkerMessage, transfer?: Transferable[]): void;
  onmessage: ((event: MessageEvent<Request>) => void) | null;
};

let core: WasmModule | undefined;
let project: WasmProject | undefined;
let source: WasmSource | undefined;
let exported: WasmExport | undefined;
const DOWNLOAD_CONCURRENCY = 8;

async function load(): Promise<WasmModule> {
  if (!core) {
    const module = await import(/* @vite-ignore */ `${import.meta.env.BASE_URL}core/hoi4_setup_web.js`) as WasmModule;
    await module.default();
    core = module;
  }
  return core;
}

function post(message: WorkerMessage, transfer: Transferable[] = []): void {
  scope.postMessage(message, transfer);
}

async function fetchFile(revision: string, path: string): Promise<Uint8Array> {
  const url = rawUrl(revision, path);
  let response: Response;
  try {
    response = await fetch(url, { redirect: "error" });
  } catch {
    post({ kind: "network", url, status: "failed", at: new Date().toISOString() });
    throw new Error(`Downloading ${path} failed. Check your connection and try again.`);
  }
  post({ kind: "network", url, status: response.status, at: new Date().toISOString() });
  if (!response.ok) throw new Error(`Downloading ${path} failed with ${response.status}.`);
  return new Uint8Array(await response.arrayBuffer());
}

/** Installed versions of changed files, fetched at their locked revision and
 * checked by the core against the lock's hash before a merge can use them. */
async function fetchBases(target: WasmExport): Promise<{ loaded: string[]; failed: Array<{ path: string; reason: string }> }> {
  const bases = JSON.parse(target.mergeBases()) as Array<{ path: string; revision: string; source_path: string; sha256: string }>;
  const loaded: string[] = [];
  const failed: Array<{ path: string; reason: string }> = [];
  for (const base of bases) {
    try {
      const bytes = (await cachedFile(base.sha256)) ?? await fetchFile(base.revision, base.source_path);
      target.addBase(base.path, bytes);
      await storeFile(base.sha256, bytes);
      loaded.push(base.path);
    } catch (error) {
      failed.push({ path: base.path, reason: error instanceof Error ? error.message : String(error) });
    }
  }
  return { loaded, failed };
}

async function download(revision: string, expected: Record<string, string>): Promise<number> {
  if (!source) throw new Error("Choose the setup source first.");
  const missing = JSON.parse(source.missing()) as string[];
  let done = 0;
  let next = 0;
  const worker = async () => {
    while (next < missing.length) {
      const path = missing[next++];
      const cached = await cachedFile(expected[path]);
      let accepted = false;
      if (cached) {
        try {
          source!.accept(path, cached);
          accepted = true;
        } catch {
          // A damaged cache entry is replaced by a fresh download.
        }
      }
      if (!accepted) {
        const bytes = await fetchFile(revision, path);
        source!.accept(path, bytes);
        await storeFile(expected[path], bytes);
      }
      done += 1;
      post({ kind: "progress", done, total: missing.length, path });
    }
  };
  await Promise.all(Array.from({ length: Math.min(DOWNLOAD_CONCURRENCY, missing.length) }, worker));
  return missing.length;
}

async function handle<Op extends Operation>(op: Op, args: CoreOperations[Op]["args"]): Promise<{ result: unknown; transfer?: Transferable[] }> {
  const module = await load();
  switch (op) {
    case "reset": {
      project?.free();
      exported?.free();
      project = new module.Project();
      exported = undefined;
      return { result: null };
    }
    case "addFiles": {
      project ??= new module.Project();
      const { files } = args as CoreOperations["addFiles"]["args"];
      for (const file of files) project.addFile(file.path, new Uint8Array(file.buffer));
      return { result: project.fileCount() };
    }
    case "scan":
      if (!project) throw new Error("Drop a mod folder first.");
      return { result: JSON.parse(project.scan()) };
    case "openSource": {
      const { manifest, revision, mode, requestedRef } = args as CoreOperations["openSource"]["args"];
      source?.free();
      source = new module.Source(new Uint8Array(manifest), revision, mode, requestedRef ?? undefined);
      return { result: null };
    }
    case "preview":
      if (!source) throw new Error("Choose the setup source first.");
      return { result: JSON.parse(source.preview((args as CoreOperations["preview"]["args"]).platform)) };
    case "select":
      if (!source) throw new Error("Choose the setup source first.");
      return { result: JSON.parse(source.select(JSON.stringify((args as CoreOperations["select"]["args"]).request))) };
    case "download": {
      const { revision, expected } = args as CoreOperations["download"]["args"];
      return { result: await download(revision, expected) };
    }
    case "export": {
      if (!source) throw new Error("Choose the setup source first.");
      const { request, existing } = args as CoreOperations["export"]["args"];
      exported?.free();
      if (existing) {
        if (!project) throw new Error("Drop a mod folder first.");
        exported = source.exportExisting(JSON.stringify(request), project);
      } else {
        exported = source.exportNew(JSON.stringify(request));
      }
      return { result: JSON.parse(exported.entries()) };
    }
    case "compare":
      if (!exported) throw new Error("Review the changes first.");
      return { result: JSON.parse(exported.compare((args as CoreOperations["compare"]["args"]).path)) };
    case "zip": {
      if (!exported) throw new Error("Review the changes first.");
      const { decisions, layout } = args as CoreOperations["zip"]["args"];
      const bytes = exported.zip(JSON.stringify(decisions), layout);
      const buffer = bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength) as ArrayBuffer;
      return { result: buffer, transfer: [buffer] };
    }
    case "evidence": {
      if (!project) throw new Error("Drop a mod folder first.");
      const { excluded } = args as CoreOperations["evidence"]["args"];
      return { result: JSON.parse(project.analysisEvidence(excluded && excluded.length > 0 ? JSON.stringify(excluded) : undefined)) };
    }
    case "exportRemoval": {
      if (!project) throw new Error("Drop a mod folder first.");
      const { layout } = args as CoreOperations["exportRemoval"]["args"];
      exported?.free();
      exported = project.exportRemoval(JSON.stringify({ layout }));
      return { result: JSON.parse(exported.entries()) };
    }
    case "fetchBases": {
      if (!exported) throw new Error("Review the changes first.");
      return { result: await fetchBases(exported) };
    }
    case "merge":
      if (!exported) throw new Error("Review the changes first.");
      return { result: exported.merge((args as CoreOperations["merge"]["args"]).path) };
    case "readiness":
      if (!exported) throw new Error("Review the changes first.");
      return { result: JSON.parse(exported.readiness()) };
    case "launcher":
      if (!exported) throw new Error("Review the changes first.");
      return { result: exported.launcherFile((args as CoreOperations["launcher"]["args"]).folder) };
    case "contextDocument": {
      const { number, name, buffer } = args as CoreOperations["contextDocument"]["args"];
      return { result: JSON.parse(module.contextDocument(number, name, new Uint8Array(buffer))) };
    }
    case "registry":
      if (!source) throw new Error("Choose the setup source first.");
      return { result: JSON.parse(source.componentRegistry()) };
    case "assistantTurn": {
      const { request, route, correction } = args as CoreOperations["assistantTurn"]["args"];
      return { result: JSON.parse(module.analysisTurn(JSON.stringify(request), JSON.stringify(route), correction ?? undefined)) };
    }
    case "assistantReply": {
      const { envelope, request, inputSha256, analysisId } = args as CoreOperations["assistantReply"]["args"];
      return { result: JSON.parse(module.readAnalysisReply(envelope, JSON.stringify(request), inputSha256, analysisId)) };
    }
    case "descriptionTurn": {
      const { route, note } = args as CoreOperations["descriptionTurn"]["args"];
      return { result: JSON.parse(module.descriptionTurn(JSON.stringify(route), note)) };
    }
    case "descriptionReply":
      return { result: JSON.parse(module.readDescriptionReply((args as CoreOperations["descriptionReply"]["args"]).envelope)) };
    case "openRouterStart": {
      const { callback } = args as CoreOperations["openRouterStart"]["args"];
      return { result: JSON.parse(module.openRouterSignIn(callback, crypto.getRandomValues(new Uint8Array(48)))) };
    }
    case "openRouterKeyRequest": {
      const { code, verifier } = args as CoreOperations["openRouterKeyRequest"]["args"];
      return { result: module.openRouterKeyRequest(code, verifier) };
    }
    case "openRouterKey":
      return { result: module.openRouterKey((args as CoreOperations["openRouterKey"]["args"]).response) };
    default:
      throw new Error(`Unknown operation ${String(op)}.`);
  }
}

scope.onmessage = (event) => {
  const { id, op, args } = event.data;
  handle(op, args).then(
    ({ result, transfer }) => post({ kind: "result", id, ok: true, result }, transfer ?? []),
    (error: unknown) => post({ kind: "result", id, ok: false, error: error instanceof Error ? error.message : String(error) }),
  );
};
