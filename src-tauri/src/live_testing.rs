use crate::models::{LiveTestingSelection, LockComponent, Platform, RemoteManifest};
use crate::AppError;
use serde_json::{json, Value};

const REMOTE: &str = "exec \"$HOME/tools/Clausewitz-MCP/.venv/bin/python\" \"$HOME/tools/Clausewitz-MCP/clausewitz.py\" serve";

pub fn legacy_selection(components: &[LockComponent]) -> LiveTestingSelection {
    LiveTestingSelection {
        provider: if components.iter().any(|c| {
            c.id == "workflow.live_tools"
                && !matches!(
                    c.state.as_str(),
                    "removed" | "not_selected" | "unsupported_platform"
                )
        }) {
            "hoi4_live_tools"
        } else {
            "none"
        }
        .into(),
        ..Default::default()
    }
}

pub fn validate(selection: &LiveTestingSelection, platform: Platform) -> Result<(), AppError> {
    let invalid = |message: &str| AppError::InvalidInput(message.into());
    if !["clausewitz_mcp", "hoi4_live_tools", "none"].contains(&selection.provider.as_str()) {
        return Err(invalid("unknown live testing provider"));
    }
    for (value, alias) in [
        (&selection.wsl_distribution, false),
        (&selection.ssh_host, true),
    ] {
        if !value.is_empty()
            && (value.len() > if alias { 128 } else { 64 }
                || !value
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-' || b == b'.')
                || !value.as_bytes()[0].is_ascii_alphanumeric()
                || (alias && value.contains('@')))
        {
            return Err(invalid(
                "live testing requires a safe WSL distribution name or SSH host alias",
            ));
        }
    }
    if selection.provider == "hoi4_live_tools" && platform != Platform::Windows {
        return Err(invalid("HOI4 LiveTools supports Windows only"));
    }
    if selection.provider == "clausewitz_mcp"
        && selection.linux_ready
        && platform == Platform::Macos
        && selection.ssh_host.is_empty()
    {
        return Err(invalid(
            "a confirmed Linux route on macOS requires an SSH host alias",
        ));
    }
    Ok(())
}

pub fn from_state(
    state: &Value,
    manifest: &RemoteManifest,
    requested: &[String],
    platform: Platform,
) -> Result<LiveTestingSelection, AppError> {
    let enabled = requested
        .iter()
        .any(|id| id == "workflow.live_testing" || id == "workflow.live_tools");
    let value = state
        .get("live_testing")
        .or_else(|| state.get("liveTesting"));
    let mut selection: LiveTestingSelection = if let Some(value) = value.filter(|v| !v.is_null()) {
        serde_json::from_value(value.clone())
            .map_err(|e| AppError::InvalidInput(format!("invalid live testing settings: {e}")))?
    } else if requested.iter().any(|id| id == "workflow.live_tools") {
        LiveTestingSelection {
            provider: "hoi4_live_tools".into(),
            ..Default::default()
        }
    } else {
        Default::default()
    };
    if !enabled {
        selection.provider = "none".into();
    }
    validate(&selection, platform)?;
    if selection.provider != "none"
        && manifest.live_testing.as_ref().is_some_and(|catalog| {
            !catalog["providers"]
                .as_array()
                .is_some_and(|rows| rows.iter().any(|row| row["id"] == selection.provider))
        })
    {
        return Err(AppError::Source(
            "selected live testing provider is absent from the verified catalog".into(),
        ));
    }
    Ok(selection)
}

pub fn components(
    requested: &mut Vec<String>,
    selection: &LiveTestingSelection,
    manifest: &RemoteManifest,
) -> Result<(), AppError> {
    let legacy = manifest.live_testing.is_none();
    if legacy && selection.provider == "clausewitz_mcp" {
        return Err(AppError::Source(
            "Clausewitz MCP is not available in this verified source revision".into(),
        ));
    }
    requested.retain(|id| {
        !matches!(
            id.as_str(),
            "workflow.live_testing" | "workflow.live_tools" | "mcp.hoi4_live.bootstrap"
        )
    });
    let ids: &[&str] = match selection.provider.as_str() {
        "none" => &[],
        "hoi4_live_tools" if legacy => &["workflow.live_tools"],
        "hoi4_live_tools" => &["workflow.live_testing", "workflow.live_tools"],
        _ => &["workflow.live_testing"],
    };
    for id in ids {
        if !manifest.components.iter().any(|c| c.id == *id) {
            return Err(AppError::Source(format!(
                "live testing component is missing from verified manifest: {id}"
            )));
        }
        requested.push((*id).into());
    }
    Ok(())
}

