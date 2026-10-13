# HOI4 Mod Setup goal prompt

Build **HOI4 Mod Setup**, a Windows, macOS and Linux desktop wizard that
creates launcher-ready Hearts of Iron IV mods and prepares provider-neutral
agentic projects.

Claude is the default assistant: user's Claude Code with Haiku 5.5
at medium effort. Keep legacy Haiku 4.5 and the bounded Advanced exact-ID path.
Also support Codex app-server, Gemini/Grok API keys, Kimi, GLM, DeepSeek, local loopback,
and custom HTTPS routes. Experimental account routes use official CLIs
in private homes and never ask users to paste credentials. Do not invent
providers, models, endpoints, commands, packages, MCP servers, platforms, or
credentials.

Providers only analyze read-only inputs. Rust owns facts, validation,
schema checks, evidence, rendering, transactions, and readiness. Suggestions
cannot write or enter a plan; generated `CLAUDE.md`, `AGENTS.md`, README,
components, MCP tooling, and later coding clients remain provider-neutral. The
runtime uses the authoritative
`docs/schemas/codex-analysis.schema.json`, assigns the analysis UUID, accepts
empty evidence references only for genuinely inferred suggestions, rejects
unknown references, and retries only completed correctable serialization or
value errors. Private, refused, truncated, transport, auth, and usage failures
preserve the draft and do not retry.

For a new project, collect a brief, review proposals, identity, descriptors,
structure, components, Git, installation, and readiness. Generate both
`descriptor.mod`, launcher descriptor, valid thumbnail.png, profile folders, README,
selected workflows. Never fabricate Workshop IDs or overwrite a thumbnail.
Existing projects get a bounded launcher confirmation and read-only scan of
descriptors, setup files, Git, and conflicts; exclude gameplay, localisation,
media, and data dumps.

Use only the canonical `hoi4-mod-setup.manifest.json`; never clone or search
source repository. Latest resolves one exact commit; pinned modes use an
exact commit or release. A rate-limit-only GitHub 403/429 may use fixed-repo
read-only ref advertisement for names and commit IDs only. A bounded 15-minute
cache is identity-bound. Install only manifest `expected_files` at that SHA,
verify sizes and hashes, and retain source evidence in the plan and lock. See
`docs/04_remote_repository_manifest.md`.

Keep ChatGPT source flattening and coding-environment selection independent of
setup assistance; flattening never uploads or starts planning. Canonical Claude
files project to other clients. Selecting an image provider selects its route
component and removes stale routes; Rust owns the closure. Offer 3D, Super
Events, portrait production (`docs/32_comfyui_portrait_pipeline.md`), and
manifest-declared HOI4 Live Tools. Optional failures stay non-blocking and
unsupported routes remain honest.

Every mutation uses all 12 transaction stages, backup, staging validation,
journaling, recovery, rollback, and hash-aware conflict handling. Never replace
modified files silently. Keep credentials in OS-vault or provider-owned
isolated storage, never in projects, plans, locks, or logs.

Readiness checks provenance, descriptors, clients, MCP, wiki, Git,
hashes, dependencies, conflicts, and optional workflows. Use the seven phases,
progressive disclosure, keyboard access, WCAG 2.2 AA, reduced motion, and 200%
scaling. Test migrations, schema/property behavior, fuzzing, fault injection,
security, accessibility, performance, and Windows/macOS/Linux E2E, including
source403 fallback, structured outputs, account-CLI isolation, image routes,
and Live Tools.

Maintain `AGENTS.md`, repo-local skills, and `fork_context=false` subagents.
Auto-install signed versions while preserving the running app on failure.
Do not claim completion while accepted behavior, recovery, security, platform,
documentation, or evidence remains incomplete. Provider details live in
`docs/31_ai_provider_profiles_and_chat_sources.md` and
`docs/34_provider_account_cli_architecture.md`.
