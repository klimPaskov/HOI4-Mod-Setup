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

> HOI4 Agent Tools needs an update, but an app connected to the HOI4 MCP is using it. Close Codex, Claude Code, Cursor, or any other app using the HOI4 MCP, then prepare the changes again. Nothing was changed.

**Cause:** A connected MCP client is holding files that the HOI4 Agent Tools update needs to replace.

**Fix:** Close Codex, Claude Code, Cursor, and any other client connected to the HOI4 MCP, then choose **Prepare changes** again.

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

## The setup source could not be downloaded

> The setup source could not be downloaded or prepared. Check your connection and try again; nothing was changed.

**Cause:** The app could not obtain or prepare the required setup files.

**Fix:** Check your internet connection and retry **Prepare changes**.

## The assistant's response could not be used

> The response did not match the required proposal format. Try the analysis again.

**Cause:** The assistant returned suggestions in a format the app could not accept.

**Fix:** Retry the assistant review.
If it keeps failing, review the selected assistant and model settings before trying again.

## Installation was interrupted

> Installation was interrupted

> Your original files stay in the verified backup until recovery finishes.

**Cause:** Setup stopped before all installation and final checks completed.

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

**Cause:** The MCP could not complete its startup check; missing Node.js LTS is one possible cause.
This message alone does not identify the cause.

**Fix:** On Windows, the reviewed HOI4 Agent Tools bootstrap installs Node.js LTS through winget when it is missing, then prepares the MCP package.
Use **Manage an existing project**, choose **Repair installation**, and review the changes before applying them so the bootstrap can run again.
Afterward, use **Refresh checks** on **Project ready**, or **Check integration** if offered.

## MCP is unavailable on macOS

> Not available on this computer

**Cause:** The current verified HOI4 Agent Tools MCP route supports Windows only.

**Fix:** Leave the unavailable MCP component unselected on macOS and continue with the supported components.
Core setup remains usable; this MCP route cannot be enabled on macOS through a setting in the app.

## Codex does not open from the Ready screen

> Codex could not be opened. Check the Codex installation or open the project folder manually.

**Cause:** The app could not open the prepared folder in Codex.

**Fix:** Check the Codex installation or open your mod's **Project folder** manually in Codex.
If **Open in Codex** is disabled, review the readiness checks and whether the Codex configuration component is installed.
