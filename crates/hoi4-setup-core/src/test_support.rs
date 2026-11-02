//! Manifest fixtures for tests of the release-binary route. Compiled only for
//! tests and the `test-support` feature.

use crate::models::RemoteManifest;
use serde_json::{json, Value};

const BUNDLED: &[u8] = include_bytes!("../../../docs/source-manifest/hoi4-mod-setup.manifest.json");
const ROUTE: &str = include_str!("../tests/fixtures/hoi4-imagegen-route.json");

/// The bundled source manifest with the retired GPTImage provider and
/// component replaced by the hoi4-imagegen provider and component, whose
/// `targets` are the given release targets, keyed by triple, at `version`.
pub fn manifest_value_with_hoi4_imagegen(version: &str, targets: Value) -> Value {
    let mut manifest: Value = serde_json::from_slice(BUNDLED).expect("bundled manifest");
    let route: Value = serde_json::from_str(ROUTE).expect("route fixture");
    let mut provider = route["provider"].clone();
    provider["route"]["release_binary"]["targets"] = targets;
    provider["route"]["release_binary"]["version"] = json!(version);
    provider["route"]["package_version"] = json!(version);
    provider["reference_mcp"]["version"] = json!(version);
    let catalog = manifest["image_generation"].as_object_mut().unwrap();
    let providers = catalog["providers"].as_array_mut().unwrap();
    providers.retain(|row| row["id"] != "gptimage");
    providers.insert(0, provider);
    catalog.insert("default_provider".into(), json!("hoi4_imagegen"));
    let components = manifest["components"].as_array_mut().unwrap();
    components.retain(|component| component["id"] != "imagegen.gptimage");
    components.push(route["component"].clone());
    manifest
}

/// [`manifest_value_with_hoi4_imagegen`], parsed and validated.
pub fn manifest_with_hoi4_imagegen(version: &str, targets: Value) -> RemoteManifest {
    let value = manifest_value_with_hoi4_imagegen(version, targets);
    crate::manifest::parse_manifest(&serde_json::to_vec(&value).unwrap(), None)
        .expect("fixture manifest is valid")
}

/// Release targets for every platform, with synthetic digests. Use only where
/// no archive is opened.
pub fn synthetic_targets(version: &str) -> Value {
    let mut targets = serde_json::Map::new();
    for triple in crate::release_binary::TARGETS {
        let windows = triple.contains("windows");
        let (archive, kind) = if windows {
            ("zip", crate::release_binary::ArchiveKind::Zip)
        } else {
            ("tar.gz", crate::release_binary::ArchiveKind::TarGz)
        };
        let suffix = if windows { ".exe" } else { "" };
        let folder = format!("hoi4-imagegen-v{version}-{triple}");
        targets.insert(
            triple.into(),
            json!({
                "url": crate::release_binary::expected_url(version, triple, kind),
                "sha256": "b".repeat(64),
                "size": 4096,
                "archive": archive,
                "entries": {
                    "mcp": format!("{folder}/hoi4-imagegen-mcp{suffix}"),
                    "cli": format!("{folder}/hoi4-imagegen{suffix}"),
                }
            }),
        );
    }
    Value::Object(targets)
}
