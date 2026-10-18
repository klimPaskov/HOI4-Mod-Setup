//! Image-generation choices shared by the desktop app and the web edition:
//! the verified provider catalog, the guidance written into skills, the
//! provider configuration file, and the image and live-testing routes in each
//! selected client's MCP file. Keys stay in the operating-system credential
//! store; nothing here reads or writes a secret.

use crate::coding_environment;
use crate::mcp_target::VerifiedMcpTarget;
use crate::models::{
    CodingEnvironmentSelection, ImageGenerationSelection, LiveTestingSelection, Platform,
    RemoteManifest,
};
use crate::AppError;
use serde_json::Value;

/// Turns the verified HOI4 Agent Tools target into the command and arguments
/// a client runs on the chosen platform.
pub type ClientLaunch<'a> =
    dyn Fn(&VerifiedMcpTarget) -> Result<(String, Vec<String>), AppError> + 'a;

/// The launch a web export records: the package's own command, resolved from
/// the user's PATH once they install HOI4 Agent Tools.
pub fn portable_client_launch(
    target: &VerifiedMcpTarget,
) -> Result<(String, Vec<String>), AppError> {
    Ok((target.target.clone(), Vec::new()))
}

/// Point the HOI4 Agent Tools route in a client file at `launch`, or remove it.
pub fn retarget_client_config(
    bytes: &[u8],
    destination: &str,
    launch: Option<&(String, Vec<String>)>,
) -> Result<Vec<u8>, AppError> {
    let Some((_, route)) = coding_environment::mcp_route_for_path(destination) else {
        return Ok(bytes.to_vec());
    };
    crate::native_mcp::retarget(route.format, bytes, launch)
}

pub fn image_generation_provider_guidance(
    manifest: &RemoteManifest,
    selection: &ImageGenerationSelection,
) -> Result<String, AppError> {
    let mut sections = Vec::new();
    for provider in image_generation_provider_rows(manifest, selection)? {
        let name = provider
            .get("display_name")
            .and_then(Value::as_str)
            .unwrap_or("Image provider");
        let route = provider.get("route").unwrap_or(&Value::Null);
        let tools = route
            .get("tool_names")
            .and_then(Value::as_object)
            .map(|tools| {
                tools
                    .iter()
                    .filter_map(|(role, name)| {
                        name.as_str().map(|name| format!("{role}: `{name}`"))
                    })
                    .collect::<Vec<_>>()
                    .join(", ")
            })
            .unwrap_or_default();
        let tool_arguments = route
            .get("tool_arguments")
            .and_then(Value::as_object)
            .map(|arguments| {
                arguments
                    .iter()
                    .map(|(role, argument)| match argument.as_str() {
                        Some(name) => format!("{role}: `{name}`"),
                        None => format!("{role}: not supported by this tool"),
                    })
                    .collect::<Vec<_>>()
                    .join(", ")
            })
            .unwrap_or_default();
        let dynamic_tools = route.get("api_protocol").and_then(Value::as_str) == Some("mcp_proxy")
            || route.get("kind").and_then(Value::as_str) == Some("remote_http") && tools.is_empty();
        let tools = if dynamic_tools {
            "The server lists its tools when it connects; use the names returned by MCP tools/list."
                .into()
        } else if tools.is_empty() {
            "No image tool is registered for this route.".into()
        } else {
            tools
        };
        let capabilities = provider.get("capabilities").unwrap_or(&Value::Null);
        let references = capabilities
            .pointer("/reference_images/notes")
            .and_then(Value::as_str)
            .unwrap_or("Use reference images only when this provider supports them.");
        let limits = capabilities
            .pointer("/size_limits/notes")
            .and_then(Value::as_str)
            .unwrap_or("Follow the selected model's size limits.");
        let maximum_pixels = capabilities
            .pointer("/size_limits/maximum_pixels")
            .and_then(Value::as_u64)
            .map(|pixels| format!(" Maximum image area: {pixels} pixels."))
            .unwrap_or_default();
        let alpha = capabilities
            .pointer("/transparent_background/notes")
            .and_then(Value::as_str)
            .unwrap_or("Check whether this route supports transparent output.");
        let fallback_alpha = capabilities
            .get("fallback_if_no_alpha")
            .and_then(Value::as_str)
            .unwrap_or("Use a configured background-removal route or ask the maintainer.");
        let edits = capabilities
            .get("image_edits")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let fallback_edits = capabilities
            .get("fallback_if_no_edits")
            .and_then(Value::as_str)
            .unwrap_or("Generate a new variation from the reference or ask the maintainer.");
        sections.push(format!(
            "### {name}\n- Tools: {tools}\n- Tool inputs: {tool_arguments}\n- References: {references}\n- Size: {limits}{maximum_pixels}\n- Transparent backgrounds: {alpha} If unavailable: {fallback_alpha}\n- Reference edits: {} If unavailable: {fallback_edits}\n- Make one image per call. For a transparent image, check that the saved PNG has at least one pixel with alpha below 255; an alpha channel alone does not prove that pixels are transparent.",
            if edits { "supported on compatible models." } else { "not available on this route." }
        ));
    }
    Ok(sections.join("\n\n"))
}

