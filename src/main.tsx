import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import App, { initialState } from "./App";
import "./styles.css";

async function start() {
  if (import.meta.env.DEV && new URLSearchParams(window.location.search).has("screenshot")) {
    const { documentationFixture } = await import("./documentation-fixtures");
    const fixture = documentationFixture(initialState);
    // Review captures may show the choice before the user selects a client.
    if (fixture && new URLSearchParams(window.location.search).get("coding-environments") === "none") {
      fixture.primaryCodingEnvironment = "";
      fixture.additionalCodingEnvironments = [];
      const clientIds = new Set(fixture.manifestPreview?.components.filter((component) => component.coding_environment).map((component) => component.id) ?? []);
      fixture.selectedComponents = fixture.selectedComponents.filter((id) => !clientIds.has(id));
      fixture.components = fixture.components.map((component) => clientIds.has(component.id) ? { ...component, selected: false } : component);
    }
    window.__HOI4_DOCUMENTATION_STATE__ = fixture;
  }

  createRoot(document.getElementById("root")!).render(
    <StrictMode>
      <App />
    </StrictMode>,
  );
}

void start();
