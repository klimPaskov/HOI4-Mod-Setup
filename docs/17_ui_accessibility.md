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

## Authentication gate

Before project selection, show a compact ChatGPT account gate with the product name, one sentence explaining that setup uses the user's Codex access, a **Sign in with ChatGPT** action, a device-code fallback behind a secondary action, current account state, and a local recovery link when an interrupted transaction exists.

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

Most setup screens should use a single column between 760 and 900 pixels. A two-column layout is reserved for direct comparisons or a selected-item detail view.

### Footer

Keep Back and the primary action visible. Show one short state note only when it prevents uncertainty, such as `Nothing is changed until the dry run.`

Do not display permanent keyboard shortcut hints in the footer. Keyboard support remains available without occupying interface space.

## Screen direction

1. **Welcome and project selection**: two choices and a compact recent-project list. Hide source policy and installation policy details.
2. **New mod description**: one large text field with optional inferred-topic chips.
3. **Project identity and descriptor setup**: compact form, generated-file rows, previews on demand, advanced fields collapsed.
4. **Existing project scan**: one progress surface, current scan stage, detected count, cancel or pause.
5. **Finding review**: compact finding list and one selected finding. Show evidence only for the selected item.
6. **Component selection**: recommended component rows, sizes, one collapsed dependency and file-list control.
7. **Optional workflows**: the two required questions as two concise rows.
8. **3D and Meshy key**: credential field, secure-storage choice, test action, compact status, requirements collapsed.
9. **LoRA and ComfyUI placeholder**: unavailable state, one sentence, one interest preference.
10. **MCP and credentials**: compact server rows and credential names. Expand capabilities and environment details on demand.
11. **Git setup**: three choices, branch and commit fields for the selected choice, remote and advanced options collapsed.
12. **Dry run**: change counts, short plan summary, preflight state, full file plan on demand.
13. **Installation progress**: one progress bar, six grouped stages, current item, transaction log collapsed.
14. **Final readiness**: one success state, Open in Codex, four grouped core checks, compact optional-workflow state.
15. **Update and repair**: four primary maintenance actions and a short installed-state list.
16. **Merge conflict review**: local and incoming comparison, resolution choices, result preview after selection.
17. **Interrupted recovery**: one checkpoint summary and three recovery choices.

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

Do not invent a percentage when total work is unknown. Do not expose the full transaction log by default.

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

All 17 implementation references are under `ui-references/`. They are 1536 by 1024 pixels and use the minimal grouped-phase design described above.

## Codex progress states

Semantic analysis uses one progress region with plain states: Preparing approved context, Analyzing project, Validating suggestions, and Ready for review. A usage-limit state preserves the draft and provides one retry action. Screen readers receive changes through a polite live region.

## Semantic review density

The sign-in screen shows one ChatGPT action, one compact status, and a device-code fallback link. Do not show OAuth fields, API-key inputs, model prices, raw protocol events, or token locations. Semantic review uses one compact table for Detected, Suggested by Codex, and Confirmed values. Reasons and input evidence open on demand.
