# Open-source license decision

HOI4 Mod Setup uses the Apache License 2.0. The project owner selected it on
2026-07-28. The complete, unmodified license text is in `LICENSE`.

## Selected license

Apache License 2.0 is a permissive license with an explicit patent grant.

| License | Status | Main consideration |
| --- | --- | --- |
| Apache License 2.0 | Selected | Longer notice and attribution requirements |

The decision is recorded here so release automation and repository mirrors do
not silently invent licensing terms.

## Release gate

Before the first binary release is described as complete:

1. Review all direct dependencies and bundled assets for compatible terms.
2. Review the generated `THIRD_PARTY_NOTICES.md` inventory against the full
   license text and bundled assets, and add any required notices.
3. Include license and notice files in source and binary distributions.
4. Complete signed Windows and macOS release evidence.
