# HOI4 Mod Setup

[![Discord](https://img.shields.io/badge/Discord-Join%20Community-7289da?logo=discord&logoColor=white)](https://discord.gg/rAXesGcT2t)
[![YouTube](https://img.shields.io/badge/YouTube-Chaos%20Redux-ff0000?logo=youtube&logoColor=white)](https://www.youtube.com/@ChaosRedux)
[![Ko-fi](https://img.shields.io/badge/Ko--fi-Support%20the%20Project-ff5e5b?logo=kofi&logoColor=white)](https://ko-fi.com/klimpaskov)
[![Download](https://img.shields.io/github/v/release/klimPaskov/HOI4-Mod-Setup?label=Download&logo=github&color=2ea44f)](https://github.com/klimPaskov/HOI4-Mod-Setup/releases/latest)
[![Guide](https://img.shields.io/badge/Guide-GitHub%20Pages-555555?logo=readthedocs&logoColor=white)](https://klimpaskov.github.io/HOI4-Mod-Setup/)
[![Issues](https://img.shields.io/badge/Bugs%20%26%20Ideas-Issues-24292f?logo=github&logoColor=white)](https://github.com/klimPaskov/HOI4-Mod-Setup/issues)

<!-- TODO-VIDEO-LINK: replace with the HOI4 Mod Setup video URL -->
[![Watch the HOI4 Mod Setup video](docs/images/setup-app-demo-thumbnail.png)](https://www.youtube.com/@ChaosRedux)

Prepare a new or existing Hearts of Iron IV mod for AI-assisted development.
HOI4 Mod Setup creates the mod files, installs project instructions, skills,
specialist subagents, tools, and an offline wiki, and opens your coding client.
You can update the setup, repair it, or add workflows later.

Choose any supported coding client. Codex, Claude Code, Cursor, Qoder, and
OpenCode can use the same modding guidance through their own client packages.
The workflow comes from [Agentic HOI4 Modding](https://github.com/klimPaskov/Agentic-HOI4-Modding).
It also installs [HOI4 Agent Tools](https://github.com/klimPaskov/hoi4-agent-tools), which lets your agent build focus trees, events, decisions and maps, and [HOI4 Live Tools](https://github.com/klimPaskov/HOI4-Live-Tools) for testing in the running game.

## Download

- Windows: [Download the `.exe` installer](https://github.com/klimPaskov/HOI4-Mod-Setup/releases/latest/download/HOI4-Mod-Setup-windows-x64-setup.exe)
- Mac with Apple silicon: [Download the `.dmg`](https://github.com/klimPaskov/HOI4-Mod-Setup/releases/latest/download/HOI4-Mod-Setup-macos-arm64.dmg)
- Mac with Intel: [Download the `.dmg`](https://github.com/klimPaskov/HOI4-Mod-Setup/releases/latest/download/HOI4-Mod-Setup-macos-x64.dmg)

On Windows, run the downloaded setup file. On Mac, open the disk image and
move HOI4 Mod Setup to Applications. Community Windows builds are signed by
ChaosX; community Mac builds use ad-hoc signing. See the
[release notes](https://github.com/klimPaskov/HOI4-Mod-Setup/releases/latest)
for your download's signing details.

Follow the [Windows first-run guide](docs/QUICKSTART.md),
[watch the setup tutorial](https://github.com/klimPaskov/HOI4-Mod-Setup/releases/latest/download/setup_app_tutorial.mp4),
or visit the [project guide](https://klimpaskov.github.io/HOI4-Mod-Setup/).
[See HOI4 Agent Tools in action](https://github.com/klimPaskov/Agentic-HOI4-Modding/releases/latest/download/hoi4_agent_tools_promo.mp4).

## Using the app

### Choose a project and setup assistant

![Create a new mod or import an existing one](docs/screenshots/01-welcome.jpg)

Choose **Create new mod** or **Import existing mod**. The setup assistant helps
prepare the mod's details; choose your coding environment separately later.

The setup assistant that plans the mod is separate from the coding client you
choose later. Select any supported assistant provider and use its account
sign-in, API key, or local connection. Choose a model and reasoning level when
the provider offers them.

To work on a mod you already have, choose **Manage an existing project** and
select its folder. The app finds its launcher file and installed setup.

![Choose a previously prepared project](docs/screenshots/11-existing-project.png)

### Describe the mod and review its details

For a new mod, enter its name and a short description.

![Enter the mod name and description](docs/screenshots/02-description.png)

The app suggests a project ID, script prefix, namespace, tags, and starter
folders. It fills the standard HOI4 mod location and launcher file. Edit any
value before continuing. Setup creates both `descriptor.mod` files and a replaceable
600x600 black `thumbnail.png`.

![Review the generated project identity and locations](docs/screenshots/03-identity.png)

For an existing mod, the app checks its descriptors, instructions, skills,
subagents, client configuration, and Git state, then shows what needs attention.

### Choose your coding clients and components

Select any supported coding client as your main one, then add other clients you use.
Codex, Claude Code, Cursor, Qoder, and OpenCode are supported.

![Choose one primary coding environment and any additional clients](docs/screenshots/15-coding-environments.png)

### Choose how to make images

Pick a main image provider and add optional providers. ChatGPT through GPTImage
is selected by default. The list also includes image APIs, local servers, Codex
image generation when Codex is the main client, and a choice to make art yourself.
The app stores API keys in the operating-system credential store.

![Choose an image-generation provider](docs/screenshots/16-image-generation.png)

Choose instructions, skills, tools, and the offline wiki. The app adapts the
guidance to your mod and fills the game and Workshop folders it finds through
Steam. Required components stay selected.

![Choose instructions, skills, tools, wiki, and ChatGPT files](docs/screenshots/04-components.png)

**Latest** uses the current published Agentic HOI4 Modding package. You can also
choose a specific commit or release. Updates bring in published components and
keep your optional workflow choices.

**Prepare a flattened ChatGPT project-sources folder** creates a separate folder
with project guidance, README, skills, and subagents for use in ChatGPT. It does
not include the offline wiki. Review its files and sizes before installation.

### Add optional workflows

![Choose the optional workflows](docs/screenshots/05-integrations.png)

- **3D models workflow** prepares Meshy and Blender on Windows. Add a Meshy key
  to use it; the key stays in Windows Credential Manager. Missing tools or
  credentials leave this workflow incomplete while the rest of setup remains
  usable.
- **Super Events workflow** adds the popup, templates, example, assets, and
  guidance for building Super Events with your mod's script prefix.
- **ComfyUI portrait production** supports Comfy Cloud, Local ComfyUI, or RunPod.
  Its expanded panel shows the 16 GB VRAM and 25 GB storage recommendation.
  See the [portrait workflow guide](docs/32_comfyui_portrait_pipeline.md).
- **HOI4 Live Tools** adds live game testing on Windows. Its setup needs Python
  3.9 or newer and installs a separate program under
  `%LOCALAPPDATA%\hoi4-live-tools`. Removing project components leaves this
  program and its client entries installed; use `python install.py uninstall`
  from the HOI4 Live Tools release to remove them.

HOI4 Agent Tools installs and registers its MCP server for your selected coding
clients on Windows, including `.mcp.json` for Claude Code. It works independently
of the assistant you used for setup.

![Review selected integrations and credential state](docs/screenshots/06-mcp-credentials.png)

### Choose Git setup

![Keep the mod local, connect a remote, or publish it](docs/screenshots/07-git.png)

Initialize Git, keep an existing repository, or skip Git. Keep the mod local,
connect a remote, or create a public GitHub repository. Publishing and pushing
ask for a separate confirmation.

### Review and install

![Review the files and folders before installation](docs/screenshots/09-dry-run.png)

Review the files and folders to add or update, then start installation. For an
edited file, choose to keep your version, use the incoming version, merge, keep
both, or skip when available. Installation shows file counts, progress, and an
estimated time.

Changes are written to your mod folder; use Git to review or undo them.

If setup stops, reopen the project to continue, undo changes, or discard
prepared files. The app offers the actions available for that installation.

![Recover an interrupted installation](docs/screenshots/14-recovery.png)

### Open the prepared project

![Completed setup and project checks](docs/screenshots/08-ready.png)

The Ready screen shows the project and tool checks. **Open in your primary
client** starts that client in the mod folder when it is available. Optional
workflows show their own setup status.

For ChatGPT, use the prepared project-source files to start planning. The app
does not upload them. Installed projects also include
`.tools/package_chatgpt_project_sources.py` and a Windows `.bat` launcher.
Run `python .tools/package_chatgpt_project_sources.py` to export a folder and
`all-project-sources.zip` to Downloads. Use `--dry-run`, `--include`, `--exclude`,
or `--config` to adjust the selection, and a new `--output` folder for each
export. See the [source-packaging guide](docs/33_chatgpt_source_packaging.md).

### Update, repair, or add workflows later

![Update and repair a prepared project](docs/screenshots/12-maintenance.png)

Open **Manage an existing project** to update or repair setup files, change
coding clients or the portrait provider, add workflows, remove components, or
recover an interrupted installation.

**Remove components** removes unchanged files installed by the app and the
folders left empty. Edited setup files are shown for review; your mod content
stays in place.

When the scan finds project instructions, flattened skills, or subagents,
**Package ChatGPT project sources** creates a ZIP in an external folder,
defaulting to Downloads. Detected instructions, README, skills, and subagents
stay selected; other root Markdown files are optional.

![Package ChatGPT project sources](docs/screenshots/13-chat-sources.png)

The app checks for updates when it opens and installs a newer signed version
automatically. If an update fails, you can keep using the app and retry.

## Troubleshooting

See [Troubleshooting](docs/TROUBLESHOOTING.md) for sign-in, setup source, MCP,
and interrupted installation fixes.

## Privacy

HOI4 Mod Setup has no telemetry. Provider keys stay in Windows Credential
Manager or macOS Keychain. Claude Code and Codex handle their own account
sign-in. Secret values are not written into the mod.

## License

HOI4 Mod Setup is released under the [Apache License 2.0](LICENSE).
