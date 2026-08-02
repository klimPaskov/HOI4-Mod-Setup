/// <reference types="vite/client" />

import type { WizardState } from "./types";

declare global {
  interface Window {
    __HOI4_DOCUMENTATION_STATE__?: WizardState;
  }
}
