# Security audit — full re-audit 2026-07-29

## Outcome

Current HEAD is **not security-complete**. No credential value, Codex token
read, OpenAI API-key fallback, external-token mode, checksum bypass, or
fork-triggered release-secret exposure was found. The OS-vault, redaction,
exact-source-revision, bounded App Server, and read-only recovery foundations
are materially present.

Five high-severity trust-boundary failures remain:

1. PATH discovery plus a hash calculated from that same PATH file is treated as
   independent executable provenance for Codex, Python, Git, and GitHub CLI.
2. “Read-only” Git inspection and later Git actions do not neutralize
   repository/system configuration or hooks, and a linked hooks directory
   fails open.
3. root/link checks return ordinary paths that are reopened later, so project,
   cache, private-script, and cleanup operations are not race-safe.
4. remote manifests are checked by a handwritten validator but not against the
   authoritative JSON Schema, leaving accepted trust-policy declarations
   unenforced.
5. stable-release signing credentials and OIDC capability are attached before
   the workflow proves that the tag is an approved protected revision.

This report records **0 critical, 5 high, 3 medium, and 2 low findings**.
Completion of the report means the parent has an evidence-backed security
handoff; it does not mean the product is ready for release.

## Audit identity and scope

- Audited commit:
  `bcfe329dd9ab0ae0d86e48b1a46ed21c83e36603`.
- Commit date and subject: 2026-07-29,
  `docs: point downloads to current preview`.
- Audit mode: bounded, read-only review. The only authored file is this
  report.
- The worktree was clean when the audit began. Parallel work later modified
  `codex.rs`, `commands.rs`, `transaction.rs`, transaction documentation and
  frontend bridge files. Those changes were preserved and excluded. Evidence
  touching a changed file was checked against `git show
  bcfe329:<repository-relative-path>`.
- No source network request, credential-vault read, login attempt, process
  execution with a real secret, release, publication, or destructive
  filesystem action was performed.
- No secret or private project content is reproduced in this report.

Governing sources read:

- `AGENTS.md`
- `docs/GOAL_PROMPT.md`
- `docs/13_security_model.md`
- `docs/04_remote_repository_manifest.md`
- `docs/14_transaction_rollback.md`
- `docs/26_open_source_github_workflow.md`
- `docs/30_codex_chatgpt_authentication.md`
- `docs/31_ai_provider_profiles_and_chat_sources.md`
- `SECURITY.md`, `CONTRIBUTING.md`, `DEVELOPMENT.md`, and `RELEASING.md`
- the security, Codex integration, source manifest, and open-source release
  repo-local skills

Implementation and delivery sources read:

- `src-tauri/src/security.rs`
- `src-tauri/src/credentials.rs`
- `src-tauri/src/paths.rs`
- `src-tauri/src/process.rs`
- `src-tauri/src/source.rs`
- `src-tauri/src/git.rs`
- security-relevant `src-tauri/src/codex.rs`, `ai.rs`, `migrations.rs`,
  `commands.rs`, and transaction tests
- all four `.github/workflows/*.yml` files, branch ruleset, CODEOWNERS, and
  Dependabot configuration
- committed-secret, release build/verify, release manifest refresh, stable
  asset curation, and preview asset curation scripts
- the inline security tests and the path, manifest, flatten, and Codex fuzz
  entry points
- the sibling exact-HEAD source-manifest and transaction-recovery handoffs

## Threat model

The audit assumes:

- a selected existing mod project can contain hostile names, links/reparse
  points, `.git` metadata, Git configuration, attributes, hooks, and file
  contents;
- another same-user process, sync client, or malicious project helper can
  change path components between validation and mutation;
- PATH, HOME-like environment variables, Git global/system configuration, and
  installed tools are workstation inputs, not immutable evidence;
- the workflow source repository, its remote manifest, GitHub API responses,
  redirects, cache contents, and downloaded files are untrusted until bound to
  an exact revision and verified hash;
- App Server stdout and provider responses are untrusted structured input;
- a contributor may submit a fork pull request, and an authorized collaborator
  or compromised account may create tags, dispatch workflows, or select a
  non-default ref;
- release dependencies or project scripts may be compromised even when the
  workflow file itself is unchanged;
- OS-vault and official Codex token persistence are trusted platform services,
  but the application must not copy their values into project state, logs,
  previews, crashes, fixtures, Git, or support artifacts.

Not currently reachable:

- no archive extraction route was found, so archive count, size, expansion
  ratio, depth, sparse-file, link, and path limits cannot presently be bypassed
  through an application archive feature;
- no application updater or updater manifest route was found;
- no support-bundle, crash-report upload, analytics, or telemetry route was
  found.

