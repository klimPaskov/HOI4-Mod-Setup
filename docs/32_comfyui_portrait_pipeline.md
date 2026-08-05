# ComfyUI HOI4 portrait pipeline

The portrait workflow is optional for generic projects and a required capability for Chaos Redux. HOI4 Mod Setup owns provider selection, non-secret configuration, source revision evidence, readiness, and repair planning. It does not own portrait generation or any provider secret.

## Canonical source

All workflow names, setup commands, model requirements, prompt rules, Cloud MCP instructions, local setup, RunPod setup, browser guidance, output handling, and validation rules come from the current [ComfyUI HOI4 Portraits repository](https://github.com/klimPaskov/comfyui-hoi4-portraits). The generated project stores the exact repository, branch, and commit in `.codex/portrait_pipeline.toml`; there is no separate upstream lock file. Plans and locks retain that exact revision, workflow hashes, and provider state; they never mix files from another revision.

The current upstream package provides source, processing-only, and text-to-image graphs. The product authorizes only `source` and `processing_only` for sourced or grounded portraits. The text-to-image graph is not an authorized product route: non-sourced fictional or impossible portraits use native ImageGen and never enter ComfyUI, and they do not receive a ComfyUI fallback.

The source graph detects and crops one person before RealESRGAN, defaults Face zoom to `0.90`, protects the complete head, headwear, and a top safety margin, and provides a manual crop override for ambiguous sources. It optionally restores with FLUX.2, then produces three independent LoRA candidates. Each run creates three `832x1120` master PNGs and three `156x210` game PNGs for review. The processing-only graph stops after crop, upscale, and optional restoration without LoRA styling. The positive prompt begins with `hoi4_portrait,` and describes only the visible person.

Chaos Redux is the required-workflow exception: its persisted provider is RunPod, its normal execution mode is API-first, and computer control is used only when the parent explicitly requests visible operation for the current job. The generic app provider matrix remains Cloud, Local, RunPod, or Disabled.

## Provider state

Generic projects can select Cloud, Local, RunPod, or Disabled. Disabled is a real persisted choice, not an error state. When disabled, the generic package keeps source-based portrait handling and removes portrait provider components, marker sections, Cloud MCP configuration, and ComfyUI-specific instructions. When a provider is enabled, the generated project receives the provider-neutral portrait contract, only the selected provider skill, the bounded portrait subagent, and the non-secret provider configuration containing the exact upstream revision. Non-selected provider skills are not installed and are removed during provider changes when their managed files are unmodified.

When the optional workflow is enabled in HOI4 Mod Setup, its expanded panel shows the minimum recommendation: **16 GB VRAM and 25 GB storage**. This is the app's minimum planning requirement and does not make the optional provider workflow a blocker for core project readiness. The current upstream model manifest contains eight pinned files totaling about 19.42 GB decimal. A 24 GB GPU is the practical target; 18 GB may work with aggressive offloading, and 16 GB is limited to slower reduced-resolution tests. A RunPod workspace should use a 30 GB volume for the repository, ComfyUI files, caches, and outputs.

The persisted shape is the `portrait_pipeline` object in project state, installation plans, and installation locks. It contains `enabled`, `provider`, `provider_status`, `workflow_repository`, `workflow_branch`, `workflow_commit`, `preferred_workflow`, local route fields, RunPod route fields, and `mcp_registered`. It never contains an API key, access token, password, cookie, or account metadata. Provider credentials remain in the OS vault or a scoped process environment.

Cloud registers the official Comfy Cloud MCP endpoint `https://cloud.comfy.org/mcp` and can remain in `needs_authorization` or `needs_subscription` while project creation completes. The source and processing graphs require the upstream `adaptive_portrait_crop` custom node in the Cloud Builder environment. The LoRA must be imported under its exact pinned filename before a source run. Automated tests never spend Cloud credits.

Local discovery is bounded to the explicitly configured root or known platform candidates. The root must contain `main.py` and `comfy/`, and the server check accepts only an HTTP loopback URL. The current setup sequence is:

```powershell
.\scripts\install_windows.ps1 -ComfyUIRoot "<COMFYUI_ROOT>"
python scripts/install_workflows.py --comfyui-root "<COMFYUI_ROOT>"
python scripts/download_models.py --comfyui-root "<COMFYUI_ROOT>"
```

The current model manifest includes the FLUX.2 base, Qwen encoder, VAE, LoRA, RealESRGAN, BiRefNet, MediaPipe, and YuNet detector files. The app reports the root, server health, NVIDIA hardware/VRAM result, pinned workflow hashes, model presence, and Hugging Face access state. It does not log or persist `HF_TOKEN`; model downloads remain a separate Hugging Face-gated action. A local route is not ready until the server, hardware, workflows, custom node, and model requirements are honestly satisfied.

RunPod stores only the non-secret workspace and URL. The current setup command is:

```bash
export HF_TOKEN="hf_..."
P=/workspace/comfyui-hoi4-portraits
COMFY_ROOT=/workspace/runpod-slim/ComfyUI
test -f "$COMFY_ROOT/main.py" || { echo "ComfyUI not found at $COMFY_ROOT; set COMFY_ROOT to the folder containing main.py."; exit 1; }
test -d "$P/.git" || git clone --depth 1 https://github.com/klimPaskov/comfyui-hoi4-portraits.git "$P"
"$P/scripts/install_runpod.sh" "$COMFY_ROOT"
"$P/scripts/start_runpod.sh" "$COMFY_ROOT"
```

The installer checks the pinned model sizes and hashes, installs the workflows, adaptive crop node, backgrounds, and sample input, and refuses partial or mismatched downloads. Open the existing pod URL only after the current workflow is visible. Browser or computer control is for visible operation only when the parent explicitly requests it; otherwise provide the API/manual steps and do not claim that a portrait was generated. RunPod is not ready until the URL opens, the workflow is found, and the required models are available. The app never starts a paid pod.

## Ownership and outputs

Portrait sourcing is separate from portrait production. A source researcher records provenance and writes the final person-only prompt, then hands a complete job manifest to the portrait producer. The producer selects the configured provider and sourced workflow, creates the master and game-size PNGs, compares identity, framing, and style with the source, converts the approved image to DDS, and reports exact output paths. Character scripting, `.gfx` wiring, gameplay, localisation, and final feature completion remain parent or auditor concerns.

Durable source packages use the runtime basename and live under `docs/assets/portraits/<event_id>_<event_slug>/`:

```text
<runtime_basename>.png
<runtime_basename>.txt
```

The PNG is the immutable highest-resolution source. The TXT contains only the final prompt. Runtime files never reference this archive. If a provider is temporarily unavailable for a grounded sourced portrait, the original source is cropped or resized and converted to a source-based DDS placeholder, the pending replacement is recorded, and the portrait is not reported as final styled art. If a generic project chose Disabled, that source-based DDS is the intended normal result and no pending ComfyUI wording is generated.

## App lifecycle

New-project creation and import show one concise portrait workflow step and the provider choice only when enabled. The selected state is carried into the generated project, installation lock, scan summary, readiness result, and maintenance plan. Reopening an installed project restores the persisted provider choice without repeating setup. Settings can change or disable the provider explicitly. Update and Repair preserve or revise the state; disabling removes only unmodified, managed portrait files and leaves modified files for review. Rollback uses the normal staged, journaled transaction contract.

Portrait readiness is non-blocking for core AI readiness, but a selected portrait workflow cannot be reported as final until its provider-specific requirements pass. Recovery, backup inspection, and managed removal remain available while a provider is signed out or unreachable.

## Validation

The automated suite covers schema and migration rejection of secret-shaped or unverified provider state, selected-provider source adaptation and cleanup, local loopback and current workflow-hash checks, transaction behavior, readiness, and UI persistence. Manual checks use mocked or local fixtures for provider discovery, deferred Cloud setup, local hardware rejection, RunPod guidance, source and prompt basename pairing, crop and output dimensions, DDS conversion, placeholder status, provider replacement, and disabled output. No automated check uses paid Cloud compute or starts a paid RunPod resource.
