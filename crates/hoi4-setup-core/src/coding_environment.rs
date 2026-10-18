//! Coding-client selection and native package mapping.
//!
//! Coding environments are deliberately independent from the setup assistant
//! AI provider and from workflow profiles.  The source manifest declares the
//! environments it supports and the component closure for each of them; this
//! module only validates the user's small, non-secret selection and resolves
//! those declarations.  The built-in list covers older manifests that predate
//! environment declarations.

use crate::models::{
    CodingEnvironmentDefinition, CodingEnvironmentSelection, ComponentDefinition, McpSupport,
    Platform, RemoteManifest, UnavailableCodingEnvironment,
};
use crate::native_mcp::McpFormat;
use crate::AppError;
use serde_json::Value;
use std::collections::HashSet;

pub const CODEX: &str = "codex";
pub const CLAUDE_CODE: &str = "claude_code";
pub const CURSOR: &str = "cursor";
pub const QODER: &str = "qoder";
pub const OPENCODE: &str = "opencode";

/// Environments declared by manifest formats before 6. Their packages and
/// adapters predate per-client MCP declarations.
pub const SUPPORTED: [&str; 5] = [CLAUDE_CODE, CODEX, CURSOR, QODER, OPENCODE];

/// A readiness check for one installed native file or folder.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layout {
    /// A non-empty UTF-8 text file.
    Text(&'static str),
    /// A parseable JSON file.
    Json(&'static str),
    /// A folder holding at least one agent file with the extension.
    Agents(&'static str, &'static str),
    /// A folder holding at least one `<name>/SKILL.md`.
    Skills(&'static str),
}

/// A project MCP file the installer manages for one client.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NativeMcp {
    pub path: &'static str,
    pub format: McpFormat,
    /// The component that installs `path`.
    pub component_id: &'static str,
    /// The file also carries non-MCP client settings and is installed on every
    /// platform, so the shared route is removed where no verified launch
    /// exists instead of being left as a placeholder.
    pub shared_file: bool,
}

/// The adapter this app version has for one coding client. A manifest can
/// declare only clients whose project MCP route matches an adapter here, or
/// clients whose MCP setup is manual or absent.
#[derive(Debug, Clone, Copy)]
pub struct NativeClient {
    pub id: &'static str,
    pub display_name: &'static str,
    /// The project surfaces the package installs, in a few words.
    pub detail: &'static str,
    pub mcp_support: McpSupport,
    pub mcp: Option<NativeMcp>,
    /// A route that an older source format installed but the client never
    /// read. Readiness keeps the old structural check while that component
    /// still owns its locked file; the next update removes the shared route
    /// from it, retires the component, and moves the project to `mcp`.
    pub legacy_mcp: Option<NativeMcp>,
    /// Official documentation for a manual MCP setup step.
    pub documentation_url: Option<&'static str>,
    /// Whether image-generation and live-testing routes are adapted into the
    /// client's MCP file. Those adapters run relative scripts, so only a
    /// client whose project servers accept a working directory qualifies,
    /// and a provider applies only when its table lists the client.
    pub image_and_live_routes: bool,
    /// Whether readiness requires the shared MCP route in `mcp`. Codex checks
    /// its route through the dedicated Codex readiness detection.
    pub readiness_checks_mcp: bool,
    pub layout: &'static [Layout],
    /// Paths that show a project already uses the client: an exact file, or a
    /// folder prefix ending in `/`.
    pub markers: &'static [&'static str],
    /// Folders the read-only scanner enumerates (one level each).
    pub scan_directories: &'static [&'static str],
    /// Small native configuration files the scanner retains and parses.
    pub config_files: &'static [(&'static str, McpFormat)],
    pub open_command: Option<&'static str>,
}

const fn project_mcp(
    path: &'static str,
    format: McpFormat,
    component_id: &'static str,
) -> NativeMcp {
    NativeMcp {
        path,
        format,
        component_id,
        shared_file: false,
    }
}