Those surfaces must remain unavailable until their required limits,
redaction, signature, rollback/freeze, and hostile-input tests exist.

## Findings by severity

### High SEC-H01 — PATH self-hashing is treated as executable trust

`ProcessSpec` verifies an optional expected hash and compares the executable
against a caller-provided allowlist (`process.rs:42-60`, `75-83`). That is
useful change detection only when the expected identity and allowlist originate
from an independent trusted source. Current high-value callers instead find a
file on PATH, hash that same file, and pass the same path back as the sole
allowlist entry:

- generic PATH lookup accepts the first link-free regular candidate
  (`process.rs:273-318`);
- Codex lookup does the same (`codex.rs:1493-1513`), and session creation calls
  it “official” before starting App Server (`commands.rs:295-315`);
- `ProcessJsonlTransport` hashes the discovered executable and checks that
  self-derived hash again immediately before `Command::spawn`
  (`codex.rs:1261-1315`, `1316-1372`);
- the 3D health check resolves `python.exe`/`python` from PATH, hashes it, and
  then gives that process the vault-sourced `MESHY_API_KEY`
  (`commands.rs:1194-1226`);
- Git and GitHub CLI use the same PATH-resolution/self-hash pattern
  (`git.rs:481-506`, `793-846`).

The security skill explicitly says a regular PATH file is not independent
trust. A malicious or replaced PATH binary can therefore become the process
that receives the Meshy key, acts as Codex while inheriting Codex account
locations, or operates with Git/GitHub credentials. The re-hash narrows a
replacement window but does not establish publisher, package, install-root, or
manifest identity. There is also a residual change window between the final
hash/open and OS process creation.

Existing tests prove only that a deliberately wrong hash and a different
caller allowlist are rejected (`process.rs:403-440`). They do not prove that an
executable was independently authorized.

Remediation:

- define a platform-specific, independently rooted identity policy for each
  executable class: verified publisher/signature and approved install receipt
  for official Codex, OS-owned fixed identity for system tools, and
  manifest/installation-lock SHA-256 plus size for every repository
  interpreter/runtime;
- if official Codex identity cannot be established without inventing a
  distribution route, fail closed and show manual installation guidance;
- make the Meshy health route unavailable until its Python interpreter/runtime
  identity is declared and verified independently of PATH;
- carry a reviewed executable identity object, not a caller-created path
  allowlist, through preview, approval, and spawn;
- bind the final identity check as closely as the platform allows to the opened
  executable used for process creation.

Regression tests:

- an earlier untrusted PATH candidate with the expected filename;
- a link-free but unapproved regular executable;
- changed publisher/install receipt with unchanged filename;
- executable replacement between review, final validation, and spawn;
- Meshy key non-injection on every identity failure;
- Codex startup refusal without official identity;
- Git/GitHub CLI refusal without approved identity on Windows and macOS.

### High SEC-H02 — Git inspection and actions can execute untrusted Git configuration or hooks

The existing-project “read-only” inspection launches Git in the selected
repository:

- `inspect_read_only` runs `git status`, `rev-parse`, `remote`, recursive
  submodule status, and `ls-files` (`git.rs:1001-1111`);
- `run_git_read_only` invokes the normal Git process with the normal inherited
  safe-environment profile and no Git configuration isolation
  (`git.rs:808-820`; `process.rs:252-270`);
- no `core.fsmonitor=false`, protected empty `core.hooksPath`, isolated
  system/global config, or equivalent direct parser is used.

A hostile repository configuration can therefore cause Git to start auxiliary
programs during what the product calls a read-only scan. The later mutation and
online routes are also incomplete:

- online configuration validation rejects only `core.sshCommand`,
  `core.gitProxy`, `core.hooksPath`, and `url.*.insteadOf`
  (`git.rs:531-570`); it does not neutralize `core.fsmonitor`, filters,
  credential helpers, include chains, external diff/merge drivers, proxy
  families, or system/global configuration;
- hook discovery returns an empty list when the hooks path contains a link,
  which online preparation interprets as “no hooks”
  (`git.rs:1181-1205`, `206-244`);
- push invokes normal `git push`, so an accepted or missed `pre-push` hook and
  Git transport configuration remain active (`git.rs:409-423`);
- initialize runs normal `git init`, `git add`, and optionally `git commit`
  (`git.rs:719-755`), permitting template hooks and configured clean filters.

Argument arrays prevent shell-string injection but do not neutralize Git’s own
command-execution features. The risk is local code execution during project
scan/setup and credential or publication compromise during online actions.

