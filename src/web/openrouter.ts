// OpenRouter sign-in in the browser (OAuth PKCE, the flow OpenRouter offers
// to third-party apps). openrouter.ai opens in a popup; the callback page
// hands the code back over a same-origin channel; the page exchanges it for a
// key tied to the user's own account. The key stays in this tab's memory,
// like a pasted key.

import { recordRequest } from "./github";
import type { CoreClient } from "./core-client";
import { AssistantError } from "./assistant";
import { SIGN_IN_CHANNEL, type SignInMessage } from "./openrouter-channel";

export const KEY_EXCHANGE_URL = "https://openrouter.ai/api/v1/auth/keys";
const SIGN_IN_TIMEOUT_MS = 5 * 60_000;

/** The callback page beside this one, without query or fragment. */
export function callbackUrl(pageUrl: string = window.location.href): string {
  const url = new URL("openrouter-callback.html", pageUrl);
  url.search = "";
  url.hash = "";
  return url.href;
}

/** Wait for the callback page to report this sign-in's code. */
export function waitForCode(state: string, signal: AbortSignal, timeoutMs = SIGN_IN_TIMEOUT_MS): Promise<string> {
  return new Promise((resolve, reject) => {
    const channel = new BroadcastChannel(SIGN_IN_CHANNEL);
    const finish = (outcome: { code?: string; error?: AssistantError }) => {
      clearTimeout(timer);
      signal.removeEventListener("abort", onAbort);
      channel.close();
      if (outcome.code) resolve(outcome.code);
      else reject(outcome.error);
    };
    const onAbort = () => finish({ error: new AssistantError("Sign-in was cancelled.") });
    const timer = setTimeout(() => finish({ error: new AssistantError("Sign-in timed out. Choose Sign in with OpenRouter again.") }), timeoutMs);
    signal.addEventListener("abort", onAbort);
    if (signal.aborted) onAbort();
    channel.onmessage = (event: MessageEvent<SignInMessage>) => {
      const data = event.data;
      if (!data || data.type !== "openrouter-sign-in") return;
      // A declined sign-in returns without code or state.
      if (!data.code && !data.state) {
        finish({ error: new AssistantError("OpenRouter sign-in was not approved.") });
        return;
      }
      if (data.state !== state) return;
      if (data.code) finish({ code: data.code });
      else finish({ error: new AssistantError("OpenRouter sign-in was not approved.") });
    };
  });
}

/** Sign in with OpenRouter and return the user's key. */
export async function signInWithOpenRouter(core: CoreClient, signal: AbortSignal, open: typeof window.open = window.open.bind(window)): Promise<string> {
  const { url, verifier, state } = await core.call("openRouterStart", { callback: callbackUrl() });
  // Listen before the popup opens so a fast reply is not missed; stop
  // listening if the popup is blocked or the caller cancels.
  const listening = new AbortController();
  const cancel = () => listening.abort();
  signal.addEventListener("abort", cancel);
  if (signal.aborted) cancel();
  const waiting = waitForCode(state, listening.signal);
  let code: string;
  try {
    const popup = open(url, "openrouter-sign-in", "popup,width=520,height=720");
    if (!popup) {
      listening.abort();
      waiting.catch(() => undefined);
      throw new AssistantError("Allow pop-ups for this page, then choose Sign in with OpenRouter again.");
    }
    code = await waiting;
  } finally {
    signal.removeEventListener("abort", cancel);
  }
  const body = await core.call("openRouterKeyRequest", { code, verifier });
  let response: Response;
  try {
    response = await fetch(KEY_EXCHANGE_URL, {
      method: "POST",
      headers: { "content-type": "application/json", accept: "application/json" },
      body,
      redirect: "error",
      credentials: "omit",
      referrerPolicy: "no-referrer",
      signal,
    });
  } catch {
    recordRequest(KEY_EXCHANGE_URL, "failed");
    throw new AssistantError("OpenRouter could not be reached. Check your connection and try again.");
  }
  recordRequest(KEY_EXCHANGE_URL, response.status);
  const text = await response.text();
  if (!response.ok) throw new AssistantError(`OpenRouter did not accept the sign-in (HTTP ${response.status}).`);
  try {
    return await core.call("openRouterKey", { response: text });
  } catch {
    throw new AssistantError("OpenRouter did not return a key. Try again.");
  }
}