/// Every client this app version can install, in the scanner's detection
/// order. Manifests declare their own display order.
pub const NATIVE_CLIENTS: &[NativeClient] = &[
    NativeClient {
        id: CODEX,
        display_name: "Codex",
        detail: "Native .codex configuration and agents",
        mcp_support: McpSupport::Project,
        mcp: Some(NativeMcp {
            path: ".codex/config.toml",
            format: McpFormat::TomlMcpServers,
            component_id: "codex.config",
            // Codex keeps its historical behavior: the route is retargeted
            // on Windows and Linux and otherwise left as the source wrote it.
            shared_file: false,
        }),
        legacy_mcp: None,
        documentation_url: None,
        image_and_live_routes: true,
        readiness_checks_mcp: false,
        layout: &[],
        // Agent TOMLs are installed for every selection and do not prove a
        // Codex package.
        markers: &[".codex/config.toml"],
        scan_directories: &[],
        config_files: &[],
        open_command: Some("codex"),
    },
    NativeClient {
        id: CLAUDE_CODE,
        display_name: "Claude Code",
        detail: "CLAUDE.md, .claude settings, agents, map, and MCP",
        mcp_support: McpSupport::Project,
        mcp: Some(project_mcp(
            ".mcp.json",
            McpFormat::JsonMcpServers,
            "runtime.claude.mcp",
        )),
        legacy_mcp: None,
        documentation_url: None,
        image_and_live_routes: true,
        readiness_checks_mcp: true,
        layout: &[
            Layout::Text("CLAUDE.md"),
            Layout::Json(".claude/settings.json"),
            Layout::Text(".claude/agent-map.md"),
            Layout::Agents(".claude/agents", ".md"),
        ],
        markers: &[
            "CLAUDE.md",
            ".claude/settings.json",
            ".claude/agent-map.md",
            ".claude/agents/",
        ],
        scan_directories: &[],
        config_files: &[
            (".claude/settings.json", McpFormat::JsonMcpServers),
            (".mcp.json", McpFormat::JsonMcpServers),
        ],
        open_command: Some("claude"),
    },
    NativeClient {
        id: CURSOR,
        display_name: "Cursor",
        detail: ".cursor agents, project settings, map, and MCP",
        mcp_support: McpSupport::Project,
        mcp: Some(project_mcp(
            ".cursor/mcp.json",
            McpFormat::JsonMcpServers,
            "runtime.cursor.mcp",
        )),
        legacy_mcp: None,
        documentation_url: None,
        image_and_live_routes: true,
        readiness_checks_mcp: true,
        layout: &[
            Layout::Text(".cursor/agent-map.md"),
            Layout::Json(".cursor/settings.json"),
            Layout::Agents(".cursor/agents", ".md"),
        ],
        markers: &[
            ".cursor/agent-map.md",
            ".cursor/mcp.json",
            ".cursor/settings.json",
            ".cursor/agents/",
        ],
        scan_directories: &[],
        config_files: &[
            (".cursor/settings.json", McpFormat::JsonMcpServers),
            (".cursor/mcp.json", McpFormat::JsonMcpServers),
        ],
        open_command: None,
    },
    NativeClient {
        id: QODER,
        display_name: "Qoder",
        detail: ".qoder agents, project settings, map, and MCP",
        mcp_support: McpSupport::Project,
        mcp: Some(project_mcp(
            ".qoder/mcp.json",
            McpFormat::JsonMcpServers,
            "runtime.qoder.mcp",
        )),
        legacy_mcp: None,
        documentation_url: None,
        image_and_live_routes: true,
        readiness_checks_mcp: true,
        layout: &[
            Layout::Json(".qoder/settings.json"),
            Layout::Text(".qoder/agent-map.md"),
            Layout::Agents(".qoder/agents", ".md"),
        ],
        markers: &[
            ".qoder/agent-map.md",
            ".qoder/settings.json",
            ".qoder/mcp.json",
            ".qoder/agents/",
        ],
        scan_directories: &[],
        config_files: &[
            (".qoder/settings.json", McpFormat::JsonMcpServers),
            (".qoder/mcp.json", McpFormat::JsonMcpServers),
        ],
        open_command: None,
    },
    NativeClient {
        id: OPENCODE,
        display_name: "OpenCode",
        detail: "opencode.json, .opencode agents, map, and MCP",
        mcp_support: McpSupport::Project,
        // OpenCode reads MCP servers only from `opencode.json(c)`.
        mcp: Some(NativeMcp {
            path: "opencode.json",
            format: McpFormat::LocalCommandArray,
            component_id: "runtime.opencode.config",
            shared_file: true,
        }),
        legacy_mcp: Some(project_mcp(
            ".opencode/mcp.json",
            McpFormat::JsonMcpServers,
            "runtime.opencode.mcp",
        )),
        documentation_url: Some("https://opencode.ai/docs/mcp-servers/"),
        image_and_live_routes: true,
        readiness_checks_mcp: true,
        layout: &[
            Layout::Json("opencode.json"),
            Layout::Json(".opencode/settings.json"),
            Layout::Text(".opencode/agent-map.md"),
            Layout::Agents(".opencode/agent", ".md"),
        ],
        markers: &[
            "opencode.json",
            ".opencode/agent-map.md",
            ".opencode/settings.json",
            ".opencode/mcp.json",
            ".opencode/agent/",
        ],
        scan_directories: &[],
        config_files: &[
            (".opencode/settings.json", McpFormat::JsonMcpServers),
            (".opencode/mcp.json", McpFormat::JsonMcpServers),
            ("opencode.json", McpFormat::LocalCommandArray),
        ],
        open_command: None,
    },
    NativeClient {
        id: "gemini_cli",
        display_name: "Gemini CLI",
        detail: "GEMINI.md, .gemini agents, shared .agents skills, and MCP",
        mcp_support: McpSupport::Project,
        mcp: Some(project_mcp(
            ".gemini/settings.json",
            McpFormat::JsonMcpServers,
            "runtime.gemini.mcp",
        )),
        legacy_mcp: None,
        documentation_url: Some("https://geminicli.com/docs/tools/mcp-server/"),
        image_and_live_routes: true,
        readiness_checks_mcp: true,
        layout: &[
            Layout::Text("GEMINI.md"),
            Layout::Text("AGENTS.md"),
            Layout::Agents(".gemini/agents", ".md"),
            Layout::Skills(".agents/skills"),
        ],
        markers: &["GEMINI.md", ".gemini/"],
        scan_directories: &[".gemini", ".gemini/agents"],
        config_files: &[(".gemini/settings.json", McpFormat::JsonMcpServers)],
        open_command: Some("gemini"),
    },
    NativeClient {
        id: "antigravity",
        display_name: "Antigravity",
        detail: "AGENTS.md, .agents agents and skills, and MCP",
        mcp_support: McpSupport::Project,
        mcp: Some(project_mcp(
            ".agents/mcp_config.json",
            McpFormat::JsonMcpServers,
            "runtime.antigravity.mcp",
        )),
        legacy_mcp: None,
        documentation_url: Some("https://antigravity.google/docs/mcp"),
        image_and_live_routes: true,
        readiness_checks_mcp: true,
        layout: &[
            Layout::Text("AGENTS.md"),
            Layout::Agents(".agents/agents", ".md"),
            Layout::Skills(".agents/skills"),
        ],
        markers: &[
            ".agents/mcp_config.json",
            ".agents/agents/",
            ".agents/rules/",
        ],
        scan_directories: &[".agents/agents", ".agents/rules"],
        config_files: &[(".agents/mcp_config.json", McpFormat::JsonMcpServers)],
        open_command: Some("agy"),
    },
    NativeClient {
        id: "grok_build",
        display_name: "Grok Build",
        detail: "AGENTS.md, .grok skills, and MCP",
        mcp_support: McpSupport::Project,
        mcp: Some(project_mcp(
            ".grok/config.toml",
            McpFormat::TomlMcpServers,
            "runtime.grok.mcp",
        )),
        legacy_mcp: None,
        documentation_url: Some("https://docs.x.ai/build/features/mcp-servers"),
        image_and_live_routes: true,
        readiness_checks_mcp: true,
        layout: &[Layout::Text("AGENTS.md"), Layout::Skills(".grok/skills")],
        markers: &[".grok/"],
        scan_directories: &[".grok"],
        config_files: &[(".grok/config.toml", McpFormat::TomlMcpServers)],
        open_command: Some("grok"),
    },
    NativeClient {
        id: "github_copilot",
        display_name: "GitHub Copilot (VS Code)",
        detail: "Copilot instructions, Claude-format agents and skills, and VS Code MCP",
        mcp_support: McpSupport::Project,
        mcp: Some(project_mcp(
            ".vscode/mcp.json",
            McpFormat::VscodeServers,
            "runtime.copilot.mcp",
        )),
        legacy_mcp: None,
        documentation_url: Some(
            "https://code.visualstudio.com/docs/copilot/customization/mcp-servers",
        ),
        image_and_live_routes: true,
        readiness_checks_mcp: true,
        layout: &[
            Layout::Text("AGENTS.md"),
            Layout::Text(".github/instructions/hoi4-mod-setup.instructions.md"),
            Layout::Agents(".claude/agents", ".md"),
            Layout::Skills(".claude/skills"),
        ],
        markers: &[
            ".github/copilot-instructions.md",
            ".github/instructions/",
            ".github/agents/",
            ".vscode/mcp.json",
        ],
        scan_directories: &[
            ".github",
            ".github/instructions",
            ".github/agents",
            ".vscode",
        ],
        config_files: &[(".vscode/mcp.json", McpFormat::VscodeServers)],
        open_command: None,
    },
    NativeClient {
        id: "kilo_code",
        display_name: "Kilo Code",
        detail: "AGENTS.md, .kilo agents, shared .agents skills, and MCP",
        mcp_support: McpSupport::Project,
        mcp: Some(project_mcp(
            ".kilo/kilo.json",
            McpFormat::LocalCommandArray,
            "runtime.kilo.mcp",
        )),
        legacy_mcp: None,
        documentation_url: Some("https://kilo.ai/docs/automate/mcp/using-in-kilo-code"),
        image_and_live_routes: false,
        readiness_checks_mcp: true,
        layout: &[
            Layout::Text("AGENTS.md"),
            Layout::Agents(".kilo/agents", ".md"),
            Layout::Skills(".agents/skills"),
        ],
        markers: &["kilo.json", "kilo.jsonc", ".kilo/", ".kilocode/"],
        scan_directories: &[".kilo", ".kilo/agents", ".kilocode"],
        config_files: &[(".kilo/kilo.json", McpFormat::LocalCommandArray)],
        open_command: Some("kilo"),
    },
    NativeClient {
        id: "kiro",
        display_name: "Kiro",
        detail: "AGENTS.md, .kiro agents and skills, and MCP",
        mcp_support: McpSupport::Project,
        mcp: Some(project_mcp(
            ".kiro/settings/mcp.json",
            McpFormat::JsonMcpServers,
            "runtime.kiro.mcp",
        )),
        legacy_mcp: None,
        documentation_url: Some("https://kiro.dev/docs/mcp/configuration/"),
        image_and_live_routes: false,
        readiness_checks_mcp: true,
        layout: &[
            Layout::Text("AGENTS.md"),
            Layout::Agents(".kiro/agents", ".md"),
            Layout::Skills(".kiro/skills"),
        ],
        markers: &[".kiro/"],
        scan_directories: &[".kiro", ".kiro/agents", ".kiro/settings", ".kiro/steering"],
        config_files: &[(".kiro/settings/mcp.json", McpFormat::JsonMcpServers)],
        open_command: None,
    },
    NativeClient {
        id: "qwen_code",
        display_name: "Qwen Code",
        detail: "AGENTS.md, .qwen agents and skills, and MCP",
        mcp_support: McpSupport::Project,
        mcp: Some(project_mcp(
            ".qwen/settings.json",
            McpFormat::JsonMcpServers,
            "runtime.qwen.mcp",
        )),
        legacy_mcp: None,
        documentation_url: Some("https://qwenlm.github.io/qwen-code-docs/en/users/features/mcp/"),
        image_and_live_routes: true,
        readiness_checks_mcp: true,
        layout: &[
            Layout::Text("AGENTS.md"),
            Layout::Agents(".qwen/agents", ".md"),
            Layout::Skills(".qwen/skills"),
        ],
        markers: &["QWEN.md", ".qwen/"],
        scan_directories: &[".qwen", ".qwen/agents"],
        config_files: &[(".qwen/settings.json", McpFormat::JsonMcpServers)],
        open_command: Some("qwen"),
    },
    NativeClient {
        id: "devin_desktop",
        display_name: "Devin Desktop",
        detail: "AGENTS.md, .devin agents, and shared .agents skills; MCP is set up in Devin",
        mcp_support: McpSupport::Manual,
        mcp: None,
        legacy_mcp: None,
        documentation_url: Some("https://docs.devin.ai/desktop/devin-local"),
        image_and_live_routes: false,
        readiness_checks_mcp: false,
        layout: &[
            Layout::Text("AGENTS.md"),
            Layout::Agents(".devin/agents", ".md"),
            Layout::Skills(".agents/skills"),
        ],
        markers: &[".devin/", ".windsurf/"],
        scan_directories: &[".devin", ".devin/agents", ".windsurf"],
        config_files: &[],
        open_command: None,
    },
    NativeClient {
        id: "augment",
        display_name: "Augment (Auggie CLI)",
        detail: "AGENTS.md, .augment agents, and Claude-format skills; MCP is set up in Auggie",
        mcp_support: McpSupport::Manual,
        mcp: None,
        legacy_mcp: None,
        documentation_url: Some("https://docs.augmentcode.com/cli/integrations"),
        image_and_live_routes: false,
        readiness_checks_mcp: false,
        layout: &[
            Layout::Text("AGENTS.md"),
            Layout::Agents(".augment/agents", ".md"),
            Layout::Skills(".claude/skills"),
        ],
        markers: &[".augment/", ".augment-guidelines"],
        scan_directories: &[".augment", ".augment/agents"],
        config_files: &[],
        open_command: Some("auggie"),
    },
    NativeClient {
        id: "cline",
        display_name: "Cline",
        detail: "AGENTS.md, a Cline rule, and Claude-format skills; MCP is set up in Cline",
        mcp_support: McpSupport::Manual,
        mcp: None,
        legacy_mcp: None,
        documentation_url: Some("https://docs.cline.bot/mcp/configuring-mcp-servers"),
        image_and_live_routes: false,
        readiness_checks_mcp: false,
        layout: &[
            Layout::Text("AGENTS.md"),
            Layout::Text(".clinerules/hoi4-mod-setup.md"),
            Layout::Skills(".claude/skills"),
        ],
        markers: &[".clinerules", ".clinerules/", ".cline/"],
        scan_directories: &[".clinerules", ".cline"],
        config_files: &[],
        open_command: None,
    },
    NativeClient {
        id: "continue",
        display_name: "Continue",
        detail:
            "A Continue rule that points to AGENTS.md and the skills; MCP is set up in Continue",
        mcp_support: McpSupport::Manual,
        mcp: None,
        legacy_mcp: None,
        documentation_url: Some("https://docs.continue.dev/customize/deep-dives/mcp"),
        image_and_live_routes: false,
        readiness_checks_mcp: false,
        layout: &[
            Layout::Text("AGENTS.md"),
            Layout::Text(".continue/rules/hoi4-mod-setup.md"),
        ],
        markers: &[".continue/"],
        scan_directories: &[".continue", ".continue/rules"],
        config_files: &[],
        open_command: None,
    },
    NativeClient {
        id: "aider",
        display_name: "Aider",
        detail: "An .aider.conf.yml that reads AGENTS.md; Aider has no MCP support",
        mcp_support: McpSupport::None,
        mcp: None,
        legacy_mcp: None,
        documentation_url: Some("https://aider.chat/docs/usage/conventions.html"),
        image_and_live_routes: false,
        readiness_checks_mcp: false,
        layout: &[Layout::Text("AGENTS.md"), Layout::Text(".aider.conf.yml")],
        markers: &[".aider.conf.yml"],
        scan_directories: &[],
        config_files: &[],
        open_command: Some("aider"),
    },
];

