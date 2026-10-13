# UI fixes handoff

Date: 2026-10-13

## Scope

Narrow React wizard and accessibility fixes only. Rust domain logic, schemas,
manifest files, and provider adapters were not changed by this handoff.

## Files changed

- `src/App.tsx`
- `src/App.test.tsx`
- `src/claude-models.json` is a shared catalog created by the parent agent and
  imported by the UI; this subtask did not author or modify its model entries.

No CSS changes were needed. No browser test source changes were needed.

This resumed pass additionally changes the conflict comparison fallback in
`src/App.tsx` and adds one focused `Conflict` component regression in
`src/App.test.tsx`.

## Before and after

### Existing-project review

Before: `Review with <provider>` stayed visually idle while the semantic review
request was awaiting the provider, allowing uncertainty and duplicate clicks.

After: the findings action immediately becomes `Reviewing with <provider>…`,
is disabled, exposes `aria-busy="true"`, and restores the review action after
the request resolves or fails. The scan and draft remain intact on failure.

### Image provider components

Before: choosing a provider updated `imageGeneration` only; manifest rows such
as `Image provider MCP adapter` or `GPTImage MCP source` could remain visibly
unchecked and the selected component state could diverge from the provider.

After: provider route component IDs are synchronized into `selectedComponents`
when the provider changes and when the catalog is loaded. Stale `imagegen.*`
IDs are removed when switching routes or choosing the manual route.

### HOI4 Live Tools

Before: `workflow.live_tools` was filtered out of the optional workflow rows,
and selecting its provider removed the component while selecting only generic
`workflow.live_testing`.

After: the manifest-declared Live Tools workflow is rendered as an optional
workflow when compatible. Selecting it also selects the live-testing route and
sets the provider to `hoi4_live_tools`; disabling or changing providers removes
only the route no longer selected. Existing installed Live Tools state also
continues through the live-testing screen.

### Claude setup assistant

Before: the fallback used Claude Haiku 4.5/high and offered no manual Claude
Code model entry.

After: the shared catalog supplies Claude Haiku 5.5/medium and the built-in
Claude choices are explicitly labeled as built-in rather than live. Advanced
contains a bounded Claude model-ID input; manually entered IDs clear stale
catalog state and do not block the authenticated Claude route.

### Conflict comparison fallback

Before: binary or non-text conflict sides with a verified hash rendered the
same “No original/current/new content” copy as genuinely missing content. A
real descriptor could therefore look absent in the comparison panes.

After: hash-backed binary/non-text sides say that their content preview is
unavailable and show the SHA-256. A side with no content and no hash keeps its
missing-content message. Text previews continue to render unchanged.

### Conflict navigation and completion status

Before: every conflict screen used the MANAGE rail and `Apply` footer label,
even for a fresh import where the next action only returns to Review changes.
An all-resolved conflict list also lacked a concise completion status.

After: fresh-import conflicts remain on the setup rail and use `Review
changes`; maintenance conflicts retain MANAGE and `Apply`. Once every choice
is selected, the comparison keeps earlier choices available while announcing
that all conflicts are resolved. Choice highlighting follows the active
conflict's persisted selection, not the last transient choice from another
file.

## Evidence

Fresh sanitized captures from the local development server:

- [image provider screen](../../artifacts/image-generation-ui-fixes.png)
- [live testing screen](../../artifacts/linux-live-testing-final.png)

The optional-workflow screenshot fixture does not declare `workflow.live_tools`,
so the restored row is covered by the dedicated Vitest regression rather than
the public fixture capture. The live-testing capture shows the restored
provider route and its disabled-on-Linux status.

Keyboard and semantic evidence includes the native radio/checkbox paths in the
Playwright checks, the immediate `aria-busy` review state regression, and the
existing heading-focus/live-region coverage.

## Tests run

- `pnpm typecheck`
- `pnpm test` (174 passed)
- `pnpm lint`
- `pnpm test:a11y`
- `pnpm build`
- `pnpm exec playwright test tests/browser/wizard.spec.ts --grep "image-generation setup|live testing follows workflows|Windows Clausewitz|Linux Clausewitz|review captures" --reporter=line` (5 passed)
- `pnpm exec vitest run src/App.test.tsx -t "binary previews|conflict"` (2 passed)
- `pnpm exec vitest run src/App.test.tsx` (144 passed)
- `pnpm exec vitest run src/App.test.tsx -t "all-resolved conflict|imported conflicts|binary previews|conflict"` (4 passed)

## Skipped or parent-owned checks

- Rust unit/integration, transaction, provider, and source-manifest tests were
  not run; the parent owns those changes and another Rust test process was
  already active.
- Native desktop screen-reader and OS-level 200% zoom checks were not run in
  this subtask. The browser suite still checked 640px responsive overflow and
  keyboard operation; no motion behavior was changed.
- No real account, credential, provider network route, or external workflow
  process was used.

## Remaining risks

- Rust plan construction remains the final authority for image route component
  closure and Live Tools dependency expansion; the UI mirrors verified route
  component IDs but does not replace that validation.
- Parent provider work may add Gemini/Grok or other provider IDs; the shared
  catalog and provider registry should remain the source of truth when those
  controls land.
- The public documentation fixture still omits the Live Tools optional row;
  updating that fixture or its screenshot is intentionally left to the parent
  if public screenshot evidence is required.
- The `.mod` descriptor text classification is a Rust-side fix owned by the
  parent; this UI patch covers the remaining binary/non-text fallback state.
- Importing an existing project still uses the maintenance `MANAGE` rail once
  conflict resolution opens; changing that navigation model would be a wider
  wizard refactor and was intentionally left out of this bounded fix.