The seven Git tests cover merge behavior, no automatic push, status parsing,
basic remote URL rejection, explicit approval, and changed HEAD. None covers
fsmonitor, filters, config includes, linked hooks, template hooks, credential
helpers, proxy/transport helpers, or pre-push execution (`git.rs:1236-1375`).

Remediation:

- do not launch Git during the first hostile-project facts pass when safe direct
  metadata parsing can provide the needed fact;
- for every necessary Git invocation, use a verified Git binary and an explicit
  hardened configuration/environment profile: disable optional locks,
  fsmonitor, hooks, filters/external drivers, prompts, credential-helper access
  where not needed, system/global config, unsafe protocol helpers, and config
  includes;
- use a core-owned protected empty hooks directory and fail closed when `.git`,
  hooks, config, or an ancestor changes identity;
- read and validate the exact local configuration needed for the operation
  without first activating hostile configuration;
- bind push to the reviewed canonical remote, branch, HEAD, and hardened
  transport profile immediately before execution.

Regression tests:

- scan of repositories declaring fsmonitor and external filters;
- local, included, global, and system unsafe configuration;
- linked/reparse hooks directories and hooks swapped after review;
- template hooks during initialize and pre-commit/pre-push hooks;
- credential-helper, proxy, SSH-command, URL-rewrite, and protocol-helper
  isolation;
- no child process or network action during deterministic scan;
- no push or public creation before a separate valid approval.

### High SEC-H03 — path containment is check-then-use rather than handle-relative

Path normalization is strong for static strings: it bounds total length,
depth, and segment length; rejects absolute paths, parent traversal, ADS
colons, trailing dot/space, and common Windows device names; and derives an
NFC-plus-lowercase collision key (`security.rs:15-105`).

The mutation boundary is not race-safe:

- `safe_join` scans links and canonicalizes the nearest existing parent, then
  returns an ordinary `PathBuf` for later use (`security.rs:108-138`);
- `atomic_write` later creates parent directories, creates a temporary by path,
  and renames/replaces by path without retaining verified ancestor handles
  (`security.rs:314-381`);
- project-root validation similarly returns a canonical path after a point-in-
  time link scan (`paths.rs:72-92`);
- verified cache reuse checks metadata and SHA-256 and then performs a separate
  path-based read (`source.rs:383-411`);
- partial-cache open uses no-follow semantics for the final file, but creates
  and reopens ancestors by path (`source.rs:627-653`);
- the private 3D script route checks directories before/after creation and then
  creates, reads, and removes by path (`commands.rs:1246-1317`);
- Git rollback checks the top `.git` path and later calls recursive removal
  without a race-safe descendant walk (`git.rs:849-872`).

A concurrently replaced parent, junction/reparse point, mount, or destination
can invalidate the earlier containment decision. Depending on the operation,
the result can be a write, replace, read, or cleanup outside the reviewed root,
or consumption of bytes different from those just hashed.

The five `security.rs` tests cover basic normalization, redaction, Unicode
normalization, and a simple property. There is no `paths.rs` test module, and
there are no adversarial link/reparse swap tests for atomic writes, cache
reuse, private scripts, or Git cleanup.

Remediation:

- implement segment-by-segment, handle-relative no-follow traversal rooted in
  an already opened trusted directory;
- on Windows, reject every unsupported reparse type and compare volume/file
  identity before mutation; on macOS, use directory descriptors and no-follow
  opens/renames;
- create missing directories relative to retained parent handles, and perform
  temp creation, final replacement, deletion, rollback, and cache promotion
  through those handles;
- revalidate the opened object’s type, identity, and containment after open,
  not only the textual path before open;
- use immutable/create-only verified cache objects or discard and re-fetch on
  any identity ambiguity.

Regression tests:

- Windows symlink, junction, mount-point, and other reparse swaps at every
  validate/open/create/rename/delete/rollback boundary;
- macOS symlink swaps at the same boundaries;
- destination and ancestor swaps after `safe_join`;
- cache object substitution between metadata, hash, and read;
- source partial-cache ancestor replacement;
- native case/Unicode/reserved-name collision matrices on both supported
  platforms.

### High SEC-H04 — remote manifests are not validated against the authoritative schema

`parse_manifest` deserializes directly to `RemoteManifest` and calls only the
handwritten `validate_manifest` (`source.rs:713-717`, `742-1043`). The runtime
does not evaluate `docs/schemas/remote-manifest.schema.json`.

The handwritten validator covers many important controls—manifest major,
repository identity, canonical paths, destination collisions, component
dependencies, platform declarations, selected-file hash/size evidence, and the
only permitted Meshy environment. It does not enforce all schema-controlled
trust declarations:

