# Quick start on Windows

Follow these steps to prepare a new or existing mod for AI-assisted development.

**Before you start:** install [Git](https://git-scm.com/download/win), [Node.js LTS](https://nodejs.org/), and [Claude Code](https://code.claude.com/docs/en/setup), and sign in to Claude Code once. Python 3.9 or newer is needed for the runtime synchronizers and the optional workflows. Nothing else has to be configured by hand; the app checks each tool and tells you what is missing.

1. **Install HOI4 Mod Setup.** Download `HOI4-Mod-Setup-windows-x64-setup.exe` from [GitHub Releases](https://github.com/klimPaskov/HOI4-Mod-Setup/releases), run the installer, and open the app.

2. **Choose your starting point.** On **Start a mod project**, choose **Create new mod** or **Import existing mod**. [Screenshot](screenshots/01-welcome.jpg)

3. **Connect the setup assistant.** Claude is the default: choose **Sign in to Claude** and finish signing in through your own Claude Code; if it is missing, use **Install Claude Code** first. Alternatively, select **Codex** and choose **Sign in with ChatGPT**, or select **Claude API key**, **Kimi**, **GLM**, or **DeepSeek**, enter your provider key, and use its connection button. The setup assistant only prepares the mod; it does not decide which client you develop with. [Screenshot](screenshots/10-provider-selection.png)

4. **Describe or scan the mod.** For a new mod, fill in **Mod name** and **Description** on **Describe the mod**, then choose **Next**. For an import, use **Browse** beside **Project folder**, let the read-only scan finish, and review **Confirm scan findings** with your selected assistant. [New mod](screenshots/02-description.png) · [Existing mod](screenshots/11-existing-project.png)

5. **Confirm the identity.** For a new mod, review **Project identity**, including the project ID, script prefix, namespace, tags, initial folders, and project and launcher paths. Edit any incorrect suggestions and use the assistant's suggestion-confirmation button before continuing; imported projects also require confirmation of the assistant's suggestions. [Screenshot](screenshots/03-identity.png)

6. **Choose your coding clients.** On **Coding environments**, pick one **Primary environment** (Claude Code is the default), then select any **Additional environments** such as Codex, Cursor, Qoder, or OpenCode. Claude Code files are the canonical source; the other clients receive generated copies of the same instructions, skills, and subagents. [Screenshot](screenshots/15-coding-environments.png)

7. **Choose components.** On **Choose what to install**, review the project instructions (`CLAUDE.md` and its `AGENTS.md` copy), skills, subagents, runtime synchronizers, HOI4 Agent Tools MCP, and offline Paradox wiki. Required items stay selected. [Screenshot](screenshots/04-components.png)

8. **Choose optional workflows.** On **Optional workflows**, enable any available **3D models workflow** (Meshy and Blender), **Super Events workflow**, **ComfyUI portrait production** (Comfy Cloud, Local ComfyUI, or RunPod), or **HOI4 Live Tools** (live game testing on Windows). If the 3D workflow asks for a Meshy key, choose **Store in vault** or **Configure later**; incomplete optional workflows do not block core setup. [Screenshot](screenshots/05-integrations.png)

9. **Review MCP and credentials.** On **MCP and credentials**, check the selected integrations and credential status, then choose **Next**. Secret values stay outside the mod folder. [Screenshot](screenshots/06-mcp-credentials.png)

10. **Choose Git setup.** On **Choose Git setup**, select **Initialize a Git repository**, **Preserve the existing repository**, or **Skip Git setup**. Leave **Keep this project local** selected unless you want an online action, which asks for separate approval. [Screenshot](screenshots/07-git.png)

11. **Review the dry run.** On **Review changes**, choose **Prepare changes** if needed, inspect the planned files and folders, and use **Resolve conflicts** if offered. Nothing has been applied yet; resolve blocking conflicts before choosing **Start installation**. [Screenshot](screenshots/09-dry-run.png)

12. **Let installation finish.** Wait on **Installing components**, then choose **Continue** when installation completes. If setup stops unexpectedly, follow the [recovery instructions](TROUBLESHOOTING.md#installation-was-interrupted).

13. **Open the prepared mod.** On **Project ready**, review the checks and choose **Open in Claude Code** (or the button for your primary client). Claude Code starts in a new terminal window in the project folder; approve the `hoi4_agent_tools` project server when it asks, then check it with `/mcp`. Choose **Finish** after reviewing readiness; optional workflow status can still need attention while core setup is ready. [Screenshot](screenshots/08-ready.png)

14. **If you selected HOI4 Live Tools**, follow the setup guide linked on **Project ready** once to register the live-game server for your user.

## Working in the prepared mod

- Edit `CLAUDE.md`, `.claude/skills/`, and `.claude/agents/` only. After a change, run `python .tools/sync/sync_instructions.py`, `python .tools/sync/sync_agents.py`, and `python .tools/sync/sync_skills.py` so Codex and the other clients get the same rules.
- Each specialist subagent has a fixed Claude model tier: Opus for the improvement loop planner, Sonnet for the 3D pipeline, feature completion auditor, localisation auditor, scripted system architect, and skill maintainer, and Haiku for the rest.
- Reopen the project in HOI4 Mod Setup with **Manage an existing project** to update, repair, add a workflow, or remove the setup.

If something blocks setup, see [Troubleshooting](TROUBLESHOOTING.md).
