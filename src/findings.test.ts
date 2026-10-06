import { describe, expect, it } from "vitest";
import { findingDisplayValue, findingLabel } from "./findings";

describe("scan finding presentation", () => {
  it("uses readable labels instead of machine keys", () => {
    expect(findingLabel("project_instructions")).toBe("Project instructions");
    expect(findingLabel("installed_environments")).toBe("Coding environments");
    expect(findingLabel("some_future_key")).toBe("Some future key");
  });

  it("summarizes structured values without raw JSON", () => {
    const repository = JSON.stringify({ branch: null, commit: null, detached: false, present: false, remotes: [] });
    expect(findingDisplayValue("repository", repository)).toBe("No Git repository");
    expect(findingDisplayValue("repository", JSON.stringify({ present: true, branch: "main", staged_files: 1, unstaged_files: 2, untracked_files: 0 }))).toBe("Branch main · 3 changed setup files");
    expect(findingDisplayValue("installed_environments", JSON.stringify({ additional: [], detected: [], primary: "codex" }))).toBe("None detected · Codex will be the default");
    expect(findingDisplayValue("installed_environments", JSON.stringify({ additional: [], detected: ["claude_code", "cursor"], primary: "codex" }))).toBe("Detected: Claude Code, Cursor");
    expect(findingDisplayValue("managed_setup", JSON.stringify({ present: false }))).toBe("Not installed yet");
    expect(findingDisplayValue("skill_count", JSON.stringify({ count: 12, malformed: ["a"] }))).toBe("12 skills, 1 need review");
    expect(findingDisplayValue("subagent_count", JSON.stringify({ count: 1, malformed: [] }))).toBe("1 subagent");
    expect(findingDisplayValue("thumbnail", JSON.stringify({ width: 300, height: 300, managed_placeholder: false }))).toBe("300 × 300");
    expect(findingDisplayValue("project_instructions", "true")).toBe("Found (AGENTS.md)");
    expect(findingDisplayValue("config_present", "false")).toBe("Not found");
    expect(findingDisplayValue("mod_name", "Legacy Test Mod")).toBe("Legacy Test Mod");
    expect(findingDisplayValue("unknown", JSON.stringify({ a: 1, nested: { b: 2 } }))).toBe("A: 1");
  });
});
