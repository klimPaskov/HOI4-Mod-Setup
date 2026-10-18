import { describe, expect, it } from "vitest";
import { deriveGeneratedIdentity } from "./identity";

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