pub fn native_client(id: &str) -> Option<&'static NativeClient> {
    NATIVE_CLIENTS.iter().find(|client| client.id == id)
}

fn path_key(path: &str) -> String {
    path.replace('\\', "/").to_ascii_lowercase()
}

/// The client and managed MCP route that own a project path. Legacy routes
/// that the client never read are not adapted.
pub fn mcp_route_for_path(path: &str) -> Option<(&'static NativeClient, NativeMcp)> {
    let key = path_key(path);
    NATIVE_CLIENTS.iter().find_map(|client| {
        client
            .mcp
            .filter(|mcp| path_key(mcp.path) == key)
            .map(|mcp| (client, mcp))
    })
}

/// The format of any native client configuration file, including legacy
/// routes, so a managed file is always merged and validated structurally.
pub fn config_format_for_path(path: &str) -> Option<McpFormat> {
    let key = path_key(path);
    NATIVE_CLIENTS.iter().find_map(|client| {
        client
            .mcp
            .iter()
            .chain(client.legacy_mcp.iter())
            .find(|mcp| path_key(mcp.path) == key)
            .map(|mcp| mcp.format)
            .or_else(|| {
                client
                    .config_files
                    .iter()
                    .find(|(file, _)| path_key(file) == key)
                    .map(|(_, format)| *format)
            })
    })
}

/// Whether a project path is a native client MCP file the installer merges
/// with local content instead of replacing.
pub fn is_native_mcp_config_path(path: &str) -> bool {
    let key = path_key(path);
    NATIVE_CLIENTS.iter().any(|client| {
        client
            .mcp
            .iter()
            .chain(client.legacy_mcp.iter())
            .any(|mcp| path_key(mcp.path) == key)
    })
}

fn marker_matches(marker: &str, key: &str) -> bool {
    let marker = path_key(marker);
    match marker.strip_suffix('/') {
        Some(folder) => key == folder || key.starts_with(&marker),
        None => key == marker,
    }
}

