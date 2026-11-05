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
  it("opens the sign-in by itself, shows the notice as information and treats completion as acceptance", async () => {
    vi.mocked(imagegenSignInStatus).mockResolvedValue({ value: { signed_in: false, detail: "" } });
    vi.mocked(imagegenLoginRiskNote).mockResolvedValue({ value: "Risk: unofficial route." });
    vi.mocked(imagegenSignIn).mockResolvedValue({ value: { signed_in: true, detail: "" } });
    render(<ImagegenSignIn projectRoot="C:/mods/auto_start" autoStart />);
    // No typed answer and no confirmation button: the program is launched directly.
    await waitFor(() => expect(imagegenSignIn).toHaveBeenCalledWith("C:/mods/auto_start"));
    expect(screen.queryByRole("button", { name: "I understand, continue" })).not.toBeInTheDocument();
    expect(await screen.findByText("Signed in")).toBeInTheDocument();
    expect(imagegenSignIn).toHaveBeenCalledTimes(1);
  });

  it("starts the sign-in once per project, not on every visit", async () => {
    vi.mocked(imagegenSignInStatus).mockResolvedValue({ value: { signed_in: false, detail: "" } });
    vi.mocked(imagegenLoginRiskNote).mockResolvedValue({ value: "Risk: unofficial route." });
    vi.mocked(imagegenSignIn).mockResolvedValue({ value: { signed_in: false, detail: "Sign-in did not finish." } });
    const { unmount } = render(<ImagegenSignIn projectRoot="C:/mods/once_only" autoStart />);
    await waitFor(() => expect(imagegenSignIn).toHaveBeenCalledTimes(1));
    unmount();
    render(<ImagegenSignIn projectRoot="C:/mods/once_only" autoStart />);
    expect(await screen.findByText("Not signed in")).toBeInTheDocument();
    expect(imagegenSignIn).toHaveBeenCalledTimes(1);
    // The user can start it again themselves.
    fireEvent.click(screen.getByRole("button", { name: "Sign in with ChatGPT" }));
    await waitFor(() => expect(imagegenSignIn).toHaveBeenCalledTimes(2));
  });

  it("does not launch anything unless asked, and an installed project already signed in is left alone", async () => {
    vi.mocked(imagegenSignInStatus).mockResolvedValue({ value: { signed_in: false, detail: "" } });
    render(<ImagegenSignIn projectRoot="C:/mods/manual" />);
    expect(await screen.findByText("Not signed in")).toBeInTheDocument();
    expect(imagegenSignIn).not.toHaveBeenCalled();
    cleanup();
    vi.mocked(imagegenSignInStatus).mockResolvedValue({ value: { signed_in: true, detail: "" } });
    render(<ImagegenSignIn projectRoot="C:/mods/signed_in" autoStart />);
    expect(await screen.findByText("Signed in")).toBeInTheDocument();
    expect(imagegenSignIn).not.toHaveBeenCalled();
  });

  it("reports a failed launch without claiming a sign-in and offers another try", async () => {
    vi.mocked(imagegenSignInStatus).mockResolvedValue({ value: { signed_in: false, detail: "" } });
    vi.mocked(imagegenLoginRiskNote).mockResolvedValue({ value: "Risk: unofficial route." });
    vi.mocked(imagegenSignIn).mockResolvedValue({ value: null, error: "hoi4-imagegen is not installed in this project yet." });
    render(<ImagegenSignIn projectRoot="C:/mods/failed_launch" autoStart />);
    expect(await screen.findByRole("alert")).toHaveTextContent("not installed");
    expect(screen.getByText("Not signed in")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Try again" })).toBeInTheDocument();
  });
});
