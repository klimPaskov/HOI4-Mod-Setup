//! Golden projects: small mods with known conventions. Each one is profiled
//! through the same core the desktop and web edition use, and every value the
//! files decide must match the expectation exactly. A change that loses or
//! alters a detected value fails here before it reaches a user.

use hoi4_setup_core::descriptors::parse_descriptor;
use hoi4_setup_core::memory_fs::MemoryFs;
use hoi4_setup_core::models::{ScanEvidence, ScanFinding};
use hoi4_setup_core::proposals::detect;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::Path;

fn load(root: &Path) -> MemoryFs {
    fn visit(fs: &mut MemoryFs, root: &Path, directory: &Path) {
        let mut entries = std::fs::read_dir(directory)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .collect::<Vec<_>>();
        entries.sort();
        for path in entries {
            if path.is_dir() {
                visit(fs, root, &path);
            } else {
                let relative = path
                    .strip_prefix(root)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/");
                fs.add_file(&relative, std::fs::read(&path).unwrap())
                    .unwrap();
            }
        }
    }
    let mut fs = MemoryFs::new();
    visit(&mut fs, root, root);
    fs
}

fn descriptor_finding(id: &str, value: Value) -> ScanFinding {
    ScanFinding {
        id: id.into(),
        category: "descriptor".into(),
        key: id.into(),
        value,
        model_excerpt: None,
        status: "accepted".into(),
        origin: "deterministic".into(),
        user_value: None,
        confidence: 1.0,
        blocking: false,
        proposed_action: None,
        decision_state: "accepted".into(),
        evidence: Vec::<ScanEvidence>::new(),
        recommendation: None,
    }
}

fn detected_for(name: &str) -> BTreeMap<String, Value> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden")
        .join(name);
    let fs = load(&root);
    let mut findings = hoi4_setup_core::profile::profile(&fs, &|| false, &mut |_, _| {})
        .unwrap()
        .findings;
    let descriptor =
        parse_descriptor(&std::fs::read(root.join("descriptor.mod")).unwrap()).unwrap();
    if let Some(name) = descriptor.fields.get("name") {
        findings.push(descriptor_finding("descriptor.name", json!(name)));
    }
    if let Some(tags) = descriptor.fields.get("tags") {
        findings.push(descriptor_finding(
            "descriptor.metadata",
            json!({ "tags": tags }),
        ));
    }
    detect(&findings)
        .into_iter()
        .map(|value| (value.key, value.value))
        .collect()
}

fn expect(name: &str, expected: Value) {
    let detected = serde_json::to_value(detected_for(name)).unwrap();
    assert_eq!(detected, expected, "{name}");
}

#[test]
fn focus_tree_mod() {
    expect(
        "focus-tree-mod",
        json!({
            "display_name": "Focus Tree Mod",
            "descriptor_tags": ["National Focuses", "Gameplay"],
            "localisation_convention": "english; UTF-8 with BOM; versioned keys such as key:0",
            "folder_profile": ["common/national_focus", "localisation"]
        }),
    );
}

#[test]
fn event_chain_mod() {
    expect(
        "event-chain-mod",
        json!({
            "display_name": "Event Chain Mod",
            "descriptor_tags": ["Events", "Alternative History"],
            "primary_namespace": "ecm_crisis",
            "script_prefix": "ecm_crisis",
            "localisation_convention": "english, german; UTF-8 with BOM; versioned keys such as key:0",
            "folder_profile": ["common/decisions", "events", "localisation"]
        }),
    );
}

#[test]
fn documented_mod() {
    expect(
        "documented-mod",
        json!({
            "display_name": "Documented Mod",
            "folder_profile": ["history/countries"],
            "documentation_convention": "Markdown documentation in docs/"
        }),
    );
}
