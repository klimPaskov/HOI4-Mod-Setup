# Troubleshooting

Find the message you see below, then follow its fix.

## Claude Code is not installed

> Claude Code is not installed. Install it, then choose Check again.

**Cause:** The app cannot find Claude Code.

**Fix:** Choose **Install Claude Code** to open Anthropic's setup instructions, install it, then choose **Check again** in HOI4 Mod Setup.

## Claude Code needs an update

> Claude Code needs an update. Run claude update, then choose Check again.

**Cause:** The installed Claude Code needs a newer version to work with setup.

**Fix:** Open a terminal, run `claude update`, then return to the app and choose **Check again**.

## Claude sign-in did not finish

> Claude sign-in did not finish. Try again, or sign in from a terminal with claude auth login and choose Check again.

**Cause:** Claude Code did not complete account sign-in.

**Fix:** Choose **Sign in to Claude** and finish in the browser, or run `claude auth login` in a terminal and choose **Check again** afterward.

## Claude Code is using an API-key account

> Claude Code is signed in with an Anthropic Console account or API key. Sign in with your Claude account, or choose Claude API key.

**Cause:** The default Claude route expects a Claude account sign-in, rather than an Anthropic Console account or API key.

**Fix:** Sign in to Claude Code with your Claude account, or select **Claude API key** as the setup assistant and connect with your Anthropic key.

## Claude model is unavailable

> Claude Code rejected the selected model

**Cause:** Claude Code does not support the selected exact model ID in the
installed version or account.

**Fix:** Update Claude Code when prompted, choose one of the built-in model
choices, or enter another exact Claude model ID under **Advanced**. The app's
Haiku 5.5 default and built-in catalog are local choices; Claude Code does not
provide a live catalog. A model choice does not select the coding client used
after setup.

## Gemini or Grok API key was not accepted

> The provider credential is not available

**Cause:** The selected API-key profile needs a key in the operating-system
credential store, and the key is never written to the project.

**Fix:** Open the provider's official API-key page, connect the key again, and
choose **Check**. Gemini uses the reviewed OpenAI-compatible endpoint and
Grok uses the reviewed Responses endpoint; use **Other provider** only for a
deliberately different HTTPS host.

## Experimental Gemini or Grok account sign-in is unavailable

> Complete sign-in in the provider terminal, close it, then choose Check again.

**Cause:** The experimental account route delegates sign-in, token storage,
refresh, and logout to the official provider CLI in an app-private home. It
does not accept pasted credentials. The package is downloaded only after the
explicit sign-in action and is checked against its pinned integrity values.

**Fix:** Finish the provider's own terminal sign-in, close that terminal, and
choose **Check again**. If the app says the route is unavailable on this
computer, use the hosted API-key profile instead; macOS is not supported by the
current experimental terminal worker. The account option is experimental: it
has not yet been tested end to end with real accounts.

## Codex cannot be found

> Codex is not installed or could not be found. Install or update Codex, then choose Check again.

**Cause:** The app cannot find a usable Codex installation.

**Fix:** Install or update Codex, then choose **Check again** in the Codex sign-in panel.

## Codex needs ChatGPT sign-in

> Sign in with ChatGPT before continuing.

**Cause:** Codex does not have the ChatGPT sign-in needed for planning.

**Fix:** Choose **Sign in with ChatGPT** and finish in the browser; if browser sign-in cannot complete, choose **Use device code**, open the device-code page, and enter the displayed code.

## Claude or Codex usage is limited

> Claude usage is currently limited. Your draft is unchanged; try again when usage is available.

> Codex usage is currently limited. Your draft is unchanged; try again when usage is available.

**Cause:** The selected assistant has no usage available for another planning request.

**Fix:** Wait until usage is available, then choose **Check again** for Claude or **Refresh account status** for signed-in Codex and retry planning.
Your draft is kept, and recovery remains available while planning is paused.

## Another client is using HOI4 Agent Tools

> This setup needs to reinstall HOI4 Agent Tools at the version it uses, but an app connected to the HOI4 MCP is using the installed copy. Close Codex, Claude Code, Cursor, or any other app using the HOI4 MCP, then prepare the changes again. For an older project, Check for updates moves it to the current version instead. Nothing was changed.

**Cause:** HOI4 Agent Tools is installed once for your user account and shared by every mod. The setup you are preparing uses a different version from the installed copy, and a connected MCP client is holding files that must be replaced.

**Fix:** Close Codex, Claude Code, Cursor, and any other client connected to the HOI4 MCP, then choose **Prepare changes** again. If you were repairing an older project while a newer HOI4 Agent Tools is installed, choose **Check for updates** instead: it moves the project to the current version rather than reinstalling the older one.

## The setup source contains a placeholder

> The setup source contains a placeholder instead of a real file. Nothing was changed; try again after the source is fixed.

**Cause:** The published setup source contains a placeholder where a complete file is required.

