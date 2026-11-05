//! The backends hoi4-imagegen can generate with, and the older image provider
//! rows each of them replaces.
//!
//! The setup package's `image_generation.providers` table lists the
//! `hoi4_imagegen` route and, for older projects, one row per provider that
//! hoi4-imagegen did not yet cover. The route may carry a `backends` list
//! (`id`, `display_name`, `default`, `credential_field`, `settings`,
//! `replaces_providers`). A package without it still works: the backends are
//! derived from the route's `credential_fields`, with the ChatGPT sign-in as
//! the default, so a new backend appears as soon as the package publishes it
//! and nothing here names one.

use crate::release_binary::{is_release_binary_provider, PROVIDER_ID};
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::BTreeMap;

/// The backend hoi4-imagegen uses when none is chosen: the ChatGPT sign-in.
pub const DEFAULT_BACKEND_ID: &str = "chatgpt_login";

/// The `route_settings` key that records the chosen backend.
pub const BACKEND_SETTING: &str = "backend";

const MAX_BACKENDS: usize = 32;
const MAX_SETTINGS: usize = 8;

/// The TOML file hoi4-imagegen reads its project settings from.
pub const PROJECT_CONFIG_PATH: &str = ".hoi4-imagegen/config.toml";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Backend {
    pub id: String,
    pub display_name: String,
    pub default: bool,
    /// Whether the backend has been verified against the real service. Shown as a hint.
    pub live_verified: bool,
    /// The `credential_fields` entry that holds this backend's key, if any.
    pub credential_field: Option<String>,
    /// Settings the backend reads from the project's `.hoi4-imagegen/config.toml`.
    pub settings: Vec<BackendSetting>,
    /// Older provider rows this backend replaces.
    pub replaces_providers: Vec<String>,
}

/// One value a backend reads from `.hoi4-imagegen/config.toml`, such as the
/// address of a ComfyUI server. `key` is the dotted TOML path under
/// `[providers.<id>]`; `kind` says how the value is checked.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BackendSetting {
    pub key: String,
    pub kind: String,
    pub required: bool,
    pub default: Option<String>,
}

/// A stored key that moves from an older provider's field to a backend's.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CredentialMove {
    pub from_field: String,
    pub to_field: String,
}

/// How an older provider row maps onto a hoi4-imagegen backend.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Replacement {
    pub backend: String,
    pub credential_moves: Vec<CredentialMove>,
}

fn id_like(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
}

fn text(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty() && text.len() <= 120)
        .map(str::to_owned)
}

fn credential_fields(provider: &Value) -> Vec<(String, String, String)> {
    provider
        .pointer("/route/credential_fields")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|field| {
            Some((
                text(field, "id")?,
                text(field, "display_name")?,
                text(field, "environment_name")?,
            ))
        })
        .collect()
}

fn setting_key_like(key: &str) -> bool {
    let parts = key.split('.').collect::<Vec<_>>();
    (3..=4).contains(&parts.len())
        && parts[0] == "providers"
        && parts.iter().all(|part| {
            part.starts_with(|c: char| c.is_ascii_lowercase())
                && part.len() <= 48
                && part
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
        })
}

fn listed_setting(entry: &Value) -> Option<BackendSetting> {
    let key = text(entry, "key").filter(|key| setting_key_like(key))?;
    let kind = text(entry, "kind")
        .filter(|kind| matches!(kind.as_str(), "url" | "project_path" | "text"))?;
    Some(BackendSetting {
        key,
        kind,
        required: entry.get("required").and_then(Value::as_bool) == Some(true),
        default: text(entry, "default"),
    })
}