pub fn route(
    selection: &LiveTestingSelection,
    platform: Platform,
) -> Result<Option<(&'static str, Value)>, AppError> {
    validate(selection, platform)?;
    if selection.provider == "none"
        || (selection.provider == "clausewitz_mcp" && !selection.linux_ready)
    {
        return Ok(None);
    }
    if selection.provider == "hoi4_live_tools" {
        let appdata = std::env::var("LOCALAPPDATA").map_err(|_| {
            AppError::InvalidInput("LOCALAPPDATA is required for HOI4 LiveTools".into())
        })?;
        let command = std::path::Path::new(&appdata)
            .join("hoi4-live-tools/app/hoi4-live-tools.cmd")
            .to_string_lossy()
            .into_owned();
        return Ok(Some((
            "hoi4-live",
            json!({"command":command,"args":[],"cwd":"."}),
        )));
    }
    let (command, args) = match platform {
        Platform::Windows => {
            let mut args = Vec::<String>::new();
            if !selection.wsl_distribution.is_empty() {
                args.extend(["--distribution".into(), selection.wsl_distribution.clone()]);
            }
            args.extend(["--exec".into(), "sh".into(), "-lc".into(), REMOTE.into()]);
            ("wsl.exe", args)
        }
        Platform::Macos => (
            "ssh",
            vec![
                "-o".into(),
                "BatchMode=yes".into(),
                "-T".into(),
                selection.ssh_host.clone(),
                REMOTE.into(),
            ],
        ),
        Platform::Unsupported => {
            return Err(AppError::InvalidInput(
                "live testing supports Windows and macOS only".into(),
            ))
        }
    };
    Ok(Some((
        "clausewitz_linux",
        json!({"command":command,"args":args,"cwd":"."}),
    )))
}

pub fn adapt_config(
    bytes: &[u8],
    destination: &str,
    selection: &LiveTestingSelection,
    platform: Platform,
) -> Result<Vec<u8>, AppError> {
    let normalized = destination.replace('\\', "/").to_ascii_lowercase();
    if ![
        ".mcp.json",
        ".cursor/mcp.json",
        ".qoder/mcp.json",
        "opencode.json",
        ".codex/config.toml",
    ]
    .contains(&normalized.as_str())
    {
        return Ok(bytes.to_vec());
    }
    let selected = route(selection, platform)?;
    if normalized == ".codex/config.toml" {
        let mut config: toml::Value = std::str::from_utf8(bytes)
            .map_err(|e| AppError::Source(e.to_string()))?
            .parse()
            .map_err(|e: toml::de::Error| AppError::Source(e.to_string()))?;
        let root = config
            .as_table_mut()
            .ok_or_else(|| AppError::Source("Codex configuration must be a table".into()))?;
        let routes = root
            .entry("mcp_servers")
            .or_insert_with(|| toml::Value::Table(Default::default()))
            .as_table_mut()
            .ok_or_else(|| AppError::Source("MCP routes must be a table".into()))?;
        routes.remove("clausewitz_linux");
        routes.remove("hoi4-live");
        if let Some((id, route)) = selected {
            let mut fields = toml::map::Map::new();
            fields.insert(
                "command".into(),
                toml::Value::String(route["command"].as_str().unwrap().into()),
            );
            fields.insert(
                "args".into(),
                toml::Value::Array(
                    route["args"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|v| toml::Value::String(v.as_str().unwrap().into()))
                        .collect(),
                ),
            );
            fields.insert("enabled".into(), toml::Value::Boolean(true));
            fields.insert("startup_timeout_sec".into(), toml::Value::Integer(30));
            fields.insert("tool_timeout_sec".into(), toml::Value::Integer(1800));
            routes.insert(id.into(), toml::Value::Table(fields));
        }
        if routes.is_empty() {
            root.remove("mcp_servers");
        }
        return toml::to_string_pretty(&config)
            .map(String::into_bytes)
            .map_err(|e| AppError::Source(e.to_string()));
    }
    let mut config: Value =
        serde_json::from_slice(bytes).map_err(|e| AppError::Source(e.to_string()))?;
    let root = config
        .as_object_mut()
        .ok_or_else(|| AppError::Source("client configuration must be an object".into()))?;
    let key = if normalized == "opencode.json" {
        "mcp"
    } else {
        "mcpServers"
    };
    let routes = root
        .entry(key)
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .ok_or_else(|| AppError::Source("MCP routes must be an object".into()))?;
    routes.remove("clausewitz_linux");
    routes.remove("hoi4-live");
    if let Some((id, mut route)) = selected {
        if key == "mcp" {
            let mut command = vec![route["command"].clone()];
            command.extend(route["args"].as_array().unwrap().clone());
            route = json!({"type":"local","command":command,"enabled":true});
        }
        routes.insert(id.into(), route);
    }
    if routes.is_empty() {
        root.remove(key);
    }
    serde_json::to_vec_pretty(&config).map_err(|e| AppError::Source(e.to_string()))
}

