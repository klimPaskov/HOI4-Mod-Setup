# Portrait workflow

The app integrates the ComfyUI HOI4 portrait workflow as an
optional generic capability. New and imported projects can persist Cloud,
Local, RunPod, or Disabled. The provider state is carried through create,
scan/import, settings, readiness, Update, Repair, and rollback without storing
credentials. Cloud registers the official MCP route; Local and RunPod expose
bounded setup and health guidance. Disabled projects retain source-based
portrait handling and remove provider components and ComfyUI-specific
instructions.

The canonical repository, current commit, prompt/output contract, local setup,
RunPod browser playbook, fallback rules, native ImageGen boundary, and validation are maintained in
[`docs/32_comfyui_portrait_pipeline.md`](32_comfyui_portrait_pipeline.md) and
the checked-in upstream lock. Legacy `workflow.lora_comfyui_interest` values
remain readable for recovery but are not reactivated.
