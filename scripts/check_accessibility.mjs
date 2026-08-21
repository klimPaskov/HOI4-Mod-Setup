import { readFileSync } from "node:fs";
import { resolve } from "node:path";

const root = resolve(import.meta.dirname, "..");
const read = (relativePath) => readFileSync(resolve(root, relativePath), "utf8");
const app = read("src/App.tsx");
const activeApp = app.replace(/\/\*[\s\S]*?\*\//g, "");
const css = read("src/styles.css");
const types = read("src/types.ts");
const tauri = read("src/lib/tauri.ts");
const sourceManifest = read("docs/source-manifest/hoi4-mod-setup.v2.manifest.json");
const portraitRepositoryUrl = "https://github.com/klimPaskov/comfyui-hoi4-portraits";

const requiredAppTokens = [
  'aria-live="polite"',
  'role="status"',
  'aria-labelledby="screen-title"',
  'aria-describedby="screen-supporting"',
  'aria-label={maintenance ? "Maintenance phases" : "Setup phases"}',
  'aria-current={active ? "step" : undefined}',
  'role="progressbar"',
  'aria-valuetext',
  'role="switch"',
  'aria-checked',
  'aria-pressed',
  'tabIndex={-1}',
  'onKeyDown={closeDisclosureOnEscape}',
  'openInCodex',
  'openExternalUrlResult',
];

const requiredPhaseLabels = ["Project", "Review", "Components", "Integrations", "Git", "Install", "Ready", "Overview", "Update", "Conflicts", "Recovery"];
const requiredScreens = ["welcome", "description", "identity", "scan", "findings", "components", "workflows", "mesh", "mcp", "git", "dry-run", "install", "ready", "update", "chat-sources", "conflict", "recovery"];
const optionalWorkflowTitles = [
  "3D models workflow",
  "Super Events workflow",
  "ComfyUI portrait production",
];
const requiredCssTokens = [":focus-visible", "prefers-reduced-motion", "@media (min-width: 1920px)", "@media (max-width: 560px)", "overflow: auto", ".visually-hidden", ".button.primary:disabled", ".toggle-row > span:last-child", ".path-preview", "overflow-wrap: anywhere", "min-width: 0", ".provider-panel", ".provider-help"];
const requiredTypeTokens = ["conflictChoice?: ConflictChoice", "recoveryChoice: RecoveryChoice", "interface InstallationPlan", "interface TransactionJournal"];
const requiredTauriTokens = ["interface TauriCommandMap", "isTauriRuntime", '"codex_account_read"', '"codex_login_start"', '"codex_login_cancel"', '"open_codex_login_url"', '"codex_analyze"', '"confirm_codex_analysis"', '"pick_project_folder"', '"pick_launcher_folder"', '"scan_project"', '"cancel_scan"', '"scan-progress"', '"preview_source_manifest"', '"preview_installation_conflict"', '"build_installation_plan"', '"build_maintenance_plan"', '"approve_installation"', '"resolve_installation_conflict"', '"apply_installation"', '"rollback_installation"', '"open_in_codex"', '"pick_chat_sources_folder"', '"preview_chat_sources"', '"package_chat_sources"', "Promise<InstallationPlan | null>", "Promise<TransactionJournal | null>"];

if (requiredScreens.length !== 17) throw new Error(`expected 17 screens, found ${requiredScreens.length}`);
for (const token of requiredAppTokens) if (!app.includes(token)) throw new Error(`missing UI accessibility hook: ${token}`);
for (const label of requiredPhaseLabels) if (!app.includes(`label: "${label}"`)) throw new Error(`missing phase label: ${label}`);
for (const screen of requiredScreens) {
  const screenToken = screen.includes("-") ? `"${screen}":` : `${screen}:`;
  if (!app.includes(screenToken)) throw new Error(`missing screen definition: ${screen}`);
}
for (const title of optionalWorkflowTitles) if (!activeApp.includes(title)) throw new Error(`optional workflow title is missing: ${title}`);
for (const question of ["Do you want to set up the 3D models workflow?", "Do you want to set up the Super Events workflow?", "Do you want to set up the portrait production workflow?"]) {
  if (activeApp.includes(question)) throw new Error(`question-form workflow title remains: ${question}`);
}
if (app.includes("Do you want to set up LoRAs and ComfyUI for portrait generation?")) throw new Error("removed portrait setup question is still present");
const portraitLinkCount = app.split(portraitRepositoryUrl).length - 1;
if (portraitLinkCount !== 1) throw new Error(`expected one Ready-screen portrait link, found ${portraitLinkCount}`);
const appWithoutPortraitUrl = app.replace(portraitRepositoryUrl, "");
for (const token of ["scan-dots", "fake-window", "window-chrome", "app-window", "Disk space", "Backup location"]) {
  if (appWithoutPortraitUrl.includes(token)) throw new Error(`retired UI pattern remains in App.tsx: ${token}`);
}
if (/\.panel-title\s*\{[^}]*border-bottom/i.test(css)) throw new Error("panel titles must not use underline borders");
if (/\.screen-heading h1\s*\{[^}]*text-decoration\s*:\s*underline/i.test(css)) throw new Error("screen headings must not be underlined");
const semanticProgressRule = css.match(/\.semantic-planning-progress\s*\{([^}]*)\}/)?.[1] ?? "";
if (!/width\s*:\s*min\(100%,\s*920px\)/i.test(semanticProgressRule) || !/margin\s*:\s*0\s+auto\s+18px/i.test(semanticProgressRule)) {
  throw new Error("Description planning progress must stay centered at the Description form width");
}
for (const [surface, contents] of Object.entries({ app, types, tauri, sourceManifest })) {
  for (const token of ["loraInterest", "installedLoraInterest", '\"lora\":', "lora_interest", "workflow.lora_comfyui_interest"]) {
    if (contents.includes(token)) throw new Error(`removed portrait setup state ${token} remains in ${surface}`);
  }
}
for (const token of requiredCssTokens) if (!css.includes(token)) throw new Error(`missing accessibility styling hook: ${token}`);
for (const token of requiredTypeTokens) if (!types.includes(token)) throw new Error(`missing typed UI state contract: ${token}`);
for (const token of requiredTauriTokens) if (!tauri.includes(token)) throw new Error(`missing typed Tauri boundary hook: ${token}`);
if (app.includes("import(\"./lib/tauri\")") || app.includes("invokeCommand")) throw new Error("App.tsx must use named typed Tauri wrappers, not the low-level invoker");

console.log("Accessibility contract covers seven setup phases, four maintenance phases, 17 screens, keyboard/focus hooks, disclosure escape handling, reduced motion, scaling, ordered declarative workflow titles, one completed-Ready portrait link, and the typed Tauri boundary.");