**Fix:** Retry after the source maintainer fixes the published files.
Your mod was not changed by this failed preparation.

## The setup source failed verification

> The setup source did not pass verification. Nothing was changed; try again later.

**Cause:** A downloaded setup file does not match its published verification information.

**Fix:** Try preparing the changes again later.
The app keeps the unverified files out of your mod.

## A pinned setup version does not match its manifest

> The files of this setup version do not match its published manifest, so it was refused. Nothing was changed. Choose Latest, a release, or a newer commit.

**Cause:** You pinned **Specific commit** (or a release) and one of that version's files is larger than its published manifest declares. The setup source refreshes its manifest in a separate commit after it changes content, so a commit made between the two is inconsistent.

**Fix:** Choose **Latest verified source**, a **Release**, or a commit whose message is "refresh setup package manifest" (the newest commit usually is). The app never installs files that disagree with their manifest.

## The setup source could not be downloaded

> The setup source could not be downloaded or prepared. Check your connection and try again; nothing was changed.

**Cause:** The app could not obtain or prepare the required setup files.

**Fix:** Check your internet connection and retry **Prepare changes**.

## The setup source returned GitHub HTTP 403

> The setup source could not be downloaded or prepared. Check your connection and try again; nothing was changed.

**Cause:** Unauthenticated GitHub REST requests can exhaust the public rate
quota. A rate-limit-only response may use the app's fixed-repository, read-only
Git ref lookup to resolve the exact revision, but unrelated 403 responses are
rejected for safety.

**Fix:** Retry **Prepare changes** later. Do not clone the source repository or
paste a GitHub token into the app. No project files are changed until the
selected manifest files pass their size and SHA-256 checks.

## The assistant's response could not be used

> The response did not match the required proposal format. Try the analysis again.

**Cause:** The assistant returned suggestions in a format the app could not accept.

**Fix:** Retry the assistant review.
If it keeps failing, review the selected assistant and model settings before trying again.

## Python for HOI4 Agent Tools is missing

> Python for HOI4 Agent Tools is missing. You can install now and finish it later with Repair.

**Cause:** HOI4 Agent Tools installs with a Python script, and no python.org Python 3.9 or newer was found.
The app looks on PATH and in the folders installers use: the Python install manager's (`%LOCALAPPDATA%\Python\pythoncore-*`, the default download for Python 3.14), the classic installer's, `Program Files`, and on macOS the framework and Homebrew folders.
It skips the Microsoft Store's `python.exe`, which is only a shortcut, and builds that are not from python.org (Anaconda, MSYS, distro Python on macOS).
The **Before setup** panel names whatever it found and skipped.

**Fix:** Install Python 3.9 or newer from python.org (either the Python install manager or the classic installer), then choose **Check again**.
You do not have to: the missing step never blocks **Start installation**. Setup finishes, HOI4 Agent Tools is marked incomplete, and **Repair** completes it once Python is installed.

## Installation was interrupted

> Installation was interrupted

> Your original files stay in the verified backup until recovery finishes.

**Cause:** Setup stopped before all installation and final checks completed.

If the message says **Access is denied**, **os error 5** or that a file **is being used by another process**, another program still has a file of this project open. That is usually a coding client (Claude Code, Codex, Cursor and others) running the project's MCP servers, or the hoi4-imagegen sign-in window. Close it first. **Undo changes** alone does not help while that program stays open, so the same stop repeats. HOI4 Mod Setup 0.9.5 and newer keep an old hoi4-imagegen version that is still in use instead of stopping, and remove it on a later Update or Repair.

**Fix:** Reopen the project through **Manage an existing project** and use **Recover interrupted setup** if offered.
On the recovery screen, select an available choice and press the matching button:

- **Continue setup:** Check the prepared files again and continue where setup stopped.
- **Discard prepared files:** Remove temporary setup files without changing the project.
- **Undo changes:** Return the project to the state it had before this setup began.

Only safe choices for the current setup are shown.
If the screen says “Some project files were already changed, so continuing automatically is unavailable.”, use an offered recovery choice rather than starting installation again.
Keep the verified backup until recovery finishes.

[Recovery screenshot](screenshots/14-recovery.png)

## MCP does not start, including when Node.js is missing

> The source-declared MCP initialize check did not pass.

**Cause:** Setup started HOI4 Agent Tools to check it, and it did not answer as expected. A missing or too old Node.js is the most common cause, but this message alone does not say which.

**Fix:** On Windows, Setup installs the Node.js LTS with winget when it is missing, then prepares HOI4 Agent Tools.
On Linux, install Node.js 22.19 or newer (or 24) from your package manager so it is at `/usr/bin/node`; see the [Linux guide](LINUX.md).
Use **Manage an existing project**, choose **Repair installation**, and review the changes before applying them so the bootstrap can run again.
Afterward, use **Refresh checks** on **Project ready**, or **Check integration** if offered.

## MCP is unavailable on macOS

> Not available on this computer

