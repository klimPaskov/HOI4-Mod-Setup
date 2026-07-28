# Current source, manifest, MCP, wiki, and flatten audit

## Verdict

**FAIL for the current-worktree source gate.**

The current snapshot passes the core exact-commit, pinned revision, selective
file, SHA-256, current-manifest wiki coverage, provider-conditioned component,
MCP executable fail-closed, and Codex-only flatten checks. Two high-severity
blockers remain:

1. the documented exact-revision bundled-manifest bootstrap cannot be reached
   when the remote manifest is stale; and
2. the default Windows MCP's honest `planned_unavailable` validation state
   becomes a blocking dependency after the success lock is read back.

No critical finding was identified.

## Scope and source revision

- Audit mode: current worktree only.
- Workspace branch: `codex/bootstrap-hoi4-mod-setup`.
- Workspace HEAD: `ac6538d7cf8a7c1180f7fb3aaf2c6e9da6926c70`.
- Snapshot time: `2026-07-28T13:12:54+03:00`.
- Bundled workflow-source revision:
  `27128a7b311d728a959afff7238a9aeeb9987f2b`
  (`source-manifest/hoi4-mod-setup.manifest.json:4`).
- Bundled manifest SHA-256:
  `cddb7ece7235d033888d85508455c255ffe320f0f28bc924999e8f4ddd1c19b5`.
- `source-manifest/hoi4-mod-setup.manifest.json` and
  `examples/repository-manifest.example.json` are byte-identical in this
  snapshot.
- No network source, remote publication state, full repository clone, local
  workflow checkout, or macOS execution was inspected or inferred.
- This worktree is not reproducible from HEAD alone. The audited
  `src-tauri/src/flatten.rs` and
  `docs/31_ai_provider_profiles_and_chat_sources.md` are untracked, while
  relevant source, schema, example, and documentation files are modified.

Primary current-worktree hashes:

| File | SHA-256 |
| --- | --- |
| `src-tauri/src/source.rs` | `64d613f5b3220bbd3023440e2db656da894d33d02b72fbacb329aac2028e7125` |
| `src-tauri/src/flatten.rs` | `1a9b6f24f7c27615b0f7bc9f7cda8d1ab8792ac18626dd30bf839f946712ac31` |
| `src-tauri/src/commands.rs` | `555d94501a1a40453a6791d5f918539d6fd2bb9037d215705fb5192dac8a3f20` |
| `src-tauri/src/models.rs` | `833f155a909088083ecfecd7c655dd8b064f2ec27e62feb1b337a24297738dc9` |
| `src-tauri/src/mcp.rs` | `e0c140dd01df2489289ee445e23864498eae6b2cd8a6c20d1c4d7fdc213a0764` |
| `src-tauri/src/readiness.rs` | `609079b78bab91d25aebc718e85285b701c0ce4cc8cea85a0b4a62c231f0c59d` |
| `src-tauri/src/transaction.rs` | `cf7c5a95cb1a45fd4e36905c0e4226a4b0223c342c042b22e0b55b97d21b053e` |

## Audited files and tests

Governing and design evidence:

- `AGENTS.md`
- `docs/04_remote_repository_manifest.md`
- `docs/05_wiki_installation.md`
- `docs/09_component_dependency_model.md`
- `docs/11_mcp_setup.md`
- `docs/31_ai_provider_profiles_and_chat_sources.md`
- `.agents/skills/hoi4-mod-setup-source-manifest/SKILL.md`

Manifest, example, models, and schemas:

- `source-manifest/hoi4-mod-setup.manifest.json`
- `examples/repository-manifest.example.json`
- `schemas/remote-manifest.schema.json`
- `schemas/installation-plan.schema.json`
- `schemas/installation-lock.schema.json`
- relevant plan/lock examples
- `src-tauri/src/models.rs`

Implementation evidence:

