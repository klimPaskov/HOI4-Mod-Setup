# Quick start

Set up a new or existing Hearts of Iron IV mod for AI-assisted development in about six steps. Each step links to the part of the [user guide](USER_GUIDE.md) with the details.

**Before you start:** install the coding client you want to use (Claude Code, Codex or another one the app lists). On Windows install Python from python.org. On Linux install Node.js 22.19 or newer and `python3` from your package manager. The [requirements](USER_GUIDE.md#requirements) list the rest.

1. **Install HOI4 Mod Setup.** Download the installer for your system from the [latest release](https://github.com/klimPaskov/HOI4-Mod-Setup/releases/latest) and open it. Windows and macOS warn about an unknown publisher the first time, which is expected: the [Run anyway guide](RUN_ANYWAY.md) shows the two clicks. On Linux follow the [Linux guide](LINUX.md).

2. **Choose a starting point and an assistant.** Choose **Create new mod**, or **Import existing mod** for a mod you already have. Pick the assistant that fills in the next screens; the free one needs no account. [Screenshot](screenshots/01-welcome.jpg) · [Details](USER_GUIDE.md#choose-a-project-and-setup-assistant)

3. **Describe the mod.** Enter a name and a short description, then check the project ID, prefix, folders and locations the assistant suggests and confirm them. An imported mod is scanned first without being changed. [Screenshot](screenshots/02-description.png) · [Details](USER_GUIDE.md#describe-the-mod-and-review-its-details)

4. **Choose your coding client and what to install.** Pick one main client and any others you use. Leave **Install hoi4-imagegen** ticked if you want generated images. The defaults are right for most mods; optional workflows such as 3D models, Super Events, portraits and live testing can be added now or later. [Screenshot](screenshots/15-coding-environments.png) · [Details](USER_GUIDE.md#choose-your-coding-clients-and-components)

5. **Review and install.** Choose a Git option (**Skip Git setup** is fine), review the planned files, then choose **Start installation**. Nothing is written until then, and your own files are never overwritten without asking. [Screenshot](screenshots/09-dry-run.png) · [Details](USER_GUIDE.md#review-and-install)

6. **Open the ready mod.** On **Project ready**, choose **Open in** your client and approve the project's tools when it asks. Then follow the kit's [first-run guide](https://github.com/klimPaskov/Agentic-HOI4-Modding/blob/main/docs/first-run.md) to check that your agent works. [Screenshot](screenshots/08-ready.png) · [Details](USER_GUIDE.md#open-the-prepared-project)

## Later

Reopen the app and choose **Manage an existing project** to update, repair, add a workflow or remove the setup. See [update, repair or add workflows](USER_GUIDE.md#update-repair-or-add-workflows-later).

## If something goes wrong

Find the message in [Troubleshooting](TROUBLESHOOTING.md). If setup stops part-way, follow the [recovery steps](TROUBLESHOOTING.md#installation-was-interrupted).
