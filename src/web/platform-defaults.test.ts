import { describe, expect, it } from "vitest";
import { seededOptionalIds } from "./platform-defaults";

const optional = [{ id: "docs.linux" }, { id: "runtime.chatgpt_sources" }, { id: "workflow.3d" }];
const desktopOnly = new Set(["workflow.3d"]);

describe("the optional components ticked at the start", () => {
  it("ticks the Linux setup guide on Linux", () => {
    expect([...seededOptionalIds(optional, new Set(), desktopOnly, "linux")]).toEqual(["docs.linux"]);
  });

  it.each(["windows", "macos"] as const)("leaves the Linux setup guide to the user on %s", (platform) => {
    expect(seededOptionalIds(optional, new Set(), desktopOnly, platform).size).toBe(0);
  });

  it("keeps the assistant's recommendations and never ticks a desktop-only component", () => {
    const seeded = seededOptionalIds(optional, new Set(["runtime.chatgpt_sources", "workflow.3d"]), desktopOnly, "windows");
    expect([...seeded]).toEqual(["runtime.chatgpt_sources"]);
  });
});
