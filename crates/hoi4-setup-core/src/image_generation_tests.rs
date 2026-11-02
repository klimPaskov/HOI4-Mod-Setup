//! Tests of the hoi4-imagegen release-binary route in client files and in
//! provider selection.

use crate::image_generation::*;
use crate::models::{CodingEnvironmentSelection, ImageGenerationSelection, RemoteManifest};
use crate::test_support::{manifest_with_hoi4_imagegen, synthetic_targets};
use serde_json::{json, Value};

const CLIENTS: [(&str, &str); 4] = [
    ("claude_code", ".mcp.json"),
    ("github_copilot", ".vscode/mcp.json"),
    ("opencode", "opencode.json"),
    ("codex", ".codex/config.toml"),
];

fn selection(primary: &str) -> ImageGenerationSelection {
    ImageGenerationSelection {
        primary_provider: primary.into(),
        ..Default::default()
    }
}

fn environments(primary: &str) -> CodingEnvironmentSelection {
    CodingEnvironmentSelection {
        primary: primary.into(),
        additional: Vec::new(),
    }
}

/// A client file as an earlier install left it: a user's own server, a
/// managed GPTImage entry and an unrelated key.
fn existing_file(path: &str) -> Vec<u8> {
    let managed = json!({"command": "node", "args": [".tools/image_generation/providers/gptimage/src/server.js"], "cwd": "."});
    let user = json!({"command": "my-own-server", "args": ["--keep"]});
    match path {
        ".mcp.json" => serde_json::to_vec(&json!({
            "mcpServers": {"hoi4_image_gptimage": managed, "my_server": user},
            "other": 1
        }))
        .unwrap(),
        ".vscode/mcp.json" => serde_json::to_vec(&json!({
            "servers": {"hoi4_image_gptimage": managed, "my_server": user},
            "inputs": []
        }))
        .unwrap(),
        "opencode.json" => serde_json::to_vec(&json!({
            "$schema": "kept",
            "mcp": {"hoi4_image_gptimage": managed, "my_server": {"type": "local", "command": ["my-own-server"]}}
        }))
        .unwrap(),
        _ => b"model = \"kept\"\n[mcp_servers.hoi4_image_gptimage]\ncommand = \"node\"\n[mcp_servers.my_server]\ncommand = \"my-own-server\"\nargs = [\"--keep\"]\n".to_vec(),
    }
}

fn entries(path: &str, bytes: &[u8]) -> serde_json::Map<String, Value> {
    if path.ends_with(".toml") {
        let value: toml::Value = std::str::from_utf8(bytes).unwrap().parse().unwrap();
        return serde_json::to_value(value["mcp_servers"].clone())
            .unwrap()
            .as_object()
            .unwrap()
            .clone();
    }
    let value: Value = serde_json::from_slice(bytes).unwrap();
    let key = match path {
        ".vscode/mcp.json" => "servers",
        "opencode.json" => "mcp",
        _ => "mcpServers",
    };
    value[key].as_object().unwrap().clone()
}

fn adapt(
    bytes: &[u8],
    path: &str,
    environment: &str,
    manifest: &RemoteManifest,
    selection: &ImageGenerationSelection,
    windows: bool,
) -> Vec<u8> {
    adapt_image_only_client_config_for(
        bytes,
        path,
        manifest,
        selection,
        &environments(environment),
        windows,
    )
    .unwrap()
}

#[test]
fn the_hoi4_imagegen_route_writes_every_client_format_and_replaces_gptimage() {
    let manifest = manifest_with_hoi4_imagegen("0.2.0", synthetic_targets("0.2.0"));
    let selected = selection("hoi4_imagegen");
    for (environment, path) in CLIENTS {
        for windows in [true, false] {
            let adapted = adapt(
                &existing_file(path),
                path,
                environment,
                &manifest,
                &selected,
                windows,
            );
            let servers = entries(path, &adapted);
            assert!(
                !servers.contains_key("hoi4_image_gptimage"),
                "{path}: the managed GPTImage entry is replaced"
            );
            assert!(servers.contains_key("my_server"), "{path}: user entry kept");
            let route = &servers["hoi4_imagegen"];
            let command = format!(
                ".tools/image_generation/hoi4-imagegen/0.2.0/hoi4-imagegen-mcp{}",
                if windows { ".exe" } else { "" }
            );
            if path == "opencode.json" {
                assert_eq!(route["type"], "local");
                assert_eq!(route["command"], json!([command, "--project-root", "."]));
            } else {
                assert_eq!(route["command"], command, "{path}");
                assert_eq!(route["args"], json!(["--project-root", "."]), "{path}");
            }
            match path {
                ".vscode/mcp.json" => {
                    assert_eq!(route["type"], "stdio");
                    assert_eq!(route["cwd"], "${workspaceFolder}");
                }
                ".codex/config.toml" => {
                    assert_eq!(route["cwd"], ".");
                    assert_eq!(route["enabled"], true);
                }
                "opencode.json" => assert_eq!(route["cwd"], "."),
                _ => assert_eq!(route["cwd"], "."),
            }
            let text = String::from_utf8(adapted.clone()).unwrap();
            match path {
                ".mcp.json" => assert!(text.contains("\"other\": 1")),
                ".vscode/mcp.json" => assert!(text.contains("\"inputs\"")),
                "opencode.json" => assert!(text.contains("\"$schema\": \"kept\"")),
                _ => assert!(text.contains("model = \"kept\"")),
            }
            let again = adapt(&adapted, path, environment, &manifest, &selected, windows);
            assert_eq!(entries(path, &again), servers, "{path}: idempotent");
        }
    }
}

