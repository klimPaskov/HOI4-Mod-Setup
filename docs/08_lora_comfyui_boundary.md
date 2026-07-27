# Future LoRA and ComfyUI integration boundary

## Version 1 contract

The wizard asks:

**Do you want to set up LoRAs and ComfyUI for portrait generation?**

Version 1 may record interest. It must not:

- download ComfyUI
- find or modify an existing ComfyUI installation
- install Python or packages
- create a Python environment
- download checkpoints, VAEs, LoRAs, ControlNets, upscalers, or custom nodes
- change GPU drivers or acceleration settings
- alter model directories
- edit workflows
- report installed or ready

## State

```json
{
  "workflow.lora_comfyui_interest": {
    "state": "planned_unavailable",
    "reason": "Automated setup is not implemented in version 1."
  }
}
```

The preference is non-blocking and appears in readiness.

## Architecture boundary

A future implementation plugs into the optional-workflow interface. It can add a real manifest component, platform support, dependency graph, model registry, distribution source, custom-node lock, workflow templates, health checks, storage estimates, GPU checks, and credentials.

Component selection, dry run, transaction, lock, update, repair, and readiness remain unchanged.

## Future safety requirements

A real implementation must define:

- authoritative ComfyUI source
- exact commit or release policy
- model licenses and sources
- per-model hashes
- custom-node sources and hashes
- Python and package lock
- GPU backend compatibility
- disk limits
- external folder ownership
- rollback rules for shared model storage
- identity and privacy handling for input portraits

## Migration

A later release may migrate `interest_recorded` or `planned_unavailable` to `eligible` and then `selected_pending`. Migration preserves the user's preference and never starts downloads automatically.

## UI rules

Say automated setup is unavailable, interest can be recorded, no files or software will change, and the state does not block setup. Do not show a fake success or a disabled Install action that implies a temporary failure.
