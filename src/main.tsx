import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import App, { initialState } from "./App";
import "./styles.css";

async function start() {
  if (import.meta.env.DEV && new URLSearchParams(window.location.search).has("screenshot")) {
    const { documentationFixture } = await import("./documentation-fixtures");
    window.__HOI4_DOCUMENTATION_STATE__ = documentationFixture(initialState);
  }

  createRoot(document.getElementById("root")!).render(
    <StrictMode>
      <App />
    </StrictMode>,
  );
}

void start();
