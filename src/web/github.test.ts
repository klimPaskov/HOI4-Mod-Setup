import { afterEach, describe, expect, it, vi } from "vitest";
import { fetchRaw, networkLog, RateLimitError, rawUrl, resolveSource } from "./github";

const COMMIT = "0b124ad9d3121fda5eb3cc5ff365f74a28db0f8f";

function json(body: unknown, status = 200, headers: Record<string, string> = {}): Response {
  return new Response(JSON.stringify(body), { status, headers: { "content-type": "application/json", ...headers } });
}

afterEach(() => vi.unstubAllGlobals());

describe("web source resolution", () => {
  it("reads raw files only at an exact commit and encodes each segment", () => {
    expect(rawUrl(COMMIT, "docs/a b#c.md")).toBe(`https://raw.githubusercontent.com/klimPaskov/Agentic-HOI4-Modding/${COMMIT}/docs/a%20b%23c.md`);
    expect(() => rawUrl("main", "README.md")).toThrow(/exact commit/);
    // A lock in a dropped folder cannot point the page at another repository.
    expect(() => rawUrl(COMMIT, "../../other/repo/main/x")).toThrow(/inside the source repository/);
  });

  it("resolves latest to the default branch head before downloading", async () => {
    const fetchMock = vi.fn()
      .mockResolvedValueOnce(json({ default_branch: "main" }))
      .mockResolvedValueOnce(json({ sha: COMMIT }));
    vi.stubGlobal("fetch", fetchMock);
    const resolved = await resolveSource("latest", "");
    expect(resolved).toEqual({ mode: "latest", revision: COMMIT, requestedRef: null, branch: "main" });
    expect(fetchMock.mock.calls[1][0]).toBe("https://api.github.com/repos/klimPaskov/Agentic-HOI4-Modding/commits/main");
    expect(fetchMock.mock.calls.every(([, init]) => init.redirect === "error")).toBe(true);
    expect(networkLog().slice(-2).map((record) => record.status)).toEqual([200, 200]);
  });

  it("accepts a pinned commit only when GitHub confirms it", async () => {
    vi.stubGlobal("fetch", vi.fn().mockResolvedValue(json({ sha: COMMIT })));
    await expect(resolveSource("pinned_commit", COMMIT.toUpperCase())).resolves.toMatchObject({ revision: COMMIT, requestedRef: COMMIT });
    await expect(resolveSource("pinned_commit", "abc")).rejects.toThrow(/40-character/);
  });

  it("explains an exhausted anonymous rate limit", async () => {
    vi.stubGlobal("fetch", vi.fn().mockResolvedValue(json({ message: "rate limited" }, 403, { "x-ratelimit-remaining": "0" })));
    await expect(resolveSource("latest", "")).rejects.toBeInstanceOf(RateLimitError);
  });

  it("reports an unreachable network without leaking details", async () => {
    vi.stubGlobal("fetch", vi.fn().mockRejectedValue(new TypeError("Failed to fetch")));
    await expect(fetchRaw(COMMIT, "README.md")).rejects.toThrow(/Downloading README.md failed/);
    expect(networkLog().at(-1)?.status).toBe("failed");
  });
});
