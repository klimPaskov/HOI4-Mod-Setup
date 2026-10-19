// OpenRouter returns here after the user approves or declines. The code is
// handed to the setup tab over a same-origin channel and removed from the
// address bar. It is useless without the PKCE verifier, which never leaves
// that tab.

import { SIGN_IN_CHANNEL, type SignInMessage } from "./openrouter-channel";

const params = new URLSearchParams(window.location.search);
const message: SignInMessage = {
  type: "openrouter-sign-in",
  code: params.get("code"),
  state: params.get("state"),
};
window.history.replaceState(null, "", window.location.pathname);
const channel = new BroadcastChannel(SIGN_IN_CHANNEL);
channel.postMessage(message);
channel.close();
const status = document.getElementById("status");
if (status) {
  status.textContent = message.code
    ? "Signed in to OpenRouter. You can close this window and return to HOI4 Mod Setup."
    : "OpenRouter sign-in did not finish. Close this window and try again.";
}
if (message.code) window.setTimeout(() => window.close(), 800);
