import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import App from "./App";
import { checkForAppUpdate, findInterruptedTransaction, readAiAccount, readAiModels, readCodexAccount } from "./lib/tauri";

// The relay address is set, as it is once the free assistant is deployed.
vi.mock("./free-assistant.json", () => ({
  default: { endpoint: "https://relay.example.workers.dev/v1/chat/completions", model: "nvidia/nemotron-3-super-120b-a12b:free" },
}));

vi.mock("./lib/tauri", async () => {
  const actual = await vi.importActual<typeof import("./lib/tauri")>("./lib/tauri");
  return { ...actual, checkForAppUpdate: vi.fn(), findInterruptedTransaction: vi.fn(), readAiAccount: vi.fn(), readAiModels: vi.fn(), readCodexAccount: vi.fn() };
});

beforeEach(() => {
  vi.mocked(checkForAppUpdate).mockResolvedValue({ value: null });
  vi.mocked(findInterruptedTransaction).mockResolvedValue(null);
  vi.mocked(readAiModels).mockResolvedValue([]);
  vi.mocked(readCodexAccount).mockResolvedValue({ available: false, authenticated: false, auth_mode: "none", usage_limited: false });
  vi.mocked(readAiAccount).mockImplementation(async (provider, model) => ({ available: true, authenticated: true, provider: provider as never, model, auth_mode: "none", usage_limited: false }));
});

afterEach(() => {
  delete (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__;
  cleanup();
  vi.clearAllMocks();
});

describe("free setup assistant once the relay is deployed", () => {
  it("is the default and needs no sign-in or key", async () => {
    (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__ = {};
    render(<App />);

    expect(screen.getByLabelText("AI provider")).toHaveValue("free");
    expect(screen.queryByRole("button", { name: "Sign in to Claude" })).not.toBeInTheDocument();
    expect(screen.queryByLabelText(/API key/i)).not.toBeInTheDocument();
    expect(await screen.findByRole("button", { name: "Continue" })).toBeEnabled();
  });
});
