import { describe, expect, it } from "vitest";
import { deriveGeneratedIdentity, detectedValues, deterministicImportValues } from "./identity";

describe("generated project identity", () => {
  it("derives valid editable conventions from a name and brief", () => {
    const generated = deriveGeneratedIdentity(
      "Iron Dawn",
      "An alternate-history event and decisions mod with new countries.",
    );

    expect(generated.projectId).toBe("iron_dawn");
    expect(generated.scriptPrefix).toBe("id");
    expect(generated.primaryNamespace).toBe("id");
    expect(generated.descriptorTags).toEqual(["Alternative History", "Events", "Gameplay"]);
    expect(generated.folderProfile).toEqual(["common", "events", "localisation/english", "gfx", "interface", "docs", "history"]);
  });

  it("keeps identifiers valid for accented and numeric names", () => {
    const accented = deriveGeneratedIdentity("Üks", "A small gameplay mod.");
    const numeric = deriveGeneratedIdentity("1944", "A focused scenario.");

    expect(accented.projectId).toMatch(/^[a-z][a-z0-9_]{1,63}$/);
    expect(accented.scriptPrefix).toMatch(/^[a-z_][a-z0-9_]{0,63}$/);
    expect(numeric.projectId).toBe("mod_1944");
  });
});

describe("deterministic import values", () => {
  const detected = (values: unknown) => [{ id: "profile.detected", value: JSON.stringify(values) }];

  it("uses what the files decide and fills only the gaps", () => {
    const values = detectedValues(detected([
      { key: "display_name", value: "Iron Dawn", reason: "descriptor" },
      { key: "descriptor_tags", value: ["Events", "Gameplay"], reason: "descriptor" },
      { key: "primary_namespace", value: "iron", reason: "events" },
      { key: "folder_profile", value: ["common/national_focus", "events"], reason: "folders" },
      { key: "localisation_convention", value: "english with versioned keys", reason: "localisation" },
    ]));
    const result = deterministicImportValues(values, "iron_dawn", "");
    expect(result).toEqual({
      displayName: "Iron Dawn",
      projectId: "iron_dawn",
      scriptPrefix: "iron",
      primaryNamespace: "iron",
      descriptorTags: ["Events", "Gameplay"],
      folderProfile: ["common/national_focus", "events"],
      localisationConvention: "english with versioned keys",
      documentationConvention: undefined,
    });
  });

  it("derives valid identifiers when nothing was detected", () => {
    const result = deterministicImportValues(detectedValues([]), "My Mod (copy)", "");
    expect(result.displayName).toBe("My Mod (copy)");
    expect(result.projectId).toMatch(/^[a-z][a-z0-9_]{1,63}$/);
    expect(result.scriptPrefix).toMatch(/^[a-z_][a-z0-9_]{0,63}$/);
    expect(result.descriptorTags.length).toBeGreaterThan(0);
    expect(result.folderProfile).toEqual([]);
  });

  it("ignores malformed or mistyped detected values", () => {
    expect(detectedValues([{ id: "profile.detected", value: "not json" }])).toEqual([]);
    expect(detectedValues(detected({ key: "display_name" }))).toEqual([]);
    const values = detectedValues(detected([
      { key: "display_name", value: 7, reason: "wrong type" },
      { key: "descriptor_tags", value: [1, 2], reason: "wrong type" },
      { value: "no key", reason: "missing" },
    ]));
    const result = deterministicImportValues(values, "folder_name", "Named");
    expect(result.displayName).toBe("Named");
    expect(result.descriptorTags).not.toContain(1);
  });
});
