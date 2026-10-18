// Source resolution for the web edition. The page talks only to the GitHub
// REST API and raw.githubusercontent.com, both of which allow browser
// requests. Every file is later checked against the manifest's SHA-256
// evidence inside the WebAssembly core, so these helpers only fetch.

export const SOURCE_OWNER = "klimPaskov";
export const SOURCE_NAME = "Agentic-HOI4-Modding";
export const MANIFEST_PATH = "hoi4-mod-setup.manifest.json";

export type SourceMode = "latest" | "pinned_commit" | "pinned_release";

export interface ResolvedSource {
  mode: SourceMode;
  revision: string;
  requestedRef: string | null;
  branch: string | null;
}

export interface NetworkRecord {
  url: string;
  status: number | "failed";
  at: string;
}

const COMMIT = /^[0-9a-f]{40}$/;
const records: NetworkRecord[] = [];
const listeners = new Set<(records: NetworkRecord[]) => void>();

/** Every request the page made, for the network panel. */
export function networkLog(): NetworkRecord[] {
  return [...records];
}

export function onNetworkLog(listener: (records: NetworkRecord[]) => void): () => void {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

export function recordRequest(url: string, status: number | "failed"): void {
  records.push({ url, status, at: new Date().toISOString() });
  for (const listener of listeners) listener(networkLog());
}

export class RateLimitError extends Error {
  constructor() {
    super("GitHub's limit for anonymous requests from this network is used up. Wait about an hour, or add a GitHub token for this session.");
  }
}

function apiUrl(path: string): string {
  return `https://api.github.com/repos/${SOURCE_OWNER}/${SOURCE_NAME}${path}`;
}

/** Raw file URL at an exact commit; each path segment is encoded. */
export function rawUrl(revision: string, path: string): string {
  if (!COMMIT.test(revision)) throw new Error("A source file can only be read at an exact commit.");
  const encoded = path.split("/").map((segment) => encodeURIComponent(segment)).join("/");
  return `https://raw.githubusercontent.com/${SOURCE_OWNER}/${SOURCE_NAME}/${revision}/${encoded}`;
}

async function getJson(url: string, token?: string): Promise<unknown> {
  let response: Response;
  try {
    response = await fetch(url, {
      headers: { Accept: "application/vnd.github+json", ...(token ? { Authorization: `Bearer ${token}` } : {}) },
      redirect: "error",
    });
  } catch {
    recordRequest(url, "failed");
    throw new Error("GitHub could not be reached. Check your connection and try again.");
  }
  recordRequest(url, response.status);
  if ((response.status === 403 || response.status === 429) && response.headers.get("x-ratelimit-remaining") === "0") {
    throw new RateLimitError();
  }
  if (!response.ok) throw new Error(`GitHub answered ${response.status} for ${url}.`);
  return response.json();
}

/** Resolve the source to an exact commit before anything is downloaded. */
export async function resolveSource(mode: SourceMode, reference: string, token?: string): Promise<ResolvedSource> {
  if (mode === "pinned_commit") {
    const commit = reference.trim().toLowerCase();
    if (!COMMIT.test(commit)) throw new Error("Enter the full 40-character commit ID.");
    const result = await getJson(apiUrl(`/commits/${commit}`), token) as { sha?: string };
    if (result.sha !== commit) throw new Error("That commit does not exist in the source repository.");
    return { mode, revision: commit, requestedRef: commit, branch: null };
  }
  if (mode === "pinned_release") {
    const tag = reference.trim();
    if (!tag) throw new Error("Enter a release tag.");
    const release = await getJson(apiUrl(`/releases/tags/${encodeURIComponent(tag)}`), token) as { tag_name?: string };
    if (release.tag_name !== tag) throw new Error("That release does not exist in the source repository.");
    const commit = await getJson(apiUrl(`/commits/${encodeURIComponent(tag)}`), token) as { sha?: string };
    if (!commit.sha || !COMMIT.test(commit.sha)) throw new Error("The release does not point at a commit.");
    return { mode, revision: commit.sha, requestedRef: tag, branch: null };
  }
  const repository = await getJson(apiUrl(""), token) as { default_branch?: string };
  const branch = repository.default_branch;
  if (!branch) throw new Error("The source repository has no default branch.");
  const commit = await getJson(apiUrl(`/commits/${encodeURIComponent(branch)}`), token) as { sha?: string };
  if (!commit.sha || !COMMIT.test(commit.sha)) throw new Error("The default branch does not point at a commit.");
  return { mode, revision: commit.sha, requestedRef: null, branch };
}

/** Bytes of one file at an exact commit. */
export async function fetchRaw(revision: string, path: string, signal?: AbortSignal): Promise<Uint8Array> {
  const url = rawUrl(revision, path);
  let response: Response;
  try {
    response = await fetch(url, { redirect: "error", signal });
  } catch (error) {
    recordRequest(url, "failed");
    throw error instanceof DOMException && error.name === "AbortError" ? error : new Error(`Downloading ${path} failed. Check your connection and try again.`);
  }
  recordRequest(url, response.status);
  if (!response.ok) throw new Error(`Downloading ${path} failed with ${response.status}.`);
  return new Uint8Array(await response.arrayBuffer());
}
