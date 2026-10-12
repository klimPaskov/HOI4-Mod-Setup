# Quick start on Windows and macOS

Follow these steps to prepare a new or existing mod for AI-assisted development.

**Before you start:** download HOI4 Mod Setup for Windows or macOS. Install any coding client you want to use; the app shows which supported clients are available. You can finish setup assistant sign-in when prompted. The app explains any extra setup needed for an optional workflow.

1. **Install HOI4 Mod Setup.** Download `HOI4-Mod-Setup-windows-x64-setup.exe` from [GitHub Releases](https://github.com/klimPaskov/HOI4-Mod-Setup/releases), run the installer, and open the app.

2. **Choose your starting point.** On **Start a mod project**, choose **Create new mod** or **Import existing mod**. [Screenshot](screenshots/01-welcome.jpg)

3. **Choose a setup assistant.** Pick any provider offered on the **Provider** screen and connect it using the sign-in, key, or local address shown there. This assistant helps prepare the mod; it does not choose the coding client you use later. [Screenshot](screenshots/10-provider-selection.png)

4. **Describe or scan the mod.** For a new mod, fill in **Mod name** and **Description** on **Describe the mod**, then choose **Next**. For an import, use **Browse** beside **Project folder**, let the read-only scan finish, and review **Confirm scan findings** with your selected assistant. [New mod](screenshots/02-description.png) · [Existing mod](screenshots/11-existing-project.png)

5. **Confirm the identity.** For a new mod, review **Project identity**, including the project ID, script prefix, namespace, tags, initial folders, and project and launcher paths. Edit any incorrect suggestions and use the assistant's suggestion-confirmation button before continuing; imported projects also require confirmation of the assistant's suggestions. [Screenshot](screenshots/03-identity.png)

6. **Choose your coding clients.** On **Coding environments**, choose any supported client as your primary one, then select any other clients you use. Setup prepares matching instructions, tools, and settings for the clients you select. [Screenshot](screenshots/15-coding-environments.png)

7. **Choose an image provider.** Pick a main provider and add optional providers if you want them. ChatGPT through GPTImage is ready as the default route; other choices include API services, local image tools, Codex image generation when Codex is selected, and making art yourself. The app keeps API keys in the Windows credential store or macOS Keychain. Change providers later in **Update and repair**. [Screenshot](screenshots/16-image-generation.png)

8. **Choose components.** On **Choose what to install**, review the project instructions, skills, subagents, tools, and offline Paradox wiki. Required items stay selected. [Screenshot](screenshots/04-components.png)

9. **Choose optional workflows.** On **Optional workflows**, enable any available **3D models workflow** (Meshy and Blender), **Super Events workflow**, **ComfyUI portrait production** (Comfy Cloud, Local ComfyUI, or RunPod), or **HOI4 Live Tools** (live game testing on Windows). If the 3D workflow asks for a Meshy key, choose **Store in vault** or **Configure later**; the key stays on your computer. Incomplete optional workflows do not block core setup. [Screenshot](screenshots/05-integrations.png)

10. **Review integrations and credentials.** Check the selected integrations and credential status, then choose **Next**. Secret values stay outside the mod folder. [Screenshot](screenshots/06-mcp-credentials.png)

11. **Choose Git setup.** Select **Initialize a Git repository**, **Preserve the existing repository**, or **Skip Git setup**. Leave **Keep this project local** selected unless you want an online action, which asks for separate approval. [Screenshot](screenshots/07-git.png)

12. **Review the changes.** Choose **Prepare changes** if needed, inspect the planned files and folders, and use **Resolve conflicts** if offered. Nothing has been applied yet; resolve blocking conflicts before choosing **Start installation**. [Screenshot](screenshots/09-dry-run.png)

13. **Let installation finish.** Wait for installation to complete, then choose **Continue**. If setup stops unexpectedly, follow the [recovery instructions](TROUBLESHOOTING.md#installation-was-interrupted).

14. **Open the prepared mod.** On **Project ready**, review the checks and choose **Open in** your primary client when it is available. Follow that client's prompt to approve project tools. Choose **Finish** after reviewing readiness; optional workflow status can still need attention while core setup is ready. [Screenshot](screenshots/08-ready.png)

14. **If you selected HOI4 Live Tools**, setup installs the live-game server for your user and registers it for the coding clients it finds; check its row on **Project ready** and restart your coding client. If the row says incomplete, follow the [troubleshooting entry](TROUBLESHOOTING.md#hoi4-live-tools-is-incomplete-or-not-listed-in-a-coding-client).

## Working in the prepared mod

- Edit the shared project instructions, skills, and specialist agents, then run the included sync tools to update the copies for your selected clients.
- Each specialist subagent has a fixed Claude model tier: Opus for the improvement loop planner, Sonnet for the 3D pipeline, feature completion auditor, localisation auditor, scripted system architect, and skill maintainer, and Haiku for the rest.
- Reopen the project in HOI4 Mod Setup with **Manage an existing project** to update, repair, add a workflow, or remove the setup.

If something blocks setup, see [Troubleshooting](TROUBLESHOOTING.md).