/// Why a value is not acceptable for a setting of this kind, or `None`.
/// An address is HTTPS anywhere or HTTP on this computer, without credentials;
/// a path stays inside the project.
pub fn setting_value_problem(kind: &str, value: &str) -> Option<&'static str> {
    if value.is_empty() || value.len() > 300 || value.chars().any(|c| c.is_control() || c == ' ') {
        return Some("enter a value without spaces");
    }
    match kind {
        "url" => {
            let https = value.starts_with("https://");
            let http = value.starts_with("http://");
            let rest = value
                .trim_start_matches("https://")
                .trim_start_matches("http://");
            let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
            if !(https || http) || authority.is_empty() || authority.contains('@') {
                Some("enter an http or https address without a user name")
            } else if http && !crate::image_generation::is_loopback_http_url(value) {
                Some("an address outside this computer must use https")
            } else {
                None
            }
        }
        "text" => {
            let plain = value.bytes().all(|b| {
                b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b':' | b'/' | b'-')
            });
            (!plain).then_some("use letters, digits and . _ : / - only")
        }
        "project_path" => {
            let relative =
                !value.starts_with('/') && !value.starts_with('\\') && !value.contains(':');
            let inside = !value.contains('\\')
                && !value.split('/').any(|part| part == ".." || part.is_empty());
            let allowed = relative && inside && value.to_ascii_lowercase().ends_with(".json");
            (!allowed).then_some("enter a .json path inside the project, with forward slashes")
        }
        _ => Some("this setting kind is not supported"),
    }
}

/// The backend's setting for a TOML key, if it declares one.
pub fn setting_for<'a>(backend: &'a Backend, key: &str) -> Option<&'a BackendSetting> {
    backend.settings.iter().find(|setting| setting.key == key)
}

fn set_toml_path(table: &mut toml::Table, key: &str, value: &str) {
    let mut parts = key.split('.').peekable();
    let mut current = table;
    while let Some(part) = parts.next() {
        if parts.peek().is_none() {
            current.insert(part.to_owned(), toml::Value::String(value.to_owned()));
            return;
        }
        let entry = current
            .entry(part.to_owned())
            .or_insert_with(|| toml::Value::Table(toml::Table::new()));
        if !entry.is_table() {
            *entry = toml::Value::Table(toml::Table::new());
        }
        current = entry.as_table_mut().expect("just made a table");
    }
}

/// The project's `.hoi4-imagegen/config.toml` with the chosen backend and its
/// settings applied, or `None` when the project records no backend choice
/// (hoi4-imagegen then picks the first ready provider itself). `existing` is
/// the file as the project has it: every other table (`[profiles.*]`,
/// `[policy]`, `[templates]`) and every other key is kept, and only
/// `defaults.provider` and the backend's declared settings are set.
pub fn project_config_content(
    provider: &Value,
    settings: Option<&BTreeMap<String, String>>,
    existing: Option<&str>,
) -> Result<Option<String>, crate::AppError> {
    let offered = backends(provider);
    let Some(chosen_id) = settings.and_then(|settings| settings.get(BACKEND_SETTING)) else {
        return Ok(None);
    };
    let Some(backend) = offered.iter().find(|backend| &backend.id == chosen_id) else {
        return Err(crate::AppError::InvalidInput(
            "choose an image backend from the list the setup package publishes".into(),
        ));
    };
    let mut table = match existing {
        Some(text) if !text.trim().is_empty() => text.parse::<toml::Table>().map_err(|error| {
            crate::AppError::Source(format!("{PROJECT_CONFIG_PATH} is not valid TOML: {error}"))
        })?,
        _ => toml::Table::new(),
    };
    set_toml_path(&mut table, "defaults.provider", &backend.id);
    for setting in &backend.settings {
        let value = settings
            .and_then(|settings| settings.get(&setting.key))
            .map(String::as_str)
            .filter(|value| !value.is_empty())
            .or(setting.default.as_deref().filter(|_| setting.required));
        if value.is_none() && setting.required {
            return Err(crate::AppError::InvalidInput(format!(
                "{}: this setting is required",
                setting.key
            )));
        }
        if let Some(value) = value {
            if let Some(problem) = setting_value_problem(&setting.kind, value) {
                return Err(crate::AppError::InvalidInput(format!(
                    "{}: {problem}",
                    setting.key
                )));
            }
            set_toml_path(&mut table, &setting.key, value);
        }
    }
    toml::to_string_pretty(&table).map(Some).map_err(|error| {
        crate::AppError::Source(format!(
            "{PROJECT_CONFIG_PATH} could not be written: {error}"
        ))
    })
}