pub const IMAGE_GUIDANCE_REFERENCE: &str = ".tools/image_generation/selected-provider.md";

pub const SKILL_BYTE_LIMIT: usize = 58_000;

pub fn image_generation_additional_provider_summary(
    selection: &ImageGenerationSelection,
) -> String {
    if selection.additional_providers.is_empty() {
        "none".into()
    } else {
        selection
            .additional_providers
            .iter()
            .map(|id| format!("`{id}`"))
            .collect::<Vec<_>>()
            .join(", ")
    }
}

pub fn adapt_image_generation_guidance(
    bytes: &[u8],
    manifest: &RemoteManifest,
    selection: &ImageGenerationSelection,
) -> Result<Vec<u8>, AppError> {
    let start_marker = "<!-- HOI4_MOD_SETUP:IMAGE_GENERATION_PROVIDER:START -->";
    let end_marker = "<!-- HOI4_MOD_SETUP:IMAGE_GENERATION_PROVIDER:END -->";
    let start_count = bytes
        .windows(start_marker.len())
        .filter(|window| *window == start_marker.as_bytes())
        .count();
    let end_count = bytes
        .windows(end_marker.len())
        .filter(|window| *window == end_marker.as_bytes())
        .count();
    if start_count == 0 && end_count == 0 {
        return Ok(bytes.to_vec());
    }
    if start_count != 1 || end_count != 1 {
        return Err(AppError::Source(
            "image provider instructions require exactly one complete marker block".into(),
        ));
    }
    let text = std::str::from_utf8(bytes).map_err(|error| AppError::Source(error.to_string()))?;
    let start = text.find(start_marker).unwrap();
    let content_start = start + start_marker.len();
    let end = text[content_start..]
        .find(end_marker)
        .map(|offset| content_start + offset)
        .ok_or_else(|| {
            AppError::Source("image provider instructions have an incomplete marker".into())
        })?;
    let guidance = image_generation_provider_guidance(manifest, selection)?;
    let compose = |content: &str| {
        format!(
            "{}{}{}",
            &text[..start],
            content,
            &text[end + end_marker.len()..]
        )
        .into_bytes()
    };
    let inline = compose(&guidance);
    if inline.len() <= SKILL_BYTE_LIMIT {
        return Ok(inline);
    }
    Ok(compose(&format!(
        "### Selected image providers\nPrimary: `{}`. Additional providers: {}.\nRead project-root `{IMAGE_GUIDANCE_REFERENCE}` before using image tools; it contains every selected provider, tool input, limit, and fallback. Make one image per call. Transparent PNGs must contain at least one pixel with alpha below 255; follow the reference's background-removal fallback when native alpha is unavailable.\n",
        selection.primary_provider, image_generation_additional_provider_summary(selection)
    )))
}

