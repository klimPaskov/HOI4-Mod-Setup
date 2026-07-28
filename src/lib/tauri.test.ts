import { beforeEach, describe, expect, it, vi } from "vitest";
import { buildInstallationPlan, cancelScan, previewDescriptors, scanProject } from "./tauri";
import type { ScanProgress, WizardState } from "../types";

const { invoke, listen, unlisten } = vi.hoisted(() => ({
  invoke: vi.fn(),
  listen: vi.fn(),
  unlisten: vi.fn(),
}));

vi.mock("@tauri-apps/api/core", () => ({ invoke }));
vi.mock("@tauri-apps/api/event", () => ({ listen }));

describe("typed scanner bridge", () => {
  beforeEach(() => {
    invoke.mockReset();
    listen.mockReset();
    unlisten.mockReset();
  });

  it("filters progress events by the opaque request ID and cleans up the listener", async () => {
    let handler: ((event: { payload: { request_id: string; stage: string; current_path: string; files_scanned: number; directories_scanned: number; bytes_read: number } }) => void) | undefined;
    listen.mockImplementation(async (_name: string, next: typeof handler) => {
      handler = next;
      return unlisten;
    });
    invoke.mockImplementation(async (_command: string, args: { requestId: string }) => {
      handler?.({ payload: { request_id: "different-request", stage: "wrong", current_path: "wrong", files_scanned: 99, directories_scanned: 99, bytes_read: 99 } });
      handler?.({ payload: { request_id: args.requestId, stage: "detecting_git", current_path: ".git", files_scanned: 4, directories_scanned: 2, bytes_read: 2048 } });
      return { scan_id: "scan-1", project_root: "C:/mods/example", completed_at: null, partial: false, cancelled: false, limits_hit: [], files_scanned: 4, directories_scanned: 2, bytes_read: 2048, findings: [] };
    });
    const progress: ScanProgress[] = [];
    let requestId = "";

    const result = await scanProject("C:/mods/example", (update) => progress.push(update), (id) => { requestId = id; });

    expect(requestId).toMatch(/^[0-9a-f-]{36}$/i);
    expect(progress).toEqual([{ stage: "detecting_git", currentPath: ".git", filesScanned: 4, directoriesScanned: 2, bytesRead: 2048 }]);
    expect(result?.filesScanned).toBe(4);
    expect(invoke).toHaveBeenCalledWith("scan_project", expect.objectContaining({ root: "C:/mods/example", requestId }));
    expect(unlisten).toHaveBeenCalledOnce();
  });

  it("uses the typed cancellation command for the active request", async () => {
    invoke.mockResolvedValue(undefined);

    const result = await cancelScan("scan-request");

    expect(result.value).toBeUndefined();
    expect(result.error).toBeUndefined();
    expect(invoke).toHaveBeenCalledWith("cancel_scan", { requestId: "scan-request" });
  });

  it("never serializes the Meshy password draft in state-bearing core commands", async () => {
    invoke.mockResolvedValue([]);
    const state = { meshKeyDraft: "test-meshy-draft-not-for-transport" } as unknown as WizardState;

    await previewDescriptors(state);
    await buildInstallationPlan(state);

    expect(invoke).toHaveBeenNthCalledWith(1, "preview_descriptors", { state: { meshKeyDraft: "" } });
    expect(invoke).toHaveBeenNthCalledWith(2, "build_installation_plan", { state: { meshKeyDraft: "" } });
    expect(JSON.stringify(invoke.mock.calls)).not.toContain("test-meshy-draft-not-for-transport");
  });
});
