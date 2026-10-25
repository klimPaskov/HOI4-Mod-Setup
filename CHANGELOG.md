# Changelog

HOI4 Mod Setup follows semantic versioning.

## Unreleased

- Prepare the package's development tools automatically. When an install, update, or repair leaves `.tools/check.py` in the project, the review lists one more setup check and, after the files are applied, the app runs the package's own `python -B .tools/check.py setup --save-game-dir <your game folder>`: it installs the Python packages the checks need, records the game folder in a gitignored settings file, makes sure the ignore entries exist, regenerates the agent projections, and checks the project. A fresh install no longer fails the `generators` check because of a stale agent map. The step never fails the installation: the Ready screen shows a **Development tools** line and the tool's own message when something is missing (for example Python), and **Repair** runs it again.
- Update `CLAUDE.md` and `AGENTS.md` without asking when you never edited them, and merge the new template into them when you added your own rules. An update used to ask about both files every time the instruction template changed, offering only Keep, Replace, Rename, or Skip, so a mod with custom rules could not take a new section without losing them. The update now rebuilds what the old template adapted to and merges three ways; an edited file is still a reviewed conflict and now offers **Merge**.
- Let a hosted setup analysis take up to five minutes instead of two. The free model reasons before it answers and needs longer than two minutes on a large installed project, which showed as "The selected provider is temporarily unavailable" on every attempt of an update or repair review.

## 0.7.1 - 2026-10-25

