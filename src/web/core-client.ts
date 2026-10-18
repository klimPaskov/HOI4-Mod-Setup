// The page's side of the core worker: typed calls, progress, and the
// network records the worker reports.

import { recordRequest } from "./github";
import type { CoreOperations, Operation, WorkerMessage } from "./protocol";

type Pending = { resolve: (value: unknown) => void; reject: (error: Error) => void };

export interface CoreClient {
  call<Op extends Operation>(op: Op, args: CoreOperations[Op]["args"], transfer?: Transferable[]): Promise<CoreOperations[Op]["result"]>;
  onProgress(listener: (done: number, total: number, path: string) => void): () => void;
  terminate(): void;
}

/** Wrap any worker-like endpoint; the web app passes the real core worker. */
export function createCoreClient(worker: Pick<Worker, "postMessage" | "terminate"> & { onmessage: ((event: MessageEvent<WorkerMessage>) => void) | null }): CoreClient {
  let nextId = 1;
  const pending = new Map<number, Pending>();
  const progress = new Set<(done: number, total: number, path: string) => void>();
  worker.onmessage = (event) => {
    const message = event.data;
    if (message.kind === "progress") {
      for (const listener of progress) listener(message.done, message.total, message.path);
      return;
    }
    if (message.kind === "network") {
      recordRequest(message.url, message.status);
      return;
    }
    const request = pending.get(message.id);
    if (!request) return;
    pending.delete(message.id);
    if (message.ok) request.resolve(message.result);
    else request.reject(new Error(message.error));
  };
  return {
    call(op, args, transfer = []) {
      const id = nextId++;
      return new Promise((resolve, reject) => {
        pending.set(id, { resolve: resolve as (value: unknown) => void, reject });
        worker.postMessage({ id, op, args }, transfer);
      });
    },
    onProgress(listener) {
      progress.add(listener);
      return () => progress.delete(listener);
    },
    terminate() {
      for (const request of pending.values()) request.reject(new Error("The setup core stopped."));
      pending.clear();
      worker.terminate();
    },
  };
}

export function startCoreWorker(): CoreClient {
  const worker = new Worker(new URL("./worker.ts", import.meta.url), { type: "module" });
  return createCoreClient(worker as unknown as Parameters<typeof createCoreClient>[0]);
}
