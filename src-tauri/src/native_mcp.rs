//! Native project MCP configuration formats.
//!
//! Each coding client stores its project MCP servers in one of a small,
//! closed set of file formats. This module reads and rewrites only the shared
//! `hoi4_agent_tools` route; every other server, key, and value in the file is
//! preserved. The format for a project path comes from the coding-client
//! registry in `coding_environment`; a manifest may name a format but cannot
//! introduce a new one.

use crate::AppError;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The provider-neutral HOI4 Agent Tools server registered in every client.
pub const SERVER_ID: &str = "hoi4_agent_tools";

/// Native client configuration files are small; larger files are rejected
/// before parsing.
pub const MAX_CONFIG_BYTES: usize = 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum McpFormat {
    /// `{"mcpServers": {"<id>": {"command": "...", "args": [...]}}}`.
    JsonMcpServers,
    /// VS Code: `{"servers": {"<id>": {"type": "stdio", "command": "...", "args": [...]}}}`.
    VscodeServers,
    /// OpenCode and Kilo: `{"mcp": {"<id>": {"type": "local", "command": ["...", ...]}}}`.
    LocalCommandArray,
    /// Codex and Grok Build: `[mcp_servers.<id>]` with `command` and `args`.
    TomlMcpServers,
}

impl McpFormat {
    pub fn servers_key(self) -> &'static str {
        match self {
            Self::JsonMcpServers => "mcpServers",
            Self::VscodeServers => "servers",
            Self::LocalCommandArray => "mcp",
            Self::TomlMcpServers => "mcp_servers",
        }
    }

    pub fn is_toml(self) -> bool {
        self == Self::TomlMcpServers
    }

    /// The transport `type` a route of this format must declare when present.
    fn route_type(self) -> Option<&'static str> {
        match self {
            Self::VscodeServers => Some("stdio"),
            Self::LocalCommandArray => Some("local"),
            Self::JsonMcpServers | Self::TomlMcpServers => None,
        }
    }
}

fn malformed() -> AppError {
    AppError::Process("the managed MCP client registration is malformed".into())
}

/// Parse JSON that may contain JSONC comments and trailing commas, as VS
/// Code, OpenCode, and Kilo configuration files allow. Strict JSON is
/// returned unchanged; string contents are never altered.
pub fn parse_json_lenient(bytes: &[u8]) -> Result<Value, serde_json::Error> {
    if let Ok(value) = serde_json::from_slice(bytes) {
        return Ok(value);
    }
    let text = String::from_utf8_lossy(bytes);
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    let mut in_string = false;
    while let Some(character) = chars.next() {
        if in_string {
            out.push(character);
            match character {
                '\\' => {
                    if let Some(escaped) = chars.next() {
                        out.push(escaped);
                    }
                }
                '"' => in_string = false,
                _ => {}
            }
            continue;
        }
        match (character, chars.peek()) {
            ('"', _) => {
                in_string = true;
                out.push(character);
            }
            ('/', Some('/')) => {
                for next in chars.by_ref() {
                    if next == '\n' {
                        out.push('\n');
                        break;
                    }
                }
            }
            ('/', Some('*')) => {
                chars.next();
                let mut previous = '\0';
                for next in chars.by_ref() {
                    if previous == '*' && next == '/' {
                        break;
                    }
                    previous = next;
                }
                out.push(' ');
            }
            _ => out.push(character),
        }
    }
    // Drop a comma that directly precedes a closing bracket or brace.
    let mut cleaned = String::with_capacity(out.len());
    let mut in_string = false;
    let mut escaped = false;
    let characters = out.chars().collect::<Vec<_>>();
    for (index, character) in characters.iter().enumerate() {
        if in_string {
            cleaned.push(*character);
            if escaped {
                escaped = false;
            } else if *character == '\\' {
                escaped = true;
            } else if *character == '"' {
                in_string = false;
            }
            continue;
        }
        if *character == '"' {
            in_string = true;
        } else if *character == ',' {
            let next = characters[index + 1..]
                .iter()
                .find(|candidate| !candidate.is_whitespace());
            if matches!(next, Some('}' | ']')) {
                continue;
            }
        }
        cleaned.push(*character);
    }
    serde_json::from_str(&cleaned)
}

