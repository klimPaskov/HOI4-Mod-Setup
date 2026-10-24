# User guide

The full walkthrough of HOI4 Mod Setup, moved here from the README. For a short
start, use the [README](../README.md) or the [quick start](QUICKSTART.md); for
fixes, see [Troubleshooting](TROUBLESHOOTING.md).

## Web edition in detail

The [web edition](https://klimpaskov.github.io/HOI4-Mod-Setup/app/) needs no
install. Choose a setup assistant: the free model, which needs no account
when it is offered, **Sign in with OpenRouter** (free models included), or
Claude, Gemini, Grok, DeepSeek, or GLM with your API key. Then start a new mod
from a short description or drop your mod folder on the page. The assistant fills in the mod's details and recommends
components; you review everything and download a ZIP to unpack yourself. Your
files stay in the browser tab: any key stays in the tab's memory, the
provider sees only your description or a scan summary, and the page downloads
only the selected modding files from GitHub at one exact commit and checks each
against its published SHA-256 hash.

It covers most of the desktop app: documents and images as optional context,
coding clients, components, image providers, the portrait workflow, Clausewitz
MCP live testing, a review of every changed file with keep, replace, merge,
rename, or skip, and the same readiness checks. A folder that already has a
setup can be updated, repaired, changed, or have its setup removed. A local
model on your computer can also be the setup assistant.

The web edition does not change files in place, so it has no backup or undo
step; unpack the ZIP over a copy or a Git checkout of your mod. Account
sign-in for Claude and Codex, installing and checking the HOI4 Agent Tools
server, saving image-provider keys in your credential store, Git, HOI4 Live
Tools, and the 3D models workflow need the desktop app.

On Windows, run the downloaded setup file. On Mac, open the disk image and
move HOI4 Mod Setup to Applications. Community Windows builds are signed by
ChaosX; community Mac builds use ad-hoc signing. See the
[release notes](https://github.com/klimPaskov/HOI4-Mod-Setup/releases/latest)
for your download's signing details.


## Existing mods: import, reimport, update

These are the rules the app follows with a mod that already exists. They were
checked on disposable copies of real Workshop mods (see
[the review record](development/2026-10-24-app-review.md)).

- **Import** reads the folder and writes nothing. The assistant fills in the
  details, you confirm them, and installation then adds the setup on top of
  the mod: instructions, skills, subagents, tools, the offline wiki, Git rules,
  and `.hoi4-mod-setup/install.lock.json`, which records what the app owns.
- **Your own files stay yours.** A valid `descriptor.mod`, `thumbnail.png`, and
  `README.md` are kept as they are on the first import; the app never
  rewrites your `version`, `supported_version`, or Workshop fields, and your
  mod content (`common/`, `events/`, `gfx/`, `localisation/`, …) is never touched.
- **Reimport** the same folder any time. The app recognises its setup
  (**Existing setup found**) and offers **Repair or add workflows** instead of
  starting again.
- **Update** (desktop: **Manage an existing project → Update and repair → Check
  for updates**; web: drop the folder again and choose **Update**) compares the
  folder with the newest setup source. Unchanged setup files are replaced,
  files you edited are never overwritten silently, and files you added,
  renamed, or deleted in your own mod are left alone. A file you edited that
  the source also changed is shown for review: **Keep current**, **Use new**,
  **Merge** (line by line, refused when both sides changed the same lines),
  **Keep both**, or **Skip**. An edit you already kept is not asked about again
  while the source has nothing new for that file.
- **Repeating an update** with nothing new produces a review with no conflicts.
- **If installation is interrupted** (the app was closed or the computer
  slept), reopening the folder shows **Installation was interrupted**. **Undo
  changes** returns the mod to how it was, including removing the folders the
  setup created; then run setup again and keep the app open until it finishes.
- **Web edition**: nothing is written in place. The ZIP is the result; unpack it
  over a copy or a Git checkout of the mod, and drop that folder again later to
  update.

## Using the desktop app

### Choose a project and setup assistant

![Create a new mod or import an existing one](screenshots/01-welcome.jpg)

Choose **Create new mod** or **Import existing mod**. The setup assistant helps
prepare the mod's details; choose your coding environment separately later.

The setup assistant that plans the mod is separate from the coding client you
choose later. Select any supported assistant provider and use its account
sign-in, API key, or local connection. When the free model is offered, it is
the default and needs no account: HOI4 Mod Setup sends the request through its
own relay to a free model on OpenRouter, free models may keep what you send,
and all users share one daily limit. **OpenRouter** also lets you sign in with
your own account instead of pasting a key.

While describing a new mod, type a rough idea and choose **Expand
description**: the setup assistant rewrites it as a fuller description that
you can edit or undo. Gemini and Grok API-key profiles use the
app's reviewed provider routes; experimental provider-account routes, when
shown, keep sign-in inside the provider's own CLI. Choose a model and
reasoning level when the provider offers them. Claude defaults to Haiku 5.5 at
medium effort, keeps legacy Haiku 4.5, and lets advanced users enter an exact
Claude model ID.

The experimental Gemini and Grok account options open the official provider
client in a separate sign-in window and never ask you to paste a token. They
are currently verified only on Windows and Linux; use the corresponding
API-key profile when the account route is unavailable on your computer.

To work on a mod you already have, choose **Manage an existing project** and
select its folder. The app finds its launcher file and installed setup.

![Choose a previously prepared project](screenshots/11-existing-project.png)

### Describe the mod and review its details

For a new mod, enter its name and a short description.

![Enter the mod name and description](screenshots/02-description.png)

The app suggests a project ID, script prefix, namespace, tags, and starter
folders. It fills the standard HOI4 mod location and launcher file. Edit any
value before continuing. Setup creates both `descriptor.mod` files and a replaceable
600x600 black `thumbnail.png`.

![Review the generated project identity and locations](screenshots/03-identity.png)

For an existing mod, the app checks its descriptors, instructions, skills,
subagents, client configuration, and Git state, then shows what needs attention.

### Choose your coding clients and components

Select any supported coding client as your main one, then add other clients you use.
Codex, Claude Code, Cursor, Qoder, and OpenCode are supported, and setup
packages in manifest format 6 add twelve more clients. Cline, Devin Desktop,
Augment, and Continue keep MCP servers in their own settings, so the Ready
screen links their setup guide; Aider has no MCP support.

![Choose one primary coding environment and any additional clients](screenshots/15-coding-environments.png)

### Choose how to make images

Pick a main image provider and add optional providers. ChatGPT through GPTImage
is selected by default. The list also includes image APIs, local servers, Codex
image generation when Codex is the main client, and a choice to make art yourself.
The app stores API keys in the operating-system credential store.

![Choose an image-generation provider](screenshots/16-image-generation.png)

Choose instructions, skills, tools, and the offline wiki. The app adapts the
guidance to your mod and fills the game and Workshop folders it finds through
Steam. Required components stay selected.

Choosing an image provider automatically selects its required setup component;
changing providers removes stale image-route components before the next review.
The final dependency and file plan is always checked by the app core.

![Choose instructions, skills, tools, wiki, and ChatGPT files](screenshots/04-components.png)

**Latest** is the default and uses the current published Agentic HOI4 Modding package. You can also
choose a specific commit or release. Updates bring in published components and
keep your optional workflow choices.

**ChatGPT project-sources packager** is an optional component. Check it to add
`.tools/package_chatgpt_project_sources.py`, a script you run yourself to export
project files for a ChatGPT project.

### Add optional workflows

![Choose the optional workflows](screenshots/05-integrations.png)

- **3D models workflow** prepares Meshy and Blender on Windows. Add a Meshy key
  to use it; the key stays in Windows Credential Manager. Missing tools or
  credentials leave this workflow incomplete while the rest of setup remains
  usable.
- **Super Events workflow** adds the popup, templates, example, assets, and
  guidance for building Super Events with your mod's script prefix.
- **ComfyUI portrait production** supports Comfy Cloud, Local ComfyUI, or RunPod.
  Its expanded panel shows the 16 GB VRAM and 25 GB storage recommendation.
  See the [portrait workflow guide](32_comfyui_portrait_pipeline.md).
- **Live testing** opens a separate provider step. Clausewitz MCP by smerss is
  the default and requires native Linux HOI4 1.19.2 and a separately prepared
  Linux server: WSL on Windows, or a Linux machine/VM through SSH on macOS.
  Follow the [Linux setup guide](LIVE_TESTING_LINUX.md), then explicitly
  confirm readiness. Configure later records a to-do without registration.
  HOI4 Live Tools is experimental, slower, and not fully tested; its current
  public package verifies Windows only, with an incomplete macOS scaffold.
  None disables live testing. The next capture is `17-live-testing.png`.

HOI4 Agent Tools installs and registers its MCP server for your selected coding
clients on Windows, including `.mcp.json` for Claude Code. It works independently
of the assistant you used for setup.

![Review selected integrations and credential state](screenshots/06-mcp-credentials.png)

### Choose Git setup

![Keep the mod local, connect a remote, or publish it](screenshots/07-git.png)

Initialize Git, keep an existing repository, or skip Git. Keep the mod local,
connect a remote, or create a public GitHub repository. Publishing and pushing
ask for a separate confirmation.

### Review and install

![Review the files and folders before installation](screenshots/09-dry-run.png)

Review the files and folders to add or update, then start installation. For an
edited file, choose to keep your version, use the incoming version, merge, keep
both, or skip when available. Installation shows file counts, progress, and an
estimated time.

Changes are written to your mod folder; use Git to review or undo them.

If setup stops, reopen the project to continue, undo changes, or discard
prepared files. The app offers the actions available for that installation.

![Recover an interrupted installation](screenshots/14-recovery.png)

### Open the prepared project

![Completed setup and project checks](screenshots/08-ready.png)

The Ready screen shows the project and tool checks. **Open in your primary
client** starts that client in the mod folder when it is available. Optional
workflows show their own setup status.

For ChatGPT, check the optional packager component during setup. It adds
`.tools/package_chatgpt_project_sources.py` and a Windows `.bat` launcher; the
app never runs them or uploads anything.
Run `python .tools/package_chatgpt_project_sources.py` to export a folder and
`all-project-sources.zip` to Downloads. Use `--dry-run`, `--include`, `--exclude`,
or `--config` to adjust the selection, and a new `--output` folder for each
export. See the [source-packaging guide](33_chatgpt_source_packaging.md).

### Update, repair, or add workflows later

![Update and repair a prepared project](screenshots/12-maintenance.png)

Open **Manage an existing project** to update or repair setup files, change
coding clients or the portrait provider, add workflows, remove components, or
recover an interrupted installation.

**Update source** defaults to **Latest verified source**, including for projects
installed from an older pin. Choose a commit or release to pin instead, then
review and confirm the update. MCP versions follow the selected package's
verified manifest. **Repair installation** keeps the installed revision.

**Remove components** removes unchanged files installed by the app and the
folders left empty. Edited setup files are shown for review; your mod content
stays in place.

The app checks for updates when it opens and installs a newer signed version
automatically. If an update fails, you can keep using the app and retry.

