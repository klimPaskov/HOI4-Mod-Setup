//! Deterministic adaptation of selected source files to the confirmed project:
//! identity placeholders, optional workflow sections and component blocks,
//! subagent spawn rules, and confirmed conventions. Shared by the desktop plan
//! and the web edition's export.

use crate::models::{PortraitPipelineConfig, ProjectIdentity};
use crate::AppError;
use regex::Regex;

/// Where the HOI4 game and Steam Workshop folders are, as written into the
/// project instructions. The desktop detects them; other hosts pass the
/// documented defaults.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GameFolders {
    pub game_dir: String,
    pub workshop_dir: String,
}

/// Adapts the bytes of one host-specific source file.
pub type SourceAdapter<'a> = dyn Fn(&[u8]) -> Result<Vec<u8>, AppError> + 'a;

/// Everything adaptation needs about the confirmed selection.
pub struct AdaptContext<'a> {
    pub identity: &'a ProjectIdentity,
    pub conventions: &'a ProjectConventions,
    pub folders: &'a GameFolders,
    pub super_events_selected: bool,
    pub portrait: &'a PortraitPipelineConfig,
    pub selected_components: &'a [String],
}

/// Adapt one selected source file. `codex_config` adapts the `codex.config`
/// component, whose MCP routes depend on the host.
pub fn adapt_selected_source(
    component_id: &str,
    destination: &str,
    bytes: &[u8],
    context: &AdaptContext<'_>,
    codex_config: &SourceAdapter<'_>,
) -> Result<Vec<u8>, AppError> {
    let conditional = adapt_optional_super_events_skill(bytes, context.super_events_selected)?;
    let conditional = adapt_optional_portrait_section(&conditional, context.portrait.enabled)?;
    let normalized = destination.replace('\\', "/").to_ascii_lowercase();
    let adapted = if component_id == "core.agents" || component_id == "core.claude.instructions" {
        adapt_agents_for_selection(
            &conditional,
            context.identity,
            context.folders,
            context.conventions,
            context.super_events_selected,
            context.portrait.enabled,
        )?
    } else if component_id == "codex.config" {
        codex_config(&conditional)?
    } else if normalized.starts_with(".codex/agents/") && normalized.ends_with(".toml") {
        adapt_subagent_for_spawn(&conditional)?
    } else if component_id == "workflow.portraits.config" {
        adapt_portrait_config_source(&conditional, context.portrait)?
    } else if is_super_events_runtime_component(component_id) {
        adapt_super_events_source(&conditional, context.identity)?
    } else if normalized.ends_with("/hoi4-feature-assets/tools/project_settings.json") {
        adapt_asset_tool_settings(&conditional, context.identity)
    } else if normalized == ".tools/gfx/settings.json" {
        adapt_gfx_settings(&conditional, context.identity)
    } else {
        conditional
    };
    adapt_component_blocks(adapted, context.selected_components)
}

/// Replace the string value of `key` in a JSON text, keeping the file's own
/// layout and every other line. A value that is not a plain string, or a key
/// that is absent, leaves the text unchanged.
fn set_json_string_value(text: &str, key: &str, value: &str) -> String {
    let pattern = format!(r#"("{}"\s*:\s*)"(?:[^"\\]|\\.)*""#, regex::escape(key));
    let Ok(regex) = Regex::new(&pattern) else {
        return text.to_string();
    };
    let Ok(encoded) = serde_json::to_string(value) else {
        return text.to_string();
    };
    regex
        .replace(text, |captures: &regex::Captures<'_>| {
            format!("{}{}", &captures[1], encoded)
        })
        .into_owned()
}

/// The shared asset tools read the mod's name and id prefix from this file
/// (the source ships "HOI4" and "hoi4").
fn adapt_asset_tool_settings(bytes: &[u8], identity: &ProjectIdentity) -> Vec<u8> {
    let Ok(text) = std::str::from_utf8(bytes) else {
        return bytes.to_vec();
    };
    let mut adapted = set_json_string_value(text, "project_label", identity.display_name.trim());
    if let Some(prefix) = identity
        .script_prefix
        .as_deref()
        .filter(|p| !p.trim().is_empty())
    {
        adapted = set_json_string_value(&adapted, "id_prefix", prefix.trim());
    }
    adapted.into_bytes()
}

