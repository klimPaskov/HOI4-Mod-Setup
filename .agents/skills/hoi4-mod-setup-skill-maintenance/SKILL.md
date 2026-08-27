---
name: hoi4-mod-setup-skill-maintenance
description: Use to create, update, audit, trim, and cross-check HOI4 Mod Setup repo-local skills when implementation knowledge changes.
---

# Repo-local skill maintenance

## Purpose

Keep `.agents/skills/` aligned with the current repository. Skills capture recurring verified workflows. They should prevent future agents from rediscovering commands, boundaries, failure modes, and validation.

## Use this skill when

- a repeated workflow changes
- a new stable workflow appears
- commands or paths change
- a security or transaction invariant changes
- a platform difference is verified
- a common failure and recovery method is learned
- several skills overlap or contradict each other
- a skill references stale implementation
- setup-provider/model optimization, development-client isolation, flattening, or a new credential boundary changes

## Workflow

1. Read `AGENTS.md` and `docs/27_repo_local_skill_strategy.md`.
2. Identify the product surfaces changed by the current work.
3. Map them to existing skills.
4. Inspect actual source, tests, scripts, and docs before editing a skill.
5. Update the smallest owning skill.
6. Check adjacent skills and AGENTS for contradictions.
7. Remove stale commands and obsolete paths.
8. Keep one-off ticket details out.
9. Report skills changed and why.

## New skill gate

Create a skill only when the workflow recurs, has a stable boundary, is not cleanly owned elsewhere, and needs repository-specific detail.

Do not create:

- one skill per feature
- one skill per tool
- a central mega-skill
- a skill that only repeats requirements
- a skill for temporary debugging

## Content standard

A skill should include:

- trigger
- required sources
- ownership
- invariants
- workflow
- tests or evidence
- update triggers
- completion standard

Use current source paths and repository scripts. When a version comes from a manifest or lock, instruct the agent to read that source rather than copying a temporary version into the skill.

## Pull request check

Before completion, answer:

- Did a repeated workflow change?
- Did a command or file location change?
- Did an invariant, schema, platform route, validation, or recovery method change?
- Did the selected-provider profiles, model binding, or flatten mapping change?
- Which skill owns it?
- Was the owning skill updated?
- Do adjacent skills remain consistent?

If no skill update is needed, record the reason in the pull request.

## Completion standard

The skill set describes the current verified workflow without stale commands, overlap, ticket-specific content, or contradictions with AGENTS and source documentation.
