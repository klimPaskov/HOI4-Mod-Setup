# Quick start on Windows

Follow these steps to prepare a new or existing mod for AI-assisted development.

1. **Install HOI4 Mod Setup.** Download `HOI4-Mod-Setup-windows-x64-setup.exe` from [GitHub Releases](https://github.com/klimPaskov/HOI4-Mod-Setup/releases), run the installer, and open the app.

2. **Choose your starting point.** On **Start a mod project**, choose **Create new mod** or **Import existing mod**. [Screenshot](screenshots/01-welcome.jpg)

3. **Connect the setup assistant.** Claude is the default: choose **Sign in to Claude** and finish signing in through your own Claude Code; if it is missing, use **Install Claude Code** first. Alternatively, select **Codex** and choose **Sign in with ChatGPT**, or select **Claude API key**, **Kimi**, **GLM**, or **DeepSeek**, enter your provider key, and use its connection button. [Screenshot](screenshots/10-provider-selection.png)

4. **Describe or scan the mod.** For a new mod, fill in **Mod name** and **Description** on **Describe the mod**, then choose **Next**. For an import, use **Browse** beside **Project folder**, let the read-only scan finish, and review **Confirm scan findings** with your selected assistant. [New mod](screenshots/02-description.png) · [Existing mod](screenshots/11-existing-project.png)

5. **Confirm the identity.** For a new mod, review **Project identity**, including the project ID, script prefix, namespace, tags, initial folders, and project and launcher paths. Edit any incorrect suggestions and use the assistant's suggestion-confirmation button before continuing; imported projects also require confirmation of the assistant's suggestions. [Screenshot](screenshots/03-identity.png)

6. **Choose your coding clients.** On **Coding Environments**, pick one **Primary environment** (Codex is the default), then select any **Additional environments** from Claude Code, Codex, Cursor, Qoder, and OpenCode. To use both Claude Code and Codex, make one primary and add the other; this choice is separate from the setup assistant. [Screenshot](screenshots/15-coding-environments.png)

7. **Choose components.** On **Choose what to install**, review the selected project instructions, skills, subagents, client configuration, HOI4 Agent Tools MCP, and offline Paradox wiki. Required items stay selected, and the selected clients receive their project guidance and configuration, including `AGENTS.md` and, for Claude Code, `CLAUDE.md`. [Screenshot](screenshots/04-components.png)

8. **Choose optional workflows.** On **Optional workflows**, enable any available **3D models workflow** (Meshy and Blender), **Super Events workflow**, or **ComfyUI portrait production** (Comfy Cloud, Local ComfyUI, or RunPod). If the 3D workflow asks for a Meshy key, choose **Store in vault** or **Configure later**; incomplete optional workflows do not block core setup. [Screenshot](screenshots/05-integrations.png)

9. **Review MCP and credentials.** On **MCP and credentials**, check the selected integration and credential status, then choose **Next**. Secret values stay outside the mod folder. [Screenshot](screenshots/06-mcp-credentials.png)

10. **Choose Git setup.** On **Choose Git setup**, select **Initialize a Git repository**, **Preserve the existing repository**, or **Skip Git setup**. Leave **Keep this project local** selected unless you want an online action, which asks for separate approval. [Screenshot](screenshots/07-git.png)

11. **Review the dry run.** On **Review changes**, choose **Prepare changes** if needed, inspect the planned files and folders, and use **Resolve conflicts** if offered. Nothing has been applied yet; resolve blocking conflicts before choosing **Start installation**. [Screenshot](screenshots/09-dry-run.png)

12. **Let installation finish.** Wait on **Installing components**, then choose **Continue** when installation completes. If setup stops unexpectedly, follow the [recovery instructions](TROUBLESHOOTING.md#installation-was-interrupted).

13. **Open the prepared mod.** On **Project ready**, review the checks and use **Open in Codex** when enabled, or open the same **Project folder** manually in your selected agent client, such as Claude Code or Cursor. Choose **Finish** after reviewing readiness; optional workflow status can still need attention while core setup is ready. [Screenshot](screenshots/08-ready.png)

If something blocks setup, see [Troubleshooting](TROUBLESHOOTING.md).