/// Parse a native configuration into a JSON view. TOML is converted so one
/// validator covers every format.
fn json_view(format: McpFormat, bytes: &[u8]) -> Result<Value, AppError> {
    if bytes.len() > MAX_CONFIG_BYTES {
        return Err(malformed());
    }
    if format.is_toml() {
        let config: toml::Value = std::str::from_utf8(bytes)
            .map_err(|_| malformed())?
            .parse()
            .map_err(|_| malformed())?;
        serde_json::to_value(config).map_err(|_| malformed())
    } else {
        parse_json_lenient(bytes).map_err(|_| malformed())
    }
}

/// The enabled HOI4 Agent Tools launch registered in a native configuration,
/// or `None` when the route is absent or explicitly disabled.
pub fn registration(
    format: McpFormat,
    bytes: &[u8],
) -> Result<Option<(String, Vec<String>)>, AppError> {
    let value = json_view(format, bytes)?;
    let Some(route) = value
        .get(format.servers_key())
        .and_then(|servers| servers.get(SERVER_ID))
    else {
        return Ok(None);
    };
    if !route.is_object() {
        return Err(malformed());
    }
    for name in ["enabled", "disabled"] {
        if route.get(name).is_some_and(|value| !value.is_boolean()) {
            return Err(malformed());
        }
    }
    if route["enabled"] == false || route["disabled"] == true {
        return Ok(None);
    }
    if let (Some(expected), Some(declared)) = (format.route_type(), route.get("type")) {
        if declared.as_str() != Some(expected) {
            return Err(malformed());
        }
    }
    let strings = |items: &[Value]| {
        items
            .iter()
            .map(|item| item.as_str().map(str::to_owned).ok_or_else(malformed))
            .collect::<Result<Vec<_>, _>>()
    };
    let (command, args) = if format == McpFormat::LocalCommandArray {
        let line = strings(route["command"].as_array().ok_or_else(malformed)?)?;
        let (command, args) = line.split_first().ok_or_else(malformed)?;
        (command.clone(), args.to_vec())
    } else {
        let command = route["command"].as_str().ok_or_else(malformed)?.to_owned();
        let args = match route.get("args") {
            Some(args) => strings(args.as_array().ok_or_else(malformed)?)?,
            None => Vec::new(),
        };
        (command, args)
    };
    if command.is_empty()
        || command.len() > 4096
        || command.chars().any(char::is_control)
        || args.len() > 256
        || args
            .iter()
            .any(|arg| arg.len() > 4096 || arg.chars().any(char::is_control))
    {
        return Err(malformed());
    }
    Ok(Some((command, args)))
}

/// Whether a native configuration declares a HOI4 Agent Tools entry at all,
/// enabled or not.
pub fn declares_route(format: McpFormat, bytes: &[u8]) -> Result<bool, AppError> {
    Ok(json_view(format, bytes)?
        .get(format.servers_key())
        .and_then(|servers| servers.get(SERVER_ID))
        .is_some())
}

/// Whether a native configuration holds nothing but an empty server table,
/// so deleting the file loses no setting.
pub fn is_empty(format: McpFormat, bytes: &[u8]) -> Result<bool, AppError> {
    Ok(match json_view(format, bytes)? {
        Value::Object(map) => map.iter().all(|(key, value)| {
            key == format.servers_key()
                && value.as_object().is_some_and(|servers| servers.is_empty())
        }),
        _ => false,
    })
}