- `src-tauri/src/source.rs`
- `src-tauri/src/commands.rs`
- `src-tauri/src/flatten.rs`
- `src-tauri/src/mcp.rs`
- source/lock/flatten readiness in `src-tauri/src/readiness.rs`
- plan validation and lock construction in `src-tauri/src/transaction.rs`
- relative-path and duplicate-destination helpers in
  `src-tauri/src/security.rs`

Completed checks:

- `cargo test --manifest-path src-tauri/Cargo.toml --all-features -- --test-threads=1`:
  **144 passed**.
- Focused results included source **12 passed**, MCP **4 passed**, flatten
  **3 passed**, and command **19 passed**.
- `python scripts/validate_repository_templates.py`: **12 integrity groups
  passed**, including schema/example validation.
- Static current-manifest audit: **917** expected files, zero malformed
  lowercase SHA-256 values, zero invalid sizes, zero NFC/case-folded evidence
  collisions, and zero NFC/case-folded resolved destination collisions.

## Surface results

| Surface | Result | Evidence |
| --- | --- | --- |
| Latest default branch to exact commit | Pass | `HttpSourceClient::resolve_commit` reads `default_branch`, resolves `/branches/{branch}`, validates a full SHA, and verifies the commit object (`src-tauri/src/source.rs:182-216`, `:429-442`). |
| One revision for manifest and files | Pass | `resolve_source` resolves once and fetches the manifest at that revision (`source.rs:463-469`); tree and selected file fetches use `resolution.identity.resolved_revision` (`commands.rs:2342-2371`, `:3341-3369`). |
| Pinned commit | Pass | Full 40-hex commit and GitHub commit-object verification are required (`source.rs:217-225`, `:812-820`). |
| Pinned release | Pass with bounded identity | Release tag identity is canonicalized, annotated tags are dereferenced with a four-hop bound, and the result must be a verified commit (`source.rs:226-296`). Plan/lock models retain requested ref, canonical release, and commit (`models.rs:341-367`, `transaction.rs:2125-2132`). No release-asset install route is claimed. |
| Stale-manifest bundled bootstrap | **Fail** | See High finding 1. |
| No clone or checkout discovery | Pass | The runtime source adapter uses GitHub metadata/tree/raw endpoints. No clone or local checkout discovery exists in the audited runtime path (`source.rs:80-442`). |
| Selective component download | Pass, resume missing | Dependencies and platform are resolved before tree selection; only selected manifest-evidenced blobs are fetched (`commands.rs:2317-2366`; `source.rs:870-1153`). The recursive tree is metadata, rejects truncation, and is bounded (`source.rs:301-343`). |
| Redirect, host, response, and cache policy | Pass for current raw-file route | HTTPS/no-userinfo/no-port and GitHub host allowlisting, three redirects, response bounds, revision/hash-addressed cache, link rejection, and cache hash/size revalidation are present (`source.rs:80-156`, `:374-427`, `:445-453`). |
| Archive policy | Explicitly unsupported | `SourceKind` supports only file/tree/generated (`models.rs:98-104`; remote schema `source.kind`). No archive extraction route or release archive is claimed. |
| SHA-256 and initial size verification | Pass | Manifest validation requires lowercase hash and size for every non-generated file (`source.rs:610-643`); cache fetch and `verify_download` enforce them (`source.rs:374-427`, `:1155-1194`). Plan and lock construction carry source hash/size (`commands.rs:2438-2449`; `transaction.rs:1945-1968`). |
| Component dependency/support | Pass with exposure gap | Unknown dependencies and cycles reject; expansion is topological; reverse dependencies are deterministic; unsupported optional components remain explicit (`source.rs:731-760`, `:836-1001`). Non-Codex closure is rejected after expansion (`commands.rs:1629-1642`, `:2317-2319`). |
| Current wiki tree and required pages | Pass | `wiki.snapshot` is a root-contained `paradox_wiki` tree (`source-manifest/hoi4-mod-setup.manifest.json:2016-2048`), with 586 evidenced files and all 11 required pages evidenced (`source-manifest/hoi4-mod-setup.manifest.json:2069-4993`, `:5203-5216`). Current media policy is `all_declared`; provenance is `repository_only`; license is `not_found` (`source-manifest/hoi4-mod-setup.manifest.json:5216-5222`). |
| MCP declaration and executable identity | Pass fail-closed; readiness fails | The manifest declares only Windows, bare `hoi4-agent-tools.cmd`, node/command health text, no environment names, and six capabilities (`source-manifest/hoi4-mod-setup.manifest.json:1846-1915`). `mcp::manifest_target` requires executable SHA-256 and size and therefore rejects the current route (`mcp.rs:41-88`); execution also rehashes a regular link-free resolved file (`mcp.rs:139-183`). |
| Codex-only flatten output | Pass | Non-Codex selection is rejected in planning and apply validation (`commands.rs:2300-2306`; `transaction.rs:52-65`). The exact required/skill/subagent/extra mapping, UTF-8, no-follow reads, bounds, secret checks, and case-insensitive output collision rejection are implemented (`flatten.rs:25-194`, `:196-397`). Outputs remain generated transaction operations (`commands.rs:2595-2718`). |
| Conflict-driven and maintenance flatten refresh | Pass in code; dedicated maintenance test missing | Accepted incoming/local conflict state filters flatten inputs (`commands.rs:2811-2831`); refresh rebuilds outputs and preserves only still-valid reviewed flat decisions (`commands.rs:2844-3067`); every non-flat conflict resolution triggers refresh (`commands.rs:3814-3919`). Maintenance regenerates the flat set from current accepted inputs (`commands.rs:3592-3672`). The present regression test proves reviewed flat-decision preservation after a non-flat source change (`commands.rs:4522-4801`) but does not construct a maintenance-mode plan. |

