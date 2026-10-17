import { describe, expect, it } from "vitest";
import { findingDisplayValue, findingLabel } from "./findings";

describe("scan finding presentation", () => {
  it("summarizes actual profile coverage and observed conventions", () => {
    expect(findingDisplayValue("project_coverage", JSON.stringify({ coverage: { files_indexed: 12345, samples_read: 120, coverage: "sampled" } }))).toContain("120 content samples");
    expect(findingDisplayValue("project_structure", JSON.stringify({ content_categories: [{ category: "common/national_focus", files: 300 }] }))).toBe("common/national_focus: 300");
    expect(findingDisplayValue("project_identifiers", JSON.stringify({ namespace_declarations: [{ value: "GER" }, { value: "FRA" }], identifier_prefixes: [{ value: "GER" }] }))).toBe("Sampled: GER, FRA");
    expect(findingDisplayValue("project_localisation", JSON.stringify({ language_headers: [{ value: "english" }], utf8_bom: 4 }))).toBe("english; 4 sampled files with UTF-8 BOM");
    expect(findingDisplayValue("project_documentation", JSON.stringify({ documents: [{ path: "README.md", headings: ["Kaiserreich"] }] }))).toBe("README.md: Kaiserreich");
  });
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
