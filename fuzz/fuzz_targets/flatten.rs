#![no_main]

use hoi4_mod_setup::flatten::build_artifacts;
use hoi4_mod_setup::models::PreparedFile;
use libfuzzer_sys::fuzz_target;
use std::path::Path;

fn fuzz_skill_name(bytes: &[u8]) -> String {
    let mut name = bytes
        .iter()
        .take(32)
        .map(|byte| char::from(b'a' + (byte % 26)))
        .collect::<String>();
    if name.is_empty() {
        name.push_str("fuzz_skill");
    }
    name
}

fuzz_target!(|bytes: &[u8]| {
    let skill_name = fuzz_skill_name(bytes);
    let arbitrary_destination = String::from_utf8_lossy(bytes).into_owned();
    let prepared = vec![
        PreparedFile {
            operation_id: "fuzz-agents".into(),
            destination: "AGENTS.md".into(),
            bytes: b"# adapted instructions\n".to_vec(),
            expected_sha256: String::new(),
        },
        PreparedFile {
            operation_id: "fuzz-readme".into(),
            destination: "README.md".into(),
            bytes: b"# project\n".to_vec(),
            expected_sha256: String::new(),
        },
        PreparedFile {
            operation_id: "fuzz-skill".into(),
            destination: format!(".agents/skills/{skill_name}/SKILL.md"),
            bytes: bytes.to_vec(),
            expected_sha256: String::new(),
        },
        PreparedFile {
            operation_id: "fuzz-subagent".into(),
            destination: ".codex/agents/worker.toml".into(),
            bytes: b"name = \"worker\"\ndescription = \"fuzz\"\ndeveloper_instructions = \"fork_context=false\"\n".to_vec(),
            expected_sha256: String::new(),
        },
        PreparedFile {
            operation_id: "fuzz-arbitrary".into(),
            destination: arbitrary_destination,
            bytes: bytes.to_vec(),
            expected_sha256: String::new(),
        },
    ];
    let _ = build_artifacts(
        &prepared,
        &[],
        Path::new("__hoi4_mod_setup_fuzz_project_does_not_exist__"),
    );
});