fn derived_backend(field_id: &str, display_name: &str) -> Option<Backend> {
    let id = field_id
        .strip_suffix("_api_key")
        .or_else(|| field_id.strip_suffix("_key"))
        .unwrap_or(field_id);
    let display = display_name
        .strip_suffix(" API key")
        .or_else(|| display_name.strip_suffix(" key"))
        .unwrap_or(display_name);
    id_like(id).then(|| Backend {
        id: id.to_owned(),
        display_name: display.to_owned(),
        default: false,
        live_verified: false,
        credential_field: Some(field_id.to_owned()),
        settings: Vec::new(),
        replaces_providers: Vec::new(),
    })
}

fn listed_backend(entry: &Value, fields: &[(String, String, String)]) -> Option<Backend> {
    let id = text(entry, "id").filter(|id| id_like(id))?;
    let display_name = text(entry, "display_name")?;
    let credential_field = match entry.get("credential_field") {
        None | Some(Value::Null) => None,
        Some(_) => Some(
            text(entry, "credential_field")
                .filter(|wanted| fields.iter().any(|(field, _, _)| field == wanted))?,
        ),
    };
    Some(Backend {
        id,
        display_name,
        default: entry.get("default").and_then(Value::as_bool) == Some(true),
        live_verified: entry.get("live_verified").and_then(Value::as_bool) != Some(false),
        credential_field,
        settings: entry
            .get("settings")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(listed_setting)
            .take(MAX_SETTINGS)
            .collect(),
        replaces_providers: entry
            .get("replaces_providers")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .filter(|id| id_like(id))
            .map(str::to_owned)
            .collect(),
    })
}

/// The backends a `hoi4_imagegen` provider row offers, the default first.
/// Rows that are not the hoi4-imagegen route offer none.
pub fn backends(provider: &Value) -> Vec<Backend> {
    if !is_release_binary_provider(provider) {
        return Vec::new();
    }
    let fields = credential_fields(provider);
    let mut found: Vec<Backend> = Vec::new();
    for entry in provider
        .pointer("/route/backends")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .take(MAX_BACKENDS)
    {
        if let Some(backend) = listed_backend(entry, &fields) {
            if !found.iter().any(|known| known.id == backend.id) {
                found.push(backend);
            }
        }
    }
    if found.is_empty() {
        found.push(Backend {
            id: DEFAULT_BACKEND_ID.into(),
            display_name: "ChatGPT sign-in".into(),
            default: true,
            live_verified: true,
            credential_field: None,
            settings: Vec::new(),
            replaces_providers: Vec::new(),
        });
        found.extend(
            fields
                .iter()
                .filter_map(|(id, name, _)| derived_backend(id, name))
                .take(MAX_BACKENDS - 1),
        );
    }
    // Exactly one default, and it comes first.
    let default_index = found
        .iter()
        .position(|backend| backend.default)
        .unwrap_or(0);
    for (index, backend) in found.iter_mut().enumerate() {
        backend.default = index == default_index;
    }
    found.swap(0, default_index);
    found
}

/// The backend a project records, or the default when none is recorded or
/// the recorded one is no longer offered.
pub fn chosen_backend<'a>(
    backends: &'a [Backend],
    settings: Option<&BTreeMap<String, String>>,
) -> Option<&'a Backend> {
    settings
        .and_then(|settings| settings.get(BACKEND_SETTING))
        .and_then(|id| backends.iter().find(|backend| &backend.id == id))
        .or_else(|| backends.first())
}

