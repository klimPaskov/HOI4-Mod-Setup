# HOI4 Mod Setup

HOI4 Mod Setup is a desktop wizard for preparing a Hearts of Iron IV mod for
AI-assisted development. Start a new mod from its name and description, or
choose an existing mod to add, update, or repair its development setup.

The app supports Codex, Claude, Kimi, GLM, DeepSeek, local models, and custom
providers. Codex is selected by default.

## Download

- Windows: [Download the `.exe` installer](https://github.com/klimPaskov/HOI4-Mod-Setup/releases/latest/download/HOI4-Mod-Setup-windows-x64-setup.exe)
- Mac with Apple silicon: [Download the `.dmg`](https://github.com/klimPaskov/HOI4-Mod-Setup/releases/latest/download/HOI4-Mod-Setup-macos-arm64.dmg)
- Mac with Intel: [Download the `.dmg`](https://github.com/klimPaskov/HOI4-Mod-Setup/releases/latest/download/HOI4-Mod-Setup-macos-x64.dmg)

[View the latest release](https://github.com/klimPaskov/HOI4-Mod-Setup/releases/latest)

On Windows, run the downloaded setup file. On Mac, open the disk image and
move HOI4 Mod Setup to Applications. Your computer may occasionally flag a new
community build as harmful falsely. The project is safe and open source.

## Using the app

The wizard has seven short phases. Nothing is added to the mod until you have
reviewed the changes and started installation.

### 1. Choose a project and AI provider

![Create a new mod or import an existing one](docs/screenshots/01-welcome.png)

Choose **Create new mod** or **Import existing mod**. For Codex, sign in with
ChatGPT. For Claude, Kimi, GLM, or DeepSeek, paste an API key from the provider;
the app fills the normal connection details automatically. Local and custom
models let you enter the address supplied by that model service.

![Choose and connect another AI provider](docs/screenshots/10-provider-selection.png)

If the app has prepared the project before, **Manage an existing project** lets
you repair it, update it, add an optional workflow, or remove installed setup
files. Choose the project folder; the app finds its launcher file.

![Choose a previously prepared project](docs/screenshots/11-existing-project.png)

### 2. Describe the mod and review its details

For a new mod, enter its name and a short description.

![Enter the mod name and description](docs/screenshots/02-description.png)

The app fills the project ID, script prefix, namespace, valid descriptor tags,
starter folders, standard HOI4 mod location, and launcher file. Every generated
value remains editable. It also prepares `descriptor.mod`, the launcher
descriptor, and a replaceable 600x600 black `thumbnail.png`.

![Review the generated project identity and locations](docs/screenshots/03-identity.png)

For an existing mod, a read-only scan checks the descriptors, structure,
naming, localisation, current tools, and possible file conflicts before any
changes are offered.

### 3. Choose what to install

![Choose instructions, skills, tools, wiki, and ChatGPT files](docs/screenshots/04-components.png)

Select the instructions, skills, helpers, tools, and offline wiki you want.
Required items stay selected so the project remains usable.

When Codex is selected, **Prepare a flattened ChatGPT project-sources folder**
creates a separate folder with the adapted project guidance, README, selected
skills, and selected subagents. The offline wiki is not copied into this
folder. Its file list and sizes are shown before installation.

### 4. Add optional workflows

![Choose the optional workflows](docs/screenshots/05-integrations.png)

The 3D workflow is optional. If selected, a Meshy key can be stored in Windows
Credential Manager or macOS Keychain and is never written into the mod.

The optional Super Events workflow adds the reusable popup, templates,
examples, images, and supporting guidance needed to add more Super Events.

The optional ComfyUI portrait production workflow supports Comfy Cloud, Local
ComfyUI, RunPod, or Disabled for generic projects. Its expanded panel shows the
minimum requirement: **16 GB VRAM and 25 GB storage**. Only the selected
provider's instructions are installed. Sourced portraits use the pinned source
or processing workflow; non-sourced fictional or impossible portraits use
native ImageGen and never enter ComfyUI. See
[`docs/32_comfyui_portrait_pipeline.md`](docs/32_comfyui_portrait_pipeline.md)
for the current workflow and setup details.

![Review selected integrations and credential state](docs/screenshots/06-mcp-credentials.png)

### 5. Choose Git setup

![Keep the mod local, connect a remote, or publish it](docs/screenshots/07-git.png)

Initialize Git, preserve an existing repository, or skip Git. You can keep the
project local, connect an existing remote, or create a public GitHub repository.
Publishing and pushing always require a separate confirmation.

### 6. Review and install

![Review the files and folders before installation](docs/screenshots/09-dry-run.png)

Review the files and folders that will be added or updated. If an existing file
was edited, the app asks whether to keep it, use the new version, merge it, keep
both, or skip the change when those choices are valid.

Installation shows live progress, completed file counts, percentages, and an
estimated time. If setup is interrupted, reopening the project offers the safe
recovery choices that still apply.

### 7. Open the prepared project

![Completed setup and readiness checks](docs/screenshots/08-ready.png)

The Ready screen checks the project, instructions, selected tools, offline
wiki, Git state, and optional workflows. **Open in Codex** becomes available
when the required checks pass.

If flattened ChatGPT files were selected, the Ready screen links directly to
[ChatGPT Chat](https://chatgpt.com). When the optional portrait workflow is
enabled, it also shows its persisted Cloud, Local, or RunPod readiness and
canonical source guidance, or the explicit Disabled state. Chaos Redux uses
RunPod API-first; computer control is used only when explicitly requested.
Non-sourced fictional or impossible portraits use native ImageGen. Disabled
generic projects keep source-based portrait handling without ComfyUI-specific
project files. See
[`docs/32_comfyui_portrait_pipeline.md`](docs/32_comfyui_portrait_pipeline.md).

### Update, repair, or add a workflow later

![Update and repair a prepared project](docs/screenshots/12-maintenance.png)

Open **Manage an existing project** to check for setup updates, repair missing
or damaged files, change the portrait provider, add the 3D or Super Events
workflow later, remove installed components, or recover an interrupted setup.
Changes are shown for review before they are applied.

HOI4 Mod Setup also checks for app updates when it opens. When a new version is
available, choose **Update now**.

## Privacy

HOI4 Mod Setup has no telemetry. Provider keys stay in Windows Credential
Manager or macOS Keychain. ChatGPT sign-in is handled by Codex, and secret
values are not written into the mod.

## License

HOI4 Mod Setup is released under the [Apache License 2.0](LICENSE).