- wiki source and license states are schema enums
  (`remote-manifest.schema.json:515-535`) but are copied to install metadata
  without runtime enum checks (`source.rs:720-727`);
- latest-mode `resolve_default_branch` and `record_commit` are schema constants
  (`remote-manifest.schema.json:549-568`), while runtime checks only positive
  rollback retention at the end of manifest validation
  (`source.rs:1038-1041`);
- manifest signing declarations are deserialized but not enforced
  (`remote-manifest.schema.json:595-614`).

The current checked-in manifest passes the repository package validator. This
finding is not a claim that current source bytes bypass SHA-256. It is a remote
trust-boundary failure: a future or compromised manifest can be accepted while
declaring policy, provenance, or required signing states that the runtime does
not support or enforce.

Existing source tests parse valid examples and cover several handwritten
invariants, but there is no JSON-Schema-versus-Rust differential suite and no
hostile signing/update-policy/provenance fixture (`source.rs:1510-1674`).

Remediation:

- validate the exact remote manifest bytes against the authoritative Draft
  2020-12 schema before use, then apply the stricter semantic Rust invariants;
- explicitly reject every unsupported signing or update-policy combination;
- keep the raw manifest hash, exact resolved commit, and validation result
  bound to selection, plan, lock, cache, and maintenance;
- add a differential test ensuring no schema-invalid manifest is accepted by
  runtime and no runtime-required invariant is omitted by the schema.

Regression tests:

- invalid wiki source/license enums;
- false/missing exact-revision policy constants;
- `signing.required=true` without a supported verified signature;
- unsupported signing algorithm/key ID;
- unknown fields and malformed policy combinations;
- schema-valid but semantically unsafe destination, command, platform,
  dependency, and evidence combinations.

### High SEC-H05 — release trust is established after signing authority is available

The stable workflow triggers for every `v*` tag and manual dispatch
(`.github/workflows/release.yml:3-7`). The build matrix immediately attaches
the `release` environment and gives every Windows and macOS build job
`id-token: write` (`release.yml:13-33`).

Secret-bearing/signing setup happens before revision policy and release gates:

- Windows PFX secrets are written/imported at `release.yml:81-108`;
- macOS certificate and notarization credentials are exposed and private-key
  material is written/imported at `release.yml:109-145`;
- semantic tag validation first occurs at `release.yml:146-150`;
- project lint, tests, scripts, Cargo tests, and fuzz compilation run only
  afterward (`release.yml:151-164`);
- the signing material remains available while project build/verification code
  runs, with cleanup deferred to `always()` steps
  (`release.yml:165-254`).

This ordering means code from the tagged revision—and compromised dependency
or build scripts—runs after release signing authority has been made available.
The repository ruleset protects only the default branch and contains no tag
rule (`.github/rulesets/main-protected.json:1-11`). The repository cannot prove
the external `release` environment’s reviewer/ref policy, so that external
control is mitigation, not an in-repository trust proof.

Positive controls:

- all third-party actions in all four workflows are pinned to full 40-hex
  revisions;
- workflows default to `contents: read`;
- the final draft-release job alone receives stable-publication
  `contents: write`, curates a fixed artifact set, rechecks hashes, and uses
  `--verify-tag` (`release.yml:262-298`);
- pull-request workflows receive no release secrets and there is no
  `pull_request_target`.

Remediation:

- add a first no-environment, no-secret, no-OIDC preflight job that validates a
  strict tag, exact tag target, approved default-branch ancestry, clean source
  revision, and required release approvals;
- protect stable tag patterns against creation/deletion/update outside the
  release procedure;
- split unsigned build/test from signing: build and test with read-only
  permissions, publish immutable hash-bound artifacts, then let narrow
  protected signing jobs consume only those artifacts;
- grant `id-token: write` only to the Azure signing job that needs it;
- never run package installation, project tests, or other broad source scripts
  after private signing material is imported;
- create private key files with restrictive permissions, minimize their
  lifetime, and bind publication evidence to exact signer certificate
  identity;
- put the write-capable release job behind the protected release environment
  as well.

Regression policy tests:

- reject broad/malformed tags and tags outside an approved protected revision;
- reject secret/environment/OIDC jobs that do not depend on trust preflight;
- reject source-script execution after signing authority becomes available;
- reject broad matrix `id-token: write`;
- reject release publication without exact tag, source SHA, artifact hash,
  signer identity, notarization, and protected-environment evidence;
- verify fork pull requests have read-only tokens and no secrets.

### Medium SEC-M01 — development-preview publication accepts a dispatched non-default ref

