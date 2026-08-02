# UI specification and accessibility

## Visual direction

- dark charcoal desktop canvas
- restrained blue and violet accents
- green pass, amber review, red block
- flat surfaces with light borders and little ornament
- one clear focal area per screen
- no marketing layout inside the application
- no imitation of the Hearts of Iron IV game interface
- generous empty space around the current task

## Minimal interface rule

Every screen should show only what the user needs to decide or verify at that moment.

Use these limits as the default:

- one title
- zero or one supporting sentence
- one primary work area
- no more than two visible content regions
- no more than seven visible rows before scrolling or progressive disclosure
- one primary action
- one secondary action when needed
- status chips only when they affect the current decision

Do not repeat the same fact in the heading, body text, status chip, footer, and evidence panel. State it once in the place where it is most useful.

Do not explain ordinary controls. A field called `Default branch` does not need a paragraph explaining what a default branch is. A `Browse` button does not need supporting copy. Use help text only for risk, ambiguity, credentials, irreversible actions, or behavior that differs from normal desktop expectations.

## Progressive disclosure

Keep secondary information behind a clear control such as:

- Details
- Preview
- Advanced
- Show log
- Dependencies and file list
- Open full plan

Evidence, hashes, source paths, detector notes, full dependency graphs, file lists, tool schemas, and verbose explanations should be hidden until requested unless the user is reviewing that exact material.

The application must never require a hidden detail panel to understand the primary decision. Details support review. They do not carry essential instructions.

## Provider gate

Before project selection, show a compact provider gate with the product name,
provider and model selection, and one short configuration state. Codex shows a
**Sign in with ChatGPT** action and device-code fallback through the official
App Server. Known hosted profiles show an official API-key page, one key field,
and a Connect action; verified model and address defaults stay under Advanced.
Local profiles show a loopback address and no hosted account claim.
Keep a local recovery link when an interrupted transaction exists.

Do not show token fields, model marketing copy, plan comparisons, protocol details, or long privacy text. Put technical details behind one disclosure.

After sign-in, keep account state in the title bar or settings as one compact row. The review labels remain `Detected`, `Suggested by Codex`, and `Confirmed`.

## Window structure

### Title bar

Show the app name, one compact workflow-source state, and native window controls.

### Persistent phase rail

Use seven grouped setup phases instead of listing every screen:

1. Project
2. Review
3. Components
4. Integrations
5. Git
6. Install
7. Ready

A phase can contain several screens. The rail shows completed, current, and future phases. It does not show maintenance screens during the setup wizard.

Update, conflict resolution, and interrupted recovery use a separate four-item maintenance rail:

1. Overview
2. Update
3. Conflicts
4. Recovery

### Main viewport

Keep the active form or review surface near the upper left of the content area. Use a comfortable maximum width instead of stretching controls across the window.

Most setup screens use a centered single column up to 920 pixels, aligned with the screen heading. A two-column layout is reserved for direct comparisons or a selected-item detail view.

### Footer

Keep Back and the primary action visible. Show one short state note only when it prevents uncertainty, such as `Nothing is changed until the dry run.`

Do not display permanent keyboard shortcut hints in the footer. Keyboard support remains available without occupying interface space.

## Screen direction

1. **Welcome and project selection**: two choices and a compact recent-project list. Hide source policy and installation policy details.
2. **New mod description**: mod name plus one large brief field with optional inferred-topic chips.
3. **Project identity and descriptor setup**: populated generated identity fields, auto-filled root and launcher paths with editable overrides, compact form, generated-file rows, previews on demand, advanced fields collapsed.
4. **Existing project scan**: visible bounded launcher-candidate confirmation before one progress surface with current scan stage, detected count, and cancel or pause.
5. **Finding review**: compact finding list and one selected finding. Show evidence only for the selected item.
6. **Component selection**: recommended component rows, sizes, one collapsed dependency and file-list control.
7. **Optional workflows**: two concise rows in order—the exact 3D question,
   immediately followed by **Do you want to set up the Super Events workflow?**
   The Super Events row shows no credential control or provider-specific
   status.
8. **3D and Meshy key**: credential field, secure-storage choice, test action, compact status, requirements open by default. Keep credential actions visibly separated from the field; Configure later advances without deleting an existing stored key.
9. **MCP and credentials**: compact server rows and credential names. Expand capabilities and environment details on demand.
10. **Git setup**: three choices, branch and commit fields for the selected choice, remote and advanced options collapsed.
11. **Install review**: change counts, a read-only summary of selected ChatGPT project files, short plan summary, preflight state, and a lazily rendered full file-and-folder plan on demand. Show starter directories as **Create folder** entries and never as `.gitkeep` files. Keep the preparation panel aligned to the same maximum width as the review below it.
12. **Installation progress**: one progress bar with the current `x of y files` count, percentage, and time estimate; six grouped stages; transaction log collapsed.
13. **Final readiness**: one success state, an Open in Codex action only for Codex, four grouped core checks, compact 3D state, the final ChatGPT “Chat” recommendation when applicable, and one concise fixed HTTPS external portrait-workflow link.
14. **Update and repair**: four primary maintenance actions and a short installed-state list.
15. **Merge conflict review**: local and incoming comparison, resolution choices, result preview after selection.
16. **Interrupted recovery**: one checkpoint summary and three recovery choices.

