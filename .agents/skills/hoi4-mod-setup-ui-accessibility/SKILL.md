---
name: hoi4-mod-setup-ui-accessibility
description: Use for wizard screens, interaction design, visual density, progressive disclosure, file previews, conflict review, progress, readiness, keyboard behavior, accessibility, or visual regression changes.
---

# Minimal desktop UI and accessibility

## Required sources

Read:

- `AGENTS.md`
- `docs/02_user_flows.md`
- `docs/17_ui_accessibility.md`
- `docs/ui-references/README.md`
- the relevant full-resolution screen references
- `docs/22_acceptance_criteria.md`
- `docs/screenshots/README.md` when adding or auditing implementation screenshots

## Design baseline

Use a clean dark desktop interface with restrained blue and violet accents.

Use seven grouped phases:

- Project
- Review
- Components
- Integrations
- Git
- Install
- Ready

Provider selection is the first compact setup step and defaults to Codex. Codex uses a compact ChatGPT sign-in with one primary action, one status line, a device-code fallback link, and a visible cancellation action while the App Server is waiting. Known hosted profiles auto-fill their verified model and address, show one fixed official API-key link, one secret field, and one Connect action, and place model/address overrides under Advanced. Local and custom profiles expose only the details they genuinely require. Usage-limited state preserves the draft and offers retry or refresh without pretending planning succeeded. Never show raw protocol logs, model billing details, or technical terms such as endpoint in the normal hosted-provider path.

Each screen normally has:

- one title
- zero or one short supporting sentence
- one focal task
- no more than two visible content regions
- persistent navigation controls

Do not add permanent evidence sidebars, repeated status summaries, help paragraphs that restate controls, or visible technical detail that is not needed for the current decision.

## Progressive disclosure

Hide these by default:

- full evidence
- hashes
- source URLs
- dependency graphs
- complete file lists
- raw logs
- advanced settings
- command environment detail

Use `Details`, `Preview`, `Advanced`, or `Show log` controls. Preserve state when a section is opened and the user navigates back.

The Components screen's collapsed `Dependencies and file list` disclosure must
render the exact manifest-declared dependencies, platform routes, destination,
and declared file count for each component. Keep hashes and full source paths
in the dry-run evidence instead of expanding the default component rows.

Conflict review is the exception. It may show a three-way comparison and more controls because comparison is the primary task.

## Interaction rules

- One primary action per screen.
- Back remains visible where navigation is reversible.
- Destructive actions require clear scope and confirmation.
- Disabled actions explain the blocking reason in a tooltip or adjacent concise status.
- The Description primary action uses a visible `Preparing details…` pending
  label with `aria-busy`, blocks duplicate submission, and exposes any provider
  or schema failure in the persistent announced footer without losing the
  entered name or brief.
- The Welcome and planning actions remain disabled until the selected provider
  is ready and not usage-limited; Codex requires ChatGPT mode. Keep signed-out
  recovery and removal reachable from Welcome.
- Progress shows current stage and durable checkpoint, not the full log.
- Plan preparation must have a visible indeterminate busy region so the window never appears frozen while the reviewed file plan is built. Installation rows show 0% before work and 100% after completion; staging and apply may show intermediate percentages only from journaled completed-file counts, together with the current root-relative destination. Use an active non-numeric state when a stage has no measurable total. Show an approximate remaining-time label only after real elapsed progress is available and keep a once-per-second clock while active so a long file does not make the window appear frozen.
- Starting installation rechecks for an unfinished setup on the selected
  project. When found, open the recovery screen with a concise message and the
  core-approved Continue, Undo, or Discard choices instead of leaving a raw
  overlapping-transaction error on the Install screen.
- Once a reviewed installation starts, switch to Progress before awaiting the
  core transaction and suspend background recovery discovery until that call
  finishes. On Recovery, refresh the journal and select only its currently
  allowed recommended action; a stale or disabled choice must never retain the
  selected appearance.
- Render only recovery actions that the normalized journal currently permits. Do not show Undo when no project files changed, or show Continue/Discard after apply began, as disabled cards that look broken.
- While the transaction command is active, poll its exact reviewed transaction
  ID through the typed journal reader and render the durable stage plus a
  human-readable file count. Accept the schema's `complete` stage status; do
  not leave completed rows labeled Next or show Awaiting transaction after the
  journal exists.
- Existing-project scans show the current stage, bounded relative path, file/directory/byte counters, and an accessible Cancel scan action. Use an indeterminate progress bar until a total is known; partial and cancelled results must remain visibly incomplete and announce that no provider evidence was approved.
- On the optional-workflow screen, keep the exact first question **Do you want to set up the 3D models workflow?** and place the **Do you want to set up the Super Events workflow?** checkbox/toggle immediately after it. Preserve that order in maintenance when both workflows are offered; do not paraphrase the 3D question.
- When scan evidence identifies a valid managed installation, show a concise
  existing-setup callout with a keyboard-accessible **Repair or add workflows**
  action. The maintenance screen must show the exact 3D question again only
  for a previously unselected workflow; an already installed workflow is a
  non-duplicating state. If the lock reports a selected workflow without a
  stored key, keep the exact question visible as a disabled installed state
  and expose the vault-only key field for repair.