The preview workflow is manual-only but has no default-branch/ref condition
(`development-preview.yml:3-5`). Its build checks, artifact hashes, and preview
tag bind to `github.sha`, but they accept whichever ref was selected for the
dispatch (`development-preview.yml:29-69`, `85-89`). The publish job has
`contents: write`, no protected environment, and creates a prerelease targeted
at that SHA (`development-preview.yml:71-112`).

An authorized or compromised collaborator can therefore publish source-built
installers from an unreviewed branch as an official repository prerelease. The
artifacts are deliberately unsigned, but repository-hosted distribution still
confers trust.

Remediation:

- require `github.ref == 'refs/heads/main'` and an exact protected-main
  revision, or use a separately reviewed immutable commit input;
- put preview publication behind a protected preview environment;
- split read-only build from write-capable publication and reverify complete
  artifacts in the publication job;
- test that a branch/tag dispatch cannot obtain `contents: write` or create a
  release.

### Medium SEC-M02 — the committed-secret gate is a narrow current-tree scan

`scripts/check_committed_secrets.py`:

- walks the filesystem rather than Git tracked/staged/history objects
  (`check_committed_secrets.py:61-80`);
- skips `dist`, artifact, release-artifact, and UI-reference directories
  regardless of whether a file is tracked (`:11-19`);
- reads only an extension allowlist (`:20-38`), so `.env.example`,
  suffixless text, many certificate/key formats, binaries, and screenshots are
  not covered;
- detects only selected high-signal token prefixes and a narrow assignment set
  (`:40-58`);
- has no fixture/unit test suite.

The security workflow checks out normal current source and runs only this
script (`.github/workflows/security.yml:16-23`); it does not provide a
history-aware or binary/entropy scanner.

Audit checks:

- `python scripts/check_committed_secrets.py` passed with
  `No committed secret patterns found.`;
- a separate read-only high-signal scan inspected all 34 reachable Git commits
  without printing matching values. Its only matching path was the documented
  Meshy placeholder in `credentials.rs`; there were zero non-placeholder
  Meshy matches and no other configured high-signal match.

No actual committed credential was identified. The finding is that CI cannot
establish the broader “no secrets in Git, fixtures, screenshots, logs, or
crashes” promise.

Remediation:

- enumerate Git tracked and staged content, including tracked files under
  normally skipped directories;
- add a reviewed history-aware scanner in the release gate with safe
  false-positive handling;
- add bounded text fallback independent of suffix and explicit handling for
  `.env.example`, PEM/private-key markers, structured JSON/YAML/TOML values,
  logs, crash text, source maps, and fixture files;
- establish a binary/screenshot review or scanning policy;
- keep findings value-free in CI output.

Regression fixtures:

- `.env.example`, suffixless text, PEM/private key, PFX/P12 policy, structured
  assignments, tracked skipped-directory files, staged renames/deletions,
  prior-history leakage, logs/crashes, screenshots, valid placeholders, public
  certificates, and false-positive controls.

### Medium SEC-M03 — one generic process profile is broader than the reviewed action

`ProcessSpec` checks only that a working directory is absolute and exists; it
does not receive an allowed-root set or bind the directory’s identity
(`process.rs:87-93`). Every child gets a common environment containing PATH,
home/profile, app-data, and temporary-directory locations
(`process.rs:252-270`). Codex transport additionally receives `CODEX_HOME` and
XDG data/config roots, as required for Codex-owned account state
(`codex.rs:1336-1355`), but the generic Git/Python routes are not reduced to
their own minimum profiles.

Timeout handling is bounded, but Windows relies on a `SystemRoot`-derived
`taskkill.exe`; non-Windows targets kill only the direct child rather than a
verified process group (`process.rs:223-249`). The Windows browser opener also
derives its “fixed” executable root from inherited `SystemRoot`
(`process.rs:320-350`).

All current core processes run as the current user, and no core elevation route
was found. Arguments are arrays, stdin is null, stdout/stderr are independently
bounded and redacted, and declared Meshy environment injection rejects extra
secret names (`process.rs:107-189`). Those are useful controls, but they do not
replace per-action roots, environment, descendants, network, write, and
privilege policy.

Remediation:

- define separate immutable process profiles for Codex, Meshy/Python, Git,
  GitHub CLI, MCP, and OS openers;
- require a canonical allowed working root and revalidate its opened identity
  at spawn;
- give each profile only the environment entries it needs;
- use OS job/process-group supervision that terminates descendants on Windows
  and macOS;
- derive OS-owned executable locations from trusted platform APIs or verified
  fixed identities, not a mutable environment variable;
- make network, expected writes, and privilege behavior explicit and enforced,
  not only displayed as `not_declared`.

