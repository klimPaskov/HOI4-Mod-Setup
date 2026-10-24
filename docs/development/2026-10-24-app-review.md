# App review, 2026-10-24

A full review of the web and desktop apps, started from a first-time user's
report: "I'm trying to use the mod tool on an existing mod but it keeps on
telling me to undo the changes. It's my first time using it." Every result
below was observed in a running app or in a test; what could not be checked is
listed under [Not verified](#not-verified).

Representative screenshots are in [`review-2026-10-24/`](review-2026-10-24/).

## Tested environment

| | |
| --- | --- |
| OS | Windows 11 Pro 10.0.26200 (only platform available) |
| Desktop | Release build of this branch (`cargo build --release --features desktop,tauri/custom-protocol`, Tauri 2 on WebView2 / Edge 155.0.4283.62), driven over the WebView2 debug port with Playwright. A debug build and the Vite dev server were used for the first pass |
| Web | Local `build_web` output served on 127.0.0.1, Chromium via Playwright 1.62.1. The live page `https://klimpaskov.github.io/HOI4-Mod-Setup/app/` answers 200, and its JS, CSS and HTML bundles match a local build of this branch |
| Setup assistant | A loopback OpenAI-compatible planner (`scripts/released_e2e_support.mjs`) on the desktop; a CORS variant on the web. No real provider account was used |
| Setup source | `klimPaskov/Agentic-HOI4-Modding` Latest, resolved to `856940f2a629e0e2f3e03fc484cb0c0309c96734` for the final desktop runs |
| Profiles | Every desktop run used its own `APPDATA`, `LOCALAPPDATA`, and WebView2 data folder. Mods were copied and the originals never edited |

## The reported issue

**What the message is.** "Installation was interrupted" with **Undo changes** is
the Recovery screen (`src/App.tsx`, `Recovery`). The desktop app opens it
whenever a journal for the selected folder is not finished
(`find_incomplete_transaction`), both when the folder is chosen
and again before every install, so it returns each time the folder is opened
until **Undo changes** completes. The web edition has no such screen: it never
writes in place.

**What I could reproduce on real mods**, each with a screenshot or log:

1. **The app is closed or the computer sleeps during install** (the likeliest
   cause for a first-time user). A full install writes 800+ files and takes
   several minutes. Killing the app in the apply stage of the
   Great War Redux copy and of the Chaos Mod copy reproduced the screen
   exactly: "Installation was interrupted", only **Undo changes**, and no
   reason ([before](review-2026-10-24/desktop-recovery-before.png)). Undo is
   correct here, but the screen did not say what happened or what to do next.
2. **HOI4 Agent Tools cannot finish.** Its bootstrap needs a python.org Python on `PATH`.
   Without one it reports `incomplete`, readiness blocks the success lock, and
   the whole transaction fails after the files were written
   (`failed_mcp_bootstrap_cause_reaches_recovery_without_a_success_lock`
   documents this as intended). The first-run **Review changes** screen showed
   "Setup checks ✓ 3 included" and "ready to install" on a computer with no
   Python ([before](review-2026-10-24/desktop-review-before-no-python-check.png)).
   Every retry after **Undo changes** fails the same way. I confirmed the
   missing-Python condition in the app, and the code path in the Rust test; I
   did not drive a failed install end to end because the new check now stops
   it before any file is written.
3. **Smaller contributors found along the way**: an existing mod's own
   `descriptor.mod`, `thumbnail.png`, and `README.md` came up as "Resolve file
   conflicts" with a diff of the user's descriptor against a generated one
   ([before](review-2026-10-24/desktop-existing-mod-conflict-before.png)); a mod that already has a `.git`
   folder defaulted to **Initialize**, which planning refuses with "a Git
   repository already controls the project"; and Undo left an empty
   `.claude/skills/...` folder tree in the mod.

**Not reproduced**: OneDrive placeholder files (`FILE_ATTRIBUTE_REPARSE_POINT`
is treated as a link) may block mods kept in a OneDrive "Documents" folder. This
machine's OneDrive folder holds no placeholders, so it stays a hypothesis.

**Fixes**

- Review changes checks the tools the plan needs before anything is written
  (`check_installation_requirements`, shown as a requirement row with **Check again**), blocks
  **Start installation** while Python is missing, and `apply_installation`
  refuses with "Nothing was changed." if it is still missing
  ([after](review-2026-10-24/desktop-review-after-python-requirement.png)).
  HOI4 Agent Tools has no separate switch (it follows the coding-client
  choice), so the message says to install Python.
- The recovery screen shows **What happened** or **What stopped setup** and a
  next step without opening Details
  ([after](review-2026-10-24/desktop-recovery-after.png)), and the message after Undo says the mod is as it was.
- Undo also removes the folders an install created for new files and tolerates
  folders an interrupted apply never created. This was found by the Chaos Mod
  run: the first version of the cleanup failed on a folder that never existed, the
  files were restored but folders remained and Undo reported an error, and a
  second **Undo changes** then finished and restored 117/117 files and the
  identical folder tree.
- An existing mod's valid `descriptor.mod`, `thumbnail.png`, and `README.md` are kept by
  default ([after](review-2026-10-24/desktop-existing-mod-no-conflicts-after.png)); **Preserve** is the default for a mod that already has a repository.

Regression tests: `a_missing_python_is_reported_with_the_safe_next_step_before_install`,
`setup_requirements_cover_only_plans_that_run_the_mcp_bootstrap`,
`installing_without_a_required_tool_stops_before_any_change`,
`an_existing_mods_own_project_files_are_kept_by_default`,
`undo_removes_folders_that_only_existed_for_new_files_and_keeps_the_rest`
(fails without the fix), and the `App.test.tsx` cases for the requirement row, the
interrupted-install explanation, and the failed-tool explanation.

## Other defects found and fixed

| Area | Defect | Evidence | Fix |
| --- | --- | --- | --- |
| Reimport | Every installed `.claude/agents/*.md` was reported as a malformed subagent with a **blocking** conflict (the scan did not retain the bytes it validated) | 19 blocking conflicts on a fresh install | `scan_content_policy` retains native agent files; test `installed_claude_subagents_are_not_reported_as_malformed` |
| Update | Files that already held the incoming bytes (`CLAUDE.md`, `.mcp.json`, `.gitignore`) were conflicts at every update; their "Current" and "New" panes were identical | 5 conflicts on a no-source-change update | `update_operations` plans them as no-ops; test `an_update_does_not_review_or_rewrite_files_that_already_hold_the_incoming_bytes` |
| Update | An edit already kept was asked about again at every update | second update after "Keep current" | `keep_reviewed_edit` ([after](review-2026-10-24/desktop-repeat-update-no-conflicts.png)) |
| Merge | **Merge** refused any edit that changed the line count ("could not be proven safe"), including appending a line | `SKILL.md` merge failed | Line-based three-way merge in `crates/hoi4-setup-core/src/merge.rs`; two new tests |
| Update | After **Merge** the lock held the merged bytes as the installed baseline, so the next update planned **Update** for that file and would have replaced the merged edit with the source version (seen as the lone "Update 1" on a no-change update) | lock inspection on the Great War Redux project | Source stays the baseline, merged bytes recorded as a kept edit; test `a_merge_keeps_the_source_as_baseline_and_records_the_users_edit`. Locks written before this fix keep the old baseline |
| Readiness | A kept edit to a managed file (`AGENTS.md`) made `hashes.managed`, `source.revision`, and `conflicts.resolved` block, so the project read "Needs review" after an update that kept it (probe: `core_ready=false`) | `time_installed_readiness` probe | Reviewed, still-intact edits warn instead of blocking; changed again, they block; test `a_kept_edit_to_a_managed_file_does_not_block_readiness_until_it_changes_again` |
| Repair | **Repair** asked again about an edit the user had already kept | one conflict (`AGENTS.md`) on Repair | `keep_reviewed_edit` in `repair_operations`; extended repair test |
| Update | Changed assistant address gave "the confirmed review no longer matches this session" | diagnostics log | Specific message; test `a_changed_assistant_address_is_named_in_the_planning_message` |
| Context | A reimport profiled the setup's own `README.md` and `docs/` as the mod's documentation | "Documentation conventions: README.md, docs/systems/…" | Unmodified lock-owned files are skipped; core and scanner tests |
| Defaults | An unmanaged existing mod defaulted to Codex while a new mod defaults to Claude Code | Coding environments screen | Scan only reads a primary from a recorded installation |
| Progress | Plan preparation sat at "96%, about 1 seconds remaining" for minutes | screenshot, 4 min on a first install | Says "still working, a first install can take a few minutes"; install screen asks to keep the window open |
| Performance | Verifying the installed 5,387-file, 283 MB HOI4 Agent Tools package took 140 s for the read alone on a release build, so **Ready** showed "Checking readiness" for 151 s | timing probe `time_package_tree_materialization` | Files are read on several threads. Measured in the timing probe: read 140 s → 14 s, whole copy-and-verify 235 s → 88 s. The Ready screen's 151 s was not re-measured end to end after the change; the screen now says it can take a minute or two. The private copy that follows is unchanged and still the larger part (see [Limitations](#limitations)) |
| First run | "invalid input: AI model must be non-empty" appeared before a local model's details were entered | screenshot | Hidden until details are entered |
| Visual | The Python requirement text sat flush against the panel edge; the recovery cause box merged with the overview panel | screenshots | Padding and placement |

The web edition's review is in [Web edition](#web-edition). Its fixes are in
`src/web/**` and `crates/hoi4-setup-web/**`.

## Real-mod lifecycle

| Mod | Source | Size | Used for |
| --- | --- | --- | --- |
| Great War Redux: Chemical Warfare | Steam Workshop `2924972770`, `descriptor.mod` version 1.2, supported 1.12.*, `remote_file_id`, no `picture` line | 10 files | Full desktop lifecycle (below), interrupted install, no-Python run; web import and ZIP round trip |
| Random Events Mod | Workshop `3199436992`, repository head `baad25a` (2024-07-20) | 830 files, `.git`, README | Plan, full install, Ready on desktop; web import and export |
| Chaos Mod | Workshop `3249341121`, repository head `d261ee3` (2025-12-04) | 468 files, `.git`, `.vscode`, `.gitignore` | Interrupted install and exact Undo on desktop; web import |

Synthetic edits supplemented the real revisions: the mods have no newer
public revision to import, so source changes were made on the copy.

**Great War Redux, desktop (release build, Python on `PATH`)**

| Step | Result |
| --- | --- |
| 1 Initial import | Scan: 10 files, name, tags, supported version, thumbnail detected; coding environment default Claude Code (after fix). Plan: 843 create, 2 skip (descriptor, thumbnail kept), 0 conflicts |
| 2 Install | 845 files written, HOI4 Agent Tools bootstrapped and verified, lock written. All 10 original files byte-identical |
| 3 Persistence | Restarted the app; **Manage an existing project** found "Installed by HOI4 Mod Setup" and offered **Repair or add workflows** |
| 4–5 Source and external edits | Added `events/gwr_chem_events.txt`, edited `common/combat_tactics.txt`, renamed one `.dds`, deleted one, appended to the managed `AGENTS.md` and a skill |
| 6 Update | After the fixes: 2 conflicts, exactly the two managed files edited. **Merge** of the skill succeeded; `AGENTS.md` kept. After Apply, only `install.lock.json` changed on disk; all mod edits intact; the `AGENTS.md` edit recorded in `local_modifications` |
| 7 Repeat update | Planned again with nothing new. Before the last fix the edit to `AGENTS.md` was asked about again; after it the plan went straight to Review changes with 0 conflicts (0 create, 1 update, 844 skip). That plan was not applied |
| 8 Further cycle | Not run beyond planning: only one update was applied, because each apply takes minutes and the second and third plans were identical in structure |
| 9 Downstream | The reimport scan after the edits reported `gwr_chem` as the namespace and prefix from the new events file, counted the added, renamed and deleted files, and profiled documentation without the setup's own `README.md` and `docs/` |
| 10 Recovery | Interrupted install killed in the apply stage → Recovery → Undo → 10/10 files identical. Chaos Mod: same, plus a half-finished Undo that resumed and restored 117/117 files and the identical folder tree |

Round trip on the web: an overlay ZIP unpacked over each copy preserved every
original byte (0 changed, 0 removed) and a re-drop was recognised as an existing
setup. Export and re-open on the desktop was done by the desktop run itself, which writes in place.

## Web edition

Reviewed by a separate pass in Chromium against a local build; details were
condensed from its report. Reading a folder is read-only, and the ZIP is the only output.

| Area | Result |
| --- | --- |
| Existing-mod import of all three mods | OK; `.git` skipped; descriptor, README, thumbnail now kept by default and the descriptor row warns about `remote_file_id` and `replace_path`. "Use the setup's version" had replaced the Workshop descriptor with `version="0.1.0"` and no `remote_file_id` (verified in the ZIP) |
| Overlay ZIP | OK, originals byte-identical, lock accurate. Full-copy ZIP **not run** |
| Reimport / update / repair | First update showed spurious `AGENTS.md` and `CLAUDE.md` conflicts (state file only written for new mods); fixed, 0 conflicts on a fresh round trip and on an old-format ZIP. Edit/delete/add then update: 2 restored, 3 kept, 826 unchanged; a repeat gives the same review |
| Merge preview | Core refuses line-count changes; now shares the new merge |
| Failures | 429, 500, 401, refusal, truncated and garbage replies, offline and rate-limited GitHub: readable messages, retry works; secrets in `.env`, README and notes never reach the assistant (12 needles) |
| Folder handling | Unicode and space names OK; unreadable file named; dropping a file refused; over 2 GB or 200,000 files refused; reading 36,000 files 359 s → 119 s; Kaiserreich (36,000 files, 1.09 GB) about 538 s → 165 s |
| Layout | Phone: 300 px gap and clipped step rail fixed ([before](review-2026-10-24/web-mobile-before.png), [after](review-2026-10-24/web-mobile-after.png)); focus ring added; dark only (no light theme) |
| Links | All external links resolve except two (`console.x.ai`, `platform.deepseek.com`) that answer 403 to curl, and an Ollama docs link that was 404 and is replaced |

## Desktop feature inventory

Windows, release build unless noted. "Fixture" means the screen was captured
from the documentation fixture and covered by unit tests, not driven live.

| Screen or feature | Expected | Result |
| --- | --- | --- |
| Start: provider list, Create / Import / Manage | Choose assistant and path | Verified live. Local model connected; Claude showed "Checking Claude Code…" because it is not installed; other providers need accounts (not exercised) |
| Description and wand (new mod) | Draft and expand | Fixture and unit tests only; no new mod was created live |
| Project identity (existing) | Folder field, Browse, launcher candidate | Folder field verified; the native **Browse** dialog cannot be driven over the debug port |
| Scan | Read-only, bounded, progress | Verified; cancel not exercised |
| Findings and assistant review | Detected values, suggestions, confirm | Verified; fixed the agent false positives and documentation self-reference |
| Coding environments | One primary | Verified; default fixed |
| Image generation | One main provider | Verified (default ChatGPT/GPTImage) |
| Components | Required stay selected | Verified |
| Optional workflows, 3D, MCP review, live testing | Selection and credentials | Fixture only |
| Git | Initialize / Preserve / Skip | Preserve default and clear refusal verified |
| Review changes | Counts, files, checks | Verified, including the new requirement row; **Check again** in unit tests |
| Resolve conflicts | Keep / use new / merge / keep both / skip | Verified; merge fixed |
| Install | Progress, keep-open note | Verified; ≈8 minutes for 845 files on this machine |
| Ready | Readiness, open client | Verified; "Open in Claude Code" not clicked (not installed) |
| Update and repair | Check, repair, remove, recover | Check for updates, reanalysis and Apply verified; **Repair** planned live (found the repeated question above; the fixed build was not re-run end to end); **Remove components** and source pinning not driven live |
| Recovery | Resume / undo / discard | Undo verified three times including resume of an interrupted Undo; Resume and Discard in unit tests |
| Window sizes | 960×640 and larger | 16 fixture screens at 960×640 and 1280×960: no page overflow or console errors |
| Startup updater, external links in the app | Signed updates | Not exercised |

## Validation

| Check | Result |
| --- | --- |
| `cargo test --workspace --all-features` (`pnpm test:rust`) | Passed: 702 desktop-crate tests (16 opt-in ignored), 71 core, 15 web binding, and the remaining crates |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | Passed |
| `cargo fmt --all --check` | Passed (after formatting the new code) |
| `vitest run` | 256 passed (11 files) |
| `tsc --noEmit`, `scripts/lint_frontend.mjs` | Passed |
| `scripts/validate_repository.py` | Passed |
| Web: `vitest src/web`, `build_web` + `check_web_build`, Playwright (Chromium) `tests/web` | 47 passed, build ok, 6 passed (rerun on the final sources) |

## Limitations

- Windows only. macOS and Linux behaviour, signing, notarization, the AppImage and the updater were not run.
- The assistants are a loopback fixture; real provider quality, sign-in, usage limits, and Claude Code / Codex account flows are untested.
- Verifying HOI4 Agent Tools still copies the 283 MB package into a private folder each time (read, write, re-read). The read is now parallel; the write was left sequential because a parallel write measured no better. Caching the verified copy would remove most of the remaining time but changes a security boundary, so it was not done here.
- A mod root longer than about 90 characters can exceed the 260-character Windows path limit once the setup's longest paths are added.
- The local model address must be the one recorded in the lock for **Check for updates** to accept a reanalysis; the message for a different address ("the confirmed review no longer matches this session") does not say that.
- HOI4 Agent Tools has no switch to leave it out, so Python is required to finish a Windows or Linux install.
- Locks written before the merge-baseline fix (for example the Great War Redux copy tested here) still plan **Update** for a file merged earlier; the fix applies to merges made from now on.
- A very long project path (this review's scratch folder is about 240 characters) makes Git report "Filename too long", the targeted Git status comes back partial, and the scan reports "Scan incomplete" with **Next** disabled. The same installed project copied to a short path scanned complete.

## Not verified

New-mod creation on the desktop, the 3D, Super Events, portrait and live-testing screens, Repair and Remove on the desktop, source pinning, the full-copy ZIP and change-client and remove flows on the web, Firefox and WebKit, real directory drag and drop, OneDrive placeholder folders, and anything on macOS or Linux.
