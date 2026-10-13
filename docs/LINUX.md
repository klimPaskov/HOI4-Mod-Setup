# Install on Linux

Download the x86-64 [AppImage](https://github.com/klimPaskov/HOI4-Mod-Setup/releases/latest/download/HOI4-Mod-Setup-linux-x64.AppImage) or [Debian package](https://github.com/klimPaskov/HOI4-Mod-Setup/releases/latest/download/HOI4-Mod-Setup-linux-x64.deb). Ubuntu 22.04 is the release build and CI baseline; Ubuntu 24.04 is also tested locally. A graphical desktop, WebKitGTK 4.1, GTK 3, and a working desktop Secret Service are required. Other distributions and Linux Arm builds have not been verified.

For the AppImage, mark the downloaded file executable and open it:

```sh
chmod +x ~/Downloads/HOI4-Mod-Setup-linux-x64.AppImage
~/Downloads/HOI4-Mod-Setup-linux-x64.AppImage
```

If FUSE 2 is missing, install your distribution's compatibility package (`libfuse2` on Ubuntu 22.04 or `libfuse2t64` on Ubuntu 24.04). AppImage also supports extraction without FUSE:

```sh
~/Downloads/HOI4-Mod-Setup-linux-x64.AppImage --appimage-extract-and-run
```

For the Debian package, use your package manager so runtime dependencies are installed:

```sh
sudo apt install ~/Downloads/HOI4-Mod-Setup-linux-x64.deb
```

Open **HOI4 Mod Setup** from the desktop application menu, then follow the [quick start](QUICKSTART.md). Run setup as your ordinary user. The app updater verifies signed updates; Debian updates may require desktop administrator authorization, while a writable AppImage can be replaced as the current user.

## Steam and mod locations

The native Paradox mod directory is `$XDG_DATA_HOME/Paradox Interactive/Hearts of Iron IV/mod`, with `~/.local/share` as the default data directory. Launch HOI4 once to create its user directory before preparing a mod.

Setup checks bounded native Steam roots, including `~/.steam/steam`, `~/.local/share/Steam` and `$XDG_DATA_HOME/Steam`, reads Steam library declarations, and finds HOI4 under `steamapps/common/Hearts of Iron IV`. It also checks Flatpak Steam at `~/.var/app/com.valvesoftware.Steam/.local/share/Steam` and the sandbox data directory. Flatpak Paradox user files may be under `~/.var/app/com.valvesoftware.Steam/data/Paradox Interactive/Hearts of Iron IV` or the persisted `.local/share` directory. When multiple valid locations exist, choose the one used by your game. You can always supply the game, Workshop and mod paths manually.

Select the native Linux game for Clausewitz MCP. A Proton Windows executable does not match its Linux memory layout. [Clausewitz setup](LIVE_TESTING_LINUX.md) uses the native game and server directly; no WSL, VM or SSH environment is needed on Linux.

## Coding clients, tools and keys

Install the Linux versions of the coding clients you select. Project registrations stay in their native files: `.mcp.json` for Claude Code, `.codex/config.toml` for Codex, `.cursor/mcp.json` for Cursor, `.qoder/mcp.json` for Qoder, and `opencode.json` for OpenCode. Instructions, skills and agent projections use the same project-relative client directories as on other platforms. For Claude Code and Codex, **Open project terminal** opens a terminal in the prepared mod; run `claude` or `codex` there yourself. Other clients show the project path for manual opening.

HOI4 Agent Tools uses native Node.js 22 or newer and npm. Setup installs the exact reviewed package into `$XDG_DATA_HOME/hoi4-mod-setup/npm` (default `~/.local/share/hoi4-mod-setup/npm`), verifies the Linux package tree, and registers its native launcher. Use root-managed distribution packages for Node and Python; executable discovery does not make a tool trusted. GPTImage also needs native Node and Python. Windows executables imported through a WSL PATH are not native Linux tools.

API image-provider adapters also require `libsecret-tools`; the Debian package installs it automatically. API keys use Linux Secret Service, normally GNOME Keyring or a compatible desktop service. Unlock your login keyring before storing a key. A missing or locked service is reported; setup never falls back to plaintext key files. Claude Code and Codex keep their own account sign-in state outside the mod. A headless shell without a session D-Bus and Secret Service cannot store API keys.

Settings use `$XDG_CONFIG_HOME/hoi4-mod-setup`, application data uses `$XDG_DATA_HOME/hoi4-mod-setup`, and verified downloads use `$XDG_CACHE_HOME/hoi4-mod-setup`, with standard XDG defaults. ChatGPT source exports use the configured XDG Downloads directory. Project transaction journals and installation locks remain under `.hoi4-mod-setup` in the selected mod.

The verified Meshy/Blender 3D bootstrap and experimental HOI4 Live Tools installer remain Windows-only. Their unavailable state does not block the Linux core workflow.
