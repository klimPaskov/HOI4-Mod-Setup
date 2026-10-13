import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { AccountProviderAccess } from "./AccountProviderAccess";
import { initialState } from "./App";
import { cancelProviderAccountLogin, logoutProviderAccount, readAiAccountResult, readPendingProviderAccount, startProviderAccountLogin, waitForProviderAccountLogin } from "./lib/tauri";
import type { AiAccountStatus } from "./types";

vi.mock("./lib/tauri", async () => ({
  ...await vi.importActual<typeof import("./lib/tauri")>("./lib/tauri"),
  startProviderAccountLogin: vi.fn(),
  cancelProviderAccountLogin: vi.fn(async () => ({ value: null })),
  readAiAccountResult: vi.fn(),
  logoutProviderAccount: vi.fn(),
  waitForProviderAccountLogin: vi.fn(),
  readPendingProviderAccount: vi.fn(),
}));
afterEach(() => { cleanup(); vi.clearAllMocks(); });

describe("experimental account access", () => {
  it.each(["gemini_account", "grok_account"] as const)("shows account controls without credentials for %s", (provider) => {
    render(<AccountProviderAccess state={{ ...initialState, aiProvider: provider }} update={vi.fn()} desktop onUseApi={vi.fn()} />);
    expect(screen.getByRole("button", { name: /Sign in to/ })).toBeEnabled();
    expect(document.querySelector('input[type="password"]')).toBeNull();
    expect(screen.getByText(/Experimental account integration/)).toBeVisible();
  });

  it("shows immediate pending feedback, blocks duplicates, and ignores a cancelled login result", async () => {
    let finish!: (result: { value: AiAccountStatus }) => void;
    vi.mocked(startProviderAccountLogin).mockResolvedValue({ value: "attempt-1" });
    vi.mocked(waitForProviderAccountLogin).mockReturnValue(new Promise((resolve) => { finish = resolve; }));
    const update = vi.fn();
    render(<AccountProviderAccess state={{ ...initialState, aiProvider: "gemini_account" }} update={update} desktop onUseApi={vi.fn()} />);
    const button = screen.getByRole("button", { name: "Sign in to Gemini" });
    await act(async () => { fireEvent.click(button); });
    fireEvent.click(button);
    expect(button).toBeDisabled();
    expect(screen.getByRole("status")).toHaveTextContent("Preparing Gemini sign-in");
    expect(startProviderAccountLogin).toHaveBeenCalledTimes(1);
    await act(async () => { fireEvent.click(screen.getByRole("button", { name: "Cancel sign-in" })); });
    expect(cancelProviderAccountLogin).toHaveBeenCalledWith("gemini_account", "attempt-1");
    update.mockClear();
    await act(async () => { finish({ value: { available: true, authenticated: true, provider: "gemini_account", model: "gemini-3.8-flash", auth_mode: "provider_account", usage_limited: false } }); });
    expect(update).not.toHaveBeenCalled();
  });

  it("checks the selected model and accepts only returned account state", async () => {
    const account: AiAccountStatus = { available: true, authenticated: true, provider: "grok_account", model: "grok-build", auth_mode: "provider_account", usage_limited: false };
    vi.mocked(readAiAccountResult).mockResolvedValue({ value: account });
    const update = vi.fn();
    render(<AccountProviderAccess state={{ ...initialState, aiProvider: "grok_account", aiModel: "grok-build" }} update={update} desktop onUseApi={vi.fn()} />);
    await act(async () => { fireEvent.click(screen.getByRole("button", { name: "Check again" })); });
    expect(readAiAccountResult).toHaveBeenCalledWith("grok_account", "grok-build", initialState.aiReasoningEffort, "");
    expect(update).toHaveBeenLastCalledWith(expect.objectContaining({ aiAccount: account, codexAnalysisRecord: undefined }));
  });

  it("keeps the sign-in window and cancel action while a check still reports pending", async () => {
    const pending: AiAccountStatus = { available: true, authenticated: false, provider: "gemini_account", model: "gemini-3.8-flash", auth_mode: "provider_login_pending", usage_limited: false };
    vi.mocked(startProviderAccountLogin).mockResolvedValue({ value: "attempt-2" });
    vi.mocked(waitForProviderAccountLogin).mockResolvedValue({ value: pending });
    vi.mocked(readAiAccountResult).mockResolvedValue({ value: pending });
    const { unmount } = render(<AccountProviderAccess state={{ ...initialState, aiProvider: "gemini_account" }} update={vi.fn()} desktop onUseApi={vi.fn()} />);
    await act(async () => { fireEvent.click(screen.getByRole("button", { name: "Sign in to Gemini" })); });
    await act(async () => { fireEvent.click(screen.getByRole("button", { name: "Check again" })); });
    expect(screen.getByRole("button", { name: "Sign-in window open" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Cancel sign-in" })).toBeVisible();
    unmount();
    expect(cancelProviderAccountLogin).toHaveBeenCalledWith("gemini_account", "attempt-2");
  });

  it("announces a bridge failure and clears stale authenticated state", async () => {
    vi.mocked(readAiAccountResult).mockResolvedValue({ value: null, error: "The account check was interrupted." });
    const update = vi.fn();
    render(<AccountProviderAccess state={{ ...initialState, aiProvider: "grok_account", aiAccount: { available: true, authenticated: true, provider: "grok_account", model: "grok-build", auth_mode: "provider_account", usage_limited: false } }} update={update} desktop onUseApi={vi.fn()} />);
    await act(async () => { fireEvent.click(screen.getByRole("button", { name: "Check again" })); });
    expect(update).toHaveBeenLastCalledWith(expect.objectContaining({ aiAccount: null, transactionError: "The account check was interrupted." }));
  });

  it("labels and cancels Gemini's official sign-out window accurately", async () => {
    vi.mocked(logoutProviderAccount).mockResolvedValue({ value: "attempt-3" });
    vi.mocked(waitForProviderAccountLogin).mockResolvedValue({ value: { available: true, authenticated: false, provider: "gemini_account", model: "gemini-3.8-flash", auth_mode: "provider_logout_pending", usage_limited: false } });
    const { unmount } = render(<AccountProviderAccess state={{ ...initialState, aiProvider: "gemini_account", aiAccount: { available: true, authenticated: true, provider: "gemini_account", model: "gemini-3.8-flash", auth_mode: "provider_account", usage_limited: false } }} update={vi.fn()} desktop onUseApi={vi.fn()} />);
    await act(async () => { fireEvent.click(screen.getByRole("button", { name: "Sign out" })); });
    expect(screen.getByRole("button", { name: "Sign out" })).toBeDisabled();
    await act(async () => { fireEvent.click(screen.getByRole("button", { name: "Cancel sign-out" })); });
    expect(cancelProviderAccountLogin).toHaveBeenCalledTimes(1);
    unmount();
    expect(cancelProviderAccountLogin).toHaveBeenCalledTimes(1);
  });

  it("recovers the exact pending attempt after the app is reopened", async () => {
    vi.mocked(readPendingProviderAccount).mockResolvedValue({ value: "restored-attempt" });
    render(<AccountProviderAccess state={{ ...initialState, aiProvider: "gemini_account", aiAccount: { available: true, authenticated: false, provider: "gemini_account", model: "gemini-3.8-flash", auth_mode: "provider_login_pending", usage_limited: false } }} update={vi.fn()} desktop onUseApi={vi.fn()} />);
    expect(await screen.findByRole("button", { name: "Sign-in window open" })).toBeDisabled();
    await act(async () => { fireEvent.click(screen.getByRole("button", { name: "Cancel sign-in" })); });
    expect(cancelProviderAccountLogin).toHaveBeenCalledWith("gemini_account", "restored-attempt");
  });
});