pub fn image_client_config_default(environment: &str) -> &'static [u8] {
    let toml = coding_environment::native_client(environment)
        .and_then(|client| client.mcp)
        .is_some_and(|mcp| mcp.format.is_toml());
    if toml {
        b""
    } else {
        b"{}"
    }
}

pub fn merge_json_objects_preserving_local(
    incoming: &mut serde_json::Map<String, Value>,
    local: serde_json::Map<String, Value>,
) {
    for (key, local_value) in local {
        match (incoming.get_mut(&key), local_value) {
            (Some(Value::Object(incoming_object)), Value::Object(local_object)) => {
                merge_json_objects_preserving_local(incoming_object, local_object);
            }
            (_, local_value) => {
                incoming.insert(key, local_value);
            }
        }
    }
}

pub fn merge_toml_tables_preserving_local(
    incoming: &mut toml::map::Map<String, toml::Value>,
    local: toml::map::Map<String, toml::Value>,
) {
    for (key, local_value) in local {
        match (incoming.get_mut(&key), local_value) {
            (Some(toml::Value::Table(incoming_table)), toml::Value::Table(local_table)) => {
                merge_toml_tables_preserving_local(incoming_table, local_table);
            }
            (_, local_value) => {
                incoming.insert(key, local_value);
            }
        }
    }
}

pub fn managed_image_server_ids(manifest: &RemoteManifest) -> std::collections::HashSet<String> {
    manifest
        .image_generation
        .as_ref()
        .and_then(|catalog| catalog.get("providers"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|provider| provider.get("id").and_then(Value::as_str))
        .map(|id| format!("hoi4_image_{id}"))
        .collect()
}

pub fn image_generation_provider_rows<'a>(
    manifest: &'a RemoteManifest,
    selection: &ImageGenerationSelection,
) -> Result<Vec<&'a Value>, AppError> {
    let rows = manifest
        .image_generation
        .as_ref()
        .and_then(|table| table.get("providers"))
        .and_then(Value::as_array);
    if rows.is_none()
        && selection.primary_provider == "none"
        && selection.additional_providers.is_empty()
    {
        return Ok(Vec::new());
    }
    let rows = rows.ok_or_else(|| {
        AppError::Source("the verified source manifest has no image provider table".into())
    })?;
    let selected =
        std::iter::once(&selection.primary_provider).chain(selection.additional_providers.iter());
    selected
        .map(|id| {
            rows.iter()
                .find(|row| row.get("id").and_then(Value::as_str) == Some(id.as_str()))
                .ok_or_else(|| {
                    AppError::InvalidInput(
                        "selected image provider is missing from the source table".into(),
                    )
                })
        })
        .collect()
}

/// The project-relative launch for a selected image provider. `windows`
/// picks the Python launcher name.
pub fn image_server_route_for(provider: &Value, windows: bool) -> Option<Value> {
    let route = provider.get("route")?;
    let id = provider.get("id")?.as_str()?;
    let kind = route.get("kind")?.as_str()?;
    let (command, args): (String, Vec<String>) = match kind {
        "stdio_adapter" => (
            if windows { "python" } else { "python3" }.into(),
            vec![
                ".tools/image_generation/runtime/server.py".into(),
                "--provider".into(),
                id.into(),
            ],
        ),
        "local_package" if id == "gptimage" => (
            "node".into(),
            vec![".tools/image_generation/providers/gptimage/src/server.js".into()],
        ),
        _ => return None,
    };
    Some(serde_json::json!({
        "command": command,
        "args": args,
        "cwd": ".",
    }))
}

