# Implementation screenshot gallery

These screenshots are the current implementation gallery linked from the
user-facing [README](../../README.md). They were captured from the local React
browser preview at a large desktop viewport while exercising the wizard's
Project, Components, Integrations, Git, Install, provider-selection, and
existing-project views. The description and identity captures also show
generated, editable project fields based on a mod name and brief. Headings use
plain text; keyboard focus remains visible on the active control.

The preview does not have a live source manifest, native process adapter, or
credentials. The unavailable states shown in the gallery are therefore honest
browser-preview states, not evidence that a packaged installer or a clean
machine is ready. Native package verification and platform evidence are
published with releases when available.

Capture details:

- local preview served with `pnpm exec vite --host 127.0.0.1 --port 4173`
- viewport override: 1600 px wide; height increased where the full screen needed
  it, with the browser scrollbar retained when scrolling was part of the view
- no credentials, identity documents, private mod projects, or secret values
- screenshots are implementation captures; the source planning package keeps
  the separate design references outside this generated repository template.
- `11-existing-project.png` shows the local repair entry before a folder is
  selected
- image filenames and alt text are stable so README links remain reviewable