**Cause:** HOI4 Agent Tools runs on Apple silicon Macs. It needs HOI4 Mod Setup 0.9.5 or newer and a setup package in manifest format 7.
Intel Macs have no verified route, because the package bundles its engine for Apple silicon only.
An older package or an older app shows the component as unavailable, and an older app asks to be updated when the package is newer.
Everything else installs on every Mac: instructions, skills, subagents, the offline wiki and hoi4-imagegen.
The [README](../README.md#desktop-app-recommended) lists what each system gets.

**Fix:** On an Apple silicon Mac, update the app, then use **Manage an existing project** and **Update** (or **Repair installation**) to add HOI4 Agent Tools.
The setup needs Python 3.9 or newer, either from python.org or Apple's Command Line Tools (`xcode-select --install`).
It does not need Node.js: the package's bootstrap installs a private, signature-checked copy of the official build.
On an Intel Mac, leave the component unselected; core setup remains usable.

## Windows or macOS blocks the installer

> Windows protected your PC

> "HOI4 Mod Setup" Not Opened

**Cause:** The builds are free and not signed with a paid certificate, so both systems ask you to confirm the first launch.

**Fix:** Follow the [Run anyway and Open Anyway guide](RUN_ANYWAY.md).

## Image generation does not work after setup

**Cause:** hoi4-imagegen is installed in the project, but it has no sign-in or key yet, or the backend you chose needs a key or server address that is missing.

**Fix:** For the default ChatGPT sign-in, open the project in HOI4 Mod Setup and finish the sign-in on the Ready screen. For another backend, use **Manage an existing project**, **Update and repair**, open **Use another backend** on the image step, add the key or address, and repair. Keys stay in your operating system's credential store, never in the project.

## HOI4 Live Tools is unavailable

> Not available on this computer

**Cause:** HOI4 Live Tools is an experimental, slower workflow. It installs on
Windows and macOS only, so Linux shows it as unavailable.

**Fix:** Leave the route disabled on an unsupported computer, or choose the
Clausewitz MCP route after preparing its separate Linux game/server
environment. Core setup remains usable and no unsupported command is invented.

## The project does not open from the Ready screen

> Claude Code could not be opened. Check its installation or open the project folder manually.

**Cause:** The app could not start your primary coding client for the prepared folder. For Claude Code, the app starts `claude` in a new terminal window in the project folder; other clients are opened manually.

**Fix:** Open a terminal in the mod's **Project folder** and run `claude`, or open the folder in your client.
For Codex, **Open in Codex** stays disabled until the Codex configuration component is installed and the readiness checks pass.

## The setup package needs a newer HOI4 Mod Setup

> The setup package uses manifest format 3, which needs a newer HOI4 Mod Setup. Update the app, then try again.

**Cause:** The published workflow package changed its file format, and this copy of the app is older than that format.

**Fix:** Restart the app so its automatic update can install the current version, or download the newest installer from [GitHub Releases](https://github.com/klimPaskov/HOI4-Mod-Setup/releases/latest). Nothing in your mod was changed.

## Claude Code does not show the project skills, agents, or MCP server

**Cause:** Claude Code reads `.claude/skills/`, `.claude/agents/`, and the project `.mcp.json` from the folder it was started in, and it asks once before it starts a project MCP server.

**Fix:** Start Claude Code in the mod's project folder, approve the `hoi4_agent_tools` server when asked, and check it with `/mcp`. If the server was declined earlier, run `claude mcp reset-project-choices` in the project folder and start Claude Code again.

## Codex or another client follows outdated rules

**Cause:** `CLAUDE.md`, `.claude/skills/`, and `.claude/agents/` are the canonical files; `AGENTS.md`, `.agents/skills/`, and the Codex, Cursor, Qoder, and OpenCode agents are generated from them. A canonical file was edited without regenerating the copies.

**Fix:** In the project folder, run:

```powershell
python .tools/sync/sync_instructions.py
python .tools/sync/sync_agents.py
python .tools/sync/sync_skills.py
```

On macOS and Linux, type `python3` instead of `python`. Add `--check` to any of them to see what is out of date without writing.

## HOI4 Live Tools is incomplete or not listed in a coding client

**Cause:** Setup installs HOI4 Live Tools for your user, outside the mod, and registers it in each coding client's user configuration. The readiness page shows **incomplete** with the reason when that did not finish: Python 3.9 or newer was not found (or is not the python.org build), the release could not be downloaded or did not match the pinned checksum (for example before it is published), PyPI was unreachable for the dependencies, the installed server did not answer, or a `hoi4-live` entry that belongs to another program already exists in a client configuration (it is left untouched).

**Fix:** Correct the reason, then run **Check for updates** or **Repair** to run the setup again, or install it by hand: download the release, run `python install.py`, and restart the client (`python install.py status` lists what is installed and registered; the project's `.claude/skills/hoi4-live-playtest/references/setup.md` has the details and the troubleshooting table).
