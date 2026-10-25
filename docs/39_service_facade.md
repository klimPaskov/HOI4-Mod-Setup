# Service facade for server hosts

`hoi4_mod_setup::service` lets a server host Setup's analysis, approval and
transaction gates on server paths, for many accounts at once. The desktop
commands do not change: they reach the same gate code through the
process-wide stores they have always used.

## Why a facade

The desktop commands in `commands.rs` are private, compile only with the
`desktop` feature, and keep their state in process-wide stores:

- `PREPARED_PLANS` holds every prepared plan.
- `CODEX_ANALYSES` holds every analysis and its confirmation.
- `CODEX_APPROVED_EVIDENCE` holds one entry, the most recent scan, so a
  second account's scan would replace the first account's approval.

They read credentials from the operating system vault, and they derive every
journal, backup and staging path from the user's application-data folder.
None of that is safe to share between accounts. The public `scanner`,
`transaction` and `paths` modules already work on server paths, but the gates
that bind a scan, an analysis, a plan and an apply to one another lived only
in the command layer, and the root-identity token that `run_transaction`
requires had no public constructor.

## Shape

| Item | Role |
|---|---|
| `service::SetupService` | Dependencies injected once and shared: a `CredentialStore`, an optional `PlanBuilder`, an optional `NodeRuntime`, an optional post-install runner. |
| `service::SessionConfig` | A workspace root that every project path must be inside, and a state root for journals, backups and staging. Both must exist, be absolute and link-free, and must not contain each other. |
| `service::SetupSession` | One onboarding session. It owns the latest scan evidence, the analyses, the prepared plans and the approvals. |
| `service::root_identity`, `SetupSession::root_identity` | The identity token of a directory. |
| `service::plan_sha256` | The digest a reviewer approves. It is the digest the journal records as `plan_sha256`. |
| `service::PlanBuilder`, `PlanRequest`, `BuiltPlan` | The seam through which a host supplies the plan. |
| `service::NodeRuntime`, `service::runtime::mcp_client_launch` | The Node runtime the MCP registration needs, as a verified value. |
| `credentials::DeniedCredentialStore` | A store that refuses everything, for services that need no provider credential. |

## Operations

The operations follow a session in order.

| Operation | Gate it keeps |
|---|---|
| `inspect` | Read-only. The root must stay the same directory object for the whole scan. A new scan, a failed scan and a stopped or partial scan all withdraw earlier evidence approval. No launcher discovery runs. |
| `approve_evidence` | Every item must be a value the core scanned in the latest scan of the same root, and the scan id must be current. |
| `analysis_turn`, `analyze`, `record_analysis` | Evidence in the request must be the approved evidence. `analyze` uses only the injected credential store and refuses the providers that run a local account tool (`codex`, the Claude account route, the Gemini and Grok account routes). `record_analysis` accepts a reply the host obtained itself, through the same deterministic validator. In both the analysis identifier is assigned by the core and the result is bound to the resolved source revision. |
| `confirm_analysis` | Only an analysis this session retained can be confirmed, with the exact values the reviewer saw. |
| `build_plan` | The analysis must be confirmed in this session, for the same provider, model, reasoning effort, scan, root and reviewed values. The builder receives the verified facts and its plan is checked: it must carry the verified root identity and the confirmed analysis, arrive unapproved, and write nothing outside the project root. |
| `decide_conflict` | Only a decision the plan offered. Any decision withdraws an earlier approval. |
| `approve_plan` | The caller passes the digest of the plan the reviewer saw. A different stored plan approves nothing. The approval binds the post-approval digest and the root identity. |
| `apply` | Requires the approval, the same digest, the root still inside the workspace, and the same directory object the plan bound. The attempt consumes the approval. The journal goes to the state root. |
| `inspect_recovery`, `read_journal` | Read the native journal. A journal bound to another root is refused. |
| `recover` | Only the single action the transaction permits for that journal. After a partial apply that is rollback alone; resume and discard are refused. |

## What is shared with the desktop

The gate logic moved into `service::evidence` and `service::plans` and is
called by both paths with explicit state: the scan-evidence record, evidence
approval, the analysis request check, analysis confirmation, the confirmed
analysis a plan needs, the plan's root-identity binding, plan storage,
conflict resolution, flattened-output refresh and approval. The desktop
commands lock their process-wide stores and call these functions, so a change
to a gate applies to both. The functions' behavior and error text are
unchanged.

## Credentials

`keyring` is now an optional dependency behind the `os-keyring` feature. It is
on by default and implied by `desktop`, so desktop builds are unchanged. A
server builds with `--no-default-features`, which removes `keyring` and, on
Linux, the native `libdbus` dependency. `OsCredentialStore` then refuses every
request, and the service reads credentials only from the `CredentialStore` it
was given.

## Node

The only Node dependency inside Setup's own planning code is the launch
command registered for the HOI4 Agent Tools MCP server on Linux. It was
`/usr/bin/node` plus a per-user npm prefix, found implicitly. It is now a
`NodeRuntime`, checked with the same publisher verification as before, and the
service passes it to the plan builder in `PlanRequest::node`. The desktop
builds its runtime with `NodeRuntime::linux_system()`, which is the previous
behavior.

## The default plan builder

`service::DefaultPlanBuilder` is the desktop's plan builder, moved out of the
command layer so a server can run it. `build_plan_with` in
`service::builder` is the same function the desktop calls, and the helpers that
read the reviewed state (`service::inputs`) are shared. The builder takes every
ambient input from a `PlanHost`:

| Input | Desktop host | Service host |
|---|---|---|
| Project root, its identity token and the confirmed analysis | Process-wide scan and analysis stores | The `PlanRequest`, which the session already verified |
| Backups and staging | The user's application-data folder | `<state root>/backups` and `<state root>/staging` |
| Workflow package | `HttpSourceClient::new()` (user cache, optional local checkout) | A `SourceFetcher`: `DefaultPlanBuilder::new(cache_root)` uses the public repository with verified blobs under `cache_root`; `with_source` takes any other |
| Node runtime in coding-client MCP configuration | Detected on this machine | `PlanRequest::node` |
| Game and Workshop folders in project instructions | Detected installation | The placeholders `[HOI4_GAME_DIR]` and `[STEAM_WORKSHOP_DIR]` |
| Launcher registration for a new project | Required | Refused: it lives outside the project root |
| 3D workflow, kept context documents and images | Vault, app-owned launcher, attachment store | Refused |

`SourceFetcher` is the seam for the package. Integrity does not depend on it:
`resolve_source` validates the manifest it returns, and the planner checks every
selected file against the manifest's size and SHA-256 before it is staged. The
process-wide manifest and resolution caches are used only by a source that
reports `shares_process_cache`, which only the public-repository client does.
`HttpSourceClient::for_cache_root` never consults the developer's local
checkout variable.

The desktop's plan for the same state is unchanged. Before the move, plans for
a new project and for an import with each conflict choice were built from an
offline package and compared with the plans built after it: operations,
conflicts, generated artifacts, external actions and prepared-file hashes were
identical apart from per-run directory-identity tokens.

## Not in this change

- A new project cannot be applied through a server yet. The transaction's
  readiness gate requires a launcher registration for every new project, and
  the registration is outside the project root. Plans build and can be
  approved; `apply` stops at readiness. An import applies. Resolving this
  needs a decision on whether a server-created project is launcher-ready.
- Context documents and images are refused: their store is process-wide.
- Plans that write outside the project root (a launcher folder, user-level
  client configuration) are refused.
- Post-install actions run only when the host supplies a runner. The desktop
  runner is still in the command layer.
- Maintenance plans (update, repair, remove).