/// How `legacy` maps onto a backend of the catalog's hoi4-imagegen row, if it
/// does. A backend claims a row through `replaces_providers`; without that, a
/// row with one key maps onto the backend whose key is read from the same
/// environment variable, and its stored key carries over.
pub fn replacement_for(providers: &[Value], legacy: &str) -> Option<Replacement> {
    if legacy == PROVIDER_ID || legacy == "none" {
        return None;
    }
    let target = providers
        .iter()
        .find(|row| is_release_binary_provider(row))?;
    let row = providers
        .iter()
        .find(|row| row.get("id").and_then(Value::as_str) == Some(legacy))?;
    if is_release_binary_provider(row) {
        return None;
    }
    let target_fields = credential_fields(target);
    let legacy_fields = credential_fields(row);
    let carry = |backend: &Backend| -> Vec<CredentialMove> {
        match (backend.credential_field.as_ref(), legacy_fields.as_slice()) {
            (Some(to_field), [(from_field, _, _)]) => vec![CredentialMove {
                from_field: from_field.clone(),
                to_field: to_field.clone(),
            }],
            _ => Vec::new(),
        }
    };
    let offered = backends(target);
    if let Some(backend) = offered
        .iter()
        .find(|backend| backend.replaces_providers.iter().any(|id| id == legacy))
    {
        return Some(Replacement {
            backend: backend.id.clone(),
            credential_moves: carry(backend),
        });
    }
    let [(_, _, environment)] = legacy_fields.as_slice() else {
        return None;
    };
    offered
        .iter()
        .find(|backend| {
            backend.credential_field.as_ref().is_some_and(|field| {
                target_fields
                    .iter()
                    .any(|(id, _, name)| id == field && name == environment)
            })
        })
        .map(|backend| Replacement {
            backend: backend.id.clone(),
            credential_moves: carry(backend),
        })
}

/// The catalog as the interface shows it: the hoi4-imagegen row gains its
/// normalized `route.backends`, and every older row that a backend replaces
/// gains `replaced_by`. The package's own rows are otherwise unchanged.
pub fn annotate_catalog(catalog: &Value) -> Value {
    let mut annotated = catalog.clone();
    let Some(rows) = annotated.get_mut("providers").and_then(Value::as_array_mut) else {
        return annotated;
    };
    let snapshot = rows.clone();
    for row in rows.iter_mut() {
        if is_release_binary_provider(row) {
            let listed = backends(row);
            if let Some(route) = row.get_mut("route").and_then(Value::as_object_mut) {
                route.insert("backends".into(), json!(listed));
            }
        } else if let Some(id) = row.get("id").and_then(Value::as_str) {
            if let Some(replacement) = replacement_for(&snapshot, id) {
                if let Some(object) = row.as_object_mut() {
                    object.insert("replaced_by".into(), json!(replacement));
                }
            }
        }
    }
    annotated
}

#[cfg(test)]
mod tests {
    use super::*;

    fn route(extra: Value) -> Value {
        let mut route = json!({
            "kind": "release_binary",
            "credential_fields": [
                {"id": "openai_api_key", "display_name": "OpenAI API key", "environment_name": "OPENAI_API_KEY"},
                {"id": "fal_key", "display_name": "fal key", "environment_name": "FAL_KEY"},
                {"id": "comfyui_api_key", "display_name": "ComfyUI API key", "environment_name": "COMFYUI_API_KEY"}
            ]
        });
        if let (Some(target), Some(extra)) = (route.as_object_mut(), extra.as_object()) {
            target.extend(extra.clone());
        }
        json!({"id": "hoi4_imagegen", "route": route})
    }

    fn legacy(id: &str, field: &str, environment: &str) -> Value {
        json!({"id": id, "route": {"kind": "stdio_adapter", "credential_fields": [
            {"id": field, "display_name": "Key", "environment_name": environment}
        ]}})
    }

    fn ids(offered: &[Backend]) -> Vec<&str> {
        offered.iter().map(|backend| backend.id.as_str()).collect()
    }

    #[test]
    fn a_package_without_a_backend_list_derives_it_from_the_keys() {
        let offered = backends(&route(json!({})));
        assert_eq!(ids(&offered), ["chatgpt_login", "openai", "fal", "comfyui"]);
        assert!(offered[0].default && offered.iter().skip(1).all(|b| !b.default));
        assert_eq!(offered[1].display_name, "OpenAI");
        assert_eq!(
            offered[1].credential_field.as_deref(),
            Some("openai_api_key")
        );
        assert_eq!(offered[0].credential_field, None);
    }

