# HOI4 Mod Setup

[![Discord](https://img.shields.io/badge/Discord-Join%20Community-7289da?logo=discord&logoColor=white)](https://discord.gg/rAXesGcT2t)
[![YouTube](https://img.shields.io/badge/YouTube-Chaos%20Redux-ff0000?logo=youtube&logoColor=white)](https://www.youtube.com/@ChaosRedux)
[![Ko-fi](https://img.shields.io/badge/Ko--fi-Support%20the%20Project-ff5e5b?logo=kofi&logoColor=white)](https://ko-fi.com/klimpaskov)
[![Download](https://img.shields.io/github/v/release/klimPaskov/HOI4-Mod-Setup?label=Download&logo=github&color=2ea44f)](https://github.com/klimPaskov/HOI4-Mod-Setup/releases/latest)
[![Issues](https://img.shields.io/badge/Bugs%20%26%20Ideas-Issues-24292f?logo=github&logoColor=white)](https://github.com/klimPaskov/HOI4-Mod-Setup/issues)

[![Watch the HOI4 Mod Setup video: One App, All HOI4 Tools](docs/images/setup-app-demo-thumbnail.png)](https://www.youtube.com/watch?v=qdFbLO92dQI)

Prepare a new or existing **Hearts of Iron IV mod** for AI-assisted development.
It adds project instructions, skills, specialist subagents, HOI4 tools, and an
offline wiki, adapted to your mod, for Claude Code, Codex, Cursor, and
[many more coding clients](docs/36_coding_environment_catalog.md). It is for HOI4
modders who want an AI agent that knows how their mod and the game's script work.

## Recommended: the desktop app

The desktop app is the full setup. It writes into your mod after you review every
change, backs up first, and can undo it. It also installs and checks the HOI4
Agent Tools server, keeps your keys in the system credential store, supports
Claude and ChatGPT account sign-in, Git, the 3D workflow and live testing, and
can update, repair, or remove its setup later.

## Download

- Windows: [`.exe` installer](https://github.com/klimPaskov/HOI4-Mod-Setup/releases/latest/download/HOI4-Mod-Setup-windows-x64-setup.exe)
- Mac, Apple silicon: [`.dmg`](https://github.com/klimPaskov/HOI4-Mod-Setup/releases/latest/download/HOI4-Mod-Setup-macos-arm64.dmg) · Intel: [`.dmg`](https://github.com/klimPaskov/HOI4-Mod-Setup/releases/latest/download/HOI4-Mod-Setup-macos-x64.dmg)
- Linux x86-64: [AppImage](https://github.com/klimPaskov/HOI4-Mod-Setup/releases/latest/download/HOI4-Mod-Setup-linux-x64.AppImage) or [Debian package](https://github.com/klimPaskov/HOI4-Mod-Setup/releases/latest/download/HOI4-Mod-Setup-linux-x64.deb) ([Linux guide](docs/LINUX.md))

The app updates itself. Signing details for each download are in the
[release notes](https://github.com/klimPaskov/HOI4-Mod-Setup/releases/latest).

[![The desktop app: create a new mod or import an existing one](docs/screenshots/01-welcome.jpg)](docs/USER_GUIDE.md)

1. Open the app and choose **Create new mod** or **Import existing mod**.
2. Pick a setup assistant (the free model when offered, or your own account or API key).
3. Review what it found and suggests, then start the installation.

## Lightweight option: the web edition

Not ready to install anything, or only need the basics? The
[web edition](https://klimpaskov.github.io/HOI4-Mod-Setup/app/) is a faster, lighter
way to get the same project instructions, skills, and subagents. Nothing is
installed or uploaded: you start a new mod or drop your mod's folder on the page,
review the result, and download a ZIP to unpack over a **copy** of your mod (or a
Git checkout).

[![The web edition: drop a mod folder or start a new mod](docs/screenshots/18-web-edition.png)](https://klimpaskov.github.io/HOI4-Mod-Setup/app/)

It does not need the full workflow. When you want more later, download the extra
pieces manually from the [Agentic HOI4 Modding](https://github.com/klimPaskov/Agentic-HOI4-Modding)
and [HOI4 Agent Tools](https://github.com/klimPaskov/hoi4-agent-tools) repositories,
or drop the same folder into the desktop app for the fuller install.

| | Desktop (recommended) | Web (lightweight) |
| --- | --- | --- |
| Install | Windows, macOS, Linux | None |
| Writes to your mod | Yes, after review, with backup and **Undo changes** | No: you download a ZIP |
| Existing mods | Pick the folder | Drop the folder (read-only) |
| Update or repair later | **Manage an existing project** | Drop the folder again |
| Your API key | OS credential store | The tab's memory only |
| Claude / ChatGPT account sign-in | Yes | No |
| HOI4 Agent Tools install and checks, Git, 3D workflow, live testing | Yes | No |

## Using an existing mod

- **Your files stay yours.** A valid `descriptor.mod`, `thumbnail.png`, and
  `README.md` are kept as they are, and your mod content is never touched.
- **Import** only reads the folder. Installation then adds the setup on top.
- **Reimport or update** the same folder any time: the app recognises its setup
  and offers **Update** or **Repair**. Unchanged setup files are refreshed; files
  you edited are never overwritten silently and are shown for review (keep,
  use new, merge, keep both, or skip). Repeating an update with nothing new
  changes nothing.
- **Interrupted?** If the app is closed or the computer sleeps during install, the next
  time you open the folder it offers **Undo changes**, which restores the mod
  as it was. A full install takes several minutes, so keep the app open.

More in the [user guide](docs/USER_GUIDE.md#existing-mods-import-reimport-update).

## Learn more

- [Quick start with screenshots](docs/QUICKSTART.md) and the [full user guide](docs/USER_GUIDE.md)
- [Troubleshooting](docs/TROUBLESHOOTING.md): sign-in, setup source, MCP, interrupted installs
- [Contributing](CONTRIBUTING.md), [development setup](DEVELOPMENT.md), [releasing](RELEASING.md), [security](SECURITY.md)
- The workflow comes from [Agentic HOI4 Modding](https://github.com/klimPaskov/Agentic-HOI4-Modding)
  and [HOI4 Agent Tools](https://github.com/klimPaskov/hoi4-agent-tools)

## Privacy

No telemetry. Provider keys stay in Windows Credential Manager, macOS Keychain, or
Linux Secret Service (desktop) or in the open tab's memory (web); secret values
are never written into your mod. The web edition lists every network request it
makes under **Network**.

## License

Apache License 2.0. See [LICENSE](LICENSE).
