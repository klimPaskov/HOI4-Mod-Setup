# UI and accessibility audit handoff

Date: 2026-07-26

Scope: `src/App.tsx`, `src/styles.css`, `src/types.ts`, `src/lib/tauri.ts`, `src/App.test.tsx`, and `scripts/check_accessibility.mjs`.

Authority: `.codex/agents/hoi4setup_ui_accessibility_auditor.toml`, with the repo UI/accessibility skill, `docs/02_user_flows.md`, `docs/17_ui_accessibility.md`, `docs/22_acceptance_criteria.md`, `ui-references/README.md`, and all 17 full-resolution references reviewed. No Rust source was changed. The requested agent configuration requires `fork_context=false`; this session exposed no separate subagent-spawn interface, so the bounded auditor scope was applied directly and no separate subagent handoff is claimed.

## Result

A narrow UI/accessibility patch was applied. The seven setup phases and all 17 screen definitions remain in scope. Maintenance screens now use the specified four-item rail. Screen navigation has a named main region, a polite title status, and focus recovery. Selection, progress, disclosure, diff, readiness, and Tauri-boundary semantics were tightened without redesigning the wizard.

## Files changed

- `src/App.tsx`
  - Added the four maintenance phases: Overview, Update, Conflicts, Recovery.
  - Moves focus to the new screen heading and exposes screen title/supporting text to the main landmark.
  - Keeps the whole main viewport out of `aria-live`; adds a small polite title status instead.
  - Adds Escape-to-close behavior for open native disclosures.
  - Adds accessible progress labels/value text, pressed state for card/row choices, conflict/recovery state, semantic diff headings, and explicit field labels.
  - Prevents dry-run installation while conflicts are unresolved and keeps the UI from advancing when the typed plan/journal boundary returns no result.
  - Makes readiness unavailable until the readiness report says it is openable, and does not present a stored Meshy credential as verified.
  - Routes “Open in Codex” through the named Tauri wrapper.
- `src/styles.css`
  - Adds visually-hidden utility styling and a visible heading focus ring.
  - Resets semantic panel/diff headings.
  - Uses one-column narrow layouts at the smallest breakpoint to reduce 200% zoom and translation overflow risk.
  - Retains reduced-motion and wide-window rules.
- `src/types.ts`
  - Adds typed conflict/recovery/error state and wire types for installation plans and transaction journals.
- `src/lib/tauri.ts`
  - Adds a command-to-argument/result map for the Tauri boundary and typed wrappers for plan, apply, and Open in Codex commands.
- `src/App.test.tsx`
  - Adds assertions for grouped phase semantics, exact workflow switch state, heading focus, named scan progress, and unresolved-conflict blocking.
- `scripts/check_accessibility.mjs`
  - Verifies the seven phases, four maintenance phases, exactly 17 screen definitions, exact optional questions, focus/live/progress/disclosure hooks, narrow scaling/reduced-motion hooks, and the typed Tauri wrapper boundary.
- `docs/development/handoffs/ui-audit/report.md`
  - This handoff.

## Before and after

| Surface | Before | After |
| --- | --- | --- |
| Phase rail | Maintenance screens still rendered the seven setup phases. | Maintenance uses Overview, Update, Conflicts, Recovery; setup retains Project, Review, Components, Integrations, Git, Install, Ready. |
| Screen changes | Focus could remain on an unmounted control. | The new `h1` receives programmatic focus without scrolling; main is labelled by the heading and supporting sentence. |
| Live announcements | The complete main viewport was `aria-live="polite"`. | Only the changing screen title is in a small polite status region. |
| Progress | Progressbars had no accessible name or value text. | Scan and installation progress have labels, bounded values, and `aria-valuetext`. |
| Selection state | Several selected cards, findings, conflicts, and recovery choices were conveyed mainly by styling. | Native buttons expose `aria-pressed`; switches retain `role="switch"` and `aria-checked`. |
| Field semantics | `Field` wrapped an input and Browse button inside a `label`. | `Field` uses an explicit `label`/`htmlFor` association, leaving the action button outside the label. |
| Disclosure | Native details had no Escape behavior. | Escape closes the focused open disclosure and returns focus to its summary. |
| Dry run | `merge` was preselected, showing zero conflicts and enabling installation. | Conflicts start unresolved, are visible as a blocking status, and disable Start installation until a resolution is selected. |
| Readiness | A missing report defaulted to an enabled Open in Codex action. | Readiness is pending and Open in Codex is disabled until the typed report enables it. |
| Tauri boundary | App dynamically called the generic invoker for Open in Codex; plan/apply used `unknown`. | App uses named wrappers; command argument/result shapes are mapped and plan/journal types are explicit. |
| Narrow scaling | The smallest breakpoint kept two columns for forms, metrics, and action tiles. | The smallest breakpoint stacks those grids to reduce clipping and horizontal pressure. |

## Screen and criteria audit