- Turn on the free setup assistant. The relay is deployed, so a first run needs no account, key, or sign-in: the free model is offered and is the default on desktop and web. It sends one structured request through the maintainer's Cloudflare relay to a free OpenRouter model, and the app shows that free models may keep what is sent and that everyone shares one daily limit. When the limit is reached the app says so and offers another assistant.
- Replace the free model `nvidia/nemotron-3-super-120b-a12b:free`, whose only host was down (0% uptime), with `nvidia/nemotron-3.5-super-vl:free`, which supports structured output, for the free assistant and for OpenRouter with your own key. The relay now lets OpenRouter fall back to two other structured-output free models, and includes the provider's short reason when it refuses a request.
- Fix the relay answering 502 to every request: the upstream call used a redirect mode that Node accepts but Cloudflare Workers reject.
- Add one command for the maintainer, `node scripts/setup_free_assistant.mjs`, that validates an OpenRouter key and a Cloudflare token, stores the secrets, deploys the relay, tests it, and opens the pull request that sets its address.
- Make the release notes list installers in the order Windows, macOS, Linux.
- Say why a pinned setup version is refused when its files disagree with its published manifest (a commit made before the source's manifest refresh), instead of asking the user to check their connection. Pinning **Specific commit** and **Release** was checked live on a release build: both resolve to the exact full revision and record it in the plan.

## 0.7.0 - 2026-10-25

- Fix first installs that kept asking to undo changes: when HOI4 Agent Tools is selected but no python.org Python is on the computer, Review changes now shows the missing requirement and how to fix it, with a **Check again** button, and blocks **Start installation** before any file is written. Previously the bootstrap failed after the files were applied, readiness blocked the lock, and the only recovery was **Undo changes**, which returned on every retry.
- Explain an interrupted installation on the recovery screen: **What stopped setup** and the next step are visible without opening Details, and the message after **Undo changes** says the mod is as it was and what to change before trying again.
- Keep an existing mod's own valid `descriptor.mod`, `thumbnail.png`, and `README.md` by default on its first import, instead of asking the user to compare their descriptor with a generated one. Invalid files and a thumbnail chosen in the app still go through conflict review.
- Give an existing mod that this app has not set up the same default coding client as a new mod, rather than the legacy Codex default that only applies to a recorded installation.
- Say what happened when an installation was interrupted by closing the app or a sleeping computer, name the next step, and keep the install and Review changes screens honest: the install screen asks to keep the window open, and a plan that takes longer than its estimate says so instead of holding at "1 second remaining".
- Undo changes now also removes the folders an installation created only to hold new files (for example `.claude/skills/<skill>/`), instead of leaving an empty folder tree in the mod.
- Stop reporting every installed `.claude/agents/*.md` subagent as malformed with a blocking conflict when a project that already has a setup is scanned again: the scan now keeps native subagent files long enough to validate their frontmatter.
- Leave files an earlier setup installed and the user has not changed (README.md, `docs/`) out of the project profile, so a reimport does not hand the assistant the setup's own documentation as the mod's.
- Stop asking about files an update would not change: instruction files, client MCP files, and `.gitignore` that already hold what the setup wrote no longer appear in Resolve file conflicts, and unchanged managed files are no longer rewritten with their own bytes. After a no-change update the review lists only files you actually edited.
- Make **Merge** work for the edits people actually make. The text merge used to give up unless every version had the same number of lines, so appending a line to a skill or `AGENTS.md` always failed with "could not be proven safe". It is now a line-based three-way merge that combines additions and deletions in different places, keeps Windows line endings, and still refuses two different edits to the same lines.
- Choose **Preserve the existing repository** by default when the scan finds a Git repository, and say what to do when **Initialize** is refused for a mod that already has one, instead of stopping on "a Git repository already controls the project". Stop showing "AI model must be non-empty" before a local model's details are entered.
- Web edition: keep a mod's own valid `descriptor.mod`, `README.md`, and `thumbnail.png` by default (a collapsed "Your own files, kept as they are" group, with a warning that the setup's descriptor leaves out `remote_file_id` and `replace_path`), carry the mod's own `version` and `supported_version` into the setup descriptor, write `.hoi4-mod-setup/state.json` for existing mods so a first update no longer shows spurious `AGENTS.md` and `CLAUDE.md` conflicts, read large folders in parallel groups (a 36,000-file folder went from 359 s to 119 s), name the unreadable file in an error, warn before a reload that would lose work, reset fully on **Prepare another mod**, fix the empty gap and clipped step rail on phones, show a focus ring on the folder button, and default the supported game version to `1.19.*` like the desktop app.
- Keep your merged edits safe at the next update. After **Merge**, the lock recorded the merged file as the installed baseline, so the following update saw an unmodified file and replaced it with the source version, silently dropping the merged edit. The source is now the baseline and the merged bytes are recorded as a kept edit.
- Stop blocking **Project ready** on an edit you kept. A managed file you edited and kept in a reviewed update (for example `AGENTS.md`) made the managed-file hash, source revision, and conflict checks block while the file still held the bytes you kept; it is now a warning, and a file changed again after it was kept still blocks. **Repair** no longer asks again about such an edit either.
- Fix **Remove components** refusing to apply with "a confirmed selected-provider analysis is required before apply": a removal plan still carried the image provider's install step, so it was not treated as a pure removal. Removal now plans only deletions, needs no assistant, and works while signed out or disconnected. It also no longer stops at its last stage trying to re-run the HOI4 Agent Tools setup or the image provider's install step (the recovery screen blamed Python), and it keeps the mod's own `descriptor.mod`, launcher file, `thumbnail.png`, `README.md`, and documents you added: they stay on disk and are no longer managed.
- Name the cause when an update is refused because the connected assistant address differs from the one the project was set up with, instead of "the confirmed review no longer matches this session".
- Read the installed HOI4 Agent Tools package on several threads when verifying it. Verifying a 5,400-file, 283 MB package took 140 s for the read alone on a release build and now takes about 14 s; the Ready screen says that the first check can take a minute or two.
- Fix an installation that stopped before its success lock was written: recovery now offers rollback, which restores the exact earlier files, instead of a resume that could never finish.
- Fix Claude API setup analysis failing with an HTTP 400 about `minimum` and `maximum`: the schema sent to Anthropic leaves out the keywords its structured output does not support, and Setup still checks every reply against the full schema.
- Add `hoi4_mod_setup::service`, a facade for server hosts: per-session scan, analysis, plan and approval state with no process-wide stores, an injected credential store, an explicit workspace root and state root, a public root-identity function, and a declared Node runtime. Desktop commands call the same gate code. `keyring` is now the default-on `os-keyring` feature, so a server can build without it and without `libdbus`. See `docs/39_service_facade.md`.
- Fix exact restoration after a process stop during apply: new bytes are placed through a temporary file named for the transaction and operation (and for the lock), so rollback removes the `.hoi4ms-placing-*.tmp` file a hard stop left beside the destination instead of leaving an extra file in the project, and a retry after a stop during rollback replaces its own stale temporary. Directories that an operation created beside its destination and files outside the project root are not yet covered.
- Move the plan builder behind `service::DefaultPlanBuilder` and a `SourceFetcher` seam so a server builds the same plans as the desktop from a caller-given state root, cache root and Node runtime. The desktop path is unchanged. See `docs/39_service_facade.md`.
- Add a free setup assistant that needs no account: the app sends one structured request through the maintainer's relay to a free OpenRouter model, and it becomes the default once the relay is deployed. The maintainer's key stays a relay secret and never ships in the app or the web page.
- Add OpenRouter as a setup assistant on desktop and web, with **Sign in with OpenRouter** (OAuth PKCE) or a pasted key. A free model is the default choice.
- Add **Expand description** on the description step: the selected setup assistant turns a rough note into a fuller description you can edit or undo, on every assistant route.
- Profile existing mods from a bounded, read-only inventory and content samples so semantic review sees real gameplay structure, identifiers, localisation conventions, and documentation, not only the descriptor and README.
- Read setup manifest format 6, which declares each coding client's MCP support and lets one component serve several clients. Offer GitHub Copilot (VS Code), Gemini CLI, Antigravity, Grok Build, Kilo Code, Kiro, and Qwen Code with project MCP registration, and Cline, Devin Desktop, Augment, Continue, and Aider with project guidance, when the source publishes them.
- Show clients this version or platform cannot install as unavailable, label manual and absent MCP routes, and list a to-do with the reviewed guide for clients whose MCP servers live in their own settings.
- Require the registered HOI4 Agent Tools route, not just parseable JSON, for every selected client on Windows and Linux. Register OpenCode in `opencode.json`, the only file it reads, once the source publishes format 6.
- Register image-generation and live-testing routes for Gemini CLI, Antigravity, Grok Build, GitHub Copilot (VS Code), and Qwen Code when the source lists them, and let manual image providers work with every coding client. Deselecting a client also removes the recorded image and live-testing routes from its unchanged MCP file.
- Keep a recorded client that the chosen source version does not offer, and explain the refusal instead of reporting a download failure.
- Report a newer manifest format as needing an app update before schema validation, and apply format 5 live-testing rules to later formats.
- Stop setup analysis from failing on meaningless formatting: line breaks, over-long convention text, identifier case, tag spelling, and folder separators are normalized before the strict rules, a reply without a structured object gets the corrective turn, retry hints name the exact rule, and the internal reason is logged to the diagnostics file.
- Accept relative `mod/<name>`, trailing-separator, and single-backslash `path=` values in an existing launcher file, and explain a real mismatch with the declared and selected folders instead of a "scan error".
- Remove ChatGPT source packaging from the app: no export screen, Review or Update callout, or flattened-folder option. The packager is the optional `runtime.chatgpt_sources` component, a `.tools` script the user checks and runs. Installs that already recorded a ChatGPT folder keep it until a later migration.
- Optionally add documents and images while describing a new mod or reviewing an existing one. Documents inform the setup assistant as redacted context and can be copied to `docs/reference/`; an image can become the thumbnail, and images can be saved to `docs/visual_direction/`.
- Prepare plans faster: JSON Schemas are compiled once per run, a commit's manifest is downloaded and validated once even when Latest, a pin, and maintenance resolve to it, and the Components screen reuses the manifest it already has.
- Add **Copy diagnostics** next to failure messages, copying the recent redacted diagnostics log for a report.
- Keep the MCP and credentials review to one readable column instead of stretching across wide windows.
- Mark values the project files decide (descriptor tags, dominant namespace and prefix, localisation and documentation conventions, folder profile) as Detected and keep them over assistant guesses, and sample focus trees, events, decisions, on_actions, and localisation first.
- Add the web edition at `klimpaskov.github.io/HOI4-Mod-Setup/app/`: drop a mod folder or start a new mod, review each conflicting file, and download a ZIP with the installation lock. Files stay in the browser, source files are verified against the manifest hashes, and optional suggestions from Claude, Gemini, Grok, DeepSeek, or GLM use a key kept only in the tab's memory.
- Record generated text once in the installation lock instead of also as a byte array.
- Bring the web edition close to the desktop app: context documents and images, a choice of the findings the assistant reads, image providers, portraits, and live testing, update, repair, change, and removal of an existing setup, merge previews and rename for changed files, readiness checks, a launcher file for existing mods, and a local model as the setup assistant.
- Record the raw source hash in installation locks written by the web edition, so the desktop app can maintain them.
- Update `rustls-webpki`, `webpki-roots`, and `webpki-root-certs` (distrusted emSign intermediates) and `vite` 6.4.4 (`launch-editor` path check bypass on Windows), so `cargo audit` and `pnpm audit` pass again.