## Findings by severity

### High 1 - Exact-revision bundled bootstrap is unreachable for a stale remote manifest

The accepted bootstrap route says a stale remote manifest may fall back to the
bundled manifest only when the bundle was generated for the exact resolved
commit (`docs/04_remote_repository_manifest.md:113-124`).

`resolve_source` first calls:

```text
parse_manifest(remote_manifest_bytes, Some(resolved_revision))
```

at `src-tauri/src/source.rs:468-469`. The validator now correctly rejects any
`generated_for_revision` mismatch at `source.rs:557-575`. Therefore execution
returns before the stale comparison and bundled branch at `source.rs:470-487`.
For every successfully parsed remote manifest, the condition at line 471 is
already true; `manifest_origin = bundled_revision_bootstrap` cannot be produced
by fresh source resolution.

Impact: if the checked-in remote manifest is stale, latest, pinned commit, and
pinned release planning fail even when the application has exact bundled
evidence for that commit. This is fail-closed for integrity but fails the
accepted bootstrap availability contract. Remote publication was not checked,
so this report does not claim whether the live branch currently needs the
bridge.

### High 2 - Honest Windows MCP unavailability blocks core readiness after lock reload

The default `core` profile selects `mcp.hoi4_agent_tools`
(`source-manifest/hoi4-mod-setup.manifest.json:5171-5182`). The current MCP rule
has no `executable_sha256` or `executable_size`
(`source-manifest/hoi4-mod-setup.manifest.json:1891-1897`), so
`mcp::manifest_target` correctly returns `UnsupportedPlatform`
(`src-tauri/src/mcp.rs:62-87`).

Lock construction correctly records this as
`validation = planned_unavailable`
(`src-tauri/src/transaction.rs:2002-2045`). Installed readiness also recognizes
the MCP state as `planned_unavailable` and makes the MCP check itself
non-blocking (`src-tauri/src/readiness.rs:1080-1101`, `:1191`).

However, the separate dependency aggregate requires every lock component
validation to equal `pass` (`readiness.rs:1182-1189`). That aggregate is a
blocking readiness check (`readiness.rs:582-590`). Consequently, the default
Windows installation can pass transaction-time readiness, write a success
lock, then become not core-ready when readiness reloads the lock.

Impact: this contradicts the documented optional/non-blocking MCP state and
creates inconsistent readiness across the transaction boundary.