    #[test]
    fn a_published_backend_list_wins_and_only_valid_entries_count() {
        let offered = backends(&route(json!({"backends": [
            {"id": "openai", "display_name": "OpenAI", "credential_field": "openai_api_key"},
            {"id": "chatgpt_login", "display_name": "ChatGPT sign-in", "default": true},
            {"id": "openai", "display_name": "Duplicate"},
            {"id": "Bad Id", "display_name": "Bad"},
            {"id": "ghost", "display_name": "Ghost", "credential_field": "no_such_field"},
            {"id": "comfy", "display_name": "ComfyUI", "credential_field": "comfyui_api_key", "settings": [
                {"key": "providers.comfy.endpoint", "kind": "url", "required": true, "default": "http://127.0.0.1:8188"},
                {"key": "providers.comfy.workflow", "kind": "project_path"},
                {"key": "policy.cost", "kind": "url"},
                {"key": "providers.comfy.other", "kind": "script"}
            ]}
        ]})));
        assert_eq!(ids(&offered), ["chatgpt_login", "openai", "comfy"]);
        assert!(offered[0].default);
        // Only settings under providers.<id> of a supported kind are kept.
        let keys = offered[2]
            .settings
            .iter()
            .map(|s| s.key.as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            keys,
            ["providers.comfy.endpoint", "providers.comfy.workflow"]
        );
        assert!(offered[2].settings[0].required);
        assert_eq!(
            offered[2].settings[0].default.as_deref(),
            Some("http://127.0.0.1:8188")
        );
    }

    fn comfy_row() -> Value {
        route(json!({"backends": [
            {"id": "chatgpt_login", "display_name": "ChatGPT sign-in", "default": true},
            {"id": "openai", "display_name": "OpenAI", "credential_field": "openai_api_key"},
            {"id": "comfyui", "display_name": "ComfyUI", "credential_field": "comfyui_api_key", "settings": [
                {"key": "providers.comfyui.endpoint", "kind": "url", "required": true, "default": "http://127.0.0.1:8188"},
                {"key": "providers.comfyui.workflow", "kind": "project_path", "required": false}
            ]}
        ]}))
    }

    fn chosen(backend: &str, extra: &[(&str, &str)]) -> BTreeMap<String, String> {
        let mut settings = BTreeMap::from([(BACKEND_SETTING.to_string(), backend.to_string())]);
        settings.extend(extra.iter().map(|(k, v)| (k.to_string(), v.to_string())));
        settings
    }

    #[test]
    fn no_recorded_backend_writes_nothing() {
        assert_eq!(
            project_config_content(&comfy_row(), None, None).unwrap(),
            None
        );
        assert_eq!(
            project_config_content(&comfy_row(), Some(&BTreeMap::new()), Some("[x]\n")).unwrap(),
            None
        );
    }

    #[test]
    fn the_chosen_backend_is_set_and_everything_else_in_the_file_is_kept() {
        let existing = "[defaults]\nprovider = \"fal\"\nstyle = \"keep me\"\n\n[profiles.mine]\nsize = 512\n\n[policy]\nmax_cost = 3\n\n[templates]\nicon = \"x\"\n";
        let written =
            project_config_content(&comfy_row(), Some(&chosen("openai", &[])), Some(existing))
                .unwrap()
                .unwrap();
        let table = written.parse::<toml::Table>().unwrap();
        assert_eq!(table["defaults"]["provider"].as_str(), Some("openai"));
        assert_eq!(table["defaults"]["style"].as_str(), Some("keep me"));
        assert_eq!(table["profiles"]["mine"]["size"].as_integer(), Some(512));
        assert_eq!(table["policy"]["max_cost"].as_integer(), Some(3));
        assert_eq!(table["templates"]["icon"].as_str(), Some("x"));
        // Choosing the default again is written explicitly, so a stale earlier
        // choice cannot linger.
        let back = project_config_content(
            &comfy_row(),
            Some(&chosen("chatgpt_login", &[])),
            Some(&written),
        )
        .unwrap()
        .unwrap();
        assert_eq!(
            back.parse::<toml::Table>().unwrap()["defaults"]["provider"].as_str(),
            Some("chatgpt_login")
        );
    }