## 0.6.3 - 2026-10-18

- Fix the app-owned Meshy MCP launcher (`--run-verified-meshy-mcp`), which failed every start with Meshy 0.6.2 because its private runtime copy hid the hoisted dependencies (`dotenv`, `axios`, `express`, `zod`, and the MCP SDK) from Node. The verified copy is now materialized under a private `node_modules` directory; the tree hash and file count, package identity, pre-spawn rehash, Node publisher verification, cleared environment, and process ownership checks are unchanged.
- Pin the Meshy server's runtime-read settings (`TRANSPORT`, `MESHY_API_HOST`, `NO_PROXY`, and `NODE_TLS_REJECT_UNAUTHORIZED`) in the launcher's cleared environment. Meshy 0.6.2 loads a `.env` from the project folder, and without these pins a project file could switch the key-bearing server to HTTP, proxy authenticated requests, or disable TLS verification.
- Build the updater without its unused archive-extraction feature, removing the `zip` 4.6.1 crate affected by RUSTSEC-2026-0367, RUSTSEC-2026-0368, and RUSTSEC-2026-0369. Updater payloads remain raw signed installers, AppImages, and macOS `.tar.gz` archives.

Among MCP routes, this patch changes only the Meshy launch route. HOI4 Agent Tools and other MCP
routes keep their existing private package layout. No schema, lock, or
credential-store change is included. The fix is covered by fixture tests under
real Node; a credential-bearing Meshy start and paid Meshy generation are not
part of automated CI.

## 0.6.2 - 2026-10-15

- Make Latest the default for new setups and existing-project Update, while keeping exact commit and release pinning available.
- Let pinned projects explicitly move to the latest verified source. Bind review and installation to the same source revision and manifest, and keep Repair on the installed revision.
- Use the selected source manifest for MCP updates, including standalone Live Tools, and record the reviewed MCP version in the installation lock.
- Show the update target before the maintenance actions, prevent duplicate reanalysis, and discard results after the reviewed source or project changes.
- Fix rollback of missing maintenance files and keep removal operation IDs unique across independently planned component batches.
- Extend hosted native acceptance through new-mod creation, Kaiserreich import/update, pinned Repair, return to Latest, and interrupted-install rollback.

MCP versions follow verified package evidence, not unreviewed registry latest
tags. Experimental Gemini/Grok account sign-in remains unverified with real
accounts; macOS account handoff remains unavailable. Real provider sign-ins,
paid image generation, and HOI4 gameplay are not covered by fixture-based CI.
This patch also retains explicit live-validation deferrals for updater
replacement/restart/retry, Windows/macOS credential-vault round-trips, and full
native macOS wizard testing; automated regression and package checks do not
stand in for those results.

## 0.6.1 - 2026-10-14

- Fix source-cache preparation and atomic filesystem operations on non-English Windows by preserving typed I/O errors instead of matching localized error messages.
- Scan newly installed projects before maintenance planning, and preserve kept Workshop descriptors during repair without treating local bytes as the rejected generated baseline.
- Reconcile refreshed instructions with existing repair operations, preventing duplicate destinations while preserving local-change review.
- Use verified private npm toolchains for reproducible HOI4 Agent Tools installation on Windows and Linux without replacing the user's npm executable or weakening package integrity checks.
- Add remote published-app acceptance coverage and link the README video with its supplied thumbnail.
- Update rustls to 0.23.46 for TLS security fixes and include Linux in application platform metadata.

## 0.6.0 - 2026-10-13