### Medium 1 - Wiki marker, provenance, license, and media policy are not revision-bound in the lock

The wiki design records an observed
`_last_updated_on_27_Nov_2025.txt` marker
(`docs/05_wiki_installation.md:5`), but the current manifest sets
`snapshot_marker` to `null` and has no expected marker file
(`source-manifest/hoi4-mod-setup.manifest.json:5202`).

Plans and locks correctly preserve the exact required-page list
(`commands.rs:2753`, `:3411`, `:3742`; `transaction.rs:2141`), but do not retain
the manifest's snapshot marker, media policy, source provenance, or license
state. Installed readiness obtains provenance/license from the application's
current bundled manifest instead of the locked manifest revision
(`readiness.rs:24-38`).

Impact: required-page coverage is reproducible, but a pinned install's displayed
wiki provenance/license/media context can drift after an application update.
The current visible states, `repository_only` and `not_found`, are honest and
must remain warnings, not be upgraded or invented.

### Medium 2 - Partial download/resume is not implemented or tested

`get_bytes_limited` streams the entire HTTP body into memory and cache content
is written only after a complete verified fetch (`source.rs:110-156`,
`:409-425`). There is no partial-file ledger, HTTP range request, or resumable
blob state. An interrupted source transfer must restart the file.

Impact: selective download is correct, but the source skill's required partial
download/resume behavior is absent.

### Medium 3 - Unicode-normalized destination identity is not enforced

`canonical_relative_key` normalizes separators and lowercases text but does not
apply Unicode normalization (`src-tauri/src/security.rs:14-64`, `:100-102`).
Both manifest and selected-file duplicate detection depend on that key
(`security.rs:265-285`; `source.rs:1116-1128`).

Impact: a hostile future manifest could present NFC/NFD-equivalent paths as
distinct and collide on a normalizing filesystem. The current manifest was
independently checked and has no such evidence or resolved-destination
collision.

### Low 1 - Reverse-dependency evidence is computed but not handed off

`reverse_dependencies` and `ComponentSupport.dependents` are deterministic
(`source.rs:43-51`, `:921-997`) and unit-tested (`source.rs:1336-1350`).
`build_plan` consumes support only to filter downloads and record unsupported
optional workflow state (`commands.rs:2319-2341`, `:2720-2724`). No plan/lock
field or command response carries reverse-dependent removal/update impact.

Impact: the core can compute the evidence, but the current plan/lock cannot
reproduce or present it during maintenance review.

### Low 2 - Maintenance re-fetches do not use locked size evidence

Initial and update selection fetches pass the manifest size
(`commands.rs:2361-2371`, `:3359-3369`). Later maintenance content and merge-base
fetches pass `None` for expected size (`commands.rs:3481-3488`, `:3553-3559`).
SHA-256 still authenticates the bytes, but the explicit size check is not
repeated. Plan and lock schemas also allow nullable `source_size`
(`schemas/installation-plan.schema.json:432-438`;
`schemas/installation-lock.schema.json:378-384`).

Impact: current core-built plans retain size evidence, but maintenance and
readiness do not fully enforce the declared size invariant.

## Reproducibility and integrity gaps

- Fresh `bundled_revision_bootstrap` resolution has no passing test and is
  structurally unreachable as described above.
- Source identity, manifest hash/origin, per-file revision/hash/size, installed
  result hash/size, and exact wiki required pages are retained in core-built
  plans and locks (`models.rs:341-367`, `:650-693`, `:695-793`;
  `transaction.rs:1945-1980`, `:2120-2143`).
- Installed readiness verifies each locked file's installed hash/size and
  source revision, but only validates the format of `source_sha256`; it does not
  re-resolve every source blob (`readiness.rs:992-1020`). This is suitable for
  local integrity, not independent source re-publication proof.
- The manifest declares a 900-second metadata cache TTL
  (`source-manifest/hoi4-mod-setup.manifest.json:5225-5235`), but latest mode
  currently re-resolves metadata on each request; no ETag/TTL behavior is
  claimed.