- Readiness leads with core status. Open in Codex is shown only for Codex; other providers receive an honest provider-specific handoff or no opener.
- Optional source-declared health actions are rendered only when readiness identifies a runnable verified route. `planned_unavailable` and `unsupported_platform` remain visible as status text without an actionable button.
- In the dry run, call manifest-declared validation actions **Setup checks**. Show a simple included count and human-readable check names by default; keep commands, folders, environment names, and expected changes behind a nested disclosure. Do not show internal risk labels or component IDs in the normal interface.
- When no verified Codex opener is available, keep a passed readiness result visible and expose the validated project path as an announced manual-opening result.
- Optional incomplete states remain secondary.
- `workflow.super_events` is provider-neutral and its `not_selected`,
  `incomplete`, or unsupported status remains visible but never disables core
  readiness. Retain the non-secret lock summary, completed scan context, and
  readiness result while navigating review/maintenance; refresh them only after
  a new scan, transaction, or explicit readiness refresh.
- Do not use decorative traffic-light dots, title boxes, title underlines, or
  an app-window mock frame inside the native window. The app shell is the
  window.
- Components link to the public Agentic HOI4 Modding source by name. Keep exact
  commit and file evidence inside progressive disclosure instead of using it
  as the primary status message.
- In the desktop runtime, reviewed source and Ready-screen links use the typed
  system-browser bridge and its fixed allowlist; browser preview may use a
  normal anchor. Do not add arbitrary URL navigation to React.
- LoRA and ComfyUI are not setup choices or readiness rows. After core setup
  completes, Ready may show one concise keyboard-accessible link to the fixed
  `klimPaskov/comfyui-hoi4-portraits` repository; do not present it as an
  install, maintenance, or health action.
- Keep a local recovery/removal entry reachable from Welcome when signed out; rollback, backup inspection, and managed removal do not depend on Codex.
- Preserve account and analysis state when logout or usage-limit errors occur, and announce the actionable error without losing focus.
- The Components option `Prepare a flattened ChatGPT project-sources folder` is a native keyboard-accessible checkbox shown only when Codex is selected. Present it with the same visual structure as the other component choices: title, concise contents, file count, and an expandable per-file size list. Source-declared sizes may appear immediately; generated-file and exact total sizes appear after the plan is prepared. Install review is read-only, there is no additional-files control, the no-automatic-upload Chat recommendation appears on Ready, and the choice never appears for another provider.
- Keep each status symbol and its label in one inline row. Do not let generic child selectors turn the `Status` component into a grid or stack the symbol above the text.
- The new-project description screen asks for the mod name and brief. The identity screen opens with generated project ID, prefix, namespace, tags, and starter folders; advanced metadata and external paths remain the only secondary/manual controls.
- New-project identity auto-fills the standard HOI4 Documents/mod destination
  and matching launcher descriptor when available, reports collisions or an
  unavailable standard directory, and keeps one explicit manual folder route.
  Existing-project selection displays only a launcher descriptor discovered
  from the selected root's immediate parent.

## Accessibility

Meet WCAG 2.2 AA.

Require:

- complete keyboard operation
- logical focus order
- visible focus
- screen-reader names and descriptions
- status not conveyed by color alone
- reduced motion
- 200 percent scaling
- long path and translation resilience
- error recovery without focus loss
- accessible diff and code preview semantics

Screen headings are plain text with no underline, rounded box, or decorative
container. Programmatic heading focus supports screen-reader navigation; visible
focus remains mandatory for every interactive control.

Keyboard shortcuts should exist where useful, but do not display a permanent shortcut legend.

Show an available application update as one compact, keyboard-accessible
title-bar action. Keep current and offline background checks silent and never
block the wizard on updater state.

## UI change evidence

A visible change requires:

- screenshots at the target desktop size
- keyboard path
- focus and screen-reader review
- 200 percent scaling review
- reduced motion review when animated
- visual regression update with explanation
- density review against the one-task rule

Implementation screenshots live in `docs/screenshots/` and are linked from the
user-facing README. Keep their alt text and capture notes accurate. The
`docs/ui-references/` directory contains design references and is not a substitute
for screenshots of the current implementation. Never capture credentials,
identity documents, private projects, or secret values.

## Update this skill when

Update this skill when phase grouping, screen structure, disclosure rules, component conventions, accessibility target, interaction states, screenshot process, or visual regression workflow changes.