- Offer the verified Claude model catalog, use Haiku 5.5 at medium effort by default, and allow another supported Claude model ID under Advanced.
- Add Gemini and Grok API providers and experimental account sign-in through pinned official clients on Windows and Linux. Real authenticated Gemini/Grok flows remain unverified; macOS account handoff is unavailable.
- Use Codex's advertised default when a fresh setup's built-in model is absent from the live catalog, while preserving explicit choices.
- Make provider review reliable with authoritative schema validation, core-assigned analysis IDs, accurate evidence rules, bounded corrective retries and clear privacy/usage failures.
- Show review progress immediately, automatically include a chosen image provider's packages, and restore HOI4 Live Tools in optional workflows.
- Expand the source package to ten fal, three Replicate and four Together image-model choices with per-model request contracts.
- Prepare source files through verified manifest evidence, cache exact resolutions and recover from GitHub REST quota exhaustion without cloning the source repository or following redirects.
- Show descriptor conflict text, distinguish binary previews from missing files, and keep fresh-import conflict review in the setup flow.
- Import existing Workshop-style mods without inventing a launcher requirement or replacing their valid descriptor image settings; retain strict launcher and thumbnail checks for new projects.

## 0.5.0 - 2026-10-13

- Add native Linux x86-64 AppImage and Debian packages beside Windows and macOS builds, with signed updater routes and dedicated desktop/package lifecycle CI.
- Resolve native and Flatpak Steam libraries, XDG Paradox mod paths, settings, cache and Downloads.
- Store Linux API keys in Secret Service and verify native system tools and official coding-client identities before execution.
- Install the pinned Agent Tools package with separate Linux integrity evidence, then register native routes for all five coding clients.
- Keep Clausewitz as the live-testing default and run its pinned server directly on Linux without WSL or SSH fields.
- Consume the tested schema-5 Agentic package while preserving older manifest compatibility, transaction safety and Windows/macOS routes.

## 0.4.1 - 2026-10-13

- Refresh the tested Agentic HOI4 Modding package, HOI4 Agent Tools, GPTImage and Meshy pins, with exact source and package integrity checks.
- Keep Clausewitz MCP preselected after choosing live testing; HOI4 Live Tools remains the experimental alternative.
- Include the package's current reference-tool workflow, split guidance and specialist model-tier verification.
- Add real scratch installations for Claude Code and Codex, checking every selected skill and starting every registered server without paid tool calls.
- Fix GPTImage installation plans to use the declared bootstrap script while preserving the reviewed arguments, hashes and process allowlist.
- Choose the current supported platform for multi-platform commands, including GPTImage installation on macOS.
- Resolve branch names containing a slash as one API path segment, so local candidate checkouts and GitHub default branches retain exact source identity.

## 0.4.0 - 2026-10-12

- Choose an image-generation provider after choosing your coding clients. ChatGPT through GPTImage is the default; hosted APIs, local image tools, Codex image generation and manual art are available. Keep API keys in the operating-system credential store.
- Change image and live-testing providers through update or repair. Review the changed tools and instructions while preserving your custom files and registrations.
- Choose Clausewitz MCP, experimental HOI4 Live Tools, or manual live testing. Clausewitz MCP by smerss needs native Linux HOI4 1.19.2 and manual setup; follow the new Windows WSL and macOS Linux guide. Unconfirmed setup remains a to-do.
- Use the tested Agentic package release by default for new projects, and keep coding-agent choice independent from the setup assistant.

- Use the resolved package's coding-environment default when a fresh planning request omits the primary client, while preserving the legacy Codex default for older installations.
- Add an isolated offline lifecycle gate for all components, import, repair, removal, rollback, and upgrade from the exact public 0.3.5 manifest.
- Bound native Rust test concurrency without skipping process/Git tests, and isolate source verification from the user's cache and environment.
- Keep the combined platform dependency inventories as internal release evidence; fix SBOM generation in a clean output directory and align the signing CLI with the locked version.
- Refresh the headless walkthrough screenshots, Claude-first architecture documentation, release instructions, and living skills.

- Install the Claude-first Agentic HOI4 Modding package: `CLAUDE.md`, `.claude/skills/`, and `.claude/agents/` are the canonical files, each specialist subagent has a fixed Claude model tier, and Codex, Cursor, Qoder, and OpenCode receive generated copies. Projects prepared with the earlier layout keep working and move to the new layout through **Check for updates**.
- Make Claude Code the default primary coding environment and open the prepared project in it from the Ready screen.
- Read the list of coding environments, their names, and the default from the source package instead of a fixed list in the app.
- Fill in the Hearts of Iron IV and Workshop folders found through your Steam libraries in the installed instructions.
- Add the optional HOI4 Live Tools workflow: setup checks for Python 3.9 or newer, downloads the release the package pins (size and SHA-256 verified), installs it for the current user with a private Python environment, confirms the server lists its tools, and registers it in the project configuration of the coding clients you select. The workflow also adds the `hoi4-live-playtest` skill and setup guide. A package that predates the installer keeps the skill-only workflow.
- Keep or remove guidance for any optional workflow the source package marks, without an app update.
- Stop requiring specific HOI4 Agent Tools route names in the app; the pinned package version's own tool list decides.
- Explain when the setup package needs a newer version of the app.
- Keep the technical cause of a failed setup plan in a bounded diagnostics log in the application data folder, and point to it from the message.
- Explain a Windows path that is too long and how to shorten or allow it.
- Show a required component only once on the Components screen, even when an optional component depends on it.
- Let package maintainers test unpublished package changes from a local checkout with `HOI4_MOD_SETUP_SOURCE_REPOSITORY`.
- Fix Check for updates, which never reached the core with its confirmed review, and show the confirmable review on the Update screen.
- Fix Repair, which could not plan for files with optional sections or adapted subagents and asked the user to resolve every healthy file.
- Fix Remove components, which stopped at the setup checks, left an empty folder tree behind, and ended on a blocked readiness screen. It now removes the folders it emptied and ends on a summary of removed and kept files.
- Recognize the subagents this app installs as valid when a project is scanned again.
- Make Claude account updates plan correctly and keep reanalysis reliable by stating the identifier, folder, and component rules to the model.
- Stop before changing any files when a running HOI4 MCP client would block the shared HOI4 Agent Tools reinstall, and explain the shared install.
- Commit the initial Git snapshot in batches instead of two Git processes per file, cutting several minutes from a fresh setup.
- Give the HOI4 MCP health check time for a cold start after installation, and name the running setup check on the progress screen.
- Install the manifest-pinned HOI4 Agent Tools 4.0.0 MCP contract; its complete runtime, package integrity, and advertised tool list are verified before readiness.
- Add a quick start and a troubleshooting guide.
- Make Claude the default setup assistant. **Sign in to Claude** runs your own
  installed Claude Code's sign-in, so the app never handles Claude credentials,
  and setup analysis uses Claude Haiku 4.5 in an isolated, tool-free Claude
  Code turn. The Anthropic API-key route remains available as Claude API key
  and also defaults to Haiku 4.5. Codex remains fully supported.
