import { describe, expect, it } from "vitest";
import { defaultImageChoice, normalizeImageChoice } from "./integrations";
import type { SourcePreview } from "./protocol";

const preview = (default_provider: string, rows: Array<{ id: string; route_kind: string }>) =>
  ({ image_generation: { default_provider, providers: rows.map((row) => ({ ...row, display_name: row.id })) } }) as unknown as SourcePreview;

describe("default image choice", () => {
  it("starts a new mod on hoi4-imagegen", () => {
    expect(defaultImageChoice(preview("hoi4_imagegen", [{ id: "hoi4_imagegen", route_kind: "release_binary" }, { id: "none", route_kind: "manual" }])).primary_provider).toBe("hoi4_imagegen");
  });

  it("never starts on an older provider, even when a package names one as its default", () => {
    const rows = [{ id: "gptimage", route_kind: "local_package" }, { id: "openai_api", route_kind: "stdio_adapter" }, { id: "none", route_kind: "manual" }];
    expect(defaultImageChoice(preview("gptimage", rows)).primary_provider).toBe("none");
    expect(defaultImageChoice(preview("openai_api", [...rows, { id: "hoi4_imagegen", route_kind: "release_binary" }])).primary_provider).toBe("hoi4_imagegen");
  });

  it("falls back to none without a catalog", () => {
    expect(defaultImageChoice(null).primary_provider).toBe("none");
  });
});

describe("retired image provider", () => {
  const rows = (withImagegen: boolean) => [
    { id: "gptimage", display_name: "ChatGPT subscription (GPTImage)", route_kind: "local_package" },
    ...(withImagegen ? [{ id: "hoi4_imagegen", display_name: "hoi4-imagegen", route_kind: "release_binary" }] : []),
    { id: "none", display_name: "None", route_kind: "manual" },
  ];

  it("is replaced by hoi4-imagegen, or by none when the package lacks it", () => {
    const recorded = { primary_provider: "gptimage", additional_providers: [], route_settings: { gptimage: { model: "x" } } };
    expect(normalizeImageChoice(recorded, rows(true))).toEqual({ primary_provider: "hoi4_imagegen", additional_providers: [], route_settings: {} });
    expect(normalizeImageChoice(recorded, rows(false))).toEqual({ primary_provider: "none", additional_providers: [], route_settings: {} });
  });

  it("is dropped from the optional providers", () => {
    const recorded = { primary_provider: "hoi4_imagegen", additional_providers: ["gptimage"], route_settings: {} };
    expect(normalizeImageChoice(recorded, rows(true)).additional_providers).toEqual([]);
  });
});