- The report's current-worktree hashes are necessary because several audited
  files are modified or untracked.

## Unsupported or uncertain routes

- Remote publication of revision
  `27128a7b311d728a959afff7238a9aeeb9987f2b` is unverified.
- Release assets, archives, archive extraction limits, and signature
  verification are unsupported. Current `signing.required` is false
  (`source-manifest/hoi4-mod-setup.manifest.json:5237-5240`).
- MCP and 3D routes are Windows-only declarations
  (`source-manifest/hoi4-mod-setup.manifest.json:1850-1853`, `:5000-5003`). No macOS executable, package, command, or
  adapter is inferred.
- The MCP package/install identity, immutable executable SHA-256/size, expected
  server version, and macOS route are missing. The health route must remain
  `planned_unavailable` and must not execute a same-named PATH command.
- Wiki formal provenance and license evidence remain unavailable; current
  states are `repository_only` and `not_found`.
- Only the current `all_declared` wiki media policy is evidenced. The schema
  accepts `referenced_only` and `none`, but this audit found no policy-specific
  selection tests.

## Meaningful tests present and missing

Present:

- exact manifest revision mismatch rejection;
- checked-in manifest and example runtime validation;
- dependency cycle, topological expansion, and deterministic reverse map;
- missing file/hash evidence rejection;
- current macOS optional MCP support-state logic;
- MCP missing identity rejection and reviewed-target path rejection;
- flatten mapping, traversal, secret, UTF-8, and case-collision checks;
- provider-conditioned component closure;
- reviewed flatten-decision preservation after a non-flat source change;
- maintenance reanalysis binding;
- transaction fault and rollback coverage in the full Rust suite.

Missing:

- fake GitHub adapter tests for default-branch-to-commit resolution, commit
  object mismatch, annotated release dereference, moved release tags, redirect
  host rejection, response limits, truncated trees, and cache corruption;
- stale remote manifest to exact bundled bootstrap success and mismatch failure;
- selected-file-only tree expansion with extra/missing evidence, checksum
  mismatch, and explicit size mismatch;
- partial transfer interruption and resume;
- Unicode NFC/NFD destination collisions;
- Windows post-lock readiness with the default MCP
  `planned_unavailable` state;
- dedicated create and maintenance tests covering keep, replace, merge, rename,
  and skip followed by exact flattened output refresh;
- wiki marker consistency, required-page omission, case-sensitive link matching,
  media-policy variants, and locked provenance/license persistence;
- real Windows MCP initialize/tools-list integration and any macOS execution.

## Recommended parent actions

1. Parse the remote manifest structurally first, compare
   `generated_for_revision`, then strictly validate either the exact remote or
   exact bundled candidate. Add stale/exact/mismatch bootstrap tests.
2. Make an optional MCP `planned_unavailable` validation non-blocking in the
   dependency aggregate, while keeping missing required dependencies blocking.
   Add a post-lock Windows readiness regression test.
3. Persist revision-bound wiki marker, media policy, provenance, and license
   state in plan/lock evidence; do not read those states from a newer bundled
   manifest for pinned installs. Reconcile the documented marker with actual
   manifest evidence without inventing it.
4. Add Unicode normalization to canonical path identity and test NFC/NFD,
   case, reserved-name, and duplicate-destination attacks.
5. Implement or explicitly defer resumable partial downloads; add fake-source
   tests for host, redirect, truncation, cache corruption, hash, and size
   failures.
6. Carry reverse-dependent impact into the maintenance review contract and
   require locked source size during repair/reinstall fetches.
7. Retain the current MCP fail-closed identity behavior and current Codex-only,
   conflict-aware flatten refresh. Add a true maintenance-mode flatten
   acceptance test before declaring that surface complete.
8. Ensure the untracked provider/flatten files and all reviewed modifications
   are intentionally included in the parent change before using a commit as
   completion evidence.

Only this requested handoff report was written by the auditor. No product code,
schema, example, workflow, or design document was modified.
