import { useEffect, useRef, useState } from "react";
import { cancelProviderAccountLogin, logoutProviderAccount, readAiAccountResult, readPendingProviderAccount, startProviderAccountLogin, waitForProviderAccountLogin } from "./lib/tauri";
import type { AiAccountStatus, WizardState } from "./types";

export function AccountProviderAccess({ state, update, desktop, onUseApi }: {
  state: WizardState;
  update: (patch: Partial<WizardState>) => void;
  desktop: boolean;
  onUseApi: () => void;
}) {
  const provider = state.aiProvider;
  const label = provider === "gemini_account" ? "Gemini" : "Grok";
  const [action, setAction] = useState<"login" | "check" | "logout" | null>(null);
  const [windowAction, setWindowAction] = useState<"login" | "logout" | null>(null);
  const generation = useRef(0);
  const attemptId = useRef<string | null>(null);
  const pendingMode = state.aiAccount?.auth_mode === "provider_login_pending" ? "login"
    : state.aiAccount?.auth_mode === "provider_logout_pending" ? "logout" : null;
  useEffect(() => () => {
    generation.current += 1;
    if (attemptId.current) void cancelProviderAccountLogin(provider, attemptId.current);
  }, [provider]);
  useEffect(() => {
    if (!pendingMode || attemptId.current) return;
    let active = true;
    void readPendingProviderAccount(provider).then((result) => {
      if (!active) return;
      if (result.value) { attemptId.current = result.value; setWindowAction(pendingMode); }
      else if (result.error) update({ transactionError: result.error });
    });
    return () => { active = false; };
  }, [provider, pendingMode]);

  const run = async (next: "login" | "check" | "logout") => {
    if (action) return;
    const request = ++generation.current;
    setAction(next);
    update({ transactionError: undefined });
    try {
      let result;
      if (next === "check") {
        result = await readAiAccountResult(provider, state.aiModel, state.aiReasoningEffort, "");
      } else {
        const reserved = await (next === "login" ? startProviderAccountLogin(provider) : logoutProviderAccount(provider));
        if (request !== generation.current) {
          if (reserved.value) void cancelProviderAccountLogin(provider, reserved.value);
          return;
        }
        if (!reserved.value) {
          update({ transactionError: reserved.error ?? "The sign-in window could not be prepared." });
          return;
        }
        attemptId.current = reserved.value;
        result = await waitForProviderAccountLogin(provider, reserved.value);
      }
      if (request !== generation.current) return;
      const account = result.value as AiAccountStatus | null | undefined;
      update({ aiAccount: account ?? null, transactionError: result.error,
        codexAnalysis: undefined, codexAnalysisRecord: undefined, maintenanceCodexAnalysisRecord: undefined });
      if (account) {
        const pending = account.auth_mode === "provider_login_pending" ? "login"
          : account.auth_mode === "provider_logout_pending" ? "logout" : null;
        setWindowAction(pending);
        if (!pending) attemptId.current = null;
      }
    } finally {
      if (request === generation.current) setAction(null);
    }
  };
  const cancel = async () => {
    ++generation.current;
    const currentAttempt = attemptId.current;
    const result = currentAttempt ? await cancelProviderAccountLogin(provider, currentAttempt) : { error: undefined };
    setAction(null);
    if (!result.error) { setWindowAction(null); attemptId.current = null; }
    update({ transactionError: result.error });
  };
  return <div className="provider-access" role="group" aria-label={`${label} account access`}>
    <p className="muted">Experimental account integration. Sign-in is handled by the official {label} client.</p>
    {!desktop && <p className="muted">Open the installed desktop app to sign in.</p>}
    {desktop && <>
      <p className={state.aiAccount?.error ? "callout review" : "muted"} role="status">
        {action === "login" ? `Preparing ${label} sign-in…` : action === "check" ? `Checking ${label}…` : action === "logout" ? `Opening ${label} sign-out…`
          : state.aiAccount?.authenticated ? `Signed in to ${label}` : state.aiAccount?.error ?? `Checking ${label} access…`}
      </p>
      <div className="button-row">
        {!state.aiAccount?.authenticated && <button type="button" className="button secondary" disabled={Boolean(action) || Boolean(windowAction) || Boolean(pendingMode)} aria-busy={action === "login" || undefined} onClick={() => void run("login")}>{windowAction === "login" ? "Sign-in window open" : windowAction === "logout" ? "Sign-out window open" : `Sign in to ${label}`}</button>}
        <button type="button" className="text-button" disabled={Boolean(action)} aria-busy={action === "check" || undefined} onClick={() => void run("check")}>Check again</button>
        {state.aiAccount?.authenticated && <button type="button" className="text-button" disabled={Boolean(action) || Boolean(windowAction)} onClick={() => void run("logout")}>Sign out</button>}
        {(windowAction || action === "login" || action === "logout") && <button type="button" className="text-button" onClick={() => void cancel()}>{windowAction === "logout" || action === "logout" ? "Cancel sign-out" : "Cancel sign-in"}</button>}
      </div>
    </>}
    <button type="button" className="text-button" onClick={onUseApi}>Use a {label} API key instead</button>
  </div>;
}
