# AGENTS and subagent architecture

## Repository AGENTS

The root `AGENTS.md` governs development of HOI4 Mod Setup itself. It defines product invariants, architecture ownership, security boundaries, transaction rules, UI constraints, skill maintenance, Git expectations, and subagent routing.

This repository instruction file must stay distinct from the `AGENTS.md` that HOI4 Mod Setup installs or adapts inside a target mod project.

## Subagent purpose

Subagents are useful for high-risk audits and bounded maintenance that can be reviewed independently. They should reduce missed surfaces without splitting final ownership.

Every project subagent is spawned with `fork_context=false`. The parent prompt includes:

- task goal
- exact files or modules
- product constraints
- accepted design decisions
- platform scope
- allowed writes
- forbidden writes
- required tests or evidence
- handoff path

The parent reviews every output and owns final integration and completion.

## Included subagents

| Subagent | Mode | Use |
| --- | --- | --- |
| `hoi4setup_codex_integration_auditor` | read-only | Audit ChatGPT authentication, App Server lifecycle, structured analysis, redaction, and deterministic proposal validation |
| `hoi4setup_source_manifest_auditor` | read-only | Audit source revision, manifest, downloads, hashes, component graph, wiki distribution |
| `hoi4setup_scanner_auditor` | read-only | Audit read-only scan behavior, evidence, confidence, and finding review |
| `hoi4setup_transaction_recovery_auditor` | read-only | Audit journal, stages, operation checkpoints, recovery, rollback, and fault tests |
| `hoi4setup_security_auditor` | read-only | Audit credentials, commands, paths, archives, logs, supply chain, and GitHub Actions |
| `hoi4setup_ui_accessibility_auditor` | narrow patch | Audit UI density and accessibility and fix bounded presentation defects |
| `hoi4setup_platform_release_auditor` | read-only | Audit Windows and macOS packaging, signing, notarization, artifacts, and releases |
| `hoi4setup_documentation_curator` | docs-only patch | Reconcile requirements, architecture, README, release docs, and handoffs |
| `hoi4setup_skill_maintainer` | skill-only patch | Create, update, trim, and cross-check living skills |

## Read-only audit boundary

Read-only auditors may write a report under:

```text
docs/development/handoffs/<task_slug>/
```

They do not patch application source, tests, schemas, workflows, or configuration. A finding should include exact file and identifier evidence, impact, and recommended next action.

## Narrow patch boundary

A narrow patch subagent may fix only a local defect inside the current task scope. It must not redesign the product, expand a feature, change architecture, or claim completion.

Every patch handoff lists:

- files changed
- before and after behavior
- tests run
- skipped relevant tests and reason
- remaining risks
- parent follow-up

## When not to use subagents

Do not spawn a subagent for:

- a known one-file wording edit
- a direct user-provided file update
- a small code fix with exact ownership and tests already known
- work that the parent can review more quickly than a handoff can explain
- a ritual final audit with no meaningful question

Use subagents when a surface is risky, cross-file, platform-sensitive, security-sensitive, or easy to falsely call complete.

## Completion sequence for large changes

1. Parent implements the accepted design.
2. Parent runs unit and integration tests.
3. Relevant auditor reviews the bounded surface.
4. Parent resolves findings.
5. Documentation curator aligns source-of-truth documents when needed.
6. Skill maintainer updates living workflow knowledge when needed.
7. Parent runs final platform and acceptance checks.
8. Parent writes the completion report.

## Codex integration auditor

Use `hoi4setup_codex_integration_auditor` after meaningful authentication, App Server, semantic analysis, redaction, or proposal-rendering changes. It is read-only and checks protocol lifecycle, login flows, no-API-key policy, token containment, output schema, deterministic validation, recovery access, and tests.