/// `gfx.py` groups registries whose file name starts with this prefix on its
/// contact sheets; the mod's own script prefix is the sensible value.
fn adapt_gfx_settings(bytes: &[u8], identity: &ProjectIdentity) -> Vec<u8> {
    let Ok(text) = std::str::from_utf8(bytes) else {
        return bytes.to_vec();
    };
    match identity
        .script_prefix
        .as_deref()
        .filter(|p| !p.trim().is_empty())
    {
        Some(prefix) => {
            set_json_string_value(text, "shared_registry_prefix", prefix.trim()).into_bytes()
        }
        None => bytes.to_vec(),
    }
}

pub fn adapt_subagent_for_spawn(bytes: &[u8]) -> Result<Vec<u8>, AppError> {
    let text = std::str::from_utf8(bytes)
        .map_err(|error| AppError::Source(format!("subagent definition is not UTF-8: {error}")))?;
    let mut value = text.parse::<toml::Value>().map_err(|error| {
        AppError::Source(format!("subagent definition is not valid TOML: {error}"))
    })?;
    if let Some(fork_context) = value.get("fork_context").and_then(toml::Value::as_bool) {
        if fork_context {
            return Err(AppError::Source(
                "subagent definition requires inherited conversation context".into(),
            ));
        }
        return Ok(bytes.to_vec());
    }
    if text.contains("fork_context=false") {
        return Ok(bytes.to_vec());
    }
    let instructions = value
        .get_mut("developer_instructions")
        .and_then(|instructions| instructions.as_str())
        .ok_or_else(|| {
            AppError::Source("subagent definition has no developer instructions".into())
        })?
        .to_string();
    value["developer_instructions"] = toml::Value::String(format!(
        "{instructions}\n\nThe parent must spawn this subagent with fork_context=false."
    ));
    let rendered = toml::to_string_pretty(&value).map_err(|error| {
        AppError::Source(format!(
            "adapted subagent definition could not be written: {error}"
        ))
    })?;
    rendered.parse::<toml::Value>().map_err(|error| {
        AppError::Source(format!("adapted subagent definition is invalid: {error}"))
    })?;
    Ok(rendered.into_bytes())
}

pub fn adapt_optional_super_events_skill(
    bytes: &[u8],
    super_events_selected: bool,
) -> Result<Vec<u8>, AppError> {
    const START: &str = "<!-- HOI4_MOD_SETUP_SUPER_EVENTS_START -->";
    const END: &str = "<!-- HOI4_MOD_SETUP_SUPER_EVENTS_END -->";
    if !bytes
        .windows(START.len())
        .any(|window| window == START.as_bytes())
        && !bytes
            .windows(END.len())
            .any(|window| window == END.as_bytes())
    {
        return Ok(bytes.to_vec());
    }
    let text = std::str::from_utf8(bytes)
        .map_err(|error| AppError::Source(format!("conditional skill is not UTF-8: {error}")))?;
    match (text.find(START), text.find(END)) {
        (Some(start), Some(end)) if start < end => {
            let block_end = end + END.len();
            let mut adapted = text.to_string();
            if super_events_selected {
                adapted.replace_range(end..block_end, "");
                adapted.replace_range(start..start + START.len(), "");
            } else {
                adapted.replace_range(start..block_end, "");
            }
            Ok(adapted.into_bytes())
        }
        _ => Err(AppError::Source(
            "skill has an incomplete optional Super Events section".into(),
        )),
    }
}