/// Whether a project path belongs to a coding-client package rather than
/// the runtime-neutral core. `AGENTS.md` and the canonical skills are core.
pub fn looks_like_client_path(path: &str) -> bool {
    let key = path_key(path);
    matches!(key.as_str(), "claude.md" | ".mcp.json" | "opencode.json")
        || [".codex/", ".claude/", ".cursor/", ".qoder/", ".opencode/"]
            .iter()
            .any(|prefix| key.starts_with(prefix))
        || NATIVE_CLIENTS.iter().any(|client| {
            client
                .markers
                .iter()
                .any(|marker| marker_matches(marker, &key))
                || client
                    .mcp
                    .iter()
                    .chain(client.legacy_mcp.iter())
                    .any(|mcp| path_key(mcp.path) == key)
        })
}

/// Clients whose project markers exist, in registry order. Markers are
/// evidence of a project surface, not proof that the client is installed.
pub fn detected_clients(
    has: impl Fn(&str) -> bool,
    under: impl Fn(&str) -> bool,
) -> Vec<&'static str> {
    NATIVE_CLIENTS
        .iter()
        .filter(|client| {
            client
                .markers
                .iter()
                .any(|marker| match marker.strip_suffix('/') {
                    Some(folder) => under(&format!("{folder}/")),
                    None => has(marker),
                })
        })
        .map(|client| client.id)
        .collect()
}

/// Whether the read-only scanner enumerates this folder (one level).
pub fn is_client_scan_directory(path: &str) -> bool {
    let key = path_key(path);
    NATIVE_CLIENTS
        .iter()
        .flat_map(|client| client.scan_directories.iter())
        .any(|folder| path_key(folder) == key)
}

/// The MCP file whose image-generation and live-testing routes the installer
/// adapts for a client, when the provider tables support that client.
pub fn image_and_live_config_path(environment: &str) -> Option<&'static str> {
    native_client(environment)
        .filter(|client| client.image_and_live_routes)
        .and_then(|client| client.mcp)
        .map(|mcp| mcp.path)
}

/// Whether readiness must find the shared HOI4 Agent Tools route for the
/// client. The route is installed only where the verified tools package
/// exists (Windows and Linux) and only when the client's MCP component is
/// part of the installation.
pub fn native_registration_required<'a>(
    environment: &str,
    selected_components: impl Iterator<Item = &'a str> + Clone,
    platform: Platform,
) -> bool {
    if !matches!(platform, Platform::Windows | Platform::Linux) {
        return false;
    }
    let Some(client) = native_client(environment) else {
        return false;
    };
    let Some(mcp) = client.mcp.filter(|_| client.readiness_checks_mcp) else {
        return false;
    };
    let selected = |id: &str| selected_components.clone().any(|selected| selected == id);
    let client_route = selected(mcp.component_id)
        || client
            .legacy_mcp
            .is_some_and(|legacy| selected(legacy.component_id));
    client_route && selected(crate::mcp_target::COMPONENT_ID)
}

fn manifest_major(manifest: &RemoteManifest) -> u64 {
    manifest
        .schema_version
        .split('.')
        .next()
        .and_then(|value| value.parse().ok())
        .unwrap_or(0)
}

fn owned_components<'a>(
    manifest: &'a RemoteManifest,
    environment: &'a str,
) -> impl Iterator<Item = &'a ComponentDefinition> + 'a {
    manifest
        .components
        .iter()
        .filter(move |component| component.coding_environment.as_deref() == Some(environment))
}

/// Why this app version cannot offer a declared client, if it cannot.
fn unavailable_reason(
    manifest: &RemoteManifest,
    definition: &CodingEnvironmentDefinition,
    platform: Platform,
) -> Option<String> {
    let client = native_client(&definition.id);
    if manifest_major(manifest) < 6 {
        // Older formats carry no MCP declaration; only the clients those
        // formats shipped have adapters for them.
        if !SUPPORTED.contains(&definition.id.as_str()) {
            return Some("This client needs a newer setup package.".into());
        }
    } else {
        match definition.mcp_support {
            Some(McpSupport::Project) => {
                let Some(declared) = definition.mcp_registration.as_ref() else {
                    return Some(
                        "The setup package does not declare this client's MCP file.".into(),
                    );
                };
                let known = client.and_then(|client| client.mcp).is_some_and(|mcp| {
                    path_key(mcp.path) == path_key(&declared.path)
                        && mcp.format == declared.format
                        && mcp.component_id == declared.component_id
                });
                if !known {
                    return Some("This client needs a newer version of HOI4 Mod Setup.".into());
                }
            }
            Some(McpSupport::Manual | McpSupport::None) => {}
            None => {
                return Some("The setup package does not declare this client's MCP support.".into())
            }
        }
    }
    let required = owned_components(manifest, &definition.id)
        .filter(|component| !component.optional)
        .collect::<Vec<_>>();
    if required.is_empty() {
        return Some("The setup package does not include this client's files.".into());
    }
    if !required.iter().all(|component| {
        component
            .platforms
            .iter()
            .any(|declared| declared.supports(platform))
    }) {
        return Some(format!(
            "This client is not supported on {}.",
            platform.display_name()
        ));
    }
    None
}

pub fn default_selection() -> CodingEnvironmentSelection {
    CodingEnvironmentSelection {
        primary: CLAUDE_CODE.into(),
        additional: Vec::new(),
    }
}

/// An environment identifier is a short lowercase token. Whether a package
/// exists for it is decided by the manifest when components are resolved.
pub fn is_supported(value: &str) -> bool {
    let mut characters = value.chars();
    matches!(characters.next(), Some('a'..='z'))
        && value.len() <= 64
        && characters.all(|character| matches!(character, 'a'..='z' | '0'..='9' | '_'))
}

fn built_in_definition(id: &str) -> CodingEnvironmentDefinition {
    let client = native_client(id).expect("built-in definitions use registry clients");
    CodingEnvironmentDefinition {
        id: id.into(),
        display_name: client.display_name.into(),
        description: Some(client.detail.into()),
        default_primary: id == CLAUDE_CODE,
        // Only the original command-line clients have a verified Open action.
        open_command: match id {
            CLAUDE_CODE | CODEX => client.open_command.map(Into::into),
            _ => None,
        },
        mcp_support: None,
        mcp_registration: None,
        documentation_url: None,
    }
}

/// Every environment the manifest declares, in display order.
fn declared_definitions(manifest: &RemoteManifest) -> Vec<CodingEnvironmentDefinition> {
    if !manifest.coding_environments.is_empty() {
        return manifest.coding_environments.clone();
    }
    let declared = manifest
        .components
        .iter()
        .filter_map(|component| component.coding_environment.as_deref())
        .collect::<HashSet<_>>();
    SUPPORTED
        .iter()
        .filter(|id| declared.contains(**id))
        .map(|id| built_in_definition(id))
        .collect()
}

/// The environments this app version can install from the manifest on the
/// platform, in display order.
pub fn definitions_for_platform(
    manifest: &RemoteManifest,
    platform: Platform,
) -> Vec<CodingEnvironmentDefinition> {
    declared_definitions(manifest)
        .into_iter()
        .filter(|definition| unavailable_reason(manifest, definition, platform).is_none())
        .map(|mut definition| {
            // The app opens only documentation it has reviewed for the client.
            definition.documentation_url = native_client(&definition.id)
                .and_then(|client| client.documentation_url)
                .map(Into::into);
            definition.mcp_support = mcp_support(manifest, &definition.id);
            definition
        })
        .collect()
}

