//! Performance budgets for the portable core on small, medium, and large
//! synthetic mods. The budgets are generous for unoptimized test builds on
//! hosted runners; a regression that makes profiling or export grow faster
//! than linearly fails here.

use hoi4_setup_core::memory_fs::MemoryFs;
use hoi4_setup_core::profile::profile;
use hoi4_setup_core::zip::{read_zip, write_zip, ZipEntry};
use std::time::{Duration, Instant};

/// A mod shaped like a real one: events, focus trees, localisation, ideas,
/// history, and graphics, spread over nested folders.
fn synthetic_mod(files: usize) -> MemoryFs {
    let mut fs = MemoryFs::new();
    fs.add_file(
        "descriptor.mod",
        b"name=\"Synthetic\"\ntags={ \"Events\" }\n".to_vec(),
    )
    .unwrap();
    for index in 0..files {
        let (path, text) = match index % 6 {
            0 => (
                format!("events/chain_{}/event_{index}.txt", index % 40),
                format!("add_namespace = syn_{}\ncountry_event = {{ id = syn_{}.{index} }}\n", index % 40, index % 40),
            ),
            1 => (
                format!("common/national_focus/tree_{index}.txt"),
                format!("focus_tree = {{ id = syn_tree_{index} focus = {{ id = SYN_focus_{index} }} }}\n"),
            ),
            2 => (
                format!("localisation/english/syn_{index}_l_english.yml"),
                format!("\u{feff}l_english:\n syn_{index}.t:0 \"Title {index}\"\n"),
            ),
            3 => (
                format!("common/ideas/syn_ideas_{index}.txt"),
                format!("ideas = {{ country = {{ SYN_idea_{index} = {{ }} }} }}\n"),
            ),
            4 => (
                format!("history/states/{index}-State.txt"),
                format!("state = {{ id = {index} name = \"STATE_{index}\" }}\n"),
            ),
            _ => (format!("gfx/interface/goals/syn_{index}.dds"), "DDS ".repeat(64)),
        };
        fs.add_file(&path, text.into_bytes()).unwrap();
    }
    fs
}

fn time<T>(work: impl FnOnce() -> T) -> (T, Duration) {
    let started = Instant::now();
    let value = work();
    (value, started.elapsed())
}

fn assert_profile_budget(files: usize, budget: Duration) {
    let fs = synthetic_mod(files);
    let (result, elapsed) = time(|| profile(&fs, &|| false, &mut |_, _| {}).unwrap());
    assert!(
        elapsed <= budget,
        "profiling {files} files took {elapsed:?}, over the {budget:?} budget"
    );
    assert!(result.stats.files_indexed > 0);
    assert!(!result.findings.is_empty());
}

#[test]
fn profiling_a_small_mod_stays_within_budget() {
    assert_profile_budget(120, Duration::from_secs(2));
}

#[test]
fn profiling_a_medium_mod_stays_within_budget() {
    assert_profile_budget(6_000, Duration::from_secs(10));
}

#[test]
fn profiling_a_large_mod_stays_within_budget() {
    // Kaiserreich-sized trees hold tens of thousands of files.
    assert_profile_budget(40_000, Duration::from_secs(45));
}

#[test]
fn a_zip_of_a_medium_mod_is_written_and_read_within_budget() {
    let entries = (0..6_000)
        .map(|index| ZipEntry {
            path: format!("syn/common/file_{index}.txt"),
            bytes: format!("value_{index} = {{ amount = {index} }}\n")
                .repeat(20)
                .into_bytes(),
        })
        .collect::<Vec<_>>();
    let (archive, write_elapsed) = time(|| write_zip(&entries).unwrap());
    let (read, read_elapsed) = time(|| read_zip(&archive).unwrap());
    assert_eq!(read.len(), entries.len());
    assert!(
        write_elapsed <= Duration::from_secs(20),
        "writing took {write_elapsed:?}"
    );
    assert!(
        read_elapsed <= Duration::from_secs(20),
        "reading took {read_elapsed:?}"
    );
}
