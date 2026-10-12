# ChatGPT source packaging

Accepted product correction: ChatGPT project sources are a minor, optional
setup check. They do not appear in Ready, its details, or the final completion
screen. There is no Start planning in ChatGPT card, action, or recommendation.
This supersedes the earlier final-step handoff wording in the user flows,
UI requirements, provider/source requirements, and acceptance criteria.

The independent Components choice and reviewed flattened-file operations stay
available. Install review shows one muted check under Before setup, with the
file evidence in its disclosure. The existing non-blocking hash check stays in
Rust; hiding it from Ready does not weaken verification. Existing-project
management retains its external-folder export tool.

Both Agentic core profiles and coding environments install the manifest-owned
`runtime.agent_sync` shared-tools component. It now also declares the packager
so existing managed setups receive it through the normal reviewed closure.
Selected files are `.tools/README.md`, `.tools/package_chatgpt_project_sources.py`,
and `.tools/package_chatgpt_project_sources.bat`, with source-owned exact Git-blob
size and SHA-256 evidence. Modified installed files require ordinary conflict
review; installation, repair, removal, and rollback use the existing transaction.
No schema or migration changes are required.

Run `python .tools/package_chatgpt_project_sources.py --dry-run` on Windows or
macOS to review the selection, then run without `--dry-run` to build it. The
Windows batch launcher opens the result. Python 3.10 or newer is required.
The default output is a new Downloads/<project-folder>-chatgpt-project-sources
folder containing flat loose text files, optional `subagents.zip`, and
`all-project-sources.zip`. Repeated exports require a new `--output` folder.
The tool accepts include/exclude globs and JSON configuration; its installed
README documents the defaults and limits. It does not upload, use credentials,
or change the project. Review contents before sharing; filename exclusions
cannot identify every secret.

Validation covers installation into a tiny fixture from the manifest-selected
file inventory, bundle contents and byte identity, dry-run/config overrides,
output containment, existing-output refusal, and selected linked-file rejection.
Ready tests assert that optional source status is absent and the development
client opener remains available. Install review tests assert the minor check.
The Ready and Install-review screenshots reflect the revised presentation.

## Concurrent-work handoff

The following files were already dirty in `setup_app_e2e` and were left untouched:

- `docs/02_user_flows.md`: remove the after-setup Chat recommendation.
- `docs/17_ui_accessibility.md`: remove the direct Ready Chat handoff.
- `docs/22_acceptance_criteria.md`: replace AI-08, UI-14, and UI-45's final
  recommendation with the minor setup check and absence from Ready.
- `src-tauri/src/commands.rs`: remove the now-unused `https://chatgpt.com`
  browser allowlist entry and its positive assertion; the shared browser bridge
  remains in use by other fixed links.
- `docs/31_ai_provider_profiles_and_chat_sources.md`: remove final recommendation
  wording while retaining independent flattening and existing-project export.
- `.agents/skills/hoi4-mod-setup-ui-accessibility/SKILL.md`: remove Ready handoff
  and recommendation requirements; use the supplemental source-packaging guide.
- `.agents/skills/hoi4-mod-setup-source-manifest/SKILL.md`: link the supplemental
  source-packaging guide describing the new shared component.

The clean integration skill and owning skill supplements record the recurring contract without modifying
another job's pending skill changes. Merge the above wording when that job
finishes. No subagents were used; authentication and semantic analysis were
unchanged. Native macOS execution and public source publication remain separate
verification steps.
