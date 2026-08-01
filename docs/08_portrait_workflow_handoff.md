# Portrait workflow handoff

HOI4 Mod Setup does not set up, detect, configure, install, repair, or report a
LoRA or ComfyUI workflow.

After a project passes core readiness, the Ready screen shows one concise link
to [ComfyUI HOI4 Portraits](https://github.com/klimPaskov/comfyui-hoi4-portraits).
The destination is a fixed HTTPS GitHub URL opened through the typed
system-browser action; it is never taken from provider, manifest, project, or
scan content. The link is informational and external. It does not:

- add a wizard step or optional-workflow choice
- create a manifest component or dependency
- persist project interest or preference state
- add transaction operations or external commands
- enter the installation lock
- add readiness or maintenance status
- claim that the external workflow is installed or ready

Legacy `workflow.lora_comfyui_interest` values remain readable in old locks so
managed recovery is not broken. The scanner and readiness evaluator ignore the
legacy value, and the next successfully verified transaction removes it from
the new lock.
