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
- `ui-references/README.md`
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

Provider selection is the first compact setup step and defaults to Codex. The selected provider's model, optimization profile, and supported credential/endpoint controls must be visible without exposing secret values. Codex uses a compact ChatGPT sign-in with one primary action, one status line, a device-code fallback link, and a visible cancellation action while the App Server is waiting. Usage-limited state preserves the draft and offers retry or refresh without pretending planning succeeded. Never show raw protocol logs or model billing details.

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
- Progress shows current stage and durable checkpoint, not the full log.
- Existing-project scans show the current stage, bounded relative path, file/directory/byte counters, and an accessible Cancel scan action. Use an indeterminate progress bar until a total is known; partial and cancelled results must remain visibly incomplete and announce that no provider evidence was approved.
- Readiness leads with core status. Open in Codex is shown only for Codex; other providers receive an honest provider-specific handoff or no opener.
- Optional source-declared health actions are rendered only when readiness identifies a runnable verified route. `planned_unavailable` and `unsupported_platform` remain visible as status text without an actionable button.
- When no verified Codex opener is available, keep a passed readiness result visible and expose the validated project path as an announced manual-opening result.
- Optional incomplete states remain secondary.
- Keep a local recovery/removal entry reachable from Welcome when signed out; rollback, backup inspection, and managed removal do not depend on Codex.
- Preserve account and analysis state when logout or usage-limit errors occur, and announce the actionable error without losing focus.
- The final Git-phase option `Prepare a flattened ChatGPT project-sources folder` is a native keyboard-accessible checkbox shown only when Codex is selected. Its optional extras control is progressive, its mapping and no-upload Chat recommendation are stated concisely, and it never appears for another provider.
- The new-project description screen asks for the mod name and brief. The identity screen opens with generated project ID, prefix, namespace, tags, and starter folders; advanced metadata and external paths remain the only secondary/manual controls.

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

Programmatically focused screen headings use a visible underline treatment,
not a rounded title box; preserve the focus cue without adding decorative
containers around headings.

Keyboard shortcuts should exist where useful, but do not display a permanent shortcut legend.

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
`ui-references/` directory contains design references and is not a substitute
for screenshots of the current implementation. Never capture credentials,
identity documents, private projects, or secret values.

## Update this skill when

Update this skill when phase grouping, screen structure, disclosure rules, component conventions, accessibility target, interaction states, screenshot process, or visual regression workflow changes.