Regression tests:

- working directory outside or swapped out of the approved root;
- ambient credential variables excluded;
- per-profile HOME/PATH/app-data behavior;
- child and grandchild timeout/termination;
- simultaneous oversized stdout/stderr and redaction at truncation boundaries;
- OS-root environment spoofing;
- no elevation and accurate network/write disclosure.

### Low SEC-L01 — redirect trust is enforced but final redirect evidence is not persisted

The source client permits at most three redirects and follows only HTTPS
`api.github.com` or `raw.githubusercontent.com` destinations; it checks the
final response URL again (`source.rs:87-100`, `116-159`, `611-620`). Exact
revision, manifest hash, selected-file size, and selected-file SHA-256 are
persisted, so no content-integrity bypass was found.

`resolve_source` records repository, mode, exact revision, requested ref,
release, manifest hash, and manifest origin, but not initial URL, final URL, or
redirect chain (`source.rs:673-697`). This weakens incident and cache evidence
when a permitted redirect occurs.

Remediation: record a bounded, sanitized initial/final host and redirect count
in source-resolution evidence, without query data or credentials. Test allowed
and denied redirect hosts, loop/count bounds, and exact final identity.

### Low SEC-L02 — Windows release signature verification uses a subject substring

The PFX import step selects exactly one certificate whose subject equals the
configured signer (`release.yml:99-103`), which is strong. The later package
verifier accepts any valid Authenticode certificate whose subject merely
contains the configured value (`scripts/release_verify.mjs:206-218`).

This is defense in depth because the same job normally produced the package,
but it is weaker than the exact signer identity claimed by release evidence.

Remediation: bind and compare an exact normalized subject plus certificate
thumbprint/public-key identity, and carry that identity into curated provenance.
Test similarly named signer subjects and a valid signature from the wrong
certificate.

## Credential leakage and Codex App Server checks

### Verified controls

- Windows/macOS production storage uses `keyring::Entry` with a fixed service;
  application persistence receives only `CredentialReference`
  (`credentials.rs:89-152`).
- Meshy references contain a UUID in the expected namespace and are restricted
  to the current platform (`credentials.rs:194-224`).
- hosted-provider references are deterministic, opaque, provider-scoped, and
  rejected across providers (`credentials.rs:226-278`).
- secret values are bounded, non-empty, control-character-free, and never
  serialized as part of the reference (`credentials.rs:281-319`).
- Meshy environment construction accepts only `MESHY_API_KEY`; child output is
  redacted against the exact known value and credential-shaped patterns
  (`credentials.rs:322-370`; `security.rs:397-433`).
- JSON persistence rejects named secret fields and credential-shaped serialized
  text before atomic write (`security.rs:384-470`).
- generated project state records provider/model and non-secret authentication
  status but no email, account ID, plan, usage data, token, provider-vault
  reference, or provider key (`commands.rs:2671-2708`).
- installation plans carry only validated Meshy opaque references; hosted AI
  provider vault references are reconstructed in memory and do not enter plan
  or lock (`commands.rs:2649-2665`, `2861-2910`;
  `docs/31_ai_provider_profiles_and_chat_sources.md:40-46`).

Codex-specific:

- Codex integration exposes no API-key field or endpoint route; hosted
  provider credential commands explicitly reject `codex`
  (`commands.rs:521-565`, `695-724`).
- the Codex module has no token-file reader or externally managed token mode;
  the normal path uses `account/read` with refresh disabled, accepts planning
  only for `chatgpt`, starts browser/device login with the required types, and
  signs out through `account/logout` (`codex.rs:227-340`;
  `commands.rs:341-386`).
- App Server transport is local stdio JSONL, clears inherited environment
  before adding its reviewed profile, discards stderr, bounds each JSONL line
  to 2 MiB, bounds notifications, and reports protocol errors by method/code
  rather than raw provider payload (`codex.rs:342-373`, `1255-1477`).
- analysis starts a dedicated read-only thread with no readable project roots
  and `approvalPolicy: never` (`codex.rs:376-418`, `699-707`).
- brief, constraints, and approved excerpts are bounded, redaction-checked,
  path-filtered, hash-bound, and limited to core-approved evidence
  (`codex.rs:574-672`; `commands.rs:748-846`).
- output rejects extra fields, unapproved evidence references,
  credential/account-shaped content, malformed identifiers, and missing
  required proposals (`codex.rs:780-1020`).
- live email/plan/usage values are transient account UI state as permitted by
  the accepted design; persisted records set
  `account_identity_persisted=false`, omit account values, and migration rejects
  persisted account identity (`codex.rs:443-462`, `2073-2112`;
  `migrations.rs:397-426`).
