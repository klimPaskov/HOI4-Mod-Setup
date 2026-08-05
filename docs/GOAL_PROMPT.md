# HOI4 Mod Setup goal prompt

Build **HOI4 Mod Setup**, an open-source Windows and macOS desktop wizard that
creates launcher-ready Hearts of Iron IV mods and prepares agentic projects.
Keep `README.md` user-facing.

Codex is default. Choose Codex, Claude, Kimi, GLM, DeepSeek, local, or custom.
Codex uses the local app-server with ChatGPT auth; hosted routes use a verified
endpoint and OS-vault key, while local is loopback-only. Never invent provider
routes, models, packages, commands, MCP servers, platform support, or credentials.

Adapt prompt, `AGENTS.md`, README, and maintenance to the selected provider.
Analysis uses approved read-only schema evidence; suggestions cannot write or
enter a plan. Rust validates and renders final bytes.

For new projects collect a brief, review provider proposals, identity/paths,
descriptors, structure, components, Git, install, and readiness. Auto-fill
editable root/launcher paths and generate `descriptor.mod`, launcher
descriptor, replaceable `thumbnail.png`, profile, README, and workflows; never
fabricate Workshop IDs or overwrite thumbnails. Existing projects confirm a
bounded launcher candidate,
then receive a read-only evidence/conflict scan of descriptors, structure, Git,
identifiers, docs, skills, subagents, Codex/MCP, and paths.

Use the live source only through its versioned manifest: never clone, search, or
require a checkout. Latest resolves one commit; pinned uses a commit/release.
Fetch declared files/wiki, verify SHA-256, and never invent provenance,
dependencies, commands, packages, servers, or support.

When Codex is selected, offer an optional Components checkbox for
`<mod_project>/chatgpt_project_sources/`, flattening skills to `<skill>.md` and
including selected subagents, adapted AGENTS, and README.
Use transaction/recovery; recommend ChatGPT "Chat" only, never upload or
start planning. Existing scans finding AGENTS, a skill, or subagent offer a
source ZIP: Downloads is default, detected primary files are selected, root
Markdown is optional, and export never mutates the mod.

Show **3D models workflow**, then **Super Events workflow**. The optional
`workflow.super_events` installs only `.agents/skills/hoi4-super-events/`; it is
credential-free, non-blocking, and remembered. Unselected installs receive no
guidance; Update may add it, Repair only from the locked revision. Store Meshy
in Windows Credential Manager or macOS Keychain, inject only `MESHY_API_KEY`,
keep a missing key non-blocking, and do not invent a macOS route. Then offer
optional ComfyUI portrait production (Cloud, Local, RunPod, or Disabled),
persist the choice, register Cloud MCP when enabled, and strip guidance when
disabled. See
`docs/32_comfyui_portrait_pipeline.md`.

Never silently overwrite modified files: compare base/local/incoming and offer
keep/replace/merge/rename/skip. Use the 12 stages (preflight, source,
selective fetch/checksum, dry-run, backup, staging, validation, apply, post-
checks, readiness, rollback), persist a journal, recover, and lock after
verification. Support maintenance and Git review; never create or push online
repositories without approval.

Readiness verifies auth, descriptors, structure, guidance, skills/subagents,
Codex/MCP, wiki, Git, names, hashes, conflicts, dependencies, and optional
workflows. Open in Codex is Codex-only after core checks. Use the seven-phase
UI, progressive disclosure, keyboard access, WCAG 2.2 AA, reduced motion, and
200% scaling.

Add migrations, unit/property, fuzz, fault, security, accessibility, performance,
and Windows/macOS E2E coverage. Tauri filesystem/network/Git/provider waits use
async thread-pool dispatch; test event-loop responsiveness. Maintain the repo,
AGENTS, living skills, and `fork_context=false` subagents. On startup, automatically
install and restart newer signed versions; failure preserves the app and offers
retry. Do not claim completion while routes, recovery,
security, platform, docs, or evidence remain.