pub fn adapt_optional_portrait_section(
    bytes: &[u8],
    portrait_enabled: bool,
) -> Result<Vec<u8>, AppError> {
    const START: &str = "<!-- HOI4_MOD_SETUP_PORTRAITS_START -->";
    const END: &str = "<!-- HOI4_MOD_SETUP_PORTRAITS_END -->";
    if !bytes
        .windows(START.len())
        .any(|window| window == START.as_bytes())
        && !bytes
            .windows(END.len())
            .any(|window| window == END.as_bytes())
    {
        return Ok(bytes.to_vec());
    }
    let text = std::str::from_utf8(bytes).map_err(|error| {
        AppError::Source(format!("portrait conditional source is not UTF-8: {error}"))
    })?;
    let Some(start) = text.find(START) else {
        if text.contains(END) {
            return Err(AppError::Source(
                "portrait section has an end marker without a start marker".into(),
            ));
        }
        return Ok(bytes.to_vec());
    };
    let end = text
        .find(END)
        .ok_or_else(|| AppError::Source("portrait section has an incomplete marker pair".into()))?;
    if start >= end {
        return Err(AppError::Source(
            "portrait section markers are out of order".into(),
        ));
    }
    let block_end = end + END.len();
    let mut adapted = text.to_string();
    if portrait_enabled {
        adapted.replace_range(end..block_end, "");
        adapted.replace_range(start..start + START.len(), "");
    } else {
        adapted.replace_range(start..block_end, "");
    }
    Ok(adapted.into_bytes())
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProjectConventions {
    pub agents_profile: String,
    pub localisation_convention: String,
    pub documentation_convention: String,
}

pub fn escape_markdown_code_span(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('`', "\\`")
        .replace(['\r', '\n'], " ")
}

pub fn adapt_agents_for_selection(
    bytes: &[u8],
    identity: &ProjectIdentity,
    folders: &GameFolders,
    conventions: &ProjectConventions,
    super_events_selected: bool,
    portrait_enabled: bool,
) -> Result<Vec<u8>, AppError> {
    let text = std::str::from_utf8(bytes)
        .map_err(|error| AppError::Source(format!("AGENTS template is not UTF-8: {error}")))?;
    let prefix = identity
        .primary_namespace
        .as_deref()
        .or(identity.script_prefix.as_deref())
        .unwrap_or(&identity.project_id);
    let mut adapted = text
        .replace("[MOD_NAME]", &identity.display_name)
        .replace("[MOD_PREFIX]", prefix)
        .replace("[HOI4_GAME_DIR]", &folders.game_dir)
        .replace("[STEAM_WORKSHOP_DIR]", &folders.workshop_dir);
    adapted = strip_agents_placeholder_guide(&adapted);
    adapted = String::from_utf8(adapt_optional_portrait_section(
        adapted.as_bytes(),
        portrait_enabled,
    )?)
    .map_err(|error| {
        AppError::Source(format!("adapted portrait guidance is not UTF-8: {error}"))
    })?;
    if adapted.contains("[MOD_") || adapted.contains("{{PROJECT_") || adapted.contains("<PROJECT_")
    {
        return Err(AppError::Source(
            "AGENTS template contains unresolved project placeholders".into(),
        ));
    }
    const SUPER_EVENTS_START: &str = "<!-- HOI4_MOD_SETUP:SUPER_EVENTS:START -->";
    const SUPER_EVENTS_END: &str = "<!-- HOI4_MOD_SETUP:SUPER_EVENTS:END -->";
    match (
        adapted.find(SUPER_EVENTS_START),
        adapted.find(SUPER_EVENTS_END),
    ) {
        (Some(start), Some(end)) if start < end => {
            let block_end = end + SUPER_EVENTS_END.len();
            if super_events_selected {
                adapted.replace_range(end..block_end, "");
                adapted.replace_range(start..start + SUPER_EVENTS_START.len(), "");
            } else {
                adapted.replace_range(start..block_end, "");
            }
        }
        (None, None)
            if super_events_selected
                && !adapted.contains("HOI4_MOD_SETUP:COMPONENT:workflow.super_events") =>
        {
            adapted.push_str(
                "\n## Optional Super Events workflow\n\n- Use `hoi4-super-events` for the complete installed runtime and presentation contract. Use the Super Events sections added to `hoi4-feature-planning`, `hoi4-events`, `hoi4-feature-assets`, `hoi4-text-audio-research`, and `hoi4-subagents` for their normal ownership boundaries. Keep registration, caller, text, quote, response, image, optional or explicitly required audio, provenance, documentation, cleanup, and the acceptance scenario aligned.\n- Spawn `hoi4_super_event_quote_researcher`, `hoi4_super_event_audio_researcher`, or `hoi4_super_event_art_researcher` without inherited conversation context, and only when its narrow handoff is needed.\n",
            );
        }
        (None, None) => {}
        _ => {
            return Err(AppError::Source(
                "AGENTS template has an incomplete Super Events section marker".into(),
            ));
        }
    }
    let convention_rows = [
        ("Agent guidance profile", &conventions.agents_profile),
        (
            "Localisation convention",
            &conventions.localisation_convention,
        ),
        (
            "Documentation convention",
            &conventions.documentation_convention,
        ),
    ]
    .into_iter()
    .filter(|(_, value)| !value.is_empty())
    .map(|(label, value)| format!("- {label}: `{}`", escape_markdown_code_span(value)))
    .collect::<Vec<_>>();
    if !convention_rows.is_empty() {
        adapted.push_str("\n## Confirmed project conventions\n\n");
        adapted.push_str(&convention_rows.join("\n"));
        adapted.push('\n');
    }
    Ok(adapted.into_bytes())
}

pub fn strip_agents_placeholder_guide(text: &str) -> String {
    let heading = Regex::new(r"(?m)^## (?:Placeholder Guide|How to use this template)[\t ]*\r?$")
        .expect("static placeholder-guide heading regex");
    let next_heading = Regex::new(r"(?m)^## [^\r\n]+").expect("static Markdown heading regex");
    let mut stripped = text.to_string();
    while let Some(section) = heading.find(&stripped) {
        let section_end = next_heading
            .find(&stripped[section.end()..])
            .map(|next| section.end() + next.start())
            .unwrap_or(stripped.len());
        stripped.replace_range(section.start()..section_end, "");
    }
    stripped
}

pub fn super_events_prefix(identity: &ProjectIdentity) -> Result<&str, AppError> {
    let prefix = identity
        .script_prefix
        .as_deref()
        .or(identity.primary_namespace.as_deref())
        .ok_or_else(|| {
            AppError::InvalidInput(
                "a confirmed script prefix or primary namespace is required for Super Events"
                    .into(),
            )
        })?;
    let valid_prefix =
        Regex::new(r"^[a-z_][a-z0-9_]{0,63}$").expect("static HOI4 identifier regex");
    if !valid_prefix.is_match(prefix) {
        return Err(AppError::InvalidInput(
            "the Super Events namespace contains unsupported characters".into(),
        ));
    }
    Ok(prefix)
}

pub fn adapt_super_events_source(
    bytes: &[u8],
    identity: &ProjectIdentity,
) -> Result<Vec<u8>, AppError> {
    let text = std::str::from_utf8(bytes)
        .map_err(|error| AppError::Source(format!("Super Events source is not UTF-8: {error}")))?;
    let prefix = super_events_prefix(identity)?;
    let escaped_name = identity
        .display_name
        .replace('\\', "\\\\")
        .replace('"', "\\\"");
    let adapted = text
        .replace("[MOD_PREFIX]", prefix)
        .replace("[MOD_NAME]", &escaped_name);
    if adapted.contains("[MOD_") {
        return Err(AppError::Source(
            "Super Events source contains unresolved project placeholders".into(),
        ));
    }
    Ok(adapted.into_bytes())
}

pub fn is_super_events_runtime_component(component_id: &str) -> bool {
    matches!(
        component_id,
        "workflow.super_events.runtime.interface"
            | "workflow.super_events.runtime.common"
            | "workflow.super_events.runtime.events"
            | "workflow.super_events.runtime.localisation"
    )
}

pub fn adapt_super_events_destination(
    component_id: &str,
    destination: &str,
    identity: &ProjectIdentity,
) -> Result<String, AppError> {
    if !is_super_events_runtime_component(component_id) {
        return Ok(destination.to_string());
    }
    let normalized = destination.replace('\\', "/");
    let Some((parent, filename)) = normalized.rsplit_once('/') else {
        return Ok(normalized);
    };
    let Some(suffix) = filename.strip_prefix("hoi4ms_") else {
        return Ok(normalized);
    };
    let prefix = super_events_prefix(identity)?;
    Ok(format!("{parent}/{prefix}_{suffix}"))
}

pub fn adapt_portrait_config_source(
    bytes: &[u8],
    portrait: &PortraitPipelineConfig,
) -> Result<Vec<u8>, AppError> {
    let text = std::str::from_utf8(bytes).map_err(|error| {
        AppError::Source(format!(
            "portrait provider configuration is not UTF-8: {error}"
        ))
    })?;
    let mut config = text.parse::<toml::Value>().map_err(|error| {
        AppError::Source(format!(
            "portrait provider configuration is not valid TOML: {error}"
        ))
    })?;
    let keep_cloud_mcp = portrait.enabled && portrait.provider == "cloud";
    if !keep_cloud_mcp {
        if let Some(root) = config.as_table_mut() {
            root.remove("mcp_servers");
        }
    }
    let table = config
        .get_mut("portrait_pipeline")
        .and_then(toml::Value::as_table_mut)
        .ok_or_else(|| {
            AppError::Source("portrait provider configuration is missing its table".into())
        })?;
    table.insert("enabled".into(), toml::Value::Boolean(portrait.enabled));
    table.insert(
        "provider".into(),
        toml::Value::String(portrait.provider.clone()),
    );
    table.insert(
        "provider_status".into(),
        toml::Value::String(portrait.provider_status.clone()),
    );
    table.insert(
        "workflow_repository".into(),
        toml::Value::String(portrait.workflow_repository.clone()),
    );
    table.insert(
        "workflow_branch".into(),
        toml::Value::String(portrait.workflow_branch.clone()),
    );
    table.insert(
        "workflow_commit".into(),
        toml::Value::String(portrait.workflow_commit.clone()),
    );
    table.insert(
        "preferred_workflow".into(),
        toml::Value::String(portrait.preferred_workflow.clone()),
    );
    if portrait.enabled && portrait.provider != "disabled" {
        table.insert(
            "provider_skill".into(),
            toml::Value::String(format!("hoi4-comfyui-{}", portrait.provider)),
        );
    } else {
        table.remove("provider_skill");
    }
    table.remove("local_comfyui_root");
    table.remove("local_server_url");
    table.remove("runpod_url");
    table.remove("runpod_workspace");
    if keep_cloud_mcp {
        table.insert(
            "mcp_server".into(),
            toml::Value::String("comfy_cloud_portraits".into()),
        );
    } else {
        table.remove("mcp_server");
    }
    match portrait.provider.as_str() {
        "local" => {
            table.insert(
                "local_comfyui_root".into(),
                toml::Value::String(portrait.local_comfyui_root.clone()),
            );
            table.insert(
                "local_server_url".into(),
                toml::Value::String(portrait.local_server_url.clone()),
            );
        }
        "runpod" => {
            table.insert(
                "runpod_url".into(),
                toml::Value::String(portrait.runpod_url.clone()),
            );
            table.insert(
                "runpod_workspace".into(),
                toml::Value::String(portrait.runpod_workspace.clone()),
            );
        }
        _ => {}
    }
    let rendered = toml::to_string_pretty(&config).map_err(|error| {
        AppError::Source(format!(
            "adapted portrait provider configuration could not be written: {error}"
        ))
    })?;
    rendered.parse::<toml::Value>().map_err(|error| {
        AppError::Source(format!(
            "adapted portrait provider configuration is invalid: {error}"
        ))
    })?;
    Ok(rendered.into_bytes())
}

pub const COMPONENT_BLOCK_PREFIX: &str = "<!-- HOI4_MOD_SETUP:COMPONENT:";

/// Keep or remove `<!-- HOI4_MOD_SETUP:COMPONENT:<id>:START -->` ...
/// `<!-- HOI4_MOD_SETUP:COMPONENT:<id>:END -->` blocks depending on whether
/// the named component is installed. The source package can therefore gate
/// guidance for any optional workflow without an app release.
pub fn adapt_component_blocks(
    bytes: Vec<u8>,
    selected_components: &[String],
) -> Result<Vec<u8>, AppError> {
    if !bytes
        .windows(COMPONENT_BLOCK_PREFIX.len())
        .any(|window| window == COMPONENT_BLOCK_PREFIX.as_bytes())
    {
        return Ok(bytes);
    }
    let Ok(text) = String::from_utf8(bytes.clone()) else {
        return Ok(bytes);
    };
    let marker = Regex::new(
        r"(?m)^[ \t]*<!-- HOI4_MOD_SETUP:COMPONENT:([a-z0-9][a-z0-9._-]{0,127}):(START|END) -->[ \t]*\r?\n?",
    )
    .expect("static component block regex");
    let mut output = String::with_capacity(text.len());
    let mut cursor = 0usize;
    let mut open: Option<(String, bool)> = None;
    for captures in marker.captures_iter(&text) {
        let whole = captures.get(0).expect("regex match");
        let component = captures[1].to_string();
        let start = &captures[2] == "START";
        match (&open, start) {
            (None, true) => {
                output.push_str(&text[cursor..whole.start()]);
                let keep = selected_components.iter().any(|id| id == &component);
                open = Some((component, keep));
            }
            (Some((open_component, keep)), false) if *open_component == component => {
                if *keep {
                    output.push_str(&text[cursor..whole.start()]);
                }
                open = None;
            }
            _ => {
                return Err(AppError::Source(format!(
                    "a source file has an unbalanced optional block for {component}"
                )))
            }
        }
        cursor = whole.end();
    }
    if let Some((component, _)) = open {
        return Err(AppError::Source(format!(
            "a source file has an unterminated optional block for {component}"
        )));
    }
    output.push_str(&text[cursor..]);
    Ok(output.into_bytes())
}

/// Adapt the Codex configuration to the selection. `meshy_launcher` is the
/// app-owned launcher for the 3D workflow's Meshy route; without it the 3D
/// routes are left out, as on hosts that cannot run that workflow.
pub fn adapt_codex_config(
    bytes: &[u8],
    mcp_selected: bool,
    meshy_launcher: Option<&str>,
    super_events_selected: bool,
    portrait: &PortraitPipelineConfig,
) -> Result<Vec<u8>, AppError> {
    let bytes = adapt_optional_portrait_section(bytes, portrait.enabled)?;
    let text = std::str::from_utf8(&bytes)
        .map_err(|error| AppError::Source(format!("Codex configuration is not UTF-8: {error}")))?;
    let mut config = text.parse::<toml::Value>().map_err(|error| {
        AppError::Source(format!("Codex configuration is not valid TOML: {error}"))
    })?;
    if let Some(table) = config.as_table_mut() {
        if !super_events_selected {
            if let Some(agents) = table.get_mut("agents").and_then(toml::Value::as_table_mut) {
                agents.retain(|id, _| !id.starts_with("hoi4_super_event_"));
            }
        }
        if let Some(meshy_launcher) = meshy_launcher {
            let mcp_servers = table
                .entry("mcp_servers")
                .or_insert_with(|| toml::Value::Table(toml::map::Map::new()))
                .as_table_mut()
                .ok_or_else(|| AppError::Source("Codex mcp_servers is not a TOML table".into()))?;
            for (id, wrapper, enabled, required, approval) in [
                ("meshy", "run_meshy_mcp.cmd", true, true, "auto"),
                (
                    "blender_hoi4",
                    "run_blender_hoi4_adapter.cmd",
                    true,
                    true,
                    "auto",
                ),
                (
                    "blender_lab",
                    "run_blender_lab_mcp.cmd",
                    false,
                    false,
                    "prompt",
                ),
            ] {
                let mut route = toml::map::Map::new();
                route.insert("enabled".into(), toml::Value::Boolean(enabled));
                route.insert("required".into(), toml::Value::Boolean(required));
                if id == "meshy" {
                    route.insert(
                        "command".into(),
                        toml::Value::String(meshy_launcher.to_owned()),
                    );
                    route.insert(
                        "args".into(),
                        toml::Value::Array(vec![toml::Value::String(
                            "--run-verified-meshy-mcp".into(),
                        )]),
                    );
                } else {
                    route.insert("command".into(), toml::Value::String("cmd.exe".into()));
                    route.insert(
                        "args".into(),
                        toml::Value::Array(
                            [
                                "/d".to_string(),
                                "/c".to_string(),
                                "call".to_string(),
                                format!(".tools/3d_pipeline/wrappers/{wrapper}"),
                            ]
                            .into_iter()
                            .map(toml::Value::String)
                            .collect(),
                        ),
                    );
                }
                route.insert("cwd".into(), toml::Value::String(".".into()));
                route.insert(
                    "env_vars".into(),
                    toml::Value::Array(if id == "meshy" {
                        vec![toml::Value::String("MESHY_API_KEY".into())]
                    } else {
                        Vec::new()
                    }),
                );
                route.insert("startup_timeout_sec".into(), toml::Value::Float(120.0));
                route.insert("tool_timeout_sec".into(), toml::Value::Float(1800.0));
                route.insert(
                    "default_tools_approval_mode".into(),
                    toml::Value::String(approval.into()),
                );
                mcp_servers.insert(id.into(), toml::Value::Table(route));
            }
        } else if let Some(mcp_servers) = table
            .get_mut("mcp_servers")
            .and_then(toml::Value::as_table_mut)
        {
            for id in ["meshy", "blender_hoi4", "blender_lab"] {
                mcp_servers.remove(id);
            }
        }
        let empty = table
            .get_mut("mcp_servers")
            .and_then(toml::Value::as_table_mut)
            .map(|mcp| {
                if !mcp_selected {
                    mcp.remove("hoi4_agent_tools");
                }
                mcp.is_empty()
            })
            .unwrap_or(false);
        if empty {
            table.remove("mcp_servers");
        }
        if portrait.provider != "cloud" {
            let empty = table
                .get_mut("mcp_servers")
                .and_then(toml::Value::as_table_mut)
                .map(|mcp| {
                    mcp.remove("comfy_cloud_portraits");
                    mcp.is_empty()
                })
                .unwrap_or(false);
            if empty {
                table.remove("mcp_servers");
            }
            if let Some(portrait_table) = table
                .get_mut("portrait_pipeline")
                .and_then(toml::Value::as_table_mut)
            {
                portrait_table.remove("mcp_server");
            }
        }
    }
    let rendered = toml::to_string_pretty(&config).map_err(|error| {
        AppError::Source(format!(
            "adapted Codex configuration could not be written: {error}"
        ))
    })?;
    rendered.parse::<toml::Value>().map_err(|error| {
        AppError::Source(format!("adapted Codex configuration is invalid: {error}"))
    })?;
    Ok(rendered.into_bytes())
}

#[cfg(test)]
mod project_settings_tests {
    use super::*;

    fn identity(name: &str, prefix: Option<&str>) -> ProjectIdentity {
        ProjectIdentity {
            display_name: name.into(),
            project_id: "x".into(),
            author: String::new(),
            version: "1".into(),
            supported_game_version: "*".into(),
            project_root: std::path::PathBuf::from("."),
            default_branch: "main".into(),
            script_prefix: prefix.map(str::to_string),
            primary_namespace: None,
            descriptor_tags: Vec::new(),
            launcher_descriptor_path: None,
        }
    }

    const ASSET: &str = "{
 \"_about\": \"keep \\\"this\\\" text\",
 \"project_label\": \"HOI4\",
 \"id_prefix\": \"hoi4\",
 \"reference_folder\": \"vanilla_reference\"
}
";
    const GFX: &str = "{
 \"asset_tools\": \"a/b\",
 \"shared_registry_prefix\": \"\"
}
";

    #[test]
    fn the_asset_tools_get_the_mods_name_and_prefix_and_nothing_else_changes() {
        let adapted = adapt_asset_tool_settings(
            ASSET.as_bytes(),
            &identity("Atlantis \"Rising\"", Some("atl")),
        );
        let text = String::from_utf8(adapted).unwrap();
        let value: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(value["project_label"], "Atlantis \"Rising\"");
        assert_eq!(value["id_prefix"], "atl");
        assert_eq!(value["reference_folder"], "vanilla_reference");
        assert_eq!(value["_about"], "keep \"this\" text");
        // Same lines in the same order: only the two values differ.
        assert_eq!(text.lines().count(), ASSET.lines().count());
        assert!(text.contains(" \"reference_folder\": \"vanilla_reference\""));
    }

    #[test]
    fn without_a_prefix_only_the_label_is_set() {
        let text = String::from_utf8(adapt_asset_tool_settings(
            ASSET.as_bytes(),
            &identity("Mod", None),
        ))
        .unwrap();
        assert!(text.contains("\"project_label\": \"Mod\""));
        assert!(text.contains("\"id_prefix\": \"hoi4\""));
    }

    #[test]
    fn the_gfx_registry_prefix_is_the_script_prefix() {
        let text = String::from_utf8(adapt_gfx_settings(
            GFX.as_bytes(),
            &identity("Mod", Some("atl")),
        ))
        .unwrap();
        assert!(text.contains("\"shared_registry_prefix\": \"atl\""));
        assert_eq!(
            adapt_gfx_settings(GFX.as_bytes(), &identity("Mod", None)),
            GFX.as_bytes()
        );
        assert_eq!(
            adapt_gfx_settings(b"not json", &identity("Mod", Some("atl"))),
            b"not json"
        );
    }
}