/// The environments the manifest offers on this machine, in display order.
pub fn definitions(manifest: &RemoteManifest) -> Vec<CodingEnvironmentDefinition> {
    definitions_for_platform(manifest, Platform::current())
}

/// Declared environments this app version cannot offer, with a short reason.
/// They are shown as unavailable instead of being silently dropped.
pub fn unavailable_definitions(
    manifest: &RemoteManifest,
    platform: Platform,
) -> Vec<UnavailableCodingEnvironment> {
    declared_definitions(manifest)
        .into_iter()
        .filter_map(|definition| {
            unavailable_reason(manifest, &definition, platform).map(|reason| {
                UnavailableCodingEnvironment {
                    id: definition.id,
                    display_name: definition.display_name,
                    reason,
                }
            })
        })
        .collect()
}

/// The MCP setup for a selected environment: the manifest declaration when
/// present, otherwise the built-in client table.
pub fn mcp_support(manifest: &RemoteManifest, environment: &str) -> Option<McpSupport> {
    manifest
        .coding_environments
        .iter()
        .find(|definition| definition.id == environment)
        .and_then(|definition| definition.mcp_support)
        .or_else(|| native_client(environment).map(|client| client.mcp_support))
}

/// Whether a component installs files for a coding environment, either as
/// its owner or as a shared copy the environment reads from the same path.
pub fn serves_environment(component: &ComponentDefinition, environment: &str) -> bool {
    component.coding_environment.as_deref() == Some(environment)
        || component
            .shared_coding_environments
            .iter()
            .any(|shared| shared == environment)
}

/// The manifest's default primary environment, or Claude Code.
pub fn default_primary(manifest: &RemoteManifest) -> String {
    let definitions = definitions(manifest);
    definitions
        .iter()
        .find(|definition| definition.default_primary)
        .or_else(|| definitions.first())
        .map(|definition| definition.id.clone())
        .unwrap_or_else(|| CLAUDE_CODE.into())
}

pub fn validate_selection(selection: &CodingEnvironmentSelection) -> Result<(), AppError> {
    if !is_supported(selection.primary.as_str()) {
        return Err(AppError::InvalidInput(
            "select a supported primary coding environment".into(),
        ));
    }
    let mut seen = HashSet::new();
    for environment in &selection.additional {
        if !is_supported(environment) {
            return Err(AppError::InvalidInput(
                "select supported additional coding environments".into(),
            ));
        }
        if environment == &selection.primary {
            return Err(AppError::InvalidInput(
                "the primary coding environment cannot also be additional".into(),
            ));
        }
        if !seen.insert(environment.as_str()) {
            return Err(AppError::InvalidInput(format!(
                "additional coding environment is duplicated: {environment}"
            )));
        }
    }
    Ok(())
}

/// Read the renderer's camelCase state while accepting the snake_case shape
/// used by persisted locks and older integrations. Missing fields are a
/// compatibility default, never an implicit second primary.
pub fn selection_from_value(value: &Value) -> Result<CodingEnvironmentSelection, AppError> {
    let nested = value.get("coding_environments");
    let primary = value
        .get("primaryCodingEnvironment")
        .or_else(|| value.get("primary_coding_environment"))
        .or_else(|| nested.and_then(|value| value.get("primary")))
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(CODEX)
        .to_string();
    let additional = value
        .get("additionalCodingEnvironments")
        .or_else(|| value.get("additional_coding_environments"))
        .or_else(|| nested.and_then(|value| value.get("additional")))
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(Value::as_str)
                .map(ToOwned::to_owned)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let selection = CodingEnvironmentSelection {
        primary,
        additional,
    };
    validate_selection(&selection)?;
    Ok(selection)
}

/// Fresh planning follows the resolved package default. Persisted legacy
/// state uses `selection_from_value` so a missing primary remains Codex.
pub fn selection_from_manifest(
    manifest: &RemoteManifest,
    value: &Value,
) -> Result<CodingEnvironmentSelection, AppError> {
    if value.get("primaryCodingEnvironment").is_some()
        || value.get("primary_coding_environment").is_some()
        || value.pointer("/coding_environments/primary").is_some()
    {
        return selection_from_value(value);
    }
    let mut state = value.as_object().cloned().ok_or_else(|| {
        AppError::InvalidInput("coding-environment selection must be an object".into())
    })?;
    state.insert(
        "primaryCodingEnvironment".into(),
        Value::String(default_primary(manifest)),
    );
    selection_from_value(&Value::Object(state))
}

/// Resolve all manifest components belonging to the selected environments.
/// A package is considered incomplete when the manifest does not declare any
/// component for a requested environment; this prevents a misleading partial
/// install when an older source revision predates a client package.
pub fn component_ids(
    manifest: &RemoteManifest,
    selection: &CodingEnvironmentSelection,
    active_component_ids: &HashSet<String>,
) -> Result<Vec<String>, AppError> {
    component_ids_for_platform(
        manifest,
        selection,
        active_component_ids,
        Platform::current(),
    )
}

/// [`component_ids`] for a chosen platform. The web edition installs for the
/// platform the user picks, not the one running the browser.
pub fn component_ids_for_platform(
    manifest: &RemoteManifest,
    selection: &CodingEnvironmentSelection,
    active_component_ids: &HashSet<String>,
    platform: Platform,
) -> Result<Vec<String>, AppError> {
    validate_selection(selection)?;
    let optional_ids = manifest
        .components
        .iter()
        .filter(|component| component.optional)
        .map(|component| component.id.as_str())
        .collect::<HashSet<_>>();
    let mut result = Vec::new();
    let mut requested = vec![selection.primary.as_str()];
    requested.extend(selection.additional.iter().map(String::as_str));
    for environment in requested {
        if manifest_major(manifest) >= 6 {
            if let Some(definition) = declared_definitions(manifest)
                .into_iter()
                .find(|definition| definition.id == environment)
            {
                if let Some(reason) = unavailable_reason(manifest, &definition, platform) {
                    // A selection the user can change is reported as such,
                    // never as a download failure.
                    return Err(AppError::InvalidInput(format!(
                        "{} cannot be installed from this setup version. {reason} Deselect it or choose another version; nothing was changed.",
                        definition.display_name
                    )));
                }
            }
        }
        let ids = manifest
            .components
            .iter()
            .filter(|component| serves_environment(component, environment))
            .filter(|component| {
                if !component.optional {
                    return true;
                }
                let activation_dependencies = component
                    .dependencies
                    .iter()
                    .filter(|dependency| optional_ids.contains(dependency.as_str()))
                    .collect::<Vec<_>>();
                !activation_dependencies.is_empty()
                    && activation_dependencies
                        .iter()
                        .all(|dependency| active_component_ids.contains(dependency.as_str()))
            })
            .map(|component| component.id.clone())
            .collect::<Vec<_>>();
        if ids.is_empty() {
            let label = native_client(environment)
                .map(|client| client.display_name)
                .unwrap_or(environment);
            return Err(AppError::InvalidInput(format!(
                "{label} is not included in this setup version, so its files cannot be kept. Choose a version that includes it or deselect it; nothing was changed."
            )));
        }
        result.extend(ids);
    }
    result.sort();
    result.dedup();
    Ok(result)
}

pub fn all_environment_component_ids(manifest: &RemoteManifest) -> HashSet<String> {
    manifest
        .components
        .iter()
        .filter(|component| component.coding_environment.is_some())
        .map(|component| component.id.clone())
        .collect()
}

