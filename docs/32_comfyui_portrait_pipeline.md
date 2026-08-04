# ComfyUI HOI4 portrait pipeline

The portrait workflow is an optional workflow for generic projects and a required capability for Chaos Redux. The app owns provider selection, non-secret configuration, source revision evidence, readiness, and repair planning. It does not own portrait generation or any provider secret.

## Canonical source

All workflow names, setup commands, model requirements, prompt rules, Cloud MCP instructions, local setup, RunPod setup, browser guidance, output handling, and validation rules come from the current [ComfyUI HOI4 Portraits repository](https://github.com/klimPaskov/comfyui-hoi4-portraits). The checked-in app evidence is `docs/portrait_pipeline/upstream-lock.json` at commit `92c8118f9ab61a0a658af24bc6868ed7f93cdebd` on `codex/portrait-pipeline`. A plan and lock retain this exact repository, branch, commit, preferred workflow, and provider state; they never mix files from another revision.

The current sourced-portrait workflow aliases are `source` and `processing_only`. The upstream repository's text-to-image graph is not an authorized product route: fictional, impossible, or otherwise non-sourced portraits use native ImageGen and never use ComfyUI. The upstream source and API graphs are used for automated execution; the editor graph is used for visible/browser control. The current source workflow emits three candidate `832x1120` master and `156x210` game pairs; the user reviews and selects one. The positive prompt begins with `hoi4_portrait,`, describes only the visible person, and omits the person's name, background, lighting, rendering, restoration, and unsupported biographical instructions.

Chaos Redux is the required-workflow exception: its persisted provider is RunPod, its normal execution mode is API-first, and computer control is used only when the parent explicitly requests it for the current job. The generic app provider matrix remains Cloud, Local, RunPod, or Disabled.

## Provider state

Generic projects can select Cloud, Local, RunPod, or Disabled. Disabled is a real persisted choice, not an error state. When disabled, the generic package keeps source-based portrait handling and removes portrait provider components, marker sections, Cloud MCP configuration, and ComfyUI-specific instructions. When a provider is enabled, the generated project receives the provider-neutral portrait contract, only the selected provider skill, the bounded portrait subagent, the non-secret provider configuration, and the exact upstream lock evidence. Non-selected provider skills are not installed and are removed during provider changes when their managed files are unmodified.

When the optional workflow is enabled in HOI4 Mod Setup, its expanded panel
shows the minimum recommendation: **16 GB VRAM and 25 GB storage**. This is a
planning requirement for the workflow and does not make the optional provider
workflow a blocker for core project readiness.

The persisted shape is the `portrait_pipeline` object in project state, installation plans, and installation locks. It contains `enabled`, `provider`, `provider_status`, `workflow_repository`, `workflow_branch`, `workflow_commit`, `preferred_workflow`, local route fields, RunPod route fields, and `mcp_registered`. It never contains an API key, access token, password, cookie, or account metadata. Provider credentials remain in the OS vault or a scoped process environment.

Cloud registers the official Comfy Cloud MCP endpoint `https://cloud.comfy.org/mcp` and can remain in `needs_authorization` or `needs_subscription` while project creation completes. A Cloud status is not `ready` until authentication, plan/model access, and a no-spend workflow check pass. Automated tests never spend Cloud credits.

Local discovery is bounded to the explicitly configured root or known platform candidates. The root must contain `main.py` and `comfy/`, and the server check accepts only an HTTP loopback URL. The app reports the root, server health, NVIDIA hardware/VRAM result, pinned workflow hashes, model presence, and Hugging Face access state. The current workflow files can be installed transactionally after checksum verification. The canonical Windows setup remains the upstream command sequence:

```powershell
.\scripts\install_windows.ps1 -ComfyUIRoot "<COMFYUI_ROOT>"
python scripts/install_workflows.py --comfyui-root "<COMFYUI_ROOT>"
python scripts/download_models.py --comfyui-root "<COMFYUI_ROOT>"
```

The app does not log or persist `HF_TOKEN`; model downloads remain a separate Hugging Face-gated action. A local route is not ready until the server, hardware, workflows, and model requirements are honestly satisfied.

RunPod stores only the non-secret workspace and URL. The UI shows the exact current upstream install command and browser-control guidance, including loading the editor workflow, finding visible node titles, uploading/selecting the source, checking the crop and prompt, verifying LoRA/restoration and background stages, queueing, reviewing the three source candidates, selecting one pair, and verifying both downloads. RunPod is not ready until the URL can be opened and the workflow is found; the app never starts a paid pod or claims a generated portrait from guidance alone.

## Ownership and outputs

Portrait sourcing is separate from portrait production. A source researcher records provenance and writes the final person-only prompt, then hands a complete job manifest to the portrait producer. The producer selects the configured provider/workflow, creates the master and game-size PNGs, compares identity/framing/style with the source, converts the approved image to DDS, and reports exact output paths. Character scripting, `.gfx` wiring, gameplay, localisation, and final feature completion remain parent/auditor concerns.

Durable source packages use the runtime basename and live under `docs/assets/portraits/<event_id>_<event_slug>/`:

```text
<runtime_basename>.png
<runtime_basename>.txt
```

The PNG is the immutable highest-resolution source. The TXT contains only the final prompt. Runtime files never reference this archive. If the provider is temporarily unavailable for a grounded sourced portrait, the original source is cropped/resized and converted to a source-based DDS placeholder, the pending replacement is recorded, and the portrait is not reported as final styled art. Non-sourced fictional or impossible portraits use native ImageGen and do not receive a ComfyUI fallback. If a generic project chose Disabled, that source-based DDS is the intended normal result and no pending ComfyUI wording is generated.

## App lifecycle

New-project creation and import show one concise portrait workflow step and the provider choice only when enabled. The selected state is carried into the generated project, installation lock, scan summary, readiness result, and maintenance plan. Reopening an installed project restores it without repeating setup. Settings can change or disable the provider explicitly. Update and Repair preserve or revise the state; disabling removes only unmodified, managed portrait files and leaves modified files for review. Rollback uses the normal staged, journaled transaction contract.

Portrait readiness is non-blocking for core AI readiness, but a selected portrait workflow cannot be reported as final until its provider-specific requirements pass. Recovery, backup inspection, and managed removal remain available while a provider is signed out or unreachable.

## Validation

The automated suite covers schema/migration rejection of secret-shaped or unverified provider state, selected-provider source adaptation and cleanup, local loopback and workflow-hash checks, transaction behavior, readiness, and UI persistence. Manual checks use mocked/local fixtures for provider discovery, deferred Cloud setup, local hardware rejection, RunPod guidance, source/prompt basename pairing, output dimensions, DDS conversion, placeholder status, provider replacement, and disabled output. No automated check uses paid Cloud compute or starts a paid RunPod resource.
