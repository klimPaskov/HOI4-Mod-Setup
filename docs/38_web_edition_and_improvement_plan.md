# Web edition and improvement plan

Status: **Accepted** (2026-10-18). The five decisions at the end were accepted
as recommended. Phases land in order; until a phase ships, the desktop contract
in `AGENTS.md` and the numbered design documents describe current behavior.

## Goals

1. Offer HOI4 Mod Setup in the browser as a static GitHub Pages site. The user
   drops a mod folder to import it, reviews everything in the page, and
   downloads a ZIP. Nothing is uploaded and no server is involved, which makes
   the tool easy to trust.
2. Make the desktop and web app faster, more accurate, and smoother, using the
   bottlenecks found in the current code.

## Verified facts (checked 2026-10-18)

| Endpoint or API | Browser access | Consequence |
| --- | --- | --- |
| `api.github.com` | CORS `*` | Source resolution works from the page |
| `raw.githubusercontent.com` | CORS `*` | Manifest and file downloads work |
| `codeload.github.com` ZIPs | Not allowed | Not needed; the app downloads files selectively |
| `github.com/...git/info/refs` | No CORS | The rate-limit fallback cannot run on the web |
| `cdn.jsdelivr.net/gh/...@<sha>` | CORS `*` | Possible mirror for files once the commit is known; branch resolution failed for this repository |
| Anthropic Messages | CORS with `anthropic-dangerous-direct-browser-access` | Claude API key route works |
| OpenAI Responses, xAI, Gemini, DeepSeek, GLM | CORS allowed | API key routes work |
| Moonshot (Kimi) | No CORS headers | Kimi is desktop-only |
| Claude Code, Codex App Server, Gemini/Grok CLIs | Local processes | Account routes are desktop-only |
| Folder drop | `webkitGetAsEntry` in Chromium, Firefox, Safari; `getAsFileSystemHandle` in Chromium only | Read-only folder import works in all three engines |

Unauthenticated GitHub REST allows 60 requests per hour per IP. Resolution
needs 2 to 5 calls, so normal use fits; heavy use needs an honest limit message
or an optional token held only in memory.

## Part 1: web edition

### How it works

1. **Load.** A static bundle from GitHub Pages: the React interface plus the
   setup core compiled to WebAssembly, running in a Web Worker.
2. **Choose.** New mod, or import an existing one by dropping its folder (or
   picking it with a directory input). The browser reads the files into a
   bounded in-memory tree; nothing leaves the machine.
3. **Scan.** The same Rust scanner and project profile run in the worker
   against that tree and produce the same findings, evidence, and limits as on
   desktop.
4. **Describe.** Optional setup assistant through a browser-capable API key
   provider. The key stays in memory for the session and is never stored or
   exported. Context documents and images work as on desktop.
5. **Resolve and download.** Exact commit resolution and selective,
   SHA-256-verified downloads from GitHub, cached in IndexedDB by hash.
6. **Review.** The same plan, merge classification, and conflict review, with
   the dropped folder as the local side.
7. **Export.** A ZIP, built in the worker:
   - new mod: the complete project folder plus `<id>.mod` with the relative
     `path="mod/<id>"` form the launcher accepts, and a short README on where
     to extract it;
   - existing mod: by default an overlay ZIP with only new and changed files,
     a `CHANGES.md` list, and the installation lock; optionally a full patched
     copy, and a ZIP of the original files that the overlay replaces.

### Architecture

- **One core, two hosts.** Move the portable Rust into a `hoi4-setup-core`
  workspace crate used natively by Tauri and compiled to `wasm32` for the web.
  Portable now: `merge`, `migrations`, `models`, `coding_environment`,
  `live_testing`, `native_mcp`, `descriptors`, and the validation, prompt, and
  schema parts of `codex`. Portable behind adapters: `source`, `scanner`,
  `project_profile`, `context`, `security` (redaction and path rules are pure),
  `readiness`, `ai`, and the planning half of `transaction`.
