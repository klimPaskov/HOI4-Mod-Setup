# Repo-local living skill strategy

The application repository should keep a small set of focused Codex skills under `.agents/skills/`. These skills are maintained during development and record verified workflows, boundaries, commands, failure modes, and validation expectations.

They are not a duplicate requirements archive. Product requirements remain in `docs/`. Skills explain how recurring work is performed safely in the current repository.

## Why skills are living files

A desktop installer that touches user projects has many workflows that are easy to rediscover incorrectly:

- source revision resolution
- selective download
- path containment
- read-only scanning
- three-way conflict classification
- journaled apply and rollback
- credential injection
- Windows and macOS differences
- UI density and accessibility
- release signing and artifact verification

When implementation changes one of these workflows, the owning skill should change in the same pull request. This keeps future Codex sessions aligned with the actual repository.

## Skill set

| Skill | Owns |
| --- | --- |
| `hoi4-mod-setup-product-contract` | Product scope, architecture boundaries, completion proof |
| `hoi4-mod-setup-codex-integration` | Codex ChatGPT sign-in, App Server lifecycle, provider-neutral semantic boundary, redaction, usage limits, proposal confirmation |
| `hoi4-mod-setup-source-manifest` | GitHub source resolution, remote manifest, selective fetch, hashes, wiki distribution |
| `hoi4-mod-setup-project-scanner` | Existing-project read-only scanning, evidence, confidence, findings |
| `hoi4-mod-setup-transactions` | Plans, backups, staging, journal, apply, recovery, rollback, repair, removal |
| `hoi4-mod-setup-security` | Credentials, process allowlists, containment, redaction, archive and supply-chain safety |
| `hoi4-mod-setup-ui-accessibility` | Minimal wizard UI, progressive disclosure, interaction states, accessibility |
| `hoi4-mod-setup-testing` | Test layers, fixtures, fault injection, platform and release gates |
| `hoi4-mod-setup-open-source-release` | GitHub workflows, dependency updates, packaging, signing, release publication |
| `hoi4-mod-setup-skill-maintenance` | Skill ownership, update triggers, overlap and staleness review |

## Mandatory update triggers

Update a skill when a pull request changes any of these:

- repeated implementation sequence
- command or script name
- file or module location used by future work
- invariant or security boundary
- schema or migration behavior
- transaction stage or journal state
- supported platform behavior
- validation or test command
- common failure and recovery procedure
- handoff format
- ownership boundary between Rust, UI, platform, or subagent work

A change can skip skill updates when it is a one-off wording fix, a narrow test data update, or an implementation detail that does not affect future workflow. The pull request should still state why no skill update was needed.

## Skill quality rules

A good skill contains:

- clear trigger description
- source-of-truth files
- ownership and forbidden scope
- stable workflow
- invariants
- required evidence
- tests or validation
- update triggers
- completion standard

A skill should not contain:

- issue numbers
- temporary branch names
- personal workstation paths
- unverified package versions
- private URLs
- copied requirements with no workflow value
- large feature-specific design prose
- stale commands kept for history

Git history is the change log. Skills describe the current accepted workflow.

## Skill ownership check

Before completing a meaningful pull request:

1. List the product surfaces changed.
2. Map each surface to the skill table.
3. Check whether commands, locations, invariants, validation, or failure handling changed.
4. Update the owning skill when needed.
5. Check adjacent skills for contradictory instructions.
6. Record the skill decision in the pull request description.

The `hoi4setup_skill_maintainer` subagent can perform this review for broad or cross-skill changes. The parent agent still owns the final decision.

## New skill gate

Create a new skill only when all are true:

- the workflow is likely to recur
- no current skill owns it cleanly
- it has a stable boundary
- it needs enough repository-specific instruction to prevent rediscovery
- adding it is clearer than extending an existing skill

Prefer extending an existing skill when the workflow is a normal part of that surface.

## Version and platform evidence

Do not hardcode a dependency or tool version into a skill unless the repository deliberately pins it and the source file is named. Prefer instructions such as:

```text
Read the version from <lock or manifest path> and verify it before use.
```

For platform support, distinguish:

- verified supported
- supported by platform-neutral logic
- planned
- unavailable
- blocked pending repository evidence

Never convert a planned macOS route into a supported route because a similar Windows command exists.

## Relationship with AGENTS and subagents

`AGENTS.md` provides repository-wide rules and routing. Skills provide the detailed recurring workflow. Subagents use the owning skill and return evidence or bounded patches.

Do not place every detail in `AGENTS.md`. Do not create a central mega-skill that repeats the whole project. Keep each layer focused.

## AI provider integration skill

`hoi4-mod-setup-codex-integration` owns the Codex App Server lifecycle,
ChatGPT authentication, the provider-neutral structured semantic boundary,
account state, usage-limit behavior, redaction, provider/model binding, and the
proposal-to-renderer boundary. Update it whenever any provider adapter, method,
schema, process rule, credential boundary, or privacy rule changes. Provider
profiles, setup-assistant isolation, and the independent flattened Chat export are documented in
`docs/31_ai_provider_profiles_and_chat_sources.md` and must stay aligned with
the source contract, schemas, and UI.
