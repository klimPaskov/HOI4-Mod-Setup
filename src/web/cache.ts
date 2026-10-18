// A cache of verified source files in this browser, keyed by the SHA-256
// the manifest declares. A cached file still passes the core's size and hash
// check before use, so a damaged entry is downloaded again. Storage can be
// missing or blocked (private windows, cleared site data); every failure
// falls back to the network.

const DATABASE = "hoi4-mod-setup-source";
const STORE = "files";

let opened: Promise<IDBDatabase | null> | undefined;

function open(): Promise<IDBDatabase | null> {
  opened ??= new Promise((resolve) => {
    try {
      const request = indexedDB.open(DATABASE, 1);
      request.onupgradeneeded = () => request.result.createObjectStore(STORE);
      request.onsuccess = () => resolve(request.result);
      request.onerror = () => resolve(null);
      request.onblocked = () => resolve(null);
    } catch {
      resolve(null);
    }
  });
  return opened;
}

const SHA256 = /^[0-9a-f]{64}$/;

export async function cachedFile(sha256: string | undefined): Promise<Uint8Array | null> {
  if (!sha256 || !SHA256.test(sha256)) return null;
  const database = await open();
  if (!database) return null;
  return new Promise((resolve) => {
    try {
      const request = database.transaction(STORE, "readonly").objectStore(STORE).get(sha256);
      request.onsuccess = () => resolve(request.result instanceof ArrayBuffer ? new Uint8Array(request.result) : null);
      request.onerror = () => resolve(null);
    } catch {
      resolve(null);
    }
  });
}

export async function storeFile(sha256: string | undefined, bytes: Uint8Array): Promise<void> {
  if (!sha256 || !SHA256.test(sha256)) return;
  const database = await open();
  if (!database) return;
  await new Promise<void>((resolve) => {
    try {
      const copy = bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength);
      const transaction = database.transaction(STORE, "readwrite");
      transaction.objectStore(STORE).put(copy, sha256);
      transaction.oncomplete = () => resolve();
      transaction.onerror = () => resolve();
      transaction.onabort = () => resolve();
    } catch {
      resolve();
    }
  });
}
