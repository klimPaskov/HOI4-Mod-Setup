# HOI4 Mod Setup

HOI4 Mod Setup is a Windows and macOS desktop wizard that prepares a new or
existing Hearts of Iron IV mod for agentic development. It creates launcher
files, project structure, workflow files, and a readiness report
without silently replacing user work.

> **Current release:** [HOI4 Mod Setup 0.1.1](https://github.com/klimPaskov/HOI4-Mod-Setup/releases/tag/v0.1.1) for Windows and macOS.

## Install

The public source repository is [klimPaskov/HOI4-Mod-Setup](https://github.com/klimPaskov/HOI4-Mod-Setup).

### Downloads

- Windows: [Download the `.exe` installer](https://github.com/klimPaskov/HOI4-Mod-Setup/releases/download/v0.1.1/HOI4-Mod-Setup-windows-x64-setup.exe)
- Mac (Apple silicon): [Download the `.dmg`](https://github.com/klimPaskov/HOI4-Mod-Setup/releases/download/v0.1.1/HOI4-Mod-Setup-macos-arm64.dmg)
- Mac (Intel): [Download the `.dmg`](https://github.com/klimPaskov/HOI4-Mod-Setup/releases/download/v0.1.1/HOI4-Mod-Setup-macos-x64.dmg)

Your computer may occasionally flag a new community build as harmful. This can
be a false positive; the app is open source and these links use its official
GitHub release page.

## Use the wizard

The wizard has seven short phases. Each screen keeps the current task in focus,
and earlier choices remain editable with Back.

### 1. Project — choose what you are preparing

![HOI4 Mod Setup welcome screen](docs/screenshots/01-welcome.png)

Choose **Create new mod** for a fresh launcher-ready scaffold or **Existing
mod** for a bounded, read-only scan. Codex is the default planning provider;
the provider selection view lets you choose Claude, Kimi, GLM, DeepSeek, a
local model, or another explicitly configured route before analysis begins.

![Provider selection with model and vault-key controls](docs/screenshots/10-provider-selection.png)

For an existing mod, choose **Import existing mod** or use **Manage an existing
project** to check a setup already created by the wizard.

![Existing project entry for repair and workflow updates](docs/screenshots/11-existing-project.png)

The selected profile shapes the generated project guidance and conventions.

### 2. Review — describe the mod and confirm its identity

For a new mod, enter only a name and a short natural-language brief. The app
fills the project ID, script prefix, primary namespace, descriptor tags, starter
folders, project path, and launcher descriptor path. Review or edit any value;
manual edits stay preserved.

New projects include `descriptor.mod`, the matching launcher `.mod` file, and
an editable `thumbnail.png`.

![Mod name and description with generated-value explanation](docs/screenshots/02-description.png)

![Editable generated identity and descriptor preview](docs/screenshots/03-identity.png)

For an existing mod, the same phase shows scan findings with evidence,
confidence, proposed action, and an editable decision. The scan does not create
temporary files or alter the project. If the project already has a valid HOI4
Mod Setup lock, the review shows **Repair or add workflows**. Use it later to
restore managed files or add the 3D or Super Events workflow without starting
over.

### 3. Components — choose workflow material

![Verified component selection](docs/screenshots/04-components.png)

Select the components you want. The app downloads only what those components
need and leaves everything else alone.

### 4. Integrations — choose optional workflows

![Optional 3D and Super Events workflows](docs/screenshots/05-integrations.png)

The 3D question is exactly **Do you want to set up the 3D models workflow?**.
Its Meshy key stays in the OS credential vault and is injected only when the
allowlisted workflow needs `MESHY_API_KEY`. A missing key leaves 3D incomplete
without blocking core setup.

Immediately after it, **Do you want to set up the Super Events workflow?**
adds a ready-to-use popup, reusable registration workflow, GUI and GFX files,
templates, example event, visual examples, and Photoshop templates. The package
is adapted to the mod's namespace and remains optional.

![MCP and credential review](docs/screenshots/06-mcp-credentials.png)

MCP and provider credentials are reviewed here. Secrets are never placed in
the project, installation lock, logs, or screenshots.

### 5. Git — keep the project local or publish it

![Git choices](docs/screenshots/07-git.png)

Choose whether to initialize, preserve, or skip Git; review `.gitignore`,
branch, initial commit, and remote choices. You can keep the mod local, push to
an existing remote, or create a public GitHub repository. Online actions ask
for separate approval.

### 6. Install — review and set up

![Install review with planned changes and optional ChatGPT sources](docs/screenshots/09-dry-run.png)

When Codex is selected, this screen offers one optional checkbox to prepare
`chatgpt_project_sources/` for ChatGPT **Chat**. It flattens skills to
`<skill>.md` and includes the adapted `AGENTS.md`, created `README.md`, and
selected subagents while leaving the normal source tree intact.

Read the planned files, conflicts, Git actions, optional workflows, and any
external actions, then start installation. Existing edits are kept unless you
approve a valid conflict choice.

### 7. Ready — open the project

The final report shows whether the project is ready for the selected provider.
**Open in Codex** is enabled when the required setup checks pass. If you selected
flattened Chat sources, the final message recommends starting planning with
ChatGPT **Chat**.

For AI portraits, the completed screen links to
[ComfyUI HOI4 Portraits](https://github.com/klimPaskov/comfyui-hoi4-portraits).

## Privacy

The app has no telemetry in version 1. Provider keys stay in Windows Credential
Manager or macOS Keychain, and ChatGPT sign-in remains managed by Codex.
User-modified files are never silently replaced.

Contributor setup, security reporting, and release maintenance are in
[CONTRIBUTING.md](CONTRIBUTING.md), [SECURITY.md](SECURITY.md),
[DEVELOPMENT.md](DEVELOPMENT.md), and [RELEASING.md](RELEASING.md).

## License

HOI4 Mod Setup is released under the [Apache License 2.0](LICENSE).
