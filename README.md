# HOI4 Mod Setup

HOI4 Mod Setup is a Windows and macOS wizard for creating a new Hearts of Iron
IV mod or preparing an existing mod for AI-assisted development. It creates the
launcher files, folders, instructions, and tools you choose while keeping your
existing work safe.

## Download

[Open the latest release page](https://github.com/klimPaskov/HOI4-Mod-Setup/releases/latest)

- Windows: [Download the `.exe` installer](https://github.com/klimPaskov/HOI4-Mod-Setup/releases/download/v0.2.0/HOI4-Mod-Setup-windows-x64-setup.exe)
- Mac (Apple silicon): [Download the `.dmg`](https://github.com/klimPaskov/HOI4-Mod-Setup/releases/download/v0.2.0/HOI4-Mod-Setup-macos-arm64.dmg)
- Mac (Intel): [Download the `.dmg`](https://github.com/klimPaskov/HOI4-Mod-Setup/releases/download/v0.2.0/HOI4-Mod-Setup-macos-x64.dmg)

Your computer may occasionally show a warning for a new community build. This
can be a false positive. HOI4 Mod Setup is safe and open source.

## Use the wizard

The wizard has seven short phases. You can go back and change earlier choices
at any time before setup begins.

### 1. Project — choose what you are preparing

![HOI4 Mod Setup welcome screen](docs/screenshots/01-welcome.png)

Choose **Create new mod** to start a fresh mod or **Existing mod** to prepare a
mod you already have. Codex is selected by default, and you can instead choose
Claude, Kimi, GLM, DeepSeek, a local model, or another supported provider.

![Provider selection with model and sign-in controls](docs/screenshots/10-provider-selection.png)

For a mod that has already been prepared, choose **Manage an existing project**
to repair missing files or add a workflow later.

![Existing project entry for repair and workflow updates](docs/screenshots/11-existing-project.png)

### 2. Review — describe the mod and check its details

For a new mod, enter its name and a short description. The app fills in the mod
ID, script prefix, namespace, tags, starter folders, project location, and
launcher file. It also creates the mod's `descriptor.mod` and a replaceable
`thumbnail.png`. You can change any of the suggested details before continuing.

![Mod name and description](docs/screenshots/02-description.png)

![Editable generated mod details](docs/screenshots/03-identity.png)

For an existing mod, the app checks its current structure and shows anything
that needs attention before making changes.

### 3. Components — choose what to install

![Component selection](docs/screenshots/04-components.png)

Choose the instructions, skills, tools, and offline wiki you want in the mod.
Required items stay selected so the setup remains usable.

When Codex is selected, you can also choose **Prepare a flattened ChatGPT
project-sources folder**. This creates one easy-to-upload folder containing the
project guidance, README, selected skills, and selected subagents. Open its file
list to see the filenames and sizes.

### 4. Integrations — choose optional workflows

![Optional 3D and Super Events workflows](docs/screenshots/05-integrations.png)

Select **Do you want to set up the 3D models workflow?** to add the available 3D
tools. Your Meshy key stays in your computer's secure credential storage and
can also be added later.

Select **Do you want to set up the Super Events workflow?** to add a ready-made
popup, templates, an example event, and reusable files for adding more events.

![Connections and credentials](docs/screenshots/06-mcp-credentials.png)

Review the optional tools and keys used by your selected workflows.

### 5. Git — keep the project local or publish it

![Git choices](docs/screenshots/07-git.png)

You can keep the mod local, connect it to an existing Git repository, or create
a public GitHub repository. Nothing is published until you approve it.

### 6. Install — review and set up

![Install review](docs/screenshots/09-dry-run.png)

Review what will be added or changed, resolve any file conflicts, and start the
setup. Your existing files remain unchanged unless you approve a replacement
or merge.

### 7. Ready — open the project

The final screen shows whether the mod is ready for your selected AI provider.
Codex projects can be opened directly with **Open in Codex**.

HOI4 Mod Setup checks for new versions when it opens. If an update is
available, choose **Update now** to install it.

If you selected the ChatGPT project folder, start planning with ChatGPT
**Chat** after setup.

For AI portraits, the final screen also links to
[ComfyUI HOI4 Portraits](https://github.com/klimPaskov/comfyui-hoi4-portraits).

## Privacy

HOI4 Mod Setup has no telemetry. Provider keys stay in Windows Credential
Manager or macOS Keychain, and ChatGPT sign-in is handled by Codex.

## License

HOI4 Mod Setup is released under the [Apache License 2.0](LICENSE).
