# Wizard screenshots

These full-window captures are used beside the matching steps in the
user-facing [README](../../README.md). They show the current Project, Review,
Components, Integrations, Git, Install, Ready, maintenance, and ChatGPT source
package screens. The automated captures use a 1280 by 960 viewport.

No credential, account identity, private mod, or secret value appears in a
public capture. The optional portrait workflow is provider-selectable for generic
projects. RunPod runs API-first; computer control is opt-in and is not shown as
a default setup route. Non-sourced portraits use the configured image generator.
The portrait workflow can be selected during setup and is linked from Ready
after a successful setup.

The captures are implementation evidence, not the design references in
[`ui-references/`](../ui-references/). They use the sanitized Atlantis Rising
documentation fixture in `src/documentation-fixtures.ts`; the fixture is
available only in a development build and cannot activate in a packaged app.
The Integrations and Ready captures show the generic RunPod route with setup
still required; Cloud and Local remain available as the other providers. The
Integrations capture also records the selected 3D
row's automatic verified Meshy/Blender preparation copy and the stable
3D/Super Events/portrait ordering.

Run `pnpm docs:screenshots` to regenerate all 17 captures in an isolated,
headless Playwright browser, with runtime-error and layout checks. It never
opens or focuses the shared desktop. GitHub Pages copies these same images.

For manual inspection with `pnpm dev` running, open `http://localhost:1420/?screenshot=<name>` where
`<name>` is one of `welcome`, `provider`, `existing`, `description`, `identity`,
`components`, `workflows`, `mcp`, `git`, `dry-run`, `ready`, `maintenance`,
`recovery`, `environments`, `image-generation`, or `live-testing`.
Capture the complete 1280 by 960 application viewport from the top of the page.

Before saving, wait for the expected screen heading and controls to be visible;
navigation completion alone is not proof that React has rendered the fixture.
Check the page for runtime errors and visually inspect the actual capture for
blank areas or missing content. Keep the capture's real file format and matching
extension (PNG or JPEG), then verify its README link before committing.

The `live-testing` fixture produces `17-live-testing.png`; the current capture includes the provider choices and Linux setup to-do. For requested review captures, use 1920 by 1080 and append `&coding-environments=none` to the screenshot URL to clear all coding-environment selections before capture. Never use a real account or credential.
