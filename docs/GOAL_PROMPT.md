# HOI4 Mod Setup goal prompt

Build **HOI4 Mod Setup**, an open-source Windows and macOS desktop wizard that
creates launcher-ready Hearts of Iron IV mods and prepares agentic projects.
Read the supplied package; keep `README.md` user-facing.

Codex is default. Choose Codex, Claude, Kimi, GLM, DeepSeek, local, or custom.
Codex uses the official local app-server with ChatGPT auth; other hosted routes
use a verified endpoint and OS-vault key, while local is loopback-only. Never
invent OAuth routes, endpoints, models, packages, commands, MCP servers, platform
support, or artifact credentials.

Adapt prompt, `AGENTS.md`, README, and maintenance to the selected provider.
Analysis uses approved read-only evidence and schema; suggestions cannot
write or enter a plan. Rust validates and renders final bytes.

For new projects collect a brief, review provider proposals, identity/paths,
descriptors, structure, components, Git, install, and readiness. Auto-fill root
and launcher paths from the project ID with editable overrides. Generate
`descriptor.mod`, replaceable `thumbnail.png`, profile, README, and workflows; never
fabricate a Workshop ID or silently overwrite a replaced thumbnail. Existing
projects confirm a bounded parent launcher candidate, then receive a read-only
evidence/conflict scan of descriptors, launcher, structure, Git, identifiers,
docs, skills, subagents, Codex/MCP, and paths.

Use the live source only through its versioned manifest. Never clone, require a
checkout, or search. Latest resolves one commit; pinned uses a commit/release.
Fetch only declared files and wiki, verify SHA-256, and never invent provenance,
dependencies, commands, packages, servers, or support.

When Codex is selected, offer an optional Components checkbox for
`<mod_project>/chatgpt_project_sources/`. Flatten each skill to `<skill>.md`;
include selected subagents, adapted AGENTS, and README.
Use normal transaction/recovery; recommend ChatGPT "Chat" only, never upload or
start planning.

Ask exactly **Do you want to set up the 3D models workflow?**, then immediately
**Do you want to set up the Super Events workflow?** The optional
`workflow.super_events` selects only the verified manifest tree at
`.agents/skills/hoi4-super-events/`; it has no credential, is non-blocking, and
its state is remembered in the lock and scan. Unselected installs receive no
Super Events `AGENTS.md` guidance. Update may add it; Repair
may add it only from the same locked revision, otherwise use Update. Store Meshy
in Windows Credential Manager or macOS Keychain, inject only `MESHY_API_KEY`,
keep a missing key non-blocking, and do not invent a macOS route. LoRA/ComfyUI
is not setup state; after success, link to
`https://github.com/klimPaskov/comfyui-hoi4-portraits`.

Never silently overwrite modified files: compare base/local/incoming and offer
keep/replace/merge/rename/skip. Use the 12 stages (preflight, exact source,
selective fetch/checksum, dry-run, backup, staging, validation, apply, post-
checks, readiness, rollback), persist a journal, recover, and lock after
verification. Support maintenance and Git review; never create or push online
repositories without approval.

Readiness verifies provider/auth, descriptors, structure, AGENTS, skills,
subagents, Codex/MCP, wiki, Git, names, hashes, conflicts, dependencies, and
3D/Super Events. Open in Codex is Codex-only after core checks. Use the minimal
seven-phase UI, progressive disclosure, keyboard access, WCAG 2.2 AA, reduced
motion, and 200% scaling.

Add migrations, unit/property, fuzz, fault, security, accessibility, performance,
and Windows/macOS end-to-end coverage. Every Tauri command dispatches
filesystem/network/Git/provider waits asynchronously to a thread pool; regression
test that the event loop remains responsive. Maintain the repo, root AGENTS,
living skills, and `fork_context=false` subagents; do not claim completion while
routes, recovery, security, platform, docs, or evidence remain.
