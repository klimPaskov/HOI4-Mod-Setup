# HOI4 Mod Setup goal prompt

Build **HOI4 Mod Setup**, a Windows and macOS desktop wizard that creates
launcher-ready Hearts of Iron IV mods and prepares agentic projects.

Claude is the default setup assistant. Choose Claude, Codex, Kimi, GLM,
DeepSeek, local, or custom. Claude uses the user's installed Claude Code;
Codex uses the local `codex app-server` with ChatGPT auth; hosted routes use verified
endpoints and OS-vault keys; local is loopback-only. Never invent provider
routes, models, packages, commands, MCP servers, platforms, or credentials.

Use the selected provider only for read-only semantic analysis. It may propose
adaptations, but canonical `CLAUDE.md`, generated `AGENTS.md`, README,
installed components, MCP tooling, and later development-client choices remain
provider-neutral. Suggestions cannot write or enter a plan; Rust validates and
renders final bytes.

For new projects collect a brief, review proposals, identity/paths, descriptors,
structure, components, Git, install, and readiness. Auto-fill editable paths and
generate `descriptor.mod`, the launcher descriptor, replaceable `thumbnail.png`,
profile, README, and
workflows; never fabricate Workshop IDs or overwrite thumbnails. Existing
projects confirm a bounded launcher candidate, then receive a read-only scan of
descriptors, agentic setup files, Git, and conflicts; gameplay, localisation,
media, and data dumps are excluded.

Use live source only through its canonical manifest: never clone, search, or
require a checkout. Latest resolves one commit; pinned uses a commit or release.
Fetch declared files/wiki, verify hashes, and never invent source metadata.

Independently offer a Components checkbox for
`<mod_project>/chatgpt_project_sources/`, flattening skills to `<skill>.md` and
including selected subagents, adapted AGENTS, and README. Use
transaction/recovery; recommend ChatGPT "Chat" only, never upload or start
planning. Existing scans finding agent sources offer an external ZIP without
mutating the mod.

Show **3D models workflow**, then **Super Events workflow**. The optional
`workflow.super_events` installs only `.claude/skills/hoi4-super-events/`; it is
credential-free, non-blocking, and remembered. Unselected installs receive no
guidance; Update may add it, Repair only from the locked revision. Store Meshy
in the OS credential vault, inject only `MESHY_API_KEY`, keep a missing key
non-blocking, and do not invent a macOS route. Then offer optional ComfyUI
portrait production (Cloud, Local, RunPod, or Disabled); see
`docs/32_comfyui_portrait_pipeline.md`.

Never silently overwrite modified files: compare base/local/incoming and offer
keep/replace/merge/rename/skip. Use all 12 transaction stages, persist a journal,
recover, and lock after verification. Support maintenance and Git review; never
create or push online repositories without approval.

Coding Environments is independent of setup assistance and workflow profiles.
New installs use the manifest default (currently Claude Code) and may add other
supported clients; valid existing locks, including older Codex primaries, are
preserved. `CLAUDE.md`, `.claude/skills/`, and `.claude/agents/` are canonical;
other client files are generated projections.

Readiness verifies setup-analysis provenance, descriptors, guidance,
skills/subagents, Codex/MCP, wiki, Git, hashes, conflicts, dependencies, and
optional workflows. Selected-client opening follows its integration, not the
setup assistant, after core checks. Use the seven-phase UI, progressive
disclosure, keyboard access, WCAG 2.2 AA, reduced motion, and 200% scaling.

Test migrations, unit/property, fuzz, fault, security, accessibility,
performance, and Windows/macOS E2E. Tauri waits use async dispatch; test
responsiveness. Maintain AGENTS, living skills, and `fork_context=false`
subagents. Auto-install newer signed versions; failure preserves the app. Do
not claim completion while routes, recovery, security, platform, docs, or
evidence remain.