pub fn selected_environment_ids(selection: &CodingEnvironmentSelection) -> Vec<String> {
    let mut result = vec![selection.primary.clone()];
    result.extend(selection.additional.clone());
    result
}

/// Compatibility helper for lock reconciliation. Future source manifests may
/// add more environment component IDs; current IDs are kept here so a
/// deselected package cannot be resurrected by a legacy lock append.
pub fn is_known_environment_component_id(id: &str) -> bool {
    id.starts_with("runtime.")
        || matches!(
            id,
            "codex.config"
                | "mcp.hoi4_agent_tools"
                | "core.claude.instructions"
                | "runtime.claude"
                | "runtime.claude.mcp"
                | "runtime.cursor"
                | "runtime.cursor.mcp"
                | "runtime.qoder"
                | "runtime.qoder.mcp"
                | "runtime.opencode"
                | "runtime.opencode.config"
                | "runtime.opencode.mcp"
                | "runtime.claude.portrait_agent"
                | "runtime.claude.super_event_agents"
                | "runtime.cursor.portrait_agent"
                | "runtime.cursor.super_event_agents"
                | "runtime.qoder.portrait_agent"
                | "runtime.qoder.super_event_agents"
                | "runtime.opencode.portrait_agent"
                | "runtime.opencode.super_event_agents"
        )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn claude_code_is_the_default_primary_environment() {
        assert_eq!(default_selection().primary, CLAUDE_CODE);
        assert!(default_selection().additional.is_empty());
        assert_eq!(default_primary(&published_manifest()), CLAUDE_CODE);
    }

    #[test]
    fn fresh_selection_uses_the_package_default_and_preserves_legacy_selection() {
        let mut manifest = published_manifest();
        assert_eq!(
            selection_from_manifest(&manifest, &json!({}))
                .unwrap()
                .primary,
            CLAUDE_CODE
        );
        assert_eq!(selection_from_value(&json!({})).unwrap().primary, CODEX);
        for state in [
            json!({"primaryCodingEnvironment": "codex"}),
            json!({"coding_environments": {"primary": "cursor"}}),
        ] {
            assert_eq!(
                selection_from_manifest(&manifest, &state).unwrap(),
                selection_from_value(&state).unwrap()
            );
        }
        for definition in &mut manifest.coding_environments {
            definition.default_primary = definition.id == CURSOR;
        }
        assert_eq!(
            selection_from_manifest(&manifest, &json!({}))
                .unwrap()
                .primary,
            CURSOR
        );
    }

    #[test]
    fn environment_identifiers_are_short_lowercase_tokens() {
        assert!(is_supported("claude_code"));
        assert!(is_supported("windsurf"));
        assert!(!is_supported(""));
        assert!(!is_supported("Claude"));
        assert!(!is_supported("2code"));
        assert!(!is_supported("../codex"));
    }

    #[test]
    fn declared_environments_come_from_the_manifest_in_order() {
        let manifest = published_manifest();
        let ids = definitions(&manifest)
            .into_iter()
            .map(|definition| definition.id)
            .collect::<Vec<_>>();
        assert_eq!(ids.first().map(String::as_str), Some(CLAUDE_CODE));
        for id in SUPPORTED {
            assert!(ids.iter().any(|declared| declared == id));
        }
    }

    #[test]
    fn primary_cannot_be_repeated_as_an_additional_environment() {
        let error = validate_selection(&CodingEnvironmentSelection {
            primary: CURSOR.into(),
            additional: vec![CURSOR.into()],
        })
        .unwrap_err();
        assert!(error.to_string().contains("cannot also be additional"));
    }

    #[test]
    fn native_registration_is_required_only_for_selected_project_routes_on_tool_platforms() {
        let selected = |ids: &'static [&'static str]| ids.iter().copied();
        for platform in [Platform::Windows, Platform::Linux] {
            for (environment, component) in [
                (CLAUDE_CODE, "runtime.claude.mcp"),
                (CURSOR, "runtime.cursor.mcp"),
                (QODER, "runtime.qoder.mcp"),
                ("gemini_cli", "runtime.gemini.mcp"),
                ("github_copilot", "runtime.copilot.mcp"),
                ("grok_build", "runtime.grok.mcp"),
            ] {
                let both: Vec<&str> = vec![component, crate::mcp_target::COMPONENT_ID];
                assert!(native_registration_required(
                    environment,
                    both.iter().copied(),
                    platform
                ));
                assert!(!native_registration_required(
                    environment,
                    selected(&["mcp.hoi4_agent_tools"]),
                    platform
                ));
                let only_client: Vec<&str> = vec![component];
                assert!(!native_registration_required(
                    environment,
                    only_client.iter().copied(),
                    platform
                ));
            }
            // A legacy OpenCode lock keeps its old structural check; a format 6
            // lock requires the route in opencode.json.
            assert!(native_registration_required(
                OPENCODE,
                selected(&["runtime.opencode.mcp", "mcp.hoi4_agent_tools"]),
                platform
            ));
            assert!(native_registration_required(
                OPENCODE,
                selected(&["runtime.opencode.config", "mcp.hoi4_agent_tools"]),
                platform
            ));
            for environment in [CODEX, "cline", "devin_desktop", "aider", "unknown_client"] {
                assert!(!native_registration_required(
                    environment,
                    selected(&["codex.config", "mcp.hoi4_agent_tools"]),
                    platform
                ));
            }
        }
        assert!(!native_registration_required(
            CLAUDE_CODE,
            selected(&["runtime.claude.mcp", "mcp.hoi4_agent_tools"]),
            Platform::Macos
        ));
    }

    #[test]
    fn client_registry_is_unique_contained_and_honest_about_mcp() {
        let mut ids = HashSet::new();
        let mut mcp_paths = HashSet::new();
        for client in NATIVE_CLIENTS {
            assert!(is_supported(client.id), "{}", client.id);
            assert!(ids.insert(client.id), "duplicate client {}", client.id);
            assert!(!client.display_name.is_empty() && client.display_name.len() <= 64);
            match client.mcp_support {
                McpSupport::Project => assert!(client.mcp.is_some(), "{}", client.id),
                McpSupport::Manual | McpSupport::None => {
                    assert!(client.mcp.is_none() && client.legacy_mcp.is_none());
                    assert!(!client.readiness_checks_mcp && !client.image_and_live_routes);
                    assert!(client
                        .documentation_url
                        .is_some_and(|url| url.starts_with("https://")));
                }
            }
            for mcp in client.mcp.iter().chain(client.legacy_mcp.iter()) {
                assert!(
                    mcp_paths.insert(path_key(mcp.path)),
                    "shared MCP path {}",
                    mcp.path
                );
                assert_eq!(
                    crate::text::normalize_relative_path(mcp.path).unwrap(),
                    mcp.path
                );
                assert!(
                    mcp.component_id.starts_with("runtime.") || mcp.component_id == "codex.config"
                );
            }
            for path in client
                .scan_directories
                .iter()
                .chain(client.config_files.iter().map(|(path, _)| path))
                .chain(client.markers.iter())
            {
                let trimmed = path.trim_end_matches('/');
                assert_eq!(
                    crate::text::normalize_relative_path(trimmed).unwrap(),
                    trimmed,
                    "{path}"
                );
            }
            assert!(!client.layout.is_empty() || client.id == CODEX);
        }
        for id in SUPPORTED {
            assert!(native_client(id).is_some());
        }
        assert_eq!(
            mcp_route_for_path(".VSCODE\\mcp.json").map(|(client, _)| client.id),
            Some("github_copilot")
        );
        assert!(mcp_route_for_path(".opencode/mcp.json").is_none());
        assert!(is_native_mcp_config_path(".opencode/mcp.json"));
        assert_eq!(
            config_format_for_path(".grok/config.toml"),
            Some(McpFormat::TomlMcpServers)
        );
        assert_eq!(image_and_live_config_path(OPENCODE), Some("opencode.json"));
        assert_eq!(
            image_and_live_config_path("gemini_cli"),
            Some(".gemini/settings.json")
        );
        // Kiro and Kilo Code document no working directory for project
        // servers, so relative image and live adapters cannot run there.
        assert_eq!(image_and_live_config_path("kiro"), None);
        assert_eq!(image_and_live_config_path("kilo_code"), None);
    }

    fn format_six(mut manifest: RemoteManifest) -> RemoteManifest {
        manifest.schema_version = "6.0.0".into();
        for definition in &mut manifest.coding_environments {
            let client = native_client(&definition.id).unwrap();
            definition.mcp_support = Some(client.mcp_support);
            definition.mcp_registration = client.mcp.map(|mcp| crate::models::McpRegistration {
                component_id: mcp.component_id.into(),
                path: mcp.path.into(),
                format: mcp.format,
            });
        }
        manifest
    }

    fn add_client(
        manifest: &mut RemoteManifest,
        id: &str,
        support: McpSupport,
        registration: Option<(&str, McpFormat)>,
        platforms: Vec<crate::models::ManifestPlatform>,
    ) {
        let mut component = manifest
            .components
            .iter()
            .find(|component| component.id == "runtime.cursor")
            .unwrap()
            .clone();
        component.id = format!("runtime.{id}");
        component.coding_environment = Some(id.into());
        component.platforms = platforms;
        manifest.components.push(component);
        manifest
            .coding_environments
            .push(CodingEnvironmentDefinition {
                id: id.into(),
                display_name: id.into(),
                description: None,
                default_primary: false,
                open_command: None,
                mcp_support: Some(support),
                mcp_registration: registration.map(|(path, format)| {
                    crate::models::McpRegistration {
                        component_id: native_client(id)
                            .and_then(|client| client.mcp)
                            .map(|mcp| mcp.component_id.to_string())
                            .unwrap_or_else(|| format!("runtime.{id}")),
                        path: path.into(),
                        format,
                    }
                }),
                documentation_url: None,
            });
    }

    #[test]
    fn format_six_offers_only_clients_with_a_matching_adapter_or_manual_setup() {
        use crate::models::ManifestPlatform;
        let mut manifest = format_six(published_manifest());
        add_client(
            &mut manifest,
            "gemini_cli",
            McpSupport::Project,
            Some((".gemini/settings.json", McpFormat::JsonMcpServers)),
            vec![ManifestPlatform::All],
        );
        add_client(
            &mut manifest,
            "kiro",
            McpSupport::Project,
            Some((".kiro/settings/mcp.json", McpFormat::VscodeServers)),
            vec![ManifestPlatform::All],
        );
        add_client(
            &mut manifest,
            "future_ide",
            McpSupport::Project,
            Some((".future/mcp.json", McpFormat::JsonMcpServers)),
            vec![ManifestPlatform::All],
        );
        add_client(
            &mut manifest,
            "future_manual",
            McpSupport::Manual,
            None,
            vec![ManifestPlatform::All],
        );
        add_client(
            &mut manifest,
            "augment",
            McpSupport::Manual,
            None,
            vec![ManifestPlatform::Macos, ManifestPlatform::Linux],
        );
        let offered = |platform| {
            definitions_for_platform(&manifest, platform)
                .into_iter()
                .map(|definition| definition.id)
                .collect::<Vec<_>>()
        };
        let windows = offered(Platform::Windows);
        for id in SUPPORTED
            .iter()
            .copied()
            .chain(["gemini_cli", "future_manual"])
        {
            assert!(windows.iter().any(|offered| offered == id), "{id}");
        }
        // A format mismatch, an unknown project route, and a platform gap are
        // shown as unavailable with a reason, never offered.
        for id in ["kiro", "future_ide", "augment"] {
            assert!(!windows.iter().any(|offered| offered == id), "{id}");
        }
        let unavailable = unavailable_definitions(&manifest, Platform::Windows);
        assert_eq!(
            unavailable
                .iter()
                .map(|item| item.id.as_str())
                .collect::<Vec<_>>(),
            vec!["kiro", "future_ide", "augment"]
        );
        assert!(unavailable[0].reason.contains("newer version"));
        assert!(unavailable[2].reason.contains("Windows"));
        assert!(offered(Platform::Linux).iter().any(|id| id == "augment"));
        // A registration that names another component never enables the
        // readiness route check through a matching path alone.
        let mut mismatched = manifest.clone();
        mismatched
            .coding_environments
            .iter_mut()
            .find(|definition| definition.id == "gemini_cli")
            .and_then(|definition| definition.mcp_registration.as_mut())
            .unwrap()
            .component_id = "runtime.gemini_cli".into();
        assert!(!definitions_for_platform(&mismatched, Platform::Windows)
            .iter()
            .any(|definition| definition.id == "gemini_cli"));

        let active = ["mcp.hoi4_agent_tools".to_string()].into_iter().collect();
        if Platform::current() != Platform::Unsupported {
            let error = component_ids(
                &manifest,
                &CodingEnvironmentSelection {
                    primary: "future_ide".into(),
                    additional: vec![],
                },
                &active,
            )
            .unwrap_err();
            assert!(error.to_string().contains("cannot be installed"));
        }
        // A format 5 package never offers a client it did not ship.
        let mut legacy = published_manifest();
        add_client(
            &mut legacy,
            "gemini_cli",
            McpSupport::Project,
            Some((".gemini/settings.json", McpFormat::JsonMcpServers)),
            vec![ManifestPlatform::All],
        );
        assert!(!definitions_for_platform(&legacy, Platform::Windows)
            .iter()
            .any(|definition| definition.id == "gemini_cli"));
    }

    /// The exact format 6 manifest generated by the source repository's owned
    /// generator (see docs/fixtures/README.md).
    fn format_six_candidate() -> RemoteManifest {
        crate::manifest::parse_manifest(
            include_bytes!("../../../docs/fixtures/format6-candidate-manifest.json"),
            None,
        )
        .expect("the generated format 6 candidate validates")
    }

    #[test]
    fn generated_format_six_provider_tables_list_exactly_the_route_capable_clients() {
        let manifest = format_six_candidate();
        let route_clients = NATIVE_CLIENTS
            .iter()
            .filter(|client| client.image_and_live_routes)
            .map(|client| client.id)
            .collect::<HashSet<_>>();
        for table in [&manifest.image_generation, &manifest.live_testing] {
            let providers = table.as_ref().unwrap()["providers"].as_array().unwrap();
            for provider in providers {
                let kind = provider["route"]["kind"].as_str().unwrap();
                if matches!(kind, "manual" | "native") {
                    continue;
                }
                let listed = provider["supported_coding_environments"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|id| id.as_str().unwrap())
                    .collect::<HashSet<_>>();
                assert_eq!(listed, route_clients, "{}", provider["id"]);
            }
        }
    }

    #[test]
    fn generated_format_six_candidate_matches_the_registry_and_resolves_every_client() {
        let manifest = format_six_candidate();
        assert_eq!(manifest.schema_version, "6.0.0");
        let declared = manifest
            .coding_environments
            .iter()
            .map(|definition| definition.id.as_str())
            .collect::<HashSet<_>>();
        let registry = NATIVE_CLIENTS
            .iter()
            .map(|client| client.id)
            .collect::<HashSet<_>>();
        assert_eq!(declared, registry);
        for definition in &manifest.coding_environments {
            let client = native_client(&definition.id).unwrap();
            assert_eq!(
                definition.mcp_support,
                Some(client.mcp_support),
                "{}",
                client.id
            );
            match (definition.mcp_registration.as_ref(), client.mcp) {
                (Some(declared), Some(mcp)) => {
                    assert_eq!(declared.path, mcp.path, "{}", client.id);
                    assert_eq!(declared.format, mcp.format, "{}", client.id);
                    assert_eq!(declared.component_id, mcp.component_id, "{}", client.id);
                }
                (None, None) => {}
                other => panic!("{} registration mismatch: {other:?}", client.id),
            }
        }
        for platform in [Platform::Windows, Platform::Macos, Platform::Linux] {
            let offered = definitions_for_platform(&manifest, platform);
            let unavailable = unavailable_definitions(&manifest, platform);
            assert_eq!(offered.len() + unavailable.len(), NATIVE_CLIENTS.len());
            assert_eq!(
                offered.iter().any(|definition| definition.id == "augment"),
                platform != Platform::Windows
            );
            assert_eq!(
                unavailable.len(),
                usize::from(platform == Platform::Windows),
                "{platform:?}"
            );
        }
        let active = [
            "mcp.hoi4_agent_tools",
            "workflow.portraits.core",
            "workflow.portraits.subagent",
            "workflow.super_events",
            "workflow.super_events.subagents",
        ]
        .into_iter()
        .map(String::from)
        .collect::<HashSet<_>>();
        for definition in definitions(&manifest) {
            let selection = CodingEnvironmentSelection {
                primary: definition.id.clone(),
                additional: vec![],
            };
            let ids = component_ids(&manifest, &selection, &active)
                .unwrap_or_else(|error| panic!("{}: {error}", definition.id));
            for id in &ids {
                let component = manifest.components.iter().find(|c| &c.id == id).unwrap();
                assert!(serves_environment(component, &definition.id), "{id}");
            }
            if let Some(registration) = definition.mcp_registration.as_ref() {
                assert!(
                    ids.contains(&registration.component_id),
                    "{}",
                    definition.id
                );
            }
            let shares_skills = ids.iter().any(|id| id == "runtime.codex.skills");
            assert_eq!(
                shares_skills,
                [
                    CODEX,
                    "gemini_cli",
                    "antigravity",
                    "kilo_code",
                    "devin_desktop"
                ]
                .contains(&definition.id.as_str()),
                "{}",
                definition.id
            );
        }
        assert!(!manifest
            .components
            .iter()
            .any(|component| component.id == "runtime.opencode.mcp"));
    }

    #[test]
    fn shared_components_serve_every_listed_client_without_duplicate_destinations() {
        let mut manifest = format_six(published_manifest());
        add_client(
            &mut manifest,
            "antigravity",
            McpSupport::Manual,
            None,
            vec![crate::models::ManifestPlatform::All],
        );
        for component in &mut manifest.components {
            if component.id == "runtime.codex.skills" {
                component.shared_coding_environments = vec!["antigravity".into()];
            }
        }
        let active = ["mcp.hoi4_agent_tools".to_string()].into_iter().collect();
        let antigravity = component_ids(
            &manifest,
            &CodingEnvironmentSelection {
                primary: "antigravity".into(),
                additional: vec![],
            },
            &active,
        )
        .unwrap();
        assert!(antigravity.iter().any(|id| id == "runtime.codex.skills"));
        assert!(!antigravity.iter().any(|id| id == "codex.config"));
        let both = component_ids(
            &manifest,
            &CodingEnvironmentSelection {
                primary: "antigravity".into(),
                additional: vec![CODEX.into()],
            },
            &active,
        )
        .unwrap();
        assert_eq!(
            both.iter()
                .filter(|id| id.as_str() == "runtime.codex.skills")
                .count(),
            1
        );
    }

    #[test]
    fn persisted_nested_state_is_read_and_validated() {
        let selection = selection_from_value(&json!({
            "coding_environments": {
                "primary": "claude_code",
                "additional": ["cursor", "opencode"]
            }
        }))
        .unwrap();
        assert_eq!(selection.primary, CLAUDE_CODE);
        assert_eq!(selection.additional, vec![CURSOR, OPENCODE]);
    }

    fn published_manifest() -> RemoteManifest {
        serde_json::from_slice(include_bytes!(
            "../../../docs/source-manifest/hoi4-mod-setup.manifest.json"
        ))
        .expect("published manifest fixture parses")
    }

    #[test]
    fn every_primary_and_additional_environment_combination_has_a_complete_package() {
        let manifest = published_manifest();
        for (primary_index, primary) in SUPPORTED.iter().enumerate() {
            let additional_candidates = SUPPORTED
                .iter()
                .enumerate()
                .filter_map(|(index, environment)| (index != primary_index).then_some(*environment))
                .collect::<Vec<_>>();
            for mask in 0..(1usize << additional_candidates.len()) {
                let additional = additional_candidates
                    .iter()
                    .enumerate()
                    .filter_map(|(index, environment)| {
                        (mask & (1usize << index) != 0).then_some((*environment).to_string())
                    })
                    .collect::<Vec<_>>();
                let selection = CodingEnvironmentSelection {
                    primary: (*primary).into(),
                    additional,
                };
                let active = ["mcp.hoi4_agent_tools".to_string()]
                    .into_iter()
                    .collect::<HashSet<_>>();
                let ids = component_ids(&manifest, &selection, &active)
                    .expect("every published environment combination is declared");
                for environment in selected_environment_ids(&selection) {
                    assert!(ids.iter().any(|id| {
                        manifest.components.iter().any(|component| {
                            component.id == id.as_str()
                                && component.coding_environment.as_deref()
                                    == Some(environment.as_str())
                        })
                    }));
                }
                assert_eq!(ids.len(), ids.iter().collect::<HashSet<_>>().len());
            }
        }
    }

    #[test]
    fn optional_environment_projections_require_their_workflow_dependency() {
        let manifest = published_manifest();
        let selection = CodingEnvironmentSelection {
            primary: CURSOR.into(),
            additional: vec![],
        };
        let shared = ["mcp.hoi4_agent_tools".to_string()]
            .into_iter()
            .collect::<HashSet<_>>();
        let core = component_ids(&manifest, &selection, &shared).unwrap();
        assert!(core.iter().any(|id| id == "runtime.cursor.mcp"));
        assert!(!core.iter().any(|id| id == "runtime.cursor.portrait_agent"));
        assert!(!core
            .iter()
            .any(|id| id == "runtime.cursor.super_event_agents"));

        let active = [
            "mcp.hoi4_agent_tools".to_string(),
            "workflow.portraits.subagent".to_string(),
            "workflow.super_events.subagents".to_string(),
        ]
        .into_iter()
        .collect::<HashSet<_>>();
        let optional = component_ids(&manifest, &selection, &active).unwrap();
        assert!(optional
            .iter()
            .any(|id| id == "runtime.cursor.portrait_agent"));
        assert!(optional
            .iter()
            .any(|id| id == "runtime.cursor.super_event_agents"));
    }
}