    #[test]
    fn a_backend_setting_is_written_under_its_provider_table() {
        let written = project_config_content(
            &comfy_row(),
            Some(&chosen(
                "comfyui",
                &[("providers.comfyui.workflow", "workflows/portrait.json")],
            )),
            None,
        )
        .unwrap()
        .unwrap();
        let table = written.parse::<toml::Table>().unwrap();
        assert_eq!(table["defaults"]["provider"].as_str(), Some("comfyui"));
        // The required endpoint falls back to its published default.
        assert_eq!(
            table["providers"]["comfyui"]["endpoint"].as_str(),
            Some("http://127.0.0.1:8188")
        );
        assert_eq!(
            table["providers"]["comfyui"]["workflow"].as_str(),
            Some("workflows/portrait.json")
        );
    }

    #[test]
    fn unsafe_or_unknown_values_are_refused() {
        for (key, value) in [
            ("providers.comfyui.endpoint", "http://192.168.1.4:8188"),
            ("providers.comfyui.endpoint", "https://user:pw@host/"),
            ("providers.comfyui.endpoint", "ftp://host"),
            ("providers.comfyui.workflow", "../escape.json"),
            ("providers.comfyui.workflow", "/etc/passwd.json"),
            ("providers.comfyui.workflow", "C:/x.json"),
            ("providers.comfyui.workflow", "notes.txt"),
        ] {
            assert!(
                project_config_content(
                    &comfy_row(),
                    Some(&chosen("comfyui", &[(key, value)])),
                    None
                )
                .is_err(),
                "{key}={value}"
            );
        }
        assert!(
            project_config_content(&comfy_row(), Some(&chosen("midjourney", &[])), None).is_err()
        );
        assert!(project_config_content(
            &comfy_row(),
            Some(&chosen("openai", &[])),
            Some("not = [toml")
        )
        .is_err());
        assert_eq!(
            setting_value_problem("url", "https://api.example.com/v1"),
            None
        );
        assert_eq!(setting_value_problem("url", "http://localhost:8188"), None);
        assert_eq!(
            setting_value_problem("project_path", "workflows/a.json"),
            None
        );
    }

    fn compatible_row() -> Value {
        route(json!({"credential_fields": [
            {"id": "openai_compatible_api_key", "display_name": "Key", "environment_name": "OPENAI_COMPATIBLE_API_KEY"}
        ], "backends": [
            {"id": "chatgpt_login", "display_name": "ChatGPT sign-in", "default": true},
            {"id": "openai_compatible", "display_name": "OpenAI-compatible", "credential_field": "openai_compatible_api_key", "live_verified": false, "settings": [
                {"key": "providers.openai_compatible.base_url", "kind": "url", "required": true},
                {"key": "providers.openai_compatible.model", "kind": "text", "required": true}
            ]}
        ]}))
    }

    #[test]
    fn a_required_setting_without_a_default_must_be_filled_in() {
        let none = chosen("openai_compatible", &[]);
        assert!(project_config_content(&compatible_row(), Some(&none), None).is_err());
        let only_url = chosen(
            "openai_compatible",
            &[(
                "providers.openai_compatible.base_url",
                "https://api.example.com/v1",
            )],
        );
        assert!(project_config_content(&compatible_row(), Some(&only_url), None).is_err());
        let both = chosen(
            "openai_compatible",
            &[
                (
                    "providers.openai_compatible.base_url",
                    "https://api.example.com/v1",
                ),
                ("providers.openai_compatible.model", "gpt-image-1"),
            ],
        );
        let written = project_config_content(&compatible_row(), Some(&both), None)
            .unwrap()
            .unwrap();
        let table = written.parse::<toml::Table>().unwrap();
        assert_eq!(
            table["providers"]["openai_compatible"]["model"].as_str(),
            Some("gpt-image-1")
        );
        assert_eq!(
            table["defaults"]["provider"].as_str(),
            Some("openai_compatible")
        );
    }

    #[test]
    fn a_text_setting_accepts_a_model_name_and_refuses_anything_else() {
        for ok in ["gpt-image-1", "black-forest-labs/flux-1.1", "model:2025_01"] {
            assert_eq!(setting_value_problem("text", ok), None, "{ok}");
        }
        for bad in [
            "has space",
            "semi;colon",
            "quote\"x",
            "new
line",
            "",
        ] {
            assert!(setting_value_problem("text", bad).is_some(), "{bad:?}");
        }
    }