/// Replace the managed image routes in a selected client's MCP file with the
/// selected providers' routes. Other servers are preserved.
pub fn adapt_image_only_client_config_for(
    bytes: &[u8],
    destination: &str,
    manifest: &RemoteManifest,
    selection: &ImageGenerationSelection,
    coding_environments: &CodingEnvironmentSelection,
    windows: bool,
) -> Result<Vec<u8>, AppError> {
    let Some((environment, mcp_format)) = coding_environment::mcp_route_for_path(destination)
        .filter(|(client, _)| client.image_and_live_routes)
        .map(|(client, route)| (client.id, route.format))
        .filter(|(environment, _)| {
            coding_environment::selected_environment_ids(coding_environments)
                .iter()
                .any(|selected| selected == environment)
        })
    else {
        return Ok(bytes.to_vec());
    };
    let providers = image_generation_provider_rows(manifest, selection)?;
    let managed_server_ids = managed_image_server_ids(manifest);
    if mcp_format.is_toml() {
        let text = std::str::from_utf8(bytes).map_err(|error| {
            AppError::Source(format!("Codex configuration is not UTF-8: {error}"))
        })?;
        let mut config = text.parse::<toml::Value>().map_err(|error| {
            AppError::Source(format!("Codex configuration is not valid TOML: {error}"))
        })?;
        let table = config
            .as_table_mut()
            .ok_or_else(|| AppError::Source("Codex configuration is not a TOML table".into()))?;
        let mcp = table
            .entry("mcp_servers")
            .or_insert_with(|| toml::Value::Table(toml::map::Map::new()))
            .as_table_mut()
            .ok_or_else(|| AppError::Source("Codex MCP settings are not a TOML table".into()))?;
        mcp.retain(|id, _| !managed_server_ids.contains(id));
        for provider in providers {
            if !provider
                .get("supported_coding_environments")
                .and_then(Value::as_array)
                .is_some_and(|values| {
                    values
                        .iter()
                        .any(|value| value.as_str() == Some(environment))
                })
            {
                continue;
            }
            let id = provider
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let Some(route) = image_server_route_for(provider, windows) else {
                continue;
            };
            let command = route
                .get("command")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let args = route
                .get("args")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(|value| toml::Value::String(value.into()))
                .collect();
            let mut fields = toml::map::Map::new();
            fields.insert("command".into(), toml::Value::String(command.into()));
            fields.insert("args".into(), toml::Value::Array(args));
            fields.insert("cwd".into(), toml::Value::String(".".into()));
            // `enabled` is a Codex key; Grok Build documents only the launch
            // and timeout fields for a project server.
            if environment == coding_environment::CODEX {
                fields.insert("enabled".into(), toml::Value::Boolean(true));
            }
            mcp.insert(format!("hoi4_image_{id}"), toml::Value::Table(fields));
        }
        if mcp.is_empty() {
            table.remove("mcp_servers");
        }
        return toml::to_string_pretty(&config)
            .map(|value| value.into_bytes())
            .map_err(|error| {
                AppError::Source(format!(
                    "Codex MCP configuration could not be written: {error}"
                ))
            });
    }
    let mut config: Value = crate::native_mcp::parse_json_lenient(bytes).map_err(|error| {
        AppError::Source(format!(
            "coding client MCP configuration is not valid JSON: {error}"
        ))
    })?;
    let root = config.as_object_mut().ok_or_else(|| {
        AppError::Source("coding client MCP settings are not a JSON object".into())
    })?;
    let key = mcp_format.servers_key();
    let routes = root
        .entry(key)
        .or_insert_with(|| Value::Object(serde_json::Map::new()))
        .as_object_mut()
        .ok_or_else(|| AppError::Source("coding client MCP routes are not a JSON object".into()))?;
    routes.retain(|id, _| !managed_server_ids.contains(id));
    for provider in providers {
        if !provider
            .get("supported_coding_environments")
            .and_then(Value::as_array)
            .is_some_and(|values| {
                values
                    .iter()
                    .any(|value| value.as_str() == Some(environment))
            })
        {
            continue;
        }
        let id = provider
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let Some(route) = image_server_route_for(provider, windows) else {
            continue;
        };
        let server_id = format!("hoi4_image_{id}");
        if mcp_format == crate::native_mcp::McpFormat::VscodeServers {
            // VS Code resolves the relative adapter path from the workspace.
            routes.insert(
                server_id,
                serde_json::json!({
                    "type": "stdio",
                    "command": route["command"],
                    "args": route["args"],
                    "cwd": "${workspaceFolder}",
                }),
            );
        } else if mcp_format == crate::native_mcp::McpFormat::LocalCommandArray {
            let mut command = vec![route["command"].clone()];
            command.extend(route["args"].as_array().cloned().unwrap_or_default());
            routes.insert(
                server_id,
                serde_json::json!({
                    "type": "local",
                    "command": command,
                    "enabled": true,
                    "cwd": ".",
                }),
            );
        } else {
            routes.insert(server_id, route);
        }
    }
    if routes.is_empty() {
        root.remove(key);
    }
    serde_json::to_vec_pretty(&config).map_err(|error| {
        AppError::Source(format!(
            "coding client MCP configuration could not be written: {error}"
        ))
    })
}

