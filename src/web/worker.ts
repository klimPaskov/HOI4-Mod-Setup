// The web edition's core worker: it loads the WebAssembly setup core, holds
// the dropped folder and the verified source files, downloads selected files
// from GitHub at the exact commit, and builds the ZIP. Keeping this off the
// page thread keeps the interface responsive on large mods.

import { rawUrl } from "./github";
import type { CoreOperations, Operation, Request, WorkerMessage } from "./protocol";

interface WasmProject {
  addFile(path: string, bytes: Uint8Array): void;
  fileCount(): number;
  scan(): string;
  free(): void;
}

interface WasmExport {
  entries(): string;
  compare(path: string): string;
  zip(decisions: string): Uint8Array;
  free(): void;
}

interface WasmSource {
  preview(platform: string): string;
  select(request: string): string;
  accept(sourcePath: string, bytes: Uint8Array): void;
  missing(): string;
  exportNew(request: string): WasmExport;
  exportExisting(request: string, project: WasmProject): WasmExport;
  free(): void;
}

interface WasmModule {
  default(): Promise<unknown>;
  Project: new () => WasmProject;
  Source: new (manifest: Uint8Array, revision: string, mode: string, requestedRef?: string) => WasmSource;
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

async function download(revision: string): Promise<number> {
  if (!source) throw new Error("Choose the setup source first.");
  const missing = JSON.parse(source.missing()) as string[];
  let done = 0;
  let next = 0;
  const worker = async () => {
    while (next < missing.length) {
      const path = missing[next++];
      const bytes = await fetchFile(revision, path);
      source!.accept(path, bytes);
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
    case "download":
      return { result: await download((args as CoreOperations["download"]["args"]).revision) };
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
      const bytes = exported.zip(JSON.stringify((args as CoreOperations["zip"]["args"]).decisions));
      const buffer = bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength) as ArrayBuffer;
      return { result: buffer, transfer: [buffer] };
    }
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