    #[test]
    fn an_unverified_backend_is_marked_and_the_default_is_not() {
        let offered = backends(&compatible_row());
        assert!(offered[0].live_verified);
        assert!(!offered[1].live_verified);
        // Derived backends have not been verified either.
        let derived = backends(&route(json!({})));
        assert!(derived[0].live_verified && !derived[1].live_verified);
    }

    #[test]
    fn other_rows_offer_no_backends() {
        assert!(backends(&legacy("openai_api", "api_key", "OPENAI_API_KEY")).is_empty());
    }

    #[test]
    fn the_recorded_backend_is_used_and_an_unknown_one_falls_back_to_the_default() {
        let offered = backends(&route(json!({})));
        let mut settings = BTreeMap::from([(BACKEND_SETTING.to_string(), "fal".to_string())]);
        assert_eq!(chosen_backend(&offered, Some(&settings)).unwrap().id, "fal");
        settings.insert(BACKEND_SETTING.into(), "retired".into());
        assert_eq!(
            chosen_backend(&offered, Some(&settings)).unwrap().id,
            "chatgpt_login"
        );
        assert_eq!(chosen_backend(&offered, None).unwrap().id, "chatgpt_login");
    }

    #[test]
    fn an_older_provider_with_the_same_key_variable_maps_onto_a_backend_and_carries_its_key() {
        let rows = vec![
            route(json!({})),
            legacy("openai_api", "api_key", "OPENAI_API_KEY"),
            legacy("replicate", "api_key", "REPLICATE_API_TOKEN"),
            json!({"id": "codex_native", "route": {"kind": "native"}}),
        ];
        let replacement = replacement_for(&rows, "openai_api").unwrap();
        assert_eq!(replacement.backend, "openai");
        assert_eq!(
            replacement.credential_moves,
            vec![CredentialMove {
                from_field: "api_key".into(),
                to_field: "openai_api_key".into()
            }]
        );
        // A provider no backend covers yet keeps working as it is.
        assert_eq!(replacement_for(&rows, "replicate"), None);
        assert_eq!(replacement_for(&rows, "codex_native"), None);
        assert_eq!(replacement_for(&rows, "hoi4_imagegen"), None);
        assert_eq!(replacement_for(&rows, "unknown"), None);
    }

    #[test]
    fn a_backend_can_claim_an_older_provider_that_has_no_key() {
        let rows = vec![
            route(json!({"backends": [
                {"id": "chatgpt_login", "display_name": "ChatGPT sign-in", "default": true, "replaces_providers": ["codex_native"]},
                {"id": "comfyui", "display_name": "ComfyUI", "credential_field": "comfyui_api_key", "replaces_providers": ["comfyui_local"]}
            ]})),
            json!({"id": "codex_native", "route": {"kind": "native"}}),
            json!({"id": "comfyui_local", "route": {"kind": "stdio_adapter"}}),
        ];
        let codex = replacement_for(&rows, "codex_native").unwrap();
        assert_eq!(codex.backend, "chatgpt_login");
        assert!(codex.credential_moves.is_empty());
        let comfy = replacement_for(&rows, "comfyui_local").unwrap();
        assert_eq!(comfy.backend, "comfyui");
        assert!(comfy.credential_moves.is_empty());
    }

    #[test]
    fn the_annotated_catalog_adds_backends_and_replacements_and_changes_nothing_else() {
        let catalog = json!({"default_provider": "hoi4_imagegen", "providers": [
            route(json!({})),
            legacy("openai_api", "api_key", "OPENAI_API_KEY"),
            legacy("replicate", "api_key", "REPLICATE_API_TOKEN"),
        ]});
        let annotated = annotate_catalog(&catalog);
        assert_eq!(
            annotated.pointer("/providers/0/route/backends/1/id"),
            Some(&json!("openai"))
        );
        assert_eq!(
            annotated.pointer("/providers/1/replaced_by/backend"),
            Some(&json!("openai"))
        );
        assert_eq!(annotated.pointer("/providers/2/replaced_by"), None);
        assert_eq!(
            annotated.pointer("/default_provider"),
            catalog.pointer("/default_provider")
        );
        assert_eq!(
            annotated.pointer("/providers/2"),
            catalog.pointer("/providers/2")
        );
    }
}
