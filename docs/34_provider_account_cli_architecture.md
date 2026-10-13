# Experimental provider-account CLI architecture

## Status and scope

This is an accepted architecture decision for the experimental Gemini and
Grok account routes. It documents the boundary being implemented and the
remaining live-test gate; it does not claim that either real account flow has
passed release validation. The hosted API profiles remain the stable bounded
routes:

| Profile | Official route | Checked-in default |
| --- | --- | --- |
| `gemini` | Google OpenAI-compatible API at `https://generativelanguage.googleapis.com/v1beta/openai/chat/completions` | `gemini-3.8-flash` |
| `grok` | xAI Responses API at `https://api.x.ai/v1/responses` | `grok-4.7` |

The account profiles are separate IDs, `gemini_account` and `grok_account`.
The current setup screen exposes them as experimental options with sign-in,
Check again, cancellation, sign-out, and a switch to the corresponding API-key
route. They use the official provider CLIs and never turn an account login into
an app-owned API-key route. Their readiness and release status remains
experimental until the completion gate below passes.

## Decision

1. The provider CLI owns interactive login, OAuth or OIDC state, token storage,
   refresh, and logout. HOI4 Mod Setup never reads or copies provider tokens,
   authorization codes, account identifiers, email, plan, usage, or rate-limit
   data.
2. The package is fetched only after the user explicitly chooses **Sign in**.
   `src-tauri/resources/account-cli-pins.json` supplies the package URL, npm
   integrity value, archive member, and entry SHA-256. The adapter verifies the
   archive and entry before starting a CLI. npm lifecycle scripts, package
   managers, redirects, and PATH shims are not trusted or executed.
3. Each provider receives an app-private data root and isolated home. The
   analysis working directory is empty and contains no target-project path.
   Grok receives approved input through a private ephemeral `--prompt-file`,
   never argv; the file is cleaned up on success, error, and cancellation.
   The adapter disables tools, extensions, MCP servers, skills, hooks, plugins,
   web search, and other write-capable features according to the provider's
   reviewed CLI policy.
4. Provider-owned session persistence is explicitly permitted only inside the
   isolated provider data root and only for approved setup inputs. The app does
   not retain raw model output or diagnostics, or copy session history into
   project files, plans, locks, logs, crash reports, or source packages. No
   account-CLI analysis can mutate the target project or approve a transaction.
5. Account status is a bounded capability result. A missing package, missing
   trusted runtime, sign-in failure, cancellation, timeout, unsupported model,
   or usage failure leaves the draft intact and blocks new semantic planning;
   recovery, rollback, backup inspection, and managed removal remain local.

## Provider flows

Gemini follows the official interactive **Sign in with Google** flow. The
current worker opens a terminal helper, uses a private Gemini CLI home, and
asks the user to finish sign-in there. The setup analysis runs with a single
bounded turn and no extensions, MCP, skills, hooks, telemetry, or write tools.
The official references are [Gemini authentication](https://geminicli.com/docs/get-started/authentication/),
[configuration](https://geminicli.com/docs/reference/configuration/), and
[session management](https://geminicli.com/docs/cli/session-management/).

Grok follows the official `grok login` and `grok logout` flow. The current
worker uses an account-only policy, a single read-only analysis turn, and a
private Grok home. The official references are the [CLI reference](https://docs.x.ai/build/cli/reference)
and [enterprise authentication](https://docs.x.ai/build/enterprise).
The app currently exposes the browser-login route only; it does not expose a
Grok device-code fallback. Codex's separate device-code route is unchanged.

The account-CLI uses the official Grok Build fallback `grok-build`, while the
hosted API profile is `grok-4.7`. These values must not be conflated. The
account-CLI model remains subject to the account-adapter worker's live checks.

## Platform boundary

The current terminal helper is verified only on Windows and Linux. macOS is
reported unavailable until a verified provider runtime and interactive terminal
route exists; the presence of a package pin alone does not establish support.
Gemini additionally requires a trusted, publisher-verified Node.js runtime.
Grok uses a pinned provider binary. Do not invent a macOS route or claim that
the account profiles are supported there without platform evidence.

## Completion gate

Before the account routes are described as stable, the parent integration must
complete and record:

- provider-owned browser or terminal sign-in and logout on each supported OS;
- cancellation, timeout, retry, and a second sign-in without cross-attempt state;
- empty-workspace, no-tool/MCP/hook/plugin, private-session, and no-target-write
  checks, including ephemeral prompt-file cleanup on success, error, and
  cancellation;
- package SRI, archive path, entry digest, runtime publisher, and spawn-time
  re-verification;
- output-schema acceptance, rejection, truncation/refusal handling, redaction,
  and no-secret persistence tests; and
- a real-account smoke when an authorized tester has an account; its absence
  remains an explicit experimental limitation rather than a request for credentials.

As of this decision, no real Gemini or Grok account has been tested. The user
accepted that limitation for these experimental routes. Mocked lifecycle and
security checks, verified-package probes, and honest platform states remain
required before release. Prompt cleanup covers success, error, cancellation,
and a bounded no-follow sweep of recognized interrupted-run residue; unrelated
content and provider credentials are never swept.