- Seven grouped setup phases: pass in `PhaseRail` (`src/App.tsx:6-14`, `src/App.tsx:257-271`).
- 17 screens: all are represented in `ScreenId`, `screenCopy`, routing, and the static contract check. The reference set reviewed was: `01_welcome_project_selection.png`, `02_new_mod_description.png`, `03_project_identity_descriptor.png`, `04_existing_project_scan.png`, `05_finding_review.png`, `06_component_selection.png`, `07_optional_workflow_selection.png`, `08_3d_meshy_key_setup.png`, `09_lora_comfyui_placeholder.png`, `10_mcp_credentials.png`, `11_git_setup.png`, `12_dry_run_review.png`, `13_installation_progress.png`, `14_final_readiness.png`, `15_update_repair.png`, `16_merge_conflict_review.png`, and `17_interrupted_install_recovery.png`.
- Progressive disclosure: the existing `details`/`summary` structure remains the default for evidence, file plans, dependencies, advanced fields, logs, requirements, and readiness JSON. Selected-finding evidence remains visible because it is the current review task. Runtime persistence of an opened disclosure across a screen unmount is not implemented.
- One-task density: the patch does not add permanent evidence, log, or shortcut regions. Conflict review remains the intentional dense exception.
- Keyboard/focus: native buttons, inputs, radios, switches, selects, summaries, and text areas remain keyboard reachable; focus-visible styling exists; screen transitions and Escape disclosure behavior are covered statically and partially by tests. Full Tab/Shift+Tab traversal across every screen still needs desktop verification.
- WCAG 2.2 AA: semantic headings, labels, landmarks, progress semantics, pressed/switch state, and text/icon status cues are improved. A runtime contrast/axe audit and screen-reader pass were not available in this workspace.
- Reduced motion: the existing `prefers-reduced-motion` rule disables transitions and animations; the progress timers still update state and need an OS-level runtime check.
- Non-color status: pass/review/block/unavailable/selected states retain visible words, symbols, `aria-current`, or `aria-pressed`; scan dots are explicitly decorative and progress text carries the meaning.
- 200% scaling: the narrow breakpoint now stacks dense grids and content remains scrollable while the footer remains outside `content-scroll`; actual Windows/macOS zoom captures remain pending.
- Exact optional questions: both required strings remain unchanged in `Workflows` and are asserted by tests and the contract script.
- Typed Tauri boundary: command names, argument shapes, and result types are mapped in `src/lib/tauri.ts:32-60`; App no longer uses `invokeCommand` directly.

## Tests and validation run

- `pnpm test` — 4 tests passed.
- `pnpm lint` — passed.
- `pnpm test:a11y` — passed; contract covers phases, 17 screens, focus, disclosure, scaling, reduced motion, exact questions, and Tauri typing.
- `pnpm typecheck` — passed.
- `pnpm build` — passed.
- `pnpm test:e2e` — passed as the repository’s structural smoke fixture; it reports that the desktop matrix must run on Windows/macOS CI runners.

## Skipped relevant checks

- No runtime screenshot capture was available because the workspace exposes no browser/desktop capture runner and Playwright/Puppeteer are not installed. The 17 supplied 1536×1024 reference images were visually reviewed.
- No NVDA, VoiceOver, or browser accessibility-tree review was available.
- No automated axe/contrast scan, 100/125/150/200% runtime capture, reduced-motion OS toggle capture, or Windows/macOS keyboard matrix was run.
- No Rust tests or transaction fault-injection tests were run; Rust was intentionally outside scope.
- No visual-regression baseline was updated; the handoff records this as pending rather than treating reference-image review as a runtime screenshot.

## Remaining risks and parent follow-up

1. Scan and installation screens still lack the reference Cancel/Pause controls and corresponding cancellation state; adding those would require explicit behavior and backend coordination.
2. Identity Preview, MCP Details, recovery Details, and some Browse affordances are presentational controls without implemented disclosure/action handlers.
3. The Meshy “Save and test” action stores a credential reference but does not run the repository-declared toolchain health check; the UI now reports the key as present rather than verified, but the action copy/behavior still needs an owning integration change.
4. The readiness screen still renders a compact hard-coded check summary and a raw JSON detail rather than mapping every returned check and evidence item into the UI.
5. The generic Tauri helper still collapses unavailable commands and backend errors to `null`; the plan path now stops safely with an alert, but scan, readiness, credential, and Open in Codex errors need differentiated user recovery states.

## Parent remediation addendum

The Meshy action now has a typed `run_3d_health_check` route. It is exposed from
the Ready screen only after installation, re-resolves the locked manifest,
verifies the installed bootstrap hash and size, and reports sanitized output;
the UI keeps the workflow incomplete unless the core reports `ready`. The dry
run also exposes manifest-declared command validations as approval-bound
external actions.
6. Conflict panes expose accessible local/incoming code previews, but the displayed sample diff does not yet render a validated result preview after a resolution choice.
7. Opened disclosure state is not persisted when a screen unmounts, and full runtime keyboard/focus/error-recovery verification remains pending.

No schema, Rust, product, or general documentation changes were made. No living-skill update was needed because this patch did not change the UI workflow contract or introduce a new recurring implementation procedure.