/// The live-testing guidance, image routes, HOI4 Agent Tools launch, and
/// live-testing routes for one source file. `launch` turns the verified tools
/// target into the command the client runs on `platform`.
#[allow(clippy::too_many_arguments)]
pub fn adapt_image_generation_client_config_with_launch(
    bytes: &[u8],
    destination: &str,
    manifest: &RemoteManifest,
    selection: &ImageGenerationSelection,
    coding_environments: &CodingEnvironmentSelection,
    live_selection: &LiveTestingSelection,
    platform: Platform,
    windows: bool,
    launch: &ClientLaunch<'_>,
) -> Result<Vec<u8>, AppError> {
    let guided = crate::live_testing::guidance(bytes, manifest, live_selection)?;
    let image_bytes = adapt_image_only_client_config_for(
        &guided,
        destination,
        manifest,
        selection,
        coding_environments,
        windows,
    )?;
    let normalized = destination.replace('\\', "/").to_ascii_lowercase();
    let route = coding_environment::mcp_route_for_path(destination);
    let environment = route.map(|(client, _)| client.id);
    let disabled = LiveTestingSelection {
        provider: "none".into(),
        ..Default::default()
    };
    let live_selection = if environment.is_some_and(|id| {
        !coding_environment::selected_environment_ids(coding_environments)
            .iter()
            .any(|selected| selected == id)
    }) {
        &disabled
    } else {
        live_selection
    };
    let image_bytes = match route {
        Some(_) if matches!(platform, Platform::Windows | Platform::Linux) => {
            let target = crate::mcp_target::manifest_target_for_platform(manifest, platform).ok();
            if platform == Platform::Windows && target.is_none() {
                image_bytes
            } else {
                let launch = target.as_ref().map(launch).transpose()?;
                retarget_client_config(&image_bytes, destination, launch.as_ref())?
            }
        }
        // A settings file installed on every platform never keeps a route to
        // a tools package that does not exist here.
        Some((_, mcp)) if mcp.shared_file => {
            retarget_client_config(&image_bytes, destination, None)?
        }
        _ => image_bytes,
    };
    let adapted =
        crate::live_testing::adapt_config(&image_bytes, destination, live_selection, platform)?;
    // Historical packages without provider adaptation must remain reproducible
    // for migration. Provider-enabled packages retain the strict skill ceiling.
    if manifest.image_generation.is_some()
        && (normalized == "skill.md" || normalized.ends_with("/skill.md"))
        && adapted.len() > SKILL_BYTE_LIMIT
    {
        return Err(AppError::Source(format!(
            "adapted skill exceeds the {SKILL_BYTE_LIMIT}-byte limit: {destination}"
        )));
    }
    Ok(adapted)
}

