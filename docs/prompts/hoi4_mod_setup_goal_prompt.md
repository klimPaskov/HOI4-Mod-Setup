# HOI4 Mod Setup goal prompt

Build **HOI4 Mod Setup**, an open-source Windows and macOS desktop wizard that
creates launcher-ready Hearts of Iron IV mods and prepares projects for agentic
development. Read supplied instructions, planning documents, schemas, examples,
UI references and skills. Keep `README.md` user-facing.

Codex is the default. Let the user choose Codex, Claude, Kimi, GLM, DeepSeek, a
local model, or another provider. Codex uses the official local
codex app-server and ChatGPT authentication; others use a verified
user endpoint and OS-vault credential when required, with local models
loopback-only. Never invent OAuth routes, endpoints, models, packages,
commands, MCP servers, or platform support, or put keys/tokens in artifacts.

Optimize prompt, adapted `AGENTS.md`, README, and maintenance to the selected
provider. Analysis is read-only with approved evidence; every provider returns
the same schema-validated proposals. Suggestions cannot write or enter a plan;
Rust validates and renders final bytes.

For new projects collect a natural-language description and review identity,
descriptors, structure, components, workflows, Git, dry run, install, and
readiness. Generate descriptor.mod and other, a replaceable `thumbnail.png`, profile,
README, and workflows; never fabricate a Workshop ID or overwrite a replaced
thumbnail. Existing projects get a bounded read-only scan of
descriptors, launcher state, structure, Git, identifiers, docs, skills,
subagents, Codex/MCP, paths, and conflicts; findings have evidence/confidence
and remain editable.

Use the live workflow source only through its versioned remote manifest. Never
clone, require a checkout, or search for it. Latest resolves one exact commit;
pinned mode uses a commit or release. Selectively fetch and SHA-256 verify
declared files and the offline wiki at
`<mod_project>/paradox_wiki/`. Never invent provenance, dependencies, commands,
packages, servers, or support.

When Codex is selected, offer a final optional checkbox for
`<mod_project>/chatgpt_project_sources/`. Flatten
`.agents/skills/<skill>/SKILL.md` to `<skill>.md`; include selected subagents,
adapted AGENTS, README, and user-specified project-relative extras. Use normal
transaction/recovery. Recommend starting planning with ChatGPT "Chat" only;
never upload or start planning.

Ask exactly **Do you want to set up the 3D models workflow?** Store Meshy in
Windows Credential Manager or macOS Keychain, inject only `MESHY_API_KEY`,
derive checks from verified files, and keep a missing key non-blocking. Do not
invent a macOS route. Ask exactly **Do you want to set up LoRAs and ComfyUI for
portrait generation?** Version 1 records interest only and installs nothing.

Never overwrite modified files silently. Compare base/local/incoming and offer
keep, replace, merge, rename, or skip where valid. Use the ordered 12-stage
transaction (preflight, exact source, selective fetch/checksum, dry-run,
backup, staging, validation, apply, post-checks, readiness, rollback), persist
a journal, recover interruptions, and lock only after verification. Support
maintenance, provider reanalysis, Git modes, branch, optional commit/remote;
never create an online repository or push without approval.

Readiness verifies the selected provider or Codex authentication, descriptors,
structure, AGENTS, skills, subagents, Codex/MCP, wiki, Git, environment names,
hashes, conflicts, dependencies, 3D, and LoRA. Open in Codex is Codex-only and
needs all core checks. Use the minimal dark seven-phase UI, progressive
disclosure, keyboard access, WCAG 2.2 AA, reduced motion, and 200% scaling.

Add migrations, unit/property tests, fuzzing, fault injection, security,
accessibility, and Windows/macOS end-to-end coverage. Maintain the GitHub-ready
repository, root AGENTS, living skills, and bounded subagents with
`fork_context=false`; do not claim completion while required route, recovery,
security, platform, document, or skill work remains unresolved.
