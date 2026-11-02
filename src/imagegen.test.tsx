import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { ImagegenSignIn, defaultImageProvider, isRetiredImageProvider } from "./App";
import { imagegenLoginRiskNote, imagegenSignIn, imagegenSignInStatus } from "./lib/tauri";
import type { ImageGenerationProviderDefinition } from "./types";

vi.mock("./free-assistant.json", () => ({ default: { endpoint: null, model: "nvidia/nemotron-3-super-120b-a12b:free" } }));
vi.mock("./lib/tauri", async () => {
  const actual = await vi.importActual<typeof import("./lib/tauri")>("./lib/tauri");
  return { ...actual, isTauriRuntime: () => true, imagegenSignIn: vi.fn(), imagegenSignInStatus: vi.fn(), imagegenLoginRiskNote: vi.fn() };
});

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

const provider = (id: string, kind: string): ImageGenerationProviderDefinition => ({
  id, display_name: id, description: "", availability: "supported", requirements: [], good_for: [], cost_summary: "", models: [],
  supported_coding_environments: ["claude_code"], route: { kind, auth_method: "none" }, capabilities: {},
});

describe("hoi4-imagegen provider choice", () => {
  it("never offers or defaults to the retired GPTImage route", () => {
    const catalog = { default_provider: "gptimage", providers: [provider("gptimage", "local_package"), provider("hoi4_imagegen", "release_binary"), provider("none", "manual")] };
    expect(isRetiredImageProvider(catalog.providers[0])).toBe(true);
    expect(isRetiredImageProvider(catalog.providers[1])).toBe(false);
    expect(defaultImageProvider(catalog)).toBe("hoi4_imagegen");
    // An older package that has no replacement leaves no provider selected.
    expect(defaultImageProvider({ default_provider: "gptimage", providers: [catalog.providers[0], catalog.providers[2]] })).toBe("none");
    expect(defaultImageProvider(null)).toBe("none");
    expect(defaultImageProvider({ default_provider: "none", providers: [catalog.providers[1], catalog.providers[2]] })).toBe("none");
  });
});

describe("hoi4-imagegen sign-in step", () => {
  it("launches the sign-in and shows the reported status", async () => {
    vi.mocked(imagegenSignInStatus).mockResolvedValue({ value: { signed_in: false, detail: "" } });
    vi.mocked(imagegenSignIn).mockResolvedValue({ value: { signed_in: true, detail: "" } });
    render(<ImagegenSignIn projectRoot="C:/mods/atlantis" />);
    expect(await screen.findByText("Not signed in")).toBeInTheDocument();
    vi.mocked(imagegenLoginRiskNote).mockResolvedValue({ value: "Risk: unofficial route." });
    fireEvent.click(screen.getByRole("button", { name: "Sign in with ChatGPT" }));
    expect(await screen.findByText("Risk: unofficial route.")).toBeInTheDocument();
    expect(imagegenSignIn).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "I understand, continue" }));
    await waitFor(() => expect(imagegenSignIn).toHaveBeenCalledWith("C:/mods/atlantis"));
    expect(await screen.findByText("Signed in")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Sign in with ChatGPT" })).not.toBeInTheDocument();
  });

  it("reports a failed launch without claiming a sign-in", async () => {
    vi.mocked(imagegenSignInStatus).mockResolvedValue({ value: { signed_in: false, detail: "" } });
    vi.mocked(imagegenSignIn).mockResolvedValue({ value: null, error: "hoi4-imagegen is not installed in this project yet." });
    render(<ImagegenSignIn projectRoot="C:/mods/atlantis" />);
    vi.mocked(imagegenLoginRiskNote).mockResolvedValue({ value: "Risk: unofficial route." });
    fireEvent.click(await screen.findByRole("button", { name: "Sign in with ChatGPT" }));
    fireEvent.click(await screen.findByRole("button", { name: "I understand, continue" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("not installed");
    expect(screen.getByText("Not signed in")).toBeInTheDocument();
  });
});