- rollback, journal read, interrupted-transaction discovery, resume, and
  staging discard are local commands with no account guard
  (`commands.rs:4336-4434`). The signed-out Welcome route exposes local project
  recovery/removal (`src/App.tsx:1138-1166`, `1609-1631`).

### Remaining credential evidence gaps

- SEC-H01 means the application has not proved that the process receiving the
  Meshy key or accessing Codex account storage is the intended executable.
- no native Windows Credential Manager or macOS Keychain integration test was
  run; current tests use `MemoryCredentialStore`.
- no route-level test proves signed-out recovery through the packaged desktop
  app.
- no crash/panic, screenshot, support-bundle, diagnostic-log, or binary-fixture
  leakage suite exists.
- redaction tests cover known values and common shapes, not split/encoded
  values, arbitrary device/user codes without labels, or every provider key
  family.
- no test serializes every plan, lock, state, journal, readiness, migration,
  and Tauri error variant with canary secrets and account metadata.

## Filesystem, archive, and process checks

Verified:

- relative destinations enforce static path bounds, common Windows reserved
  names, ADS rejection, and canonical collision keys
  (`security.rs:15-105`);
- duplicate destination checks use canonical keys
  (`security.rs:256-289`);
- external destinations require absolute file paths and reject existing links
  and non-files (`security.rs:145-236`);
- process arguments are arrays; stdin is null; timeouts are 1 second to 1 hour;
  each captured stream is capped at 16 MiB maximum configuration and redacted
  before return (`process.rs:42-94`, `107-189`);
- current core routes do not request administrator/root privilege.

Not established:

- race-safe root containment and cache identity: SEC-H03;
- independently trusted executable identity: SEC-H01;
- safe Git subprocess behavior: SEC-H02;
- per-action working roots, minimal environments, descendants, and privilege
  declarations: SEC-M03;
- full Unicode case folding rather than NFC plus lowercase
  (`security.rs:101-105`);
- restrictive permissions for application data, transaction directories,
  private health scripts, backups, and staging;
- archive entry count, per-entry/total uncompressed size, ratio, depth,
  sparse-file, link, and post-extraction containment tests. No archive route is
  reachable, so archive support must remain disabled.

## Supply-chain, GitHub Actions, and release checks

Verified:

- latest source resolves the repository default branch to one exact commit and
  verifies the commit object; pinned commit and bounded release-tag
  dereferencing also end at a verified commit (`source.rs:188-294`);
- manifest, tree, and selected files are addressed by that revision
  (`source.rs:296-378`, `673-697`);
- redirects are HTTPS/host/count bounded;
- source tree and selected payloads have file-count and byte limits, and every
  selected non-generated file requires size and SHA-256 evidence
  (`source.rs:307-349`, `1238-1427`);
- verified cache keys include revision and expected content hash, and bytes are
  rechecked before promotion (`source.rs:383-446`);
- no complete source-repository clone, application updater, or archive download
  route was found;
- all workflow third-party actions are pinned to full immutable SHAs;
- CI/security pull requests use read-only contents permission and do not
  receive release secrets;
- stable publication write permission is isolated from matrix build jobs and
  artifact curation requires exact source revision and complete manifests.

Gaps:

- remote-manifest policy enforcement: SEC-H04;
- cache and path race safety: SEC-H03;
- release tag/secret sequencing: SEC-H05;
- preview ref/publication policy: SEC-M01;
- release secret-scanner assurance: SEC-M02;
- Windows exact signer identity: SEC-L02;
- no repository-owned automated policy test checks action pins, fork behavior,
  job permissions, environment use, tag/ref policy, secret sequencing, or
  artifact signer/provenance requirements;
- updater signature, metadata expiry, rollback/freeze, channel separation, and
  artifact tests remain absent because the updater is not implemented;
- support-bundle redaction/minimization tests remain absent because support
  bundles are not implemented.

## Missing security tests

Highest-priority missing regression evidence:

1. Independent executable identity for Codex, Python, Git, GitHub CLI, MCP, and
   system openers; PATH shadowing and verify/spawn replacement.
2. Git scan/config/hook isolation, including fsmonitor, filters, config
   includes, linked hooks, templates, credential helpers, transport helpers,
   and pre-push.
3. Handle-relative path races on Windows and macOS at every
   validate/open/create/hash/read/rename/delete/rollback/cache boundary.
4. Runtime JSON Schema enforcement and schema-versus-Rust differential
   manifest validation.
5. Release policy tests for protected revision/tag ancestry, secret-free
   preflight, split unsigned build/sign, narrow OIDC, protected publication,
   and fork-secret behavior.