- Hide the reasoning-effort control for models that have no adjustable effort.
- Fix Codex planning failing as "temporarily unavailable" when current Codex
  releases stream many reasoning and message deltas during one turn.
- Import existing mods whose descriptors spread `tags` or `dependencies` over
  several lines, repeat `replace_path`, or carry `#` comments, instead of
  reporting the descriptor as unreadable.
- Check skill frontmatter with a bounded parser that rejects unterminated
  blocks, duplicate keys, and empty names or descriptions.
- Keep local ComfyUI folders and RunPod addresses on the computer: they restore
  portrait settings during import but are never sent to the setup assistant.
- Flush Windows directory entries after file replacements instead of treating
  a denied directory flush as success.
- Stop preparing an installation as soon as the source provides a Git LFS
  placeholder instead of a real file, with a clear explanation, instead of
  failing late during validation.
- Show plain next steps instead of internal error text when the installation
  plan cannot be prepared.
- Prepare the reviewed changes automatically when the dry run opens.
- Keep the mod name you typed for a new mod, keep suggested tags when you
  rename it, and let you confirm suggestions after editing them.
- Default new mods to supported game version 1.19.*.
- Check Codex and Claude Code signatures once per app run for unchanged
  binaries, cutting the first account check from about a minute to seconds.
- Download setup source files in parallel before preparing the plan.
- Retry setup analysis once with the exact validation problem when the setup
  assistant returns an incomplete proposal set, instead of failing the review.
- Show readable names and short summaries for scan findings instead of raw keys
  and JSON.
- Collapse integration technical details by default.
- Accept only a Claude plan sign-in for the Claude account route, parse Claude
  replies before any redaction, keep Claude Code from updating itself during
  setup, and discard an analysis that finishes after sign-out.
- Treat an unfinished Codex turn as a timeout instead of retrying it, and
  anchor macOS signature checks to Apple's Developer ID chain.
- Fix Claude account analysis failing with an unreadable result: Claude Code
  rejected the analysis schema's draft declaration.
- Include invalid identifiers and unsafe paths in the one corrective analysis
  retry.
- Update rustls to 0.23.45 (RUSTSEC-2026-0285) and replace the yanked chacha20
  0.10.1; update React to 19.3.0 and the Tauri CLI to 2.11.5.
- Refresh the bundled audit snapshot from exact published Git bytes, including
  the Python 3.13 3D compatibility files, and require the MCP server to advertise
  the reviewed package version during health checks.
- Fix existing-project review rejecting approved launcher descriptors, Git
  summaries, and aggregate scan findings as invalid evidence paths.
- Preserve bounded, redacted MCP bootstrap failure details through readiness
  and recovery, and allow already redacted error messages to persist safely.
- Replace the HTML-only browser smoke check with real headless wizard rendering
  and interaction tests in CI.
- Upgrade Vitest to 4.1.11 to remove the moderate path traversal advisory from
  the test dependency tree.
- Route transaction file reads and mutations through retained, no-follow
  directory handles to prevent ancestor link swaps from redirecting them.
- Bind reviewed plans and transaction journals to project-root identities,
  create new roots through the reviewed parent handle, and retain ambiguous
  roots safely after a crash before the identity checkpoint.
- Capture project identity during existing-project scan and require it again
  during semantic review and plan construction; version plan and journal
  identity evidence as schema 1.1.
- Use no-follow Windows lexical root acquisition and versioned 128-bit file IDs;
  legacy identity-less journals remain inspection-only.
- Bind the launcher descriptor's folder to its directory identity from the backup stage onward, and refuse to apply, verify, resume, or roll back into a different folder at the same path.
- Verify installed files and capture rollback backups through retained directory handles, hashing each backup from the same read that copies it.
- Bind each installation's application-data storage (its journal, plan, backups, and staged files) to the exact folders it created, and refuse to resume, roll back, discard staging, or start a new installation for the project when one of those folders was replaced or turned into a link, instead of following it. Moving the original folder back restores the normal recovery choices.
- Record the launcher descriptor folder's identity when the installation plan is prepared, and refuse to start or resume an installation if a different folder has taken its place since review.
- Stop rollback with a clear message, and keep it retryable, when the launcher folder of a changed launcher descriptor has been moved away, instead of reporting the descriptor as removed or restored.

