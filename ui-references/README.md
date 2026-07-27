# UI references

These 17 images are the primary full-resolution desktop implementation references. Every image is 1536 by 1024 pixels.

The revised interface uses a minimal grouped-phase design:

- seven setup phases instead of seventeen visible wizard steps
- one focal task per screen
- one title and no more than one supporting sentence by default
- no permanent evidence panel
- secondary details, logs, file lists, hashes, dependencies, and advanced settings stay collapsed until requested
- ordinary controls are not explained with redundant copy
- conflict review keeps the detail required for a safe comparison

| File | Screen |
| --- | --- |
| `01_welcome_project_selection.png` | Welcome and project selection |
| `02_new_mod_description.png` | New mod description |
| `03_project_identity_descriptor.png` | Project identity and descriptor setup |
| `04_existing_project_scan.png` | Existing project scan |
| `05_finding_review.png` | Finding review |
| `06_component_selection.png` | Component selection |
| `07_optional_workflow_selection.png` | Optional workflow selection |
| `08_3d_meshy_key_setup.png` | 3D workflow and Meshy key setup |
| `09_lora_comfyui_placeholder.png` | LoRA and ComfyUI placeholder |
| `10_mcp_credentials.png` | MCP and credentials |
| `11_git_setup.png` | Git setup |
| `12_dry_run_review.png` | Dry-run review |
| `13_installation_progress.png` | Installation progress |
| `14_final_readiness.png` | Final readiness |
| `15_update_repair.png` | Update and repair |
| `16_merge_conflict_review.png` | Merge conflict review |
| `17_interrupted_install_recovery.png` | Interrupted-install recovery |

The images are interface specifications. They are not screenshots of implemented software.

## ChatGPT sign-in gate

Revision 3 adds a compact ChatGPT sign-in gate before the existing project-selection reference. The 17 PNG files remain the visual-density and layout reference for the main wizard. Authentication behavior and copy are authoritative in `docs/17_ui_accessibility.md` and `docs/30_codex_chatgpt_authentication.md`.