- **Adapters the core needs** (also the fake adapters `AGENTS.md` already
  promises for tests): `ProjectFs` for bounded reads of a project tree,
  `SourceClient` for HTTP, `Clock` and `Rng` (wasm has no `Instant::now`, and
  `uuid` v4 and `chrono` need JS-backed randomness and time), `BlobCache`, and
  a `LockSink` so `build_lock` no longer needs `RootedDir`.
- **Interface bridge.** `src/lib/tauri.ts` already routes every command through
  `invokeCommandResult`. Add a `Backend` interface with `TauriBackend` and
  `WebBackend` implementations, and let the web build hide desktop-only screens
  instead of showing "desktop bridge is unavailable".
- **Desktop-only, by design:** OS credential vault, Claude Code and Codex
  sign-in, provider CLIs, Git, MCP bootstrap and health checks, ComfyUI
  discovery, writing into Documents, the updater, and "Open in client". The web
  ZIP contains the same files plus a post-extract checklist (run the HOI4 Agent
  Tools bootstrap, approve the MCP server, run `git init` if wanted).

### What stays true

- Exact-revision resolution, selective downloads, SHA-256 verification,
  manifest schema checks, path containment, duplicate-destination and platform
  rejection.
- Merge rules: base, local, and incoming comparison; keep, replace, merge,
  rename, skip; binary files never text-merged.
- Ownership types and the lock, written inside the ZIP at
  `.hoi4-mod-setup/install.lock.json`.
- Deterministic facts stay separate from proposals until confirmed.

Staging, backup, journal, resume, and rollback guard in-place writes. The web
edition never writes into the user's folder, so the original folder is the
backup and the ZIP is the reviewed result. This is the main change to the
transaction contract and needs an explicit decision.

### Trust and security on the web

- No server. Every network request goes to GitHub or the selected AI
  provider, enforced by a strict Content Security Policy `connect-src` allowlist.
- A visible network panel lists every request the page made.
- Versioned, reproducible builds; the Pages deployment publishes the bundle's
  SHA-256 and the commit it came from, and scripts carry Subresource Integrity.
- API keys live in memory only, are never written to storage, the ZIP, or logs,
  and the page states that a browser key is visible to the page.
- Dropped files are read with the same bounds and redaction as desktop scans;
  credential-shaped content still never reaches a provider.

## Part 2: faster, more accurate, smoother

### Faster

| Change | Evidence | Size |
| --- | --- | --- |
| Compile each JSON Schema validator once | `jsonschema::draft202012::new` runs per call at `source.rs:1198`, `codex.rs:1389`, and per generated file at `transaction.rs:3547` | S |
| Resolve the source once per session | `resolve_source` runs separately for analysis binding, the bundled check, preview, planning, and maintenance (`commands.rs:1705`, `:2761`, `:4444`, `:7639`, `:9289`, `:9653`, `:10029`) | M |
| Remove duplicate interface calls | `previewSourceManifestResult` twice (`App.tsx:3189`, `:3332`), automatic dry-run rebuilds (`App.tsx:1430`), maintenance rescans (`App.tsx:1172`, `:1473`) | S |
| Walk the project once | The scanner (`scanner.rs:448`) and the profile (`project_profile.rs:493`) each walk the whole tree | M |
| Hash and read in parallel | Prefetch uses 8 threads, but staging re-reads files one at a time (`commands.rs:7701`, `:7735`) | M |
| Compact the lock | Generated files are stored twice, as text and as a JSON number array (`models.rs:1043`), pretty-printed (`transaction.rs:2659`) | M |
| Prefetch while the user reviews | Resolution and downloads can start once the source is known, not when the plan is requested | M |
| Incremental maintenance scans | Keep a size and timestamp index in app data, never in the project | L |

### More accurate