## 0.3.5 - 2026-09-05

- Bind every semantic proposal key to its required JSON value type in the
  authoritative output schema, keeping malformed provider responses out of
  planning and replacing raw serialization details with a safe retry message.
- Filter gameplay, localisation, media, nested skill assets, agent archives,
  and generated documentation before targeted scan budgets are charged, so
  large mods cannot hide relevant setup evidence behind unrelated files.
- Bound credential-shaped path warnings independently and verify the read-only
  import scan against Chaos Redux without a safety-limit result or project
  metadata changes.
- Enumerate through retained no-follow directory handles, reject forged or
  replaced launcher descriptors, ignore linked setup files as positive facts,
  and report case-colliding setup paths as blocking evidence.
- Cache the immutable redaction patterns and restrict import-time Git probes to
  literal, batched Agentic setup paths while recovering tracked deletions from
  the index; the final live Chaos Redux scan completes in under ten seconds in
  the debug test harness.
- Bind launcher-parent enumeration to retained directory handles and cover
  directory swap-back, setup deletion, wildcard filename, and large Git
  path-inventory regressions.
- Mark capped Git path inventories or process output as partial, propagate scan
  cancellation into launcher discovery and supervised Git children, and bind
  evidence hashes to the exact rendered excerpt. Unicode case collisions, Unix
  backslash names, CRLF skills, and drive/UNC absolute paths now have explicit
  regression coverage.
- Restore whole-repository dirty detection for generic Git review gates and
  refuse to start Git when `HEAD`, its referenced ref, or other critical Git
  metadata is linked or externally redirected, with the policy rechecked around
  every child before output is accepted.
- Preserve the approved scan after a rejected Codex proposal or interrupted App
  Server session, while clearing only unconfirmed session proposals and keeping
  every renderer-facing provider error categorical and secret-free.
- Dispatch descriptor-preview capability checks outside the UI event loop and
  sanitize model-list, preview, install-plan, and maintenance-plan provider
  failures through the same bounded error map.

## 0.3.4 - 2026-09-02

- Pin the bundled Agentic HOI4 Modding source manifest to
  `hoi4-agent-tools@3.0.7`, including exact npm integrity, the complete
  4,848-file package-tree identity, and the source-advertised 23-tool catalog.
- Carry the latest server's detached parser, graph, and renderer memory
  retention fixes into fresh, update, and repair plans without changing the
  provider-neutral MCP contract or its security boundaries.
- Bind source-audit digests to the committed manifest bytes and test that
  checkout line-ending conversion cannot silently change the recorded identity.

## 0.3.2 - 2026-09-01

- Accept the verified `hoi4-agent-tools@3.0.5` package layout in Latest mode,
  including its 18,406,400-byte native library, while retaining exact npm
  integrity, complete package-tree hashing, link rejection, and bounded private
  runtime execution.
- Treat npm's hidden global package lock as additional evidence when present;
  installations from npm versions that omit it still require the exact
  manifest-bound 4,846-file package identity before the MCP can start.
- Keep all 23 source-advertised HOI4 Agent Tools routes available to every
  setup assistant, including Technology Tree inspect, render, and compare.

## 0.3.1 - 2026-08-31

- Publish and consume complete manifest-declared Codex, Claude Code, Cursor,
  Qoder, and OpenCode packages from the Agentic HOI4 Modding default branch,
  with platform-correct MCP registration and workflow-conditional agents.
- Restore ChatGPT sign-in against the current Codex App Server request schema
  and clear stale availability errors when a new login begins.
- Keep coding-environment names, descriptions, and primary status contained at
  normal and 200 percent scaling.
- Treat bounded Git-status evidence as a review finding instead of falsely
  failing a complete targeted scan; validate the fix against Chaos Redux and
  preserve recorded coding-environment choices from valid managed locks.

## 0.3.0 - 2026-08-27

- Add a dedicated Coding Environments step with Codex as the default primary
  client and composable Claude Code, Cursor, Qoder, and OpenCode selections.
- Install complete manifest-declared native client packages while keeping the
  canonical `.codex/agents/*.toml` definitions and generated projections in
  sync.
- Persist environment choices through project state, installation plans,
  locks, journals, update, repair, rollback, and removal, with detection and
  local-file preservation for existing projects.
- Keep coding-client selection independent from the setup assistant provider
  and Core/Core plus 3D workflow profiles.

## 0.2.16 - 2026-08-26

- Load the selected provider's live model catalog and show only the reasoning
  levels advertised for that model, labeled from Light through Max.
- Default Codex planning to GPT-5.6 Luna with Extra high reasoning and
  DeepSeek planning to its Flash model.
- Bind provider, model, and reasoning effort into confirmed analysis,
  generated project guidance, installation plans, locks, migrations, updates,
  and repairs so the selected profile cannot change silently.
- Make the verified HOI4 Agent Tools MCP package and 3D workflow independent
  of Codex configuration while retaining separate Codex client registration.
- Refresh the bundled canonical Agentic HOI4 Modding manifest to the exact
  green provider-neutral source revision.

## 0.2.15 - 2026-08-24

- Let the Windows release installer gate complete its fail-closed legacy
  machine-install scan on crowded runners without weakening fixed-key timeouts.
- Consolidate Agentic HOI4 Modding source discovery onto the single canonical
  `hoi4-mod-setup.manifest.json` schema-2 route; compatibility remains explicit
  in the manifest `schema_version` instead of parallel filenames.
- Remove the incomplete Qoder runtime components, synchronization component,
  optional profile, and generated project guidance from the Agentic setup.