#[test]
fn an_update_retargets_the_managed_entry_and_keeps_user_entries() {
    let first = manifest_with_hoi4_imagegen("0.2.0", synthetic_targets("0.2.0"));
    let second = manifest_with_hoi4_imagegen("0.3.0", synthetic_targets("0.3.0"));
    let selected = selection("hoi4_imagegen");
    for (environment, path) in CLIENTS {
        let installed = adapt(
            &existing_file(path),
            path,
            environment,
            &first,
            &selected,
            false,
        );
        let updated = adapt(&installed, path, environment, &second, &selected, false);
        let text = String::from_utf8(updated.clone()).unwrap();
        assert!(
            text.contains("hoi4-imagegen/0.3.0/hoi4-imagegen-mcp"),
            "{path}"
        );
        assert!(!text.contains("/0.2.0/"), "{path}: the old version is gone");
        assert!(entries(path, &updated).contains_key("my_server"), "{path}");
    }
}

#[test]
fn deselecting_the_provider_removes_only_the_managed_entries() {
    let manifest = manifest_with_hoi4_imagegen("0.2.0", synthetic_targets("0.2.0"));
    for (environment, path) in CLIENTS {
        let installed = adapt(
            &existing_file(path),
            path,
            environment,
            &manifest,
            &selection("hoi4_imagegen"),
            false,
        );
        let removed = adapt(
            &installed,
            path,
            environment,
            &manifest,
            &selection("none"),
            false,
        );
        let servers = entries(path, &removed);
        assert!(!servers.contains_key("hoi4_imagegen"), "{path}");
        assert!(servers.contains_key("my_server"), "{path}");
    }
}

#[test]
fn an_older_manifest_never_installs_or_registers_gptimage() {
    let bundled: RemoteManifest = serde_json::from_slice(include_bytes!(
        "../../../docs/source-manifest/hoi4-mod-setup.manifest.json"
    ))
    .unwrap();
    let old = selection("gptimage");
    assert!(
        validate_image_generation_selection(&old, &bundled, &environments("claude_code")).is_err()
    );
    let (replaced, note) = replace_retired_provider(&old, &bundled);
    assert_eq!(replaced.primary_provider, "none");
    assert!(note.unwrap().contains("retired"));
    let adapted = adapt(
        &existing_file(".mcp.json"),
        ".mcp.json",
        "claude_code",
        &bundled,
        &replaced,
        false,
    );
    let servers = entries(".mcp.json", &adapted);
    assert!(!servers.contains_key("hoi4_image_gptimage"));
    assert!(servers.contains_key("my_server"));
    let provider = bundled.image_generation.as_ref().unwrap()["providers"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["id"] == "gptimage")
        .unwrap();
    assert!(image_server_route_for(provider, false).is_none());
}

#[test]
fn a_saved_gptimage_choice_becomes_hoi4_imagegen_when_the_manifest_offers_it() {
    let manifest = manifest_with_hoi4_imagegen("0.2.0", synthetic_targets("0.2.0"));
    let mut saved = selection("gptimage");
    saved.additional_providers = vec!["openai_api".into()];
    saved
        .credential_references
        .insert("gptimage.api_key".into(), "credential://x".into());
    let (replaced, note) = replace_retired_provider(&saved, &manifest);
    assert_eq!(replaced.primary_provider, "hoi4_imagegen");
    assert_eq!(replaced.additional_providers, vec!["openai_api"]);
    assert!(replaced.credential_references.is_empty());
    assert!(note.unwrap().contains("hoi4-imagegen"));
    validate_image_generation_selection(&replaced, &manifest, &environments("claude_code"))
        .unwrap();
    let mut extra = selection("hoi4_imagegen");
    extra.additional_providers = vec!["gptimage".into()];
    let (replaced, _) = replace_retired_provider(&extra, &manifest);
    assert!(replaced.additional_providers.is_empty());
    let (same, note) = replace_retired_provider(&selection("openai_api"), &manifest);
    assert_eq!(same, selection("openai_api"));
    assert!(note.is_none());
}

#[test]
fn the_hoi4_imagegen_selection_needs_a_client_that_supports_it() {
    let manifest = manifest_with_hoi4_imagegen("0.2.0", synthetic_targets("0.2.0"));
    validate_image_generation_selection(
        &selection("hoi4_imagegen"),
        &manifest,
        &environments("claude_code"),
    )
    .unwrap();
    assert!(validate_image_generation_selection(
        &selection("hoi4_imagegen"),
        &manifest,
        &environments("kilo_code"),
    )
    .is_err());
}

#[test]
fn a_manifest_with_a_malformed_release_route_is_refused() {
    let mut value =
        crate::test_support::manifest_value_with_hoi4_imagegen("0.2.0", synthetic_targets("0.2.0"));
    value["image_generation"]["providers"][0]["route"]["release_binary"]["targets"]
        ["x86_64-pc-windows-msvc"]["url"] =
        json!("https://github.com/klimPaskov/hoi4-imagegen-mcp/releases/download/v0.2.0/other.zip");
    assert!(crate::manifest::parse_manifest(&serde_json::to_vec(&value).unwrap(), None).is_err());
}
