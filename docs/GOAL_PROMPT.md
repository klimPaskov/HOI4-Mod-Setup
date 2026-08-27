# HOI4 Mod Setup goal prompt

Build **HOI4 Mod Setup**, a Windows and macOS desktop wizard that
creates launcher-ready Hearts of Iron IV mods and prepares agentic projects.

Codex is default. Choose Codex, Claude, Kimi, GLM, DeepSeek, local, or custom.
Codex uses the local app-server with ChatGPT auth; hosted routes use a verified
endpoint and OS-vault key, while local is loopback-only. Never invent provider
routes, models, packages, commands, MCP servers, platform support, or credentials.

Use the selected provider only as the setup assistant for read-only
semantic analysis. It may propose adaptations, but generated
`AGENTS.md`, README, installed components, MCP tooling, and later development
client choices stay provider-neutral. Suggestions cannot write or enter a plan;
Rust validates and renders final bytes.

For new projects collect a brief, review provider proposals, identity/paths,
descriptors, structure, components, Git, install, and readiness. Auto-fill
editable root/launcher paths and generate `descriptor.mod`, launcher
descriptor, replaceable `thumbnail.png`, profile, README, and workflows; never
fabricate Workshop IDs or overwrite thumbnails. Existing projects confirm a
bounded launcher candidate,
then receive a targeted read-only scan of descriptors, agentic setup files,
Git, and conflicts; gameplay, localisation, media, and data dumps are excluded.

Use the live source only through its canonical manifest: never clone, search, or
require a checkout. Latest resolves one commit; pinned uses a commit/release.
Fetch declared files/wiki, verify SHA-256, and never invent provenance,
dependencies, commands, packages, servers, or support.

Independently of the setup assistant, offer an optional Components checkbox for
`<mod_project>/chatgpt_project_sources/`, flattening skills to `<skill>.md` and
including selected subagents, adapted AGENTS, and README.
Use transaction/recovery; recommend ChatGPT "Chat" only, never upload or
start planning. Existing scans finding agent sources offer an external ZIP
without mutating the mod.

Show **3D models workflow**, then **Super Events workflow**. The optional
`workflow.super_events` installs only `.agents/skills/hoi4-super-events/`; it is
credential-free, non-blocking, and remembered. Unselected installs receive no
guidance; Update may add it, Repair only from the locked revision. Store Meshy
in Windows Credential Manager or macOS Keychain, inject only `MESHY_API_KEY`,
keep a missing key non-blocking, and do not invent a macOS route. Then offer
optional ComfyUI portrait production (Cloud, Local, RunPod, or Disabled),
persist the choice, and strip disabled guidance. See
`docs/32_comfyui_portrait_pipeline.md`.

Never silently overwrite modified files: compare base/local/incoming and offer
keep/replace/merge/rename/skip. Use the 12 stages (preflight, source,
selective fetch/checksum, dry-run, backup, staging, validation, apply, post-
checks, readiness, rollback), persist a journal, recover, and lock after
verification. Support maintenance and Git review; never create or push online
repositories without approval.

Readiness verifies setup-analysis provenance, descriptors, structure, guidance,
skills/subagents, Codex/MCP, wiki, Git, names, hashes, conflicts, dependencies,
and optional workflows. Open in Codex follows the installed Codex integration,
not the setup assistant, after core checks. Use the seven-phase
UI, progressive disclosure, keyboard access, WCAG 2.2 AA, reduced motion, and
200% scaling.

Add migrations, unit/property, fuzz, fault, security, accessibility, performance,
and Windows/macOS E2E coverage. Tauri filesystem/network/Git/provider waits use
async thread-pool dispatch; test event-loop responsiveness. Maintain the repo,
AGENTS, living skills, and `fork_context=false` subagents. On startup, automatically
install and restart newer signed versions; failure preserves the app. Do not
claim completion while routes, recovery, security, platform, docs, or evidence remain.