- Reject every non-generated manifest file or tree component that resolves to
  zero tracked files, preventing incomplete source declarations from reaching
  planning again.
- Refresh the bundled source evidence to the exact current Agentic repository
  revision without changing selective download, checksum, transaction,
  credential, platform, or rollback protections.

## 0.2.14 - 2026-08-21

- Make the dynamic default-profile regression test select an explicit supported
  platform, so the Linux-only unsigned release gate verifies Windows/macOS
  behavior without treating Linux as a supported desktop target.

- Bind confirmed semantic analysis to the exact source revision and manifest
  digest, preserve installed source pins during maintenance reanalysis, and
  migrate older records only from valid provenance in the same lock.
- Disable platform-incompatible manifest components, show complete dependency
  and file evidence, and provide an announced, focus-safe Retry action when
  source loading fails.
- Keep stable releases tag-only, verify uploaded draft assets byte-for-byte
  before publication, and exercise first install, repair, removal, and rollback
  on every native release runner.
- Pin Nano ID 3.3.18 so the high-severity dependency audit remains clear.
- Move the expanded automatic MCP/3D contract to the versioned schema-2
  manifest route while retaining a schema-1 compatibility manifest for
  already installed app versions.

- Consume compatible new Agentic HOI4 Modding skills, subagents,
  default-profile additions, and optional workflow declarations from the exact
  published manifest without requiring a new app release.
- Advertise the working Technology Tree Viewer routes
  `hoi4.tech_inspect`, `hoi4.tech_render`, and `hoi4.tech_compare` from verified
  source evidence.
- Verify the complete installed HOI4 Agent Tools package tree, use a conforming
  MCP JSON-RPC initialize handshake, require exact protocol negotiation and
  the `tools` capability, execute a private verified package copy, and bind the
  one health result into the transaction journal before readiness.
- Make selected Windows 3D setup automatic after review: prepare the bounded
  Meshy/Blender MCP routes, verify and run the source bootstrap with OS-vault
  credentials, declare and prepare `uv`, and persist honest optional readiness
  without weakening core setup or rollback boundaries.
- Isolate the verified Python bootstrap, keep Meshy credentials out of Blender
  and dependency children, pin Meshy registry integrity and the io_pdx_mesh
  archive digest, execute Meshy from a complete lockfile-verified private
  runtime through the app-owned executable instead of project/PATH launchers,
  and checkpoint each selected external action separately.

- Complete read-only scans of very large HOI4 mods by inventorying only
  agentic setup surfaces: descriptors, instructions, skills, subagents,
  Codex/MCP configuration, approved docs, Git evidence, and managed setup
  metadata. Gameplay, localisation, media, root data dumps, and generated
  documentation assets are pruned before file inventory.
- Run project scanning on a dedicated blocking worker, throttle discovery
  progress, keep traversal deterministic, and preserve honest partial results
  for unreadable, oversized, timed-out, or truncated detector evidence.
- Bind file and Git evidence reads to identity-checked project and `.git`
  directory handles, use filter-free dirty-state probes, bound path,
  directory-sort, and conflict evidence, and redact secret-shaped finding
  values before they can reach the interface or semantic-review boundary.
- Prevent the Windows installer smoke test from leaving its temporary install
  directory in the current-user product registry or overwriting a registered
  current-user installation's metadata. Registry inspection fails closed,
  legacy machine-wide installs are refused, and test-owned cleanup is
  exact-path checked and asserted.
- Resolve Windows and macOS installer-smoke utilities from fixed native system
  paths with bounded termination waits instead of trusting `PATH`.
- Keep managed-install scan findings and provider-neutral suggestion origins
  valid against the authoritative scan-result schema.
- Show a launcher candidate's path match before import, allow excluding it,
  and never probe the arbitrary target declared by an unconfirmed candidate.
- Report duplicate, mismatched, or over-limit launcher registrations instead
  of silently choosing or truncating them, including during maintenance.
- Rebind a confirmed launcher path to the current unique core-discovered
  candidate before scanning, so a forged desktop command cannot approve an
  arbitrary external file.
- Preserve finding origins and core scan conflicts through the desktop review
  bridge, and bind conflict summaries into provider evidence approval.

## 0.2.13 - 2026-08-21 (not published)

- No artifacts were published. The unsigned release gate found the portable
  default-profile test issue corrected in 0.2.14 before signing began.

## 0.2.11 - 2026-08-09

