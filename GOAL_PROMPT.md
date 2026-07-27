# HOI4 Mod Setup goal prompt

Build **HOI4 Mod Setup**, an open-source Windows and macOS desktop wizard that creates launcher-ready Hearts of Iron IV mods and prepares new or existing projects for Codex development.

Read all supplied project instructions and references. Keep `README.md` user-facing.

All AI work must use the user's Codex subscription through ChatGPT sign-in via the local Codex app-server. Implement managed login, logout, and process supervision. Codex owns authentication. Never request an OpenAI API key, copy the auth cache, bill through the application, or switch providers.

Use Codex for natural-language interpretation, identity, namespaces, descriptor metadata, project profile, component selection, existing-project analysis, and AGENTS adaptation. Run analysis read-only with approved evidence. Require schema-valid output labelled Detected, Suggested by Codex, or Confirmed. Suggestions cannot write files or enter an installation plan before confirmation.

Create the project deterministically after confirmation. Generate and validate `<mod_project>/descriptor.mod`, `<HOI4 user mod directory>/<project_id>.mod`, a replaceable `<mod_project>/thumbnail.png`, the selected folder profile, a mod README, and selected workflow files. Preview these artifacts. Never fabricate a Workshop ID or silently overwrite a replaced thumbnail.

Use `https://github.com/klimPaskov/Agentic-HOI4-Modding` through a versioned manifest. Never clone it, require a checkout, or search for one. Latest mode resolves an exact commit. Pinned mode uses an immutable revision. Selectively download and SHA-256 verify every selected component and the offline wiki. Install the wiki at `<mod_project>/paradox_wiki/`. Do not invent dependencies, commands, support, provenance, or licensing.

Existing projects receive a bounded read-only scan of descriptors, launcher state, thumbnail, structure, Git, identifiers, naming, localisation, workflow files, Codex, MCP, paths, and conflicts. Send only approved text evidence to Codex and review findings in small groups.

Ask exactly **Do you want to set up the 3D models workflow?** Store the Meshy key in the OS vault, expose it only as `MESHY_API_KEY`, derive requirements from verified repository files, and keep a missing key non-blocking. Do not invent a macOS route.

Ask exactly **Do you want to set up LoRAs and ComfyUI for portrait generation?** Version 1 records interest only and installs nothing.

Never overwrite modified files silently. Compare base, local, and incoming versions. Offer keep, replace, merge, rename, or skip where valid. Use the full 12-stage journaled, staged, validated, reversible transaction. Recover from interruption and write the lock only after final verification.

Support update, repair, reinstall, rollback, managed removal, Codex reanalysis, Git initialize or preserve, `.gitignore` merge, branch choice, optional initial commit, and optional remote. Never create an online repository or push without separate approval.

Readiness verifies authenticated Codex, launcher artifacts, confirmed identity, structure, workflows, MCP, wiki, Git, hashes, conflicts, dependencies, and optional workflow states. Enable Open in Codex only when core checks pass. Recovery and rollback remain available while signed out.

Use the minimal dark seven-phase UI with compact authentication, one focal task per screen, keyboard navigation, WCAG 2.2 AA, reduced motion, and 200 percent scaling.

Implement a Rust core behind Tauri with a React TypeScript UI. Add app-server contract tests, migrations, property tests, fuzzing, fault injection, security, accessibility, and platform end-to-end coverage. Maintain the public GitHub repository, root AGENTS, living skills, and bounded subagents. Satisfy every acceptance criterion. Do not claim completion with unresolved authentication, launcher, recovery, platform, security, docs, or skill work.