pub fn guidance(
    bytes: &[u8],
    manifest: &RemoteManifest,
    selection: &LiveTestingSelection,
) -> Result<Vec<u8>, AppError> {
    const START: &str = "<!-- HOI4_MOD_SETUP:LIVE_TESTING_PROVIDER:START -->";
    const END: &str = "<!-- HOI4_MOD_SETUP:LIVE_TESTING_PROVIDER:END -->";
    let start_count = bytes
        .windows(START.len())
        .filter(|w| *w == START.as_bytes())
        .count();
    let end_count = bytes
        .windows(END.len())
        .filter(|w| *w == END.as_bytes())
        .count();
    if start_count == 0 && end_count == 0 {
        return Ok(bytes.to_vec());
    }
    if start_count != 1 || end_count != 1 {
        return Err(AppError::Source(
            "live testing guidance requires exactly one complete marker block".into(),
        ));
    }
    let text = std::str::from_utf8(bytes).map_err(|e| AppError::Source(e.to_string()))?;
    let start = text.find(START).unwrap() + START.len();
    let end = text[start..].find(END).map(|i| start + i).ok_or_else(|| {
        AppError::Source("live testing guidance marker is missing its end".into())
    })?;
    let row = manifest
        .live_testing
        .as_ref()
        .and_then(|c| c["providers"].as_array())
        .and_then(|rows| rows.iter().find(|r| r["id"] == selection.provider));
    let mut content = row
        .and_then(|r| r["guidance"].as_array())
        .map(|values| {
            values
                .iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_default();
    if selection.provider == "clausewitz_mcp" && !selection.linux_ready {
        content.push_str("\nLinux environment is unconfirmed. Do not call Clausewitz MCP until the user confirms readiness.");
    }
    if selection.provider == "none" {
        content = "Live testing is manual; no live MCP provider is registered.".into();
    }
    Ok(format!("{}\n{}\n{}", &text[..start], content, &text[end..]).into_bytes())
}

pub fn validate_catalog(manifest: &RemoteManifest) -> Result<(), AppError> {
    let Some(catalog) = manifest.live_testing.as_ref() else {
        return Ok(());
    };
    let invalid =
        |message: &str| AppError::Source(format!("invalid live testing catalog: {message}"));
    if catalog["default_provider"] != "clausewitz_mcp" {
        return Err(invalid("default provider"));
    }
    let rows = catalog["providers"]
        .as_array()
        .ok_or_else(|| invalid("providers"))?;
    let mut seen = std::collections::HashSet::new();
    for row in rows {
        let id = row["id"].as_str().ok_or_else(|| invalid("provider ID"))?;
        if !seen.insert(id) {
            return Err(invalid("duplicate provider"));
        }
        let expected = match id {
            "clausewitz_mcp" => (
                "external_stdio",
                Some("clausewitz_linux"),
                Some("workflow.live_testing"),
            ),
            "hoi4_live_tools" => (
                "local_install",
                Some("hoi4-live"),
                Some("workflow.live_tools"),
            ),
            "none" => ("manual", None, None),
            _ => return Err(invalid("unknown provider")),
        };
        let route = &row["route"];
        if route["kind"] != expected.0
            || route["server_id"].as_str() != expected.1
            || route["component_id"].as_str() != expected.2
        {
            return Err(invalid("provider route identity"));
        }
        if id == "none"
            && (route["source_repository"] != Value::Null
                || route["source_revision"] != Value::Null
                || row["supported_platforms"] != json!(["windows", "macos"]))
        {
            return Err(invalid(
                "manual provider cannot declare an executable source",
            ));
        }
        if let Some(component) = expected.2 {
            if !manifest.components.iter().any(|c| c.id == component) {
                return Err(invalid("missing provider component"));
            }
        }
        if id == "clausewitz_mcp"
            && (route["source_revision"] != "97c85437081904d5ad07bae1623ec58d1c0a5b9e"
                || row["supported_game_version"] != "1.19.2"
                || route["source_repository"] != "smerss/Clausewitz-MCP"
                || row["supported_platforms"] != json!(["windows", "macos"]))
        {
            return Err(invalid(
                "Clausewitz requires the reviewed revision and exact native Linux game build",
            ));
        }
        if id == "hoi4_live_tools"
            && (row["supported_platforms"] != json!(["windows"])
                || route["source_repository"] != "klimPaskov/HOI4-Live-Tools")
        {
            return Err(invalid("LiveTools supports Windows only"));
        }
        if id == "hoi4_live_tools" {
            let action = manifest
                .components
                .iter()
                .find(|component| component.id == "workflow.live_tools")
                .and_then(|component| {
                    component
                        .validation
                        .iter()
                        .find(|rule| rule.id == "live.bootstrap" && rule.kind == "command")
                })
                .ok_or_else(|| {
                    invalid("LiveTools local installation requires reviewed bootstrap evidence")
                })?;
            let version = action
                .parameters
                .get("release_version")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let segments = version.split('.').collect::<Vec<_>>();
            let expected_pin = format!("v{version}");
            let expected_url = format!("https://github.com/klimPaskov/HOI4-Live-Tools/releases/download/{expected_pin}/hoi4-live-tools-{version}.zip");
            if version.len() > 64
                || segments.len() != 3
                || segments.iter().any(|segment| {
                    segment.is_empty() || !segment.bytes().all(|byte| byte.is_ascii_digit())
                })
                || route["source_revision"].as_str() != Some(expected_pin.as_str())
                || action.parameters.get("release_url").and_then(Value::as_str)
                    != Some(expected_url.as_str())
            {
                return Err(invalid(
                    "LiveTools bootstrap must match the catalog repository and release pin",
                ));
            }
        }
    }
    if seen.len() != 3 {
        return Err(invalid("all three provider choices are required"));
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;

    fn confirmed() -> LiveTestingSelection {
        LiveTestingSelection {
            linux_ready: true,
            ssh_host: "hoi4-linux".into(),
            wsl_distribution: "Ubuntu-24.04".into(),
            ..Default::default()
        }
    }

    #[test]
    fn clausewitz_routes_use_fixed_commands_and_platform_specific_arguments() {
        let (_, windows) = route(&confirmed(), Platform::Windows).unwrap().unwrap();
        assert_eq!(windows["command"], "wsl.exe");
        assert_eq!(
            windows["args"],
            json!([
                "--distribution",
                "Ubuntu-24.04",
                "--exec",
                "sh",
                "-lc",
                REMOTE
            ])
        );
        let (_, mac) = route(&confirmed(), Platform::Macos).unwrap().unwrap();
        assert_eq!(mac["command"], "ssh");
        assert_eq!(
            mac["args"],
            json!(["-o", "BatchMode=yes", "-T", "hoi4-linux", REMOTE])
        );
        assert!(route(&LiveTestingSelection::default(), Platform::Windows)
            .unwrap()
            .is_none());
        assert!(route(&LiveTestingSelection::default(), Platform::Macos)
            .unwrap()
            .is_none());
    }

    #[test]
    fn every_coding_client_preserves_unrelated_routes_during_provider_change() {
        for platform in [Platform::Windows, Platform::Macos] {
            for destination in [
                ".mcp.json",
                ".cursor/mcp.json",
                ".qoder/mcp.json",
                "opencode.json",
                ".codex/config.toml",
            ] {
                let bytes = if destination.ends_with(".toml") {
                    b"[mcp_servers.unrelated]\ncommand = 'safe'\n[mcp_servers.hoi4-live]\ncommand = 'old'\n".to_vec()
                } else {
                    let key = if destination == "opencode.json" {
                        "mcp"
                    } else {
                        "mcpServers"
                    };
                    serde_json::to_vec(&json!({key:{"unrelated":{"command":"safe"},"hoi4-live":{"command":"old"}},"custom_setting":42})).unwrap()
                };
                let configured = adapt_config(&bytes, destination, &confirmed(), platform).unwrap();
                let text = String::from_utf8(configured.clone()).unwrap();
                assert!(text.contains("clausewitz_linux"), "{destination}");
                if destination.ends_with(".toml") {
                    let parsed: toml::Value = text.parse().unwrap();
                    assert_eq!(
                        parsed["mcp_servers"]["clausewitz_linux"]["startup_timeout_sec"]
                            .as_integer(),
                        Some(30)
                    );
                    assert_eq!(
                        parsed["mcp_servers"]["clausewitz_linux"]["tool_timeout_sec"].as_integer(),
                        Some(1800)
                    );
                }
                assert!(!text.contains("hoi4-live"), "{destination}");
                assert!(text.contains("unrelated"));
                let none = LiveTestingSelection {
                    provider: "none".into(),
                    ..Default::default()
                };
                let cleared = adapt_config(&configured, destination, &none, platform).unwrap();
                let text = String::from_utf8(cleared).unwrap();
                assert!(!text.contains("clausewitz_linux"));
                assert!(text.contains("unrelated"));
                let unready =
                    adapt_config(&configured, destination, &Default::default(), platform).unwrap();
                assert!(!String::from_utf8(unready)
                    .unwrap()
                    .contains("clausewitz_linux"));
            }
        }
    }

    #[test]
    fn malformed_settings_credentials_and_arbitrary_commands_are_rejected() {
        for host in [
            "user@host",
            "-oProxyCommand=evil",
            "host;echo",
            "host with spaces",
            "host/../../x",
            "$(command)",
        ] {
            let selection = LiveTestingSelection {
                ssh_host: host.into(),
                ..confirmed()
            };
            assert!(validate(&selection, Platform::Macos).is_err());
        }
        let mut selection = confirmed();
        selection.ssh_host.clear();
        assert!(route(&selection, Platform::Macos).is_err());
        selection.provider = "invented".into();
        assert!(validate(&selection, Platform::Windows).is_err());
        assert!(serde_json::from_value::<LiveTestingSelection>(
            json!({"provider":"none","command":"evil"})
        )
        .is_err());
        assert!(adapt_config(
            b"{\"mcpServers\":[]}",
            ".qoder/mcp.json",
            &confirmed(),
            Platform::Windows
        )
        .is_err());
    }

    #[test]
    fn selection_roundtrip_and_legacy_component_restore_are_explicit() {
        let selection = confirmed();
        assert_eq!(
            serde_json::from_slice::<LiveTestingSelection>(
                &serde_json::to_vec(&selection).unwrap()
            )
            .unwrap(),
            selection
        );
        let component: LockComponent = serde_json::from_value(
            json!({"id":"workflow.live_tools","version":null,"state":"installed"}),
        )
        .unwrap();
        assert_eq!(legacy_selection(&[component]).provider, "hoi4_live_tools");
        assert_eq!(legacy_selection(&[]).provider, "none");
    }
}
#[cfg(test)]
mod selection_tests {
    use super::*;
    fn manifest() -> RemoteManifest {
        let mut manifest: RemoteManifest = serde_json::from_slice(include_bytes!(
            "../../docs/source-manifest/hoi4-mod-setup.manifest.json"
        ))
        .unwrap();
        if !manifest
            .components
            .iter()
            .any(|c| c.id == "workflow.live_testing")
        {
            let mut common = manifest
                .components
                .iter()
                .find(|c| c.id == "workflow.live_tools")
                .unwrap()
                .clone();
            common.id = "workflow.live_testing".into();
            manifest.components.push(common);
        }
        manifest.live_testing = Some(json!({"default_provider":"clausewitz_mcp","providers":[
            {"id":"clausewitz_mcp","supported_game_version":"1.19.2","supported_platforms":["windows","macos"],"guidance":["Use native Linux HOI4 1.19.2."],"route":{"kind":"external_stdio","server_id":"clausewitz_linux","component_id":"workflow.live_testing","source_repository":"smerss/Clausewitz-MCP","source_revision":"97c85437081904d5ad07bae1623ec58d1c0a5b9e"}},
            {"id":"hoi4_live_tools","supported_platforms":["windows"],"route":{"kind":"local_install","server_id":"hoi4-live","component_id":"workflow.live_tools","source_repository":"klimPaskov/HOI4-Live-Tools","source_revision":"v1.1.0"}},
            {"id":"none","supported_platforms":["windows","macos"],"route":{"kind":"manual","server_id":null,"component_id":null,"source_repository":null,"source_revision":null}}
        ]}));
        manifest
    }
    #[test]
    fn maintenance_selection_swaps_closure_and_none_removes_only_live_components() {
        let manifest = manifest();
        let mut requested = vec![
            "core.agents".into(),
            "workflow.live_testing".into(),
            "workflow.live_tools".into(),
            "mcp.hoi4_live.bootstrap".into(),
        ];
        let clausewitz = from_state(
            &json!({"live_testing":{"provider":"clausewitz_mcp"}}),
            &manifest,
            &requested,
            Platform::Windows,
        )
        .unwrap();
        components(&mut requested, &clausewitz, &manifest).unwrap();
        assert_eq!(requested, vec!["core.agents", "workflow.live_testing"]);
        let live_tools = from_state(
            &json!({"live_testing":{"provider":"hoi4_live_tools"}}),
            &manifest,
            &requested,
            Platform::Windows,
        )
        .unwrap();
        components(&mut requested, &live_tools, &manifest).unwrap();
        assert!(requested.iter().any(|id| id == "workflow.live_tools"));
        let none = from_state(
            &json!({"live_testing":{"provider":"none"}}),
            &manifest,
            &requested,
            Platform::Windows,
        )
        .unwrap();
        components(&mut requested, &none, &manifest).unwrap();
        assert_eq!(requested, vec!["core.agents"]);
        assert!(from_state(
            &json!({"live_testing":{"provider":"unknown"}}),
            &manifest,
            &["workflow.live_testing".into()],
            Platform::Windows
        )
        .is_err());
    }
    #[test]
    fn catalog_rejects_repository_route_pin_and_platform_drift() {
        for (provider, pointer, value) in [
            (0, "/route/source_repository", json!("other/Clausewitz-MCP")),
            (0, "/route/server_id", json!("other")),
            (0, "/route/source_revision", json!("main")),
            (0, "/supported_game_version", json!("1.19.*")),
            (0, "/supported_platforms", json!(["windows"])),
            (
                1,
                "/route/source_repository",
                json!("other/HOI4-Live-Tools"),
            ),
            (1, "/route/source_revision", json!("latest")),
            (1, "/supported_platforms", json!(["windows", "macos"])),
            (2, "/route/source_repository", json!("other/manual")),
        ] {
            let mut manifest = manifest();
            *manifest.live_testing.as_mut().unwrap()["providers"][provider]
                .pointer_mut(pointer)
                .unwrap_or_else(|| panic!("missing fixture {pointer}")) = value;
            assert!(validate_catalog(&manifest).is_err(), "{pointer}");
        }
    }

    #[test]
    fn guidance_rejects_incomplete_duplicate_and_reversed_blocks() {
        let manifest = manifest();
        let start = "<!-- HOI4_MOD_SETUP:LIVE_TESTING_PROVIDER:START -->";
        let end = "<!-- HOI4_MOD_SETUP:LIVE_TESTING_PROVIDER:END -->";
        for text in [
            start.to_string(),
            end.to_string(),
            format!("{start}{start}{end}"),
            format!("{start}{end}{end}"),
            format!("{end}{start}"),
        ] {
            assert!(guidance(text.as_bytes(), &manifest, &Default::default()).is_err());
        }
    }

    #[test]
    fn guidance_and_catalog_are_bound_to_verified_provider_evidence() {
        let mut manifest = manifest();
        validate_catalog(&manifest).unwrap();
        let source = b"Before\n<!-- HOI4_MOD_SETUP:LIVE_TESTING_PROVIDER:START -->\nold\n<!-- HOI4_MOD_SETUP:LIVE_TESTING_PROVIDER:END -->\nAfter";
        let guided =
            String::from_utf8(guidance(source, &manifest, &Default::default()).unwrap()).unwrap();
        assert!(guided.contains("native Linux HOI4 1.19.2"));
        assert!(guided.contains("unconfirmed"));
        assert!(guided.starts_with("Before"));
        assert!(guided.ends_with("After"));
        manifest.live_testing.as_mut().unwrap()["providers"][0]["route"]["source_revision"] =
            json!("main");
        assert!(validate_catalog(&manifest).is_err());
    }
}
