# Final bounded security audit — 2026-07-29

## Scope and threat model

Read-only review of `src/lib/tauri.ts`, `src/lib/tauri.test.ts`, the Meshy/provider paths in `src/App.tsx`, `src-tauri/src/credentials.rs`, and `src-tauri/src/commands.rs`, `src-tauri/src/flatten.rs`, and `parent-review-2026-07-29.md`. Threats considered were secret entry into state-bearing IPC, project state, plans, locks, logs, or flattened output; provider-scope confusion; path/link escape; and unverified 3D process input. No network command, external checkout inspection, implementation edit, or test run was performed.

## Findings by severity

No concrete critical, high, medium, or low severity finding remains in the bounded scope.

The remaining native race-test gap is release evidence, not a demonstrated vulnerability: coordinated macOS symlink and Windows junction/reparse swaps have not been exercised on both native runners. The code and handoff correctly avoid a race-proof claim.

## Evidence

- `src/lib/tauri.ts:352-367` sends both state-bearing commands through `stateForCore`, which blanks `meshKeyDraft`. `src/lib/tauri.test.ts:54-64` regresses both `preview_descriptors` and `build_installation_plan` and proves the draft is absent from serialized invoke calls.
- `src/App.tsx:940-962,1006-1018` keeps provider-key input in component-local state, clears it after the dedicated vault call, and never places it in `WizardState`. `src/App.tsx:1256-1271` clears the Meshy draft after the dedicated vault call and retains only the returned opaque reference.
- `src-tauri/src/credentials.rs:92-152` uses the OS credential store; `:194-279` validates platform-bound Meshy UUID references and provider-scoped references; `:327-361` permits scoped process injection only as `MESHY_API_KEY` and carries known-secret redaction state.
- `src-tauri/src/commands.rs:533-562` rejects provider credentials for Codex and non-credential profiles. `:1047-1074` stores/deletes Meshy through the vault and retains only an in-memory opaque reference. `:2644-2695,2856-2883` place only validated Meshy opaque references in generated project state and the plan; provider references and secret values are absent. No scoped source contains a logging or browser-storage write of either secret.
- `src-tauri/src/commands.rs:1117-1239` re-resolves the locked manifest, reads the bootstrap through the root-bound no-follow reader, checks locked size/SHA-256, injects only `MESHY_API_KEY` into the supervised process, bounds runtime/output, and returns no environment value.
- `src-tauri/src/commands.rs:2411-2416` enforces Codex-only flattening in the core, independently of the UI.
- `src-tauri/src/flatten.rs:291-335` normalizes and contains every requested path, validates an opened regular-file handle before and after reading, and bounds file size. Unix walks ancestors with `openat` plus `O_NOFOLLOW`/`O_DIRECTORY` (`:338-374`); Windows opens the leaf as a reparse point and verifies the final handle path remains under the canonical root (`:376-451`). Secret-shaped paths/content, case collisions, per-file count/size, and aggregate size are rejected at `:126-195,457-539`.
- Existing Rust regressions cover traversal/collision/secret rejection and a root-bound regular read at `src-tauri/src/flatten.rs:595-729`; the Unix linked-ancestor regression is at `:731-770`.

## Credential, filesystem/process, and supply-chain checks

Credential values cross IPC only through the dedicated vault-store commands. State-bearing planning IPC receives a blank Meshy draft; planning and generated state receive only the accepted opaque Meshy reference; provider references remain core-derived and do not enter project artifacts. The 3D process route is bound to locked manifest and file-hash evidence and receives the Meshy value only through its scoped environment.

Flatten reads are link-aware and fail closed under the implemented handle checks. This audit makes no race-proof claim. Release workflows, updater/cache behavior, archives, Git, support bundles, and unrelated Codex App Server internals were outside the explicitly bounded file scope; no conclusion about those surfaces is added here.

## Missing evidence and recommended next step

The parent should rerun the targeted TypeScript and Rust tests, then retain coordinated native macOS symlink-swap and Windows junction/reparse-swap tests as release evidence. Continue describing the reader as link-aware and fail-closed, not race-proof, until those adversarial native tests pass.
