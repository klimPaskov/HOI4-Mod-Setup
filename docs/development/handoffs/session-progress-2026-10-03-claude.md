# Session progress: Claude default, live native runs, and fixes

Date: 2026-10-03. Continues `session-continuation-2026-10-03.md`. This is a work-in-progress record, not a release approval.

## Claude account route (user requirement: easy Claude login, default, Haiku 4.5)

- Implemented as provider `claude_account` in `src-tauri/src/claude_code.rs`, the default setup assistant, with `claude-haiku-4-5-20251001` as the default model.
- Policy basis: Anthropic's Claude Code legal and compliance page (`https://code.claude.com/docs/en/legal-and-compliance`) forbids third-party Claude.ai login and credential intermediation but permits an end user signing in to the unmodified Claude Code binary with their own subscription. The app therefore runs the user's own Anthropic-signed `claude` for `auth status`, `auth login --claudeai` (closed stdin), `auth logout`, and one isolated print-mode analysis turn.
- The Anthropic API-key route remains as `Claude API key` and also defaults to Haiku 4.5. Codex remains first-class.
- Live evidence on this Windows machine with Anthropic-signed Claude Code 2.1.286 on PATH: discovery, Authenticode `Anthropic, PBC`, isolation-flag probe, signed-out status, and real sign-in start plus cancellation (1.5 s) pass through `pnpm test:claude-live`. The native app shows the signed-out panel, the not-installed panel with the official setup link, and the correct footer prompts.
- Not yet proven: a signed-in Claude analysis turn and a full Claude create flow. They need the user to sign in to Claude Code (`claude auth login`); the agent never enters credentials.
- macOS: Developer ID `Anthropic PBC (Q6L2SF6YDW)` comes from public issue reports of `codesign -dv` output and is unverified on a Mac.

## Live native findings and fixes

All found by driving the rebuilt native app (real Rust backend, real Codex) through WebView2 remote debugging.

| Finding | Fix | Regression evidence |
| --- | --- | --- |
| Codex planning failed as "temporarily unavailable": current Codex streams more deltas than the correlated notification limit | deltas counted, not retained | `a_long_streamed_turn_completes_without_hitting_the_retained_limit`; live Codex analysis 39-58 s, 10 proposals |
| The typed mod name was replaced by the AI and never sent | `requested_mod_name` constraint plus core enforcement | `a_requested_new_project_name_is_kept_in_the_display_name_proposal`; live run kept the name |
| Editing any suggestion cleared the record; Confirm silently did nothing | record kept with confirmation cleared; explicit error otherwise | App test `keeps an edited suggestion confirmable...`; live confirm after edit |
| Renaming regenerated AI tags heuristically | tags kept when an analysis exists | same App test |
| Identity screen duplicated reviewed fields | form hides them when a review is shown | live screen |
| Supported version default `1.17.*` (installed game 1.19.2) | `1.19.*` | live descriptor preview |
| Windows directory flush denied via `ReOpenFile`; plan preparation failed | identity-checked path handle flush | `retained_directory_handle_flushes_after_a_rename`; live plan |
| Raw internal error text on the dry run | `planning_command_error` maps every category | `codex_analysis_error_categories_remain_actionable_and_sanitized` |
| Upstream wiki image is an unresolved Git LFS pointer (object 404); installs failed at validation | early `verify_download` rejection with a clear message; source fix prepared upstream | `verified_download_rejects_an_unresolved_git_lfs_pointer`; live plan stops in about 6 s |
| Codex check took about 65 s (Authenticode of a 312 MB binary up to three times) | per-run publisher memo keyed by content hash; startup warm-up | `a_verified_publisher_is_remembered_only_for_the_same_content`; live check 6-11 s |
| Plan preparation took about 4 minutes (1,236 sequential downloads) | bounded parallel prefetch into the verified cache | `prefetch_reuses_verified_cache_entries_and_tolerates_duplicates` |
| Dry run needed an extra Prepare click; new-project copy said "Keeps your existing edits" | auto-prepare once per visit; mode-specific copy | live dry run |
| Descriptor parser rejected multi-line `tags`, repeated `replace_path`, comments (Chaos Redux and most real mods) | parser accepts them; bounded blocks | `real_world_descriptors_with_multiline_blocks_and_repeated_paths_parse`; live import of a legacy fixture |
| Scan findings showed raw keys and JSON | readable labels and summaries (`src/findings.ts`) | `src/findings.test.ts`; live findings screen |
| Import review sent the Create example description (summary invented an Atlantis theme) | neutral review brief | App test `never sends the Create example description...`; live summary |
| Import review intermittently rejected (a proposal cited no evidence) | clearer prompt plus one corrective retry; validator unchanged | `analysis_without_schema_constrained_output_is_rejected`; live review passes |
| Import plan always failed: renderer record lost the core scan ID | scan ID and root restored from the core session | `import_record_from_the_renderer_regains_its_core_scan_binding`; live plan passes the binding |
| Portrait local roots and URLs were sent as AI evidence | local-only finding, refused by the core | `local_only_portrait_routes_are_never_approved_as_semantic_evidence` |
| Skill frontmatter checked by substring | bounded parser | `skill_frontmatter_check_accepts_valid_variants_and_rejects_malformed_blocks` |
| MCP screen opened technical details by default; "1 files" | collapsed disclosure; pluralization | App test |

## Transaction quarantine

A background agent implemented displaced-leaf quarantine (`mutate_live_leaf`), no-replace placement, rollback restoration, journal fields `quarantine_leaf` and `quarantine_sha256`, and ten fault tests. See `public-readiness-transaction-quarantine.md`. Unix paths were only type-checked; macOS validation remains required.

## Blocked on user approval

1. Push `fix/restore-lfs-wiki-image` (local commit `7b7d2a4` in worktree `C:\Users\klimp\Documents\Projects\agentic-wiki-fix`) to `klimPaskov/Agentic-HOI4-Modding`. Until then every installation that includes the offline wiki stops at plan preparation, including from the public v0.3.5 release.
2. Publish an immutable `hoi4-agent-tools` release with a reproducible production closure (see `public-readiness-mcp-reproducibility.md`). The local `hoi4-agent-tools` checkout contains another session's uncommitted work.
3. Dependency PRs: #75 is superseded by the candidate (Vitest 4.1.11 already included). #77 needs a curated update (breaking `sha2`/`png` APIs and an MSRV above the pinned toolchain). #78 has frontend and desktop E2E failures. `cargo-audit` fails on both and must be investigated against the final lockfile.
4. Candidate PR, version bump, tag, and signed release.

## Test artifacts

- Disposable legacy fixture: `C:\Users\klimp\Documents\Projects\hoi4-test-mods` (synthetic; safe to delete after testing).
- No test mod has been written into the real HOI4 mod folder: the only create attempt stopped before project apply because of the LFS pointer, and its staging and journal live in app data as an interrupted, pre-apply transaction (`9043b5bf-...`) that recovery can discard.