- Use the Atlantis Rising title and description as the new-project example.
- Accept the renderer's valid Windows launcher path during staging and final
  readiness instead of mistaking the internal `\\?\` filesystem prefix for a
  different project root, and render macOS system path aliases from the same
  canonical root used by validation.
- Show the durable stage, checkpoint, staged-file count, and bounded, redacted
  failure details when an interrupted installation needs recovery.
- Keep development-preview release commands bound to the intended GitHub
  repository even though the publication job does not check out source.
- Pin the transitive Nano ID dependency to its patched release so the public
  security audit remains green.

## 0.2.10 - 2026-08-06

- Keep automatic new-project suggestions separate from recovery for an
  existing project.
- Keep managed tools, temporary setup files, and offline wiki pages out of the
  existing-project scan while still detecting an installed wiki.

## 0.2.9 - 2026-08-06

- Fix preparation failures when binary portrait assets are selected alongside
  conditional portrait guidance.
- Keep the flattened ChatGPT source checkbox at the same fixed size as other
  component checkboxes.

## 0.2.8 - 2026-08-05

- Build the Windows desktop executable as a GUI application so launching the
  installed app does not open a terminal window.
- Keep read-only existing-project scans focused on mod content by excluding
  known virtual environments, tooling caches, editor metadata, and generated
  artifacts.

## 0.2.7 - 2026-08-05

- Republish the desktop launch and ChatGPT source-export fixes with the
  portable console-helper correction required by the Linux release gate.

## 0.2.6 - 2026-08-05

- Keep Codex and other supervised desktop children inside the app on Windows so
  launching from a shortcut or Start menu does not open a terminal window.
- Allow Manage an existing project to package detected ChatGPT instructions,
  skills, subagents, and optional root Markdown without requiring an
  installation lock or a complete app-managed component set.
- Prevent long UI copy, statuses, paths, and action labels from being clipped
  or bleeding outside their buttons and panels, including narrow screens.

## 0.2.5 - 2026-08-05

- Adapt Super Events runtime filenames to the confirmed mod script prefix while
  retaining `hoi4ms_*` only as stable upstream source names; updates and repair
  remove unchanged legacy filenames and preserve modified ones for review.
- Remove the retired portrait upstream-lock component and refresh the bundled
  Agentic HOI4 Modding manifest evidence to the published `main` revision.

## 0.2.4 - 2026-08-04

- Automatically install and restart into a newer signed app version found at startup, with a retry path when installation fails.
- Add an existing-project ChatGPT source-package page with a Downloads default, required flattened source selection, optional root Markdown, safe atomic ZIP output, and no project mutation.
- Refresh the source-package documentation, sanitized screenshot fixture, and accessibility, security, transaction, testing, and release guidance.

## 0.2.3 - 2026-08-04

- Update the ComfyUI portrait integration to the current upstream 2.4.1 workflow package, including adaptive source framing, eight pinned model files, current workflow hashes, and the RunPod slim ComfyUI path.
- Show the optional portrait workflow's minimum requirement of 16 GB VRAM and 25 GB storage, and refresh the related user guidance, screenshots, and release notes.

## 0.2.2 - 2026-08-04

- Show live percentage and estimated time while preparing the installation review.
- Open maintenance plans in visible review, fix the Codex desktop opener, simplify Ready-screen optional states, and link the product repository and ChatGPT handoff correctly.

- Keep script prefixes and primary namespaces out of generated HOI4 descriptor files while retaining them for project guidance and later maintenance.
- Limit descriptor tags to official HOI4 categories and present them as editable choices.

### Fixed

- Made installation use the core-owned reviewed plan so provider analysis records are not serialized through the interface
- Show only recovery actions that can actually be used at the current transaction checkpoint
- Install the complete optional Super Events guidance and research package at its intended project paths
- Keep plan preparation visibly active and show journal-backed percentages and current files during installation
- Remove decorative information letters from status labels
- Replace quadratic full-journal rewrites with a bounded durable checkpoint log for much faster large installs
- Batch crash-safe install and rollback intents, accelerate rollback backup/apply, and show live recovery progress
- Prevent valid installations from falling into recovery because selected subagents omitted the required bounded-context spawn rule
- Align plan preparation with the review, contain recovery progress text, open integration requirements by default, and make the file-plan disclosure show real files
- Show percentage and estimated time remaining while generated mod details are prepared from the description
- Remove the source template's placeholder guide from installed and flattened project instructions
- Create starter folders directly without installing `.gitkeep` marker files
- Keep centered setup panels consistently wide, separate Meshy actions from the key field, and make Configure later advance without touching a stored key
- Keep file counts with installation progress and prevent optional 3D checks from freezing or crashing the app
- Remove operating-system details and internal planning notes from normal project guidance

### Changed

- Keep the standard skills and subagents free of Super Events guidance unless the workflow is selected
- Show an elapsed-rate estimate of the remaining installation time once enough progress is available

## 0.2.1 - 2026-08-01

### Fixed

- Restored Codex planning with the current Codex App Server read-only request format
- Made stalled Codex checks return to a retryable state instead of waiting for minutes

### Changed

- Claude, Kimi, GLM, and DeepSeek now fill their connection details automatically and ask only for an API key in the normal setup path

## 0.2.0 - 2026-08-01

### Added

- Signed in-app updates with a quiet launch check and user-approved install
- Optional Super Events workflow and repair support for previously prepared mods
- Codex-only flattened ChatGPT project sources in the Components step

### Changed

- Simplified user-facing text, component review, and application artwork
- Consolidated routine dependency updates into one pull request per ecosystem

## 0.1.1 - 2026-07-29

### Added

- Windows and macOS Tauri desktop wizard for new and existing HOI4 mods
- Codex App Server sign-in plus explicit Claude, Kimi, GLM, DeepSeek, local,
  and custom provider profiles
- Automatic editable identity, descriptor, namespace, tag, and folder values
  from a mod name and natural-language description
- Selective source-manifest installation, offline wiki, repair, reinstall,
  rollback, managed removal, and interrupted-transaction recovery
- Git initialize/preserve/skip, separately approved online push and public
  GitHub creation, and optional Codex-only flattened ChatGPT sources
- Windows Credential Manager and macOS Keychain integration
- Windows and macOS native build, package, launch-smoke, and release workflows

### Changed

- The interface now uses one clean app surface, larger controls, plain titles,
  and simpler user-facing language
- The portrait setup placeholder was removed; successful setup links to the
  separate ComfyUI HOI4 Portraits project
- GitHub Releases publish three clearly named installer files

### Security

- Existing projects are scanned read-only before planning
- Downloads and transactions are bound to reviewed source and operation
  evidence before backup or apply
- Codex, Git, source-cache, process, credential, and rollback boundaries fail
  closed on unreviewed or changed state
