// The same-origin channel the OpenRouter callback page uses to hand a code
// back to the setup tab.

export const SIGN_IN_CHANNEL = "hoi4-mod-setup-openrouter-sign-in";

export interface SignInMessage {
  type: "openrouter-sign-in";
  code: string | null;
  state: string | null;
}