| Change | Why | Size |
| --- | --- | --- |
| Decide what can be decided | Descriptor tags, dominant namespace and prefix, localisation convention, and folder profile can come from parsed facts (`scanner.rs:1931`, `project_profile.rs:600`, `:640`) and show as Detected; the assistant fills only real gaps | M |
| Sample what carries meaning | Samples are hash-ordered; stratify by category and prefer focus trees, events, decisions, on_actions, and localisation | M |
| Evaluation harness | Golden set (pinned Kaiserreich, small and medium public mods, synthetic cases) with expected proposals; recorded-response replays in CI and opt-in live runs; track field accuracy, corrective-turn rate, and failure categories per provider | L |
| Provider-native strict output | Use each provider's schema-constrained mode where offered; keep the normalizer and validator authoritative | M |
| Shareable diagnostics | A "Copy diagnostics" action for the bounded reasons already logged | S |
| Optional image understanding | Send context images to vision-capable providers only when the user opts in | M |

### Smoother

- Skip screens that hold no decision (for example Integrations when nothing
  needs input) and show one review summary instead.
- Show progress by real stage, with background work continuing while the user
  edits.
- Keep a draft across restarts so an interrupted setup resumes where it left off.
- Add visual regression snapshots for every screen at desktop and narrow widths.

### Engineering health

- Add the missing adapter traits; tests then run against fakes instead of temp
  directories, which also unblocks the web core.
- Split `commands.rs` (16.5k lines) and `transaction.rs` (17k lines) by feature.
- Property tests and fuzzing for descriptor, launcher-path, and merge parsing.
- Performance budgets in CI for small, medium, and Kaiserreich-sized projects.

## Progress

- Phase 1 shipped on 2026-10-18: compile-once JSON Schema validators, a
  per-revision manifest cache, manifest reuse on the Components screen, and
  Copy diagnostics.
- Phase 2 started: `crates/hoi4-setup-core` holds `AppError`, the schema
  cache, the native MCP formats, the pure path, hashing, and redaction rules,
  the adapter traits (`ProjectFs`, `SourceClient`, `Clock`, `Entropy`), and
  `MemoryFs`. CI builds and lints it for `wasm32-unknown-unknown`. Models,
  descriptors, merge, coding clients, live testing, and the scanner move next,
  behind the same adapters.

## Roadmap

Each phase ships on its own and keeps desktop parity.

1. **Quick wins.** Cached validators, one source session, fewer duplicate
   calls, diagnostics copy. Exit: measured plan-preparation time down, all
   hosted gates green.
2. **Core extraction.** `hoi4-setup-core` crate and adapter traits, desktop
   unchanged. Exit: core builds for native and `wasm32-unknown-unknown` in CI.
3. **Accuracy.** Deterministic proposals, stratified sampling, evaluation
   harness. Exit: harness baseline recorded and field accuracy improved on the
   golden set.
4. **Web preview.** Web shell, folder drop, scan, assistant through API keys,
   plan and conflict review, ZIP export for new and existing mods. Exit:
   Playwright passes on Chromium, Firefox, and WebKit in GitHub Actions; ZIPs
   install a launcher-loadable mod.
5. **Web release.** CSP and SRI, IndexedDB cache, rate-limit handling, network
   panel, accessibility pass, versioned Pages deployment. Exit: security and
   accessibility gates pass and the published bundle hash matches the build.
6. **Polish.** Lock compaction migration, single-walk scanning, screen
   streamlining, performance budgets. Exit: budgets enforced in CI.

## Decisions (accepted 2026-10-18)

1. Should the web edition (and desktop) offer a deterministic-only path when no
   setup assistant is available? Recommended: yes, clearly labelled.
2. Default export for an existing mod: overlay ZIP (recommended) or full copy.
3. API keys in the browser: memory-only with a visible warning (recommended),
   or no assistant on the web.
4. Hosting: `klimpaskov.github.io/HOI4-Mod-Setup/app/` from this repository
   (recommended) or a separate repository.
5. Accept that account sign-in routes, MCP bootstrap, Git, and in-place
   transactions stay desktop-only, and record the web ZIP flow as an accepted
   amendment to the transaction contract.

## Risks

- Large mods in browser memory: bound reads as desktop does and stream ZIP
  output; very large folders may need Chromium's directory handles.
- GitHub rate limits from shared networks: explain the limit and offer an
  in-memory token.
- Provider CORS can change: probe on load and hide routes that stop working.
- Two hosts can drift: one core crate and shared fixtures keep them aligned.