/// Point the HOI4 Agent Tools route at the verified launch for this machine,
/// or remove the route when no verified launch exists. Other servers and
/// route fields are preserved; an absent route is never invented.
pub fn retarget(
    format: McpFormat,
    bytes: &[u8],
    launch: Option<&(String, Vec<String>)>,
) -> Result<Vec<u8>, AppError> {
    if bytes.len() > MAX_CONFIG_BYTES {
        return Err(AppError::Source(
            "client MCP configuration is too large".into(),
        ));
    }
    if format.is_toml() {
        let mut config: toml::Value = std::str::from_utf8(bytes)
            .map_err(|_| AppError::Source("client MCP configuration is not UTF-8".into()))?
            .parse()
            .map_err(|error: toml::de::Error| AppError::Source(error.to_string()))?;
        if let Some(servers) = config
            .get_mut(format.servers_key())
            .and_then(toml::Value::as_table_mut)
        {
            if let Some((command, args)) = launch {
                if let Some(route) = servers
                    .get_mut(SERVER_ID)
                    .and_then(toml::Value::as_table_mut)
                {
                    route.insert("command".into(), toml::Value::String(command.clone()));
                    route.insert(
                        "args".into(),
                        toml::Value::Array(args.iter().cloned().map(toml::Value::String).collect()),
                    );
                }
            } else {
                servers.remove(SERVER_ID);
            }
        }
        return toml::to_string_pretty(&config)
            .map(String::into_bytes)
            .map_err(|error| AppError::Source(error.to_string()));
    }
    let mut config: Value = parse_json_lenient(bytes)?;
    if let Some(servers) = config
        .get_mut(format.servers_key())
        .and_then(Value::as_object_mut)
    {
        if let Some((command, args)) = launch {
            if let Some(route) = servers.get_mut(SERVER_ID).and_then(Value::as_object_mut) {
                if format == McpFormat::LocalCommandArray {
                    let mut line = vec![command.clone()];
                    line.extend(args.iter().cloned());
                    route.insert("command".into(), serde_json::json!(line));
                } else {
                    route.insert("command".into(), serde_json::json!(command));
                    route.insert("args".into(), serde_json::json!(args));
                }
            }
        } else {
            servers.remove(SERVER_ID);
        }
    }
    serde_json::to_vec_pretty(&config).map_err(AppError::from)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const ALL: [McpFormat; 4] = [
        McpFormat::JsonMcpServers,
        McpFormat::VscodeServers,
        McpFormat::LocalCommandArray,
        McpFormat::TomlMcpServers,
    ];

    fn sample(format: McpFormat, command: &str, args: &[&str]) -> Vec<u8> {
        match format {
            McpFormat::TomlMcpServers => format!(
                "model = 'kept'\n[mcp_servers.custom]\ncommand = 'other'\n[mcp_servers.hoi4_agent_tools]\ncommand = '{command}'\nargs = [{}]\nstartup_timeout_sec = 30\n",
                args.iter().map(|arg| format!("'{arg}'")).collect::<Vec<_>>().join(", ")
            )
            .into_bytes(),
            McpFormat::LocalCommandArray => {
                let mut line = vec![command.to_string()];
                line.extend(args.iter().map(|arg| arg.to_string()));
                serde_json::to_vec(&json!({
                    "$schema": "kept",
                    "mcp": {
                        "custom": {"type": "local", "command": ["other"]},
                        "hoi4_agent_tools": {"type": "local", "command": line, "environment": {"KEEP": "1"}}
                    }
                }))
                .unwrap()
            }
            McpFormat::VscodeServers => serde_json::to_vec(&json!({
                "inputs": [],
                "servers": {
                    "custom": {"type": "stdio", "command": "other"},
                    "hoi4_agent_tools": {"type": "stdio", "command": command, "args": args, "env": {"KEEP": "1"}}
                }
            }))
            .unwrap(),
            McpFormat::JsonMcpServers => serde_json::to_vec(&json!({
                "context": {"fileName": ["AGENTS.md"]},
                "mcpServers": {
                    "custom": {"command": "other"},
                    "hoi4_agent_tools": {"command": command, "args": args, "env": {"KEEP": "1"}}
                }
            }))
            .unwrap(),
        }
    }

    #[test]
    fn every_format_reads_and_retargets_only_the_shared_route() {
        let launch = (
            "/usr/bin/node".to_string(),
            vec!["/opt/entry.js".to_string()],
        );
        for format in ALL {
            let bytes = sample(format, "hoi4-agent-tools.cmd", &[]);
            assert_eq!(
                registration(format, &bytes).unwrap(),
                Some(("hoi4-agent-tools.cmd".into(), vec![])),
                "{format:?}"
            );
            let retargeted = retarget(format, &bytes, Some(&launch)).unwrap();
            assert_eq!(
                registration(format, &retargeted).unwrap(),
                Some(launch.clone()),
                "{format:?}"
            );
            let text = String::from_utf8(retargeted.clone()).unwrap();
            assert!(
                text.contains("custom") && text.contains("other"),
                "{format:?}"
            );
            assert!(text.contains("kept") || text.contains("KEEP"), "{format:?}");
            let removed = retarget(format, &retargeted, None).unwrap();
            assert_eq!(registration(format, &removed).unwrap(), None, "{format:?}");
            assert!(String::from_utf8(removed).unwrap().contains("other"));
        }
    }

    #[test]
    fn every_format_removes_only_the_shared_route_and_detects_an_empty_remainder() {
        let launch = ("node".to_string(), vec!["/opt/entry.js".to_string()]);
        for format in ALL {
            let without_route = retarget(format, &sample(format, "x", &[]), None).unwrap();
            let route_only = match format {
                McpFormat::TomlMcpServers => {
                    b"[mcp_servers.hoi4_agent_tools]\ncommand = 'x'\nargs = []\n".to_vec()
                }
                McpFormat::LocalCommandArray => serde_json::to_vec(&json!({
                    "mcp": {"hoi4_agent_tools": {"type": "local", "command": ["x"], "enabled": true}}
                }))
                .unwrap(),
                McpFormat::VscodeServers => serde_json::to_vec(&json!({
                    "servers": {"hoi4_agent_tools": {"type": "stdio", "command": "x", "args": []}}
                }))
                .unwrap(),
                McpFormat::JsonMcpServers => serde_json::to_vec(&json!({
                    "mcpServers": {"hoi4_agent_tools": {"command": "x", "args": []}}
                }))
                .unwrap(),
            };
            assert!(declares_route(format, &route_only).unwrap(), "{format:?}");
            assert!(!is_empty(format, &route_only).unwrap(), "{format:?}");
            let emptied = retarget(format, &route_only, None).unwrap();
            assert!(!declares_route(format, &emptied).unwrap(), "{format:?}");
            assert!(is_empty(format, &emptied).unwrap(), "{format:?}");
            // Every other server and key survives the removal.
            assert!(
                !declares_route(format, &without_route).unwrap(),
                "{format:?}"
            );
            assert!(!is_empty(format, &without_route).unwrap(), "{format:?}");
            let before = json_view(format, &sample(format, "x", &[])).unwrap();
            let mut expected = before.clone();
            expected[format.servers_key()]
                .as_object_mut()
                .unwrap()
                .remove(SERVER_ID);
            assert_eq!(
                json_view(format, &without_route).unwrap(),
                expected,
                "{format:?}"
            );
            // A retargeted file is reversed to the same remainder.
            let retargeted = retarget(format, &sample(format, "x", &[]), Some(&launch)).unwrap();
            assert_eq!(
                json_view(format, &retarget(format, &retargeted, None).unwrap()).unwrap(),
                expected,
                "{format:?}"
            );
            // A disabled entry is still a declared route.
            let mut disabled = before.clone();
            disabled[format.servers_key()][SERVER_ID]["disabled"] = json!(true);
            let disabled = if format.is_toml() {
                toml::to_string(&disabled).unwrap().into_bytes()
            } else {
                serde_json::to_vec(&disabled).unwrap()
            };
            assert!(declares_route(format, &disabled).unwrap(), "{format:?}");
        }
        assert!(is_empty(McpFormat::TomlMcpServers, b"").unwrap());
        assert!(is_empty(McpFormat::JsonMcpServers, b"{}").unwrap());
        assert!(!is_empty(McpFormat::JsonMcpServers, b"[]").unwrap());
        assert!(!is_empty(McpFormat::JsonMcpServers, br#"{"mcpServers": []}"#).unwrap());
        assert!(declares_route(McpFormat::JsonMcpServers, b"{not json").is_err());
        assert!(is_empty(McpFormat::TomlMcpServers, b"[unclosed").is_err());
    }

    #[test]
    fn absent_routes_are_never_invented() {
        let launch = ("node".to_string(), vec![]);
        for format in ALL {
            let empty: &[u8] = if format.is_toml() { b"" } else { b"{}" };
            let unchanged = retarget(format, empty, Some(&launch)).unwrap();
            assert_eq!(registration(format, &unchanged).unwrap(), None);
        }
    }

    #[test]
    fn malformed_disabled_and_wrong_transport_routes_are_handled_strictly() {
        let invalid = [
            (
                McpFormat::JsonMcpServers,
                json!({"mcpServers": {"hoi4_agent_tools": {"command": "x", "args": [1]}}}),
            ),
            (
                McpFormat::JsonMcpServers,
                json!({"mcpServers": {"hoi4_agent_tools": {"command": "x", "enabled": "true"}}}),
            ),
            (
                McpFormat::JsonMcpServers,
                json!({"mcpServers": {"hoi4_agent_tools": "x"}}),
            ),
            (
                McpFormat::JsonMcpServers,
                json!({"mcpServers": {"hoi4_agent_tools": {"command": "a\nb"}}}),
            ),
            (
                McpFormat::VscodeServers,
                json!({"servers": {"hoi4_agent_tools": {"type": "http", "command": "x"}}}),
            ),
            (
                McpFormat::LocalCommandArray,
                json!({"mcp": {"hoi4_agent_tools": {"type": "local", "command": []}}}),
            ),
            (
                McpFormat::LocalCommandArray,
                json!({"mcp": {"hoi4_agent_tools": {"type": "remote", "command": ["x"]}}}),
            ),
            (
                McpFormat::LocalCommandArray,
                json!({"mcp": {"hoi4_agent_tools": {"type": "local", "command": "x"}}}),
            ),
        ];
        for (format, value) in invalid {
            assert!(
                registration(format, &serde_json::to_vec(&value).unwrap()).is_err(),
                "{value}"
            );
        }
        let disabled =
            json!({"mcpServers": {"hoi4_agent_tools": {"command": "x", "disabled": true}}});
        assert_eq!(
            registration(
                McpFormat::JsonMcpServers,
                &serde_json::to_vec(&disabled).unwrap()
            )
            .unwrap(),
            None
        );
        let off = json!({"mcp": {"hoi4_agent_tools": {"type": "local", "command": ["x"], "enabled": false}}});
        assert_eq!(
            registration(
                McpFormat::LocalCommandArray,
                &serde_json::to_vec(&off).unwrap()
            )
            .unwrap(),
            None
        );
        assert!(registration(McpFormat::JsonMcpServers, b"{not json").is_err());
        assert!(registration(McpFormat::TomlMcpServers, b"[unclosed").is_err());
        assert!(
            registration(McpFormat::JsonMcpServers, &vec![b' '; MAX_CONFIG_BYTES + 1]).is_err()
        );
        let long_args =
            json!({"mcpServers": {"hoi4_agent_tools": {"command": "x", "args": vec!["a"; 257]}}});
        assert!(registration(
            McpFormat::JsonMcpServers,
            &serde_json::to_vec(&long_args).unwrap()
        )
        .is_err());
    }

    #[test]
    fn jsonc_comments_and_trailing_commas_are_accepted_without_touching_strings() {
        let jsonc = br#"{
  // VS Code allows comments in mcp.json
  "servers": {
    /* the user's own server */
    "user": {"type": "stdio", "command": "tool // not a comment", "args": ["/* kept */",],},
    "hoi4_agent_tools": {"type": "stdio", "command": "hoi4-agent-tools.cmd", "args": []},
  },
}"#;
        assert_eq!(
            registration(McpFormat::VscodeServers, jsonc).unwrap(),
            Some(("hoi4-agent-tools.cmd".into(), vec![]))
        );
        let launch = ("C:\\npm\\hoi4-agent-tools.cmd".to_string(), vec![]);
        let retargeted: Value = serde_json::from_slice(
            &retarget(McpFormat::VscodeServers, jsonc, Some(&launch)).unwrap(),
        )
        .unwrap();
        assert_eq!(
            retargeted["servers"]["user"]["command"],
            "tool // not a comment"
        );
        assert_eq!(retargeted["servers"]["user"]["args"][0], "/* kept */");
        assert_eq!(
            retargeted["servers"]["hoi4_agent_tools"]["command"],
            "C:\\npm\\hoi4-agent-tools.cmd"
        );
        assert!(parse_json_lenient(br#"{"a": "unterminated}"#).is_err());
        assert!(parse_json_lenient(br#"{"a": 1 /* open"#).is_err());
    }

    #[test]
    fn format_names_are_a_closed_snake_case_vocabulary() {
        for (format, name) in [
            (McpFormat::JsonMcpServers, "\"json_mcp_servers\""),
            (McpFormat::VscodeServers, "\"vscode_servers\""),
            (McpFormat::LocalCommandArray, "\"local_command_array\""),
            (McpFormat::TomlMcpServers, "\"toml_mcp_servers\""),
        ] {
            assert_eq!(serde_json::to_string(&format).unwrap(), name);
        }
        assert!(serde_json::from_str::<McpFormat>("\"yaml_servers\"").is_err());
    }
}