/// The project-root reference of every selected provider's tools, inputs,
/// limits, and fallbacks, or `None` when no provider is selected.
pub fn image_generation_guidance_content(
    manifest: &RemoteManifest,
    selection: &ImageGenerationSelection,
) -> Result<Option<String>, AppError> {
    let providers = image_generation_provider_rows(manifest, selection)?;
    if providers.is_empty() {
        return Ok(None);
    }
    let routes = providers
        .iter()
        .map(|provider| {
            format!(
                "- `{}`: route `{}`, server `{}`.",
                provider["id"].as_str().unwrap_or_default(),
                provider["route"]["kind"].as_str().unwrap_or("manual"),
                provider["route"]["server_id"].as_str().unwrap_or("none")
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let content = format!("# Selected image providers\n\nPrimary: `{}`. Additional providers: {}.\n\n{routes}\n\n{}\n",
        selection.primary_provider, image_generation_additional_provider_summary(selection),
        image_generation_provider_guidance(manifest, selection)?);
    Ok(Some(content))
}

/// The non-secret provider configuration the image runtime reads, or `None`
/// when no adapter route is selected.
pub fn image_generation_config_content(
    manifest: &RemoteManifest,
    selection: &ImageGenerationSelection,
) -> Result<Option<String>, AppError> {
    let mut providers = serde_json::Map::new();
    for provider in image_generation_provider_rows(manifest, selection)? {
        if provider.pointer("/route/kind").and_then(Value::as_str) != Some("stdio_adapter") {
            continue;
        }
        let id = provider
            .get("id")
            .and_then(Value::as_str)
            .ok_or_else(|| AppError::Source("image provider is missing its id".into()))?;
        let credential_references = selection
            .credential_references
            .iter()
            .filter_map(|(key, value)| {
                key.strip_prefix(&format!("{id}."))
                    .map(|field| (field.to_owned(), Value::String(value.clone())))
            })
            .collect::<serde_json::Map<_, _>>();
        let settings = selection
            .route_settings
            .get(id)
            .cloned()
            .unwrap_or_default();
        providers.insert(
            id.to_owned(),
            serde_json::json!({
                "id": id,
                "display_name": provider.get("display_name"),
                "route": provider.get("route"),
                "capabilities": provider.get("capabilities"),
                "credential_references": credential_references,
                "settings": settings,
            }),
        );
    }
    if providers.is_empty() {
        return Ok(None);
    }
    let content = serde_json::to_string_pretty(&serde_json::json!({
        "schema_version": "1.0.0",
        "primary_provider": selection.primary_provider,
        "additional_providers": selection.additional_providers,
        "providers": providers,
    }))?;
    Ok(Some(content))
}

/// Merge incoming client configuration with the user's existing file,
/// keeping every local value. `local` is `None` when the file is absent.
pub fn merge_image_client_config(
    destination: &str,
    incoming: &[u8],
    local: Option<&[u8]>,
) -> Result<Vec<u8>, AppError> {
    let Some(local_bytes) = local else {
        return Ok(incoming.to_vec());
    };
    if coding_environment::config_format_for_path(destination)
        .is_some_and(crate::native_mcp::McpFormat::is_toml)
    {
        let incoming_text = std::str::from_utf8(incoming).map_err(|error| {
            AppError::Source(format!(
                "coding client TOML configuration is not UTF-8: {error}"
            ))
        })?;
        let local_text = std::str::from_utf8(local_bytes).map_err(|error| {
            AppError::Source(format!(
                "coding client TOML configuration is not UTF-8: {error}"
            ))
        })?;
        let mut incoming_value = incoming_text.parse::<toml::Value>().map_err(|error| {
            AppError::Source(format!(
                "coding client TOML configuration is not valid TOML: {error}"
            ))
        })?;
        let local_value = local_text.parse::<toml::Value>().map_err(|error| {
            AppError::Source(format!(
                "coding client TOML configuration is not valid TOML: {error}"
            ))
        })?;
        let (Some(incoming_table), Some(local_table)) = (
            incoming_value.as_table_mut(),
            local_value.as_table().cloned(),
        ) else {
            return Err(AppError::Source(
                "coding client TOML configuration is not a TOML table".into(),
            ));
        };
        merge_toml_tables_preserving_local(incoming_table, local_table);
        return toml::to_string_pretty(&incoming_value)
            .map(|value| value.into_bytes())
            .map_err(|error| {
                AppError::Source(format!(
                    "coding client TOML configuration could not be merged: {error}"
                ))
            });
    }
    let mut incoming_value: Value = serde_json::from_slice(incoming).map_err(|error| {
        AppError::Source(format!(
            "coding client configuration is not valid JSON: {error}"
        ))
    })?;
    // Local client files may use JSONC comments and trailing commas.
    let local_value: Value =
        crate::native_mcp::parse_json_lenient(local_bytes).map_err(|error| {
            AppError::Source(format!(
                "existing coding client configuration is not valid JSON: {error}"
            ))
        })?;
    let (Some(incoming_object), Some(local_object)) = (
        incoming_value.as_object_mut(),
        local_value.as_object().cloned(),
    ) else {
        return Err(AppError::Source(
            "coding client configuration is not a JSON object".into(),
        ));
    };
    merge_json_objects_preserving_local(incoming_object, local_object);
    serde_json::to_vec_pretty(&incoming_value).map_err(|error| {
        AppError::Source(format!(
            "coding client configuration could not be merged: {error}"
        ))
    })
}

/// The rewritten MCP file for one selected client, or `None` when it needs
/// no image or live-testing route and has none to remove. `source_bytes` is
/// the existing file or [`image_client_config_default`].
#[allow(clippy::too_many_arguments)]
pub fn image_client_config_update(
    environment: &str,
    destination: &str,
    source_bytes: &[u8],
    manifest: &RemoteManifest,
    selection: &ImageGenerationSelection,
    coding_environments: &CodingEnvironmentSelection,
    live_selection: &LiveTestingSelection,
    platform: Platform,
    windows: bool,
    launch: &ClientLaunch<'_>,
) -> Result<Option<Vec<u8>>, AppError> {
    let provider_rows = image_generation_provider_rows(manifest, selection)?;
    let has_route_for_environment = provider_rows.iter().any(|provider| {
        provider
            .get("supported_coding_environments")
            .and_then(Value::as_array)
            .is_some_and(|values| {
                values
                    .iter()
                    .any(|value| value.as_str() == Some(environment))
            })
            && image_server_route_for(provider, windows).is_some()
    });
    let has_managed_image_route = source_bytes
        .windows(b"hoi4_image_".len())
        .any(|window| window == b"hoi4_image_");
    let has_live_route = crate::live_testing::route(live_selection, platform)?.is_some();
    let has_managed_live_route = source_bytes
        .windows(b"clausewitz_linux".len())
        .any(|w| w == b"clausewitz_linux")
        || source_bytes
            .windows(b"hoi4-live".len())
            .any(|w| w == b"hoi4-live");
    if !has_route_for_environment
        && !has_managed_image_route
        && !has_live_route
        && !has_managed_live_route
    {
        return Ok(None);
    }
    let adapted = adapt_image_generation_client_config_with_launch(
        source_bytes,
        destination,
        manifest,
        selection,
        coding_environments,
        live_selection,
        platform,
        windows,
        launch,
    )?;
    if adapted == source_bytes {
        return Ok(None);
    }
    Ok(Some(adapted))
}

/// Check an image provider selection against the verified catalog and the
/// selected coding clients. Credential references are the host's to check.
pub fn validate_image_generation_selection(
    selection: &ImageGenerationSelection,
    manifest: &RemoteManifest,
    environments: &CodingEnvironmentSelection,
) -> Result<(), AppError> {
    let catalog = manifest.image_generation.as_ref();
    let table = catalog
        .and_then(|value| value.get("providers"))
        .and_then(Value::as_array);
    if table.is_none()
        && selection.primary_provider == "none"
        && selection.additional_providers.is_empty()
    {
        return Ok(());
    }
    let table = table.ok_or_else(|| {
        AppError::Source("choose no image provider for this older source package".into())
    })?;
    let find = |id: &str| {
        table
            .iter()
            .find(|provider| provider.get("id").and_then(Value::as_str) == Some(id))
    };
    if find(&selection.primary_provider).is_none() {
        return Err(AppError::InvalidInput(
            "choose an image provider from the verified source list".into(),
        ));
    }
    let mut seen = std::collections::HashSet::new();
    if selection.additional_providers.len() > 8
        || selection.additional_providers.iter().any(|id| {
            id == &selection.primary_provider || !seen.insert(id.as_str()) || find(id).is_none()
        })
    {
        return Err(AppError::InvalidInput(
            "optional image providers must be unique choices from the source list".into(),
        ));
    }
    if selection.primary_provider == "none" && !selection.additional_providers.is_empty() {
        return Err(AppError::InvalidInput(
            "choose image providers or choose none".into(),
        ));
    }
    let selected = std::iter::once(selection.primary_provider.as_str())
        .chain(selection.additional_providers.iter().map(String::as_str))
        .collect::<Vec<_>>();
    let environments_selected = coding_environment::selected_environment_ids(environments);
    for id in &selected {
        let provider = find(id).expect("provider verified above");
        // A manual provider registers nothing in any client, so it is
        // available whichever coding clients are selected.
        let manual = provider.pointer("/route/kind").and_then(Value::as_str) == Some("manual");
        let supported = manual
            || provider
                .get("supported_coding_environments")
                .and_then(Value::as_array)
                .is_some_and(|values| {
                    values.iter().filter_map(Value::as_str).any(|environment| {
                        environments_selected
                            .iter()
                            .any(|selected| selected == environment)
                    })
                });
        if !supported {
            return Err(AppError::InvalidInput(format!(
                "{} does not support a selected coding client",
                provider
                    .get("display_name")
                    .and_then(Value::as_str)
                    .unwrap_or("The image provider")
            )));
        }
        if let Some(required_primary) = provider
            .pointer("/route/requires_primary_coding_environment")
            .and_then(Value::as_str)
        {
            if environments.primary != required_primary {
                return Err(AppError::InvalidInput(
                    "Codex image generation requires Codex as the primary coding client".into(),
                ));
            }
        }
        let settings = selection.route_settings.get(*id);
        if let Some(settings) = settings {
            let protocol = provider
                .pointer("/route/api_protocol")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let supported_models = provider
                .pointer("/route/supported_models")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>();
            let allowed = match protocol {
                "comfyui" | "automatic1111" => &["server_url", "checkpoint"][..],
                "microsoft_foundry" => &["deployment_name"][..],
                "aws_bedrock" => &["region", "model"][..],
                _ if !supported_models.is_empty() => &["model"][..],
                _ => &[][..],
            };
            if settings.keys().any(|key| !allowed.contains(&key.as_str())) {
                return Err(AppError::InvalidInput(format!(
                    "{} has a setting that is not supported by this model route",
                    provider
                        .get("display_name")
                        .and_then(Value::as_str)
                        .unwrap_or("This provider")
                )));
            }
            if let Some(model) = settings.get("model") {
                if !supported_models.iter().any(|allowed| allowed == model) {
                    return Err(AppError::InvalidInput(
                        "choose a model from the selected provider's supported model list".into(),
                    ));
                }
            }
            if let Some(server_url) = settings.get("server_url") {
                if !is_loopback_http_url(server_url) {
                    return Err(AppError::InvalidInput(
                        "local image servers must use a loopback address on this computer".into(),
                    ));
                }
            }
        }
    }
    Ok(())
}

/// An http(s) address on this computer, without credentials.
pub fn is_loopback_http_url(value: &str) -> bool {
    let Some((scheme, rest)) = value.split_once("://") else {
        return false;
    };
    if !matches!(scheme.to_ascii_lowercase().as_str(), "http" | "https") {
        return false;
    }
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    if authority.contains('@') {
        return false;
    }
    let host = if let Some(bracketed) = authority.strip_prefix('[') {
        match bracketed.split_once(']') {
            Some((host, port))
                if port.is_empty()
                    || port.strip_prefix(':').is_some_and(|port| {
                        !port.is_empty() && port.bytes().all(|byte| byte.is_ascii_digit())
                    }) =>
            {
                host
            }
            _ => return false,
        }
    } else {
        let (host, port) = authority.split_once(':').unwrap_or((authority, ""));
        if authority.contains(':')
            && (port.is_empty() || !port.bytes().all(|byte| byte.is_ascii_digit()))
        {
            return false;
        }
        host
    };
    matches!(
        host.to_ascii_lowercase().as_str(),
        "localhost" | "127.0.0.1" | "::1"
    )
}