6. Preview dispatch rejection for non-default/unreviewed refs.
7. OS-vault integration and no-secret serialization across state, plan, lock,
   journal, readiness, migration, error, panic/crash, logs, screenshot policy,
   fixtures, Git, and future support bundles.
8. Route-level fake App Server tests proving official executable identity,
   browser and device-code flows, cancellation, logout, usage limits,
   interruption, output rejection, protocol redaction, and packaged signed-out
   recovery.
9. Fake HTTP tests for default-branch resolution, commit object types,
   redirects/final URL, body limits, truncated trees, selected-only fetches,
   checksum/size mismatch, cache compromise, interruption, and range resume.
10. Native path collision tests for full Unicode case folding, Windows reserved
    names/ADS/reparse variants, and case-sensitive/case-insensitive macOS
    volumes.
11. Process root/environment/network/write/privilege profiles, output bounds,
    and descendant termination.
12. History-aware secret-scanner fixtures and binary/screenshot policy tests.
13. Archive bomb/path/link/count/size/ratio/depth tests before archive support.
14. Updater signature/expiry/rollback/freeze/channel tests before updater
    support.

## Recommended fix order

1. **Protect release authority first (SEC-H05, SEC-M01, SEC-L02).** Add a
   secret-free protected-revision preflight, split unsigned build from narrow
   signing, restrict OIDC, protect stable tags and publication environments,
   pin preview publication to an approved main revision, and bind exact signer
   identity. This prevents a source/dependency compromise from reaching signing
   authority while lower layers are repaired.
2. **Replace PATH self-authorization (SEC-H01).** Introduce independently rooted
   executable identity objects and fail closed for Codex/Meshy/Git routes that
   lack them. Run PATH-shadow and replacement tests before restoring
   secret-bearing actions.
3. **Neutralize Git’s execution surface (SEC-H02).** Make initial inspection
   direct/read-only where possible and isolate every required Git invocation
   from hooks, fsmonitor, filters, helpers, include chains, and unsafe
   transports.
4. **Make containment handle-relative (SEC-H03).** Convert project, cache,
   staging, private-script, atomic replace, deletion, and rollback paths to
   no-follow handle-relative operations with adversarial Windows/macOS race
   tests.
5. **Enforce the authoritative manifest contract (SEC-H04).** Add runtime JSON
   Schema validation, explicit unsupported-policy rejection, and differential
   tests before trusting future remote manifests.
6. **Constrain each process profile (SEC-M03).** Bind working roots, environment,
   descendant supervision, network, writes, and privilege behavior per tool.
7. **Broaden credential-leakage gates (SEC-M02).** Scan tracked/staged/history
   and bounded binary/screenshot/support artifacts with fixture-tested,
   value-free reporting.
8. **Close remaining evidence gaps (SEC-L01 and missing tests).** Persist
   sanitized redirect identity, add native vault/App Server/signed-out recovery
   E2E, and keep archive, updater, and support-bundle routes disabled until
   their security gates exist.

## Validation executed

Passed:

- `python scripts/check_committed_secrets.py`
  - result: `No committed secret patterns found.`
- `python scripts/validate_repository_templates.py`
  - result: `Validated 12 integrity groups for full planning package.`
- static workflow action-ref check
  - 4 workflow files, 0 mutable third-party action refs.
- read-only high-signal Git history scan
  - 34 commits; no non-placeholder configured high-signal secret match.
- focused Rust security modules at exact HEAD:
  - `security::tests`: 5 passed;
  - `credentials::tests`: 6 passed;
  - `process::tests`: 4 passed;
  - `source::tests`: 14 passed;
  - `git::tests`: 7 passed;
  - `codex::tests`: 23 passed;
  - `ai::tests`: 7 passed;
  - `migrations::tests`: 14 passed.

Inconclusive:

- the full all-feature Rust command exceeded the first tool window after
  reaching unit, binary, and doc-test phases; the closed output pipe caused the
  doc-test command to return failure;
- a library-only rerun and a transaction-test filter also exceeded their
  bounded audit windows;
- therefore this report does **not** claim a full Rust-suite pass.

Not run:

- native Windows/macOS vault E2E;
- packaged desktop App Server/login/recovery E2E;
- network source tests;
- release workflow, signing, notarization, or publication;
- fuzz execution (the fuzz targets were inspected and compile coverage exists
  in CI, but no fuzz campaign was run).

## Change statement

No application code, tests, schemas, workflows, release scripts, security
policy, or skills were edited. Parallel worktree changes were preserved. The
only file authored by this audit is
`docs/development/handoffs/full-reaudit-2026-07-29/security.md`.