The final readiness area also shows the non-blocking `workflow.super_events`
state beside 3D, without adding a separate screen.

When scan evidence identifies a managed installation, the maintenance screen
offers **Repair or add workflows**. Show the exact 3D or Super Events question
again only for its previously unselected workflow; an installed workflow is a
non-duplicating state. The Super Events state never exposes a credential field.

## Evidence and file previews

Evidence should be attached to the finding, file, component, or server it supports. Avoid a permanent generic Evidence panel.

A detail view may show:

- source path
- source revision
- confidence
- detector or validator
- short explanation
- matching files
- hash or signature

Text previews support syntax highlighting, line numbers, wrap, source and destination labels, diff, copy, and an external-editor action. Binary previews show metadata and a visual preview when supported.

## Conflict comparison

Conflict resolution is one of the few intentionally dense screens. Keep the density focused on the actual comparison.

Show:

- local version
- incoming version
- selected resolution
- result preview after selection
- validation result

Do not surround the diff with unrelated project status, source policy, or dependency information.

## Progress

Show the current stage, completed stages, current operation, item count, elapsed time when useful, cancellation state, and last durable checkpoint.

Long-running Tauri commands remain responsive because filesystem, network, Git,
and provider waits use async/thread-pool dispatch; the UI must not imply
progress from a blocked event loop.

Do not invent a percentage when total work is unknown. Plan preparation uses a visibly active indeterminate bar. Installation rows show 0% before work and 100% after completion; staging and apply use journaled file counts for truthful intermediate percentages and expose the current relative destination. The overall surface shows a clearly approximate time remaining derived from measured elapsed progress and falls back to `Calculating time remaining` until enough evidence exists. Do not expose the full transaction log by default.

The app detects the current computer internally. Do not display operating-system or generic platform details in ordinary forms, planning summaries, component requirements, or guidance. When a selected route cannot run, use the concise user-facing state **Not available on this computer**.

Recovery uses the selected action as the primary-button label. While Continue, Undo, or Discard is running, disable duplicate actions and replace the choice cards with one live progress surface. Rollback backup and restore percentages come only from the child rollback journal's actionable-file records; use an indeterminate state until that journal exists. After a standalone undo, return to project selection so setup can start again; if a newly prepared plan encountered the older transaction, invalidate that stale plan and return to review for a fresh preparation.

Recovery progress panels use their own internal padding and contain long
project-relative paths at 200 percent scaling; text, bars, and filenames must
not touch or cross the panel border.

## Accessibility

### Keyboard

Every control is reachable with Tab and Shift+Tab. Enter activates the focused control. Escape closes a preview, disclosure, or secondary dialog before it affects the wizard.

### Focus

Use a clear high-contrast ring with at least two pixels of separation. Glow alone is insufficient.

### Screen readers

Provide semantic headings, live status regions, progress value and stage, expanded and collapsed state, diff additions and removals, and accessible names for icon-only controls.

### Contrast and color

Meet WCAG 2.2 AA. Pass, review, block, selected, modified, and unavailable states use text, icons, and distinct shapes in addition to color.

### Scaling

Test 100, 125, 150, and 200 percent on Windows and macOS. Main content scrolls when needed while Back and the primary action remain reachable.

### Motion

Respect reduced motion. Replace animated transitions with immediate state changes.

## References

All 14 implementation references are under `ui-references/`. They are 1536 by 1024 pixels and use the minimal grouped-phase design described above.

## Provider planning progress

After the user continues from the new-mod description, semantic analysis uses
one visible progress region with plain, user-facing states: Preparing your
description, Generating project details, and Checking the generated details.
The bar always shows an approximate percentage and estimated time remaining;
both advance from elapsed time within the current real request stage because
providers do not expose an exact total. A usage-limit state preserves the draft
and provides one retry action. Screen readers receive changes through a polite
live region. The progress region stays centered at the same width as the
Description form at every supported scale.

## Semantic review density

The Codex screen shows one ChatGPT action, one compact status, and a
device-code fallback link. Known hosted profiles show only their account-key
link, key field, and Connect action by default; model and address are under one
Advanced disclosure. Local and custom profiles show only the configuration
their adapter genuinely needs. Do not show provider
prices, raw protocol events, or token locations. Semantic review uses one
compact table for Detected, Suggested by the selected provider, and Confirmed
values. Reasons and input evidence open on demand.

The portrait link has a clear external destination in its accessible name and
is opened by the typed browser action. It is never labeled as setup, installed,
healthy, or ready.

## Application update action

An available application update appears as one compact title-bar action. No
banner appears when the app is current or the background check is offline. The
action names the version, is keyboard reachable, disables during install, and
never blocks the wizard.
