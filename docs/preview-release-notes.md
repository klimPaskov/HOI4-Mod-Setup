# HOI4 Mod Setup 0.4.0 preview

Download the installer for your computer:

- Windows: `HOI4-Mod-Setup-windows-x64-setup.exe`
- Mac (Apple silicon): `HOI4-Mod-Setup-macos-arm64.dmg`
- Mac (Intel): `HOI4-Mod-Setup-macos-x64.dmg`

This candidate installs the Claude-first Agentic package, with canonical
Claude instructions, skills, and specialist agents and generated projections
for other coding environments. New installs default to Claude Code; existing
Codex projects retain their selection when updating. It includes optional
HOI4 Live Tools and the manifest-pinned HOI4 Agent Tools MCP contract.

This build includes the current ComfyUI HOI4 portrait workflow with
selected-provider instructions, adaptive source framing, native ImageGen
handling for non-sourced portraits, and current Windows and macOS packaging.
The optional workflow panel shows the minimum requirement: 16 GB VRAM and
25 GB storage. Existing managed projects can package their installed ChatGPT
sources into a safe ZIP, and newer signed app versions now install
automatically on startup. The optional Super Events workflow installs runtime
files using the confirmed mod script prefix rather than the upstream
`hoi4ms_*` source names.

Windows uses the `.exe` installer. On macOS, open the `.dmg` and move the app
to Applications.

Your computer may occasionally flag a new community build as harmful. This
can be a false positive; the app is open source and this is its official
GitHub release page.

The source is public under the Apache License 2.0:
<https://github.com/klimPaskov/HOI4-Mod-Setup>.
