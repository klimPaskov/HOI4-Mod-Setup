//! Finding the Python the setup runs its tools with.
//!
//! A person says "Python is installed and on PATH" and means any of several
//! things: a classic installer folder, the Python install manager (Python 3.14
//! on Windows puts only app-execution aliases on PATH, and the interpreter in
//! its own folder), a framework install, Homebrew. Searching PATH alone and
//! rejecting every link therefore reported "not found" for working installs.
//! This module lists the real interpreters in the places installers use and
//! says why it skipped anything it saw, so the message can name it. It only
//! finds candidates; whether one is trusted is decided by the caller.

use crate::models::Platform;
use std::fs;
use std::path::{Path, PathBuf};

/// Where to look, passed in so the search can be tested.
#[derive(Debug, Clone, Default)]
pub struct Roots {
    pub path_dirs: Vec<PathBuf>,
    pub local_app_data: Option<PathBuf>,
    pub program_files: Vec<PathBuf>,
    pub system_drive_root: Option<PathBuf>,
}

impl Roots {
    pub fn from_environment() -> Self {
        let var = |name: &str| std::env::var_os(name).map(PathBuf::from);
        Self {
            path_dirs: std::env::var_os("PATH")
                .map(|path| std::env::split_paths(&path).collect())
                .unwrap_or_default(),
            local_app_data: var("LOCALAPPDATA"),
            program_files: ["ProgramFiles", "ProgramFiles(x86)", "ProgramW6432"]
                .into_iter()
                .filter_map(var)
                .collect(),
            system_drive_root: var("SystemDrive").map(|drive| {
                PathBuf::from(format!(
                    "{}\\",
                    drive.to_string_lossy().trim_end_matches(['\\', '/'])
                ))
            }),
        }
    }
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Search {
    /// Real interpreters, in the order they should be tried.
    pub candidates: Vec<PathBuf>,
    /// What was seen and skipped, in plain words.
    pub notes: Vec<String>,
}

/// `(major, minor)` from a path such as `Python312`, `pythoncore-3.14-64`,
/// `Versions/3.12/bin/python3`.
pub fn version_hint(path: &Path) -> Option<(u32, u32)> {
    let text = path.to_string_lossy().to_ascii_lowercase();
    let bytes = text.as_bytes();
    let mut best: Option<(u32, u32)> = None;
    let mut index = 0;
    while index + 1 < bytes.len() {
        if bytes[index] == b'3' && (index == 0 || !bytes[index - 1].is_ascii_digit()) {
            let rest = &text[index + 1..];
            let digits: String = rest
                .trim_start_matches(['.', '_'])
                .chars()
                .take_while(char::is_ascii_digit)
                .collect();
            if let Ok(minor) = digits.parse::<u32>() {
                if (1..=99).contains(&minor) {
                    best = best.max(Some((3, minor)));
                }
            }
        }
        index += 1;
    }
    best
}

fn is_app_alias(path: &Path) -> bool {
    path.to_string_lossy()
        .to_ascii_lowercase()
        .contains("windowsapps")
}

fn executable_names(platform: Platform) -> &'static [&'static str] {
    if platform == Platform::Windows {
        &["python.exe", "python3.exe"]
    } else {
        &["python3", "python"]
    }
}

fn child_dirs(parent: &Path, prefix: &str) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(parent) else {
        return Vec::new();
    };
    entries
        .filter_map(Result::ok)
        .filter(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .to_ascii_lowercase()
                .starts_with(prefix)
        })
        .map(|entry| entry.path())
        .collect()
}

/// Where each platform's installers put an interpreter that is not on PATH.
fn known_locations(roots: &Roots, platform: Platform) -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> = Vec::new();
    match platform {
        Platform::Windows => {
            if let Some(local) = &roots.local_app_data {
                // The Python install manager (the default download for 3.14).
                for dir in child_dirs(&local.join("Python"), "pythoncore-") {
                    found.push(dir.join("python.exe"));
                }
                // The classic per-user installer.
                for dir in child_dirs(&local.join("Programs").join("Python"), "python3") {
                    found.push(dir.join("python.exe"));
                }
            }
            for base in roots
                .program_files
                .iter()
                .chain(roots.system_drive_root.iter())
            {
                for dir in child_dirs(base, "python3") {
                    found.push(dir.join("python.exe"));
                }
            }
        }
        Platform::Macos => {
            let framework = Path::new("/Library/Frameworks/Python.framework/Versions");
            for dir in child_dirs(framework, "3") {
                found.push(dir.join("bin").join("python3"));
            }
            found.push(PathBuf::from("/opt/homebrew/bin/python3"));
            found.push(PathBuf::from("/usr/local/bin/python3"));
        }
        _ => {
            found.push(PathBuf::from("/usr/bin/python3"));
            found.push(PathBuf::from("/usr/local/bin/python3"));
        }
    }
    // Newest first inside this group.
    found.sort_by_key(|path| std::cmp::Reverse(version_hint(path)));
    found
}

pub fn search(roots: &Roots, platform: Platform) -> Search {
    let mut result = Search::default();
    let consider = |path: PathBuf, from_path_dir: bool, result: &mut Search| {
        let Ok(metadata) = fs::symlink_metadata(&path) else {
            return;
        };
        let linked = crate::security::is_link_metadata(&metadata);
        if linked && platform != Platform::Linux {
            if from_path_dir {
                let note = if is_app_alias(&path) {
                    format!(
                        "{} is a Windows app alias (a shortcut), not a Python installation.",
                        path.display()
                    )
                } else {
                    format!(
                        "{} is a link, which the setup does not run.",
                        path.display()
                    )
                };
                if !result.notes.contains(&note) {
                    result.notes.push(note);
                }
            }
            return;
        }
        if !linked && !metadata.is_file() {
            return;
        }
        let Ok(canonical) = fs::canonicalize(&path) else {
            return;
        };
        if crate::security::path_has_link_component(&canonical) && platform != Platform::Linux {
            return;
        }
        if !result.candidates.contains(&canonical) {
            result.candidates.push(canonical);
        }
    };
    for directory in &roots.path_dirs {
        if !directory.is_absolute() {
            continue;
        }
        for name in executable_names(platform) {
            consider(directory.join(name), true, &mut result);
        }
    }
    for path in known_locations(roots, platform) {
        consider(path, false, &mut result);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn touch(path: &Path) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, b"x").unwrap();
    }

    #[test]
    fn versions_are_read_from_the_folder_names_installers_use() {
        let cases = [
            (
                "C:/Users/x/AppData/Local/Python/pythoncore-3.14-64/python.exe",
                (3, 14),
            ),
            (
                "C:/Users/x/AppData/Local/Programs/Python/Python312/python.exe",
                (3, 12),
            ),
            ("C:/Program Files/Python39/python.exe", (3, 9)),
            (
                "/Library/Frameworks/Python.framework/Versions/3.13/bin/python3",
                (3, 13),
            ),
        ];
        for (path, expected) in cases {
            assert_eq!(version_hint(Path::new(path)), Some(expected), "{path}");
        }
        assert_eq!(version_hint(Path::new("/opt/homebrew/bin/python")), None);
    }

    #[test]
    fn the_python_install_managers_folder_is_found_when_path_has_only_aliases() {
        let local = tempfile::tempdir().unwrap();
        let real = local.path().join("Python/pythoncore-3.14-64/python.exe");
        touch(&real);
        let roots = Roots {
            local_app_data: Some(local.path().to_path_buf()),
            ..Default::default()
        };
        let search = search(&roots, Platform::Windows);
        assert_eq!(search.candidates, [fs::canonicalize(&real).unwrap()]);
    }

    #[test]
    fn real_files_on_path_come_first_and_known_folders_are_newest_first() {
        let root = tempfile::tempdir().unwrap();
        let on_path = root.path().join("tools/python.exe");
        touch(&on_path);
        let local = root.path().join("local");
        touch(&local.join("Programs/Python/Python311/python.exe"));
        touch(&local.join("Programs/Python/Python313/python.exe"));
        touch(&local.join("Programs/Python/Python39/python.exe"));
        let roots = Roots {
            path_dirs: vec![root.path().join("tools"), PathBuf::from("relative/dir")],
            local_app_data: Some(local),
            ..Default::default()
        };
        let found = search(&roots, Platform::Windows).candidates;
        let hints: Vec<_> = found.iter().map(|path| version_hint(path)).collect();
        assert_eq!(found.len(), 4);
        assert_eq!(found[0], fs::canonicalize(&on_path).unwrap());
        assert_eq!(&hints[1..], [Some((3, 13)), Some((3, 11)), Some((3, 9))]);
    }

    #[test]
    fn the_same_interpreter_is_listed_once() {
        let local = tempfile::tempdir().unwrap();
        let real = local.path().join("Programs/Python/Python312/python.exe");
        touch(&real);
        let roots = Roots {
            path_dirs: vec![real.parent().unwrap().to_path_buf()],
            local_app_data: Some(local.path().to_path_buf()),
            ..Default::default()
        };
        assert_eq!(search(&roots, Platform::Windows).candidates.len(), 1);
    }

    #[cfg(unix)]
    #[test]
    fn a_link_on_path_is_explained_on_macos_and_followed_on_linux() {
        let root = tempfile::tempdir().unwrap();
        let real = root.path().join("real/python3");
        touch(&real);
        let dir = root.path().join("bin");
        fs::create_dir_all(&dir).unwrap();
        std::os::unix::fs::symlink(&real, dir.join("python3")).unwrap();
        let roots = Roots {
            path_dirs: vec![dir],
            ..Default::default()
        };
        let mac = search(&roots, Platform::Macos);
        assert!(mac.candidates.is_empty());
        assert!(mac.notes.iter().any(|note| note.contains("is a link")));
        let linux = search(&roots, Platform::Linux);
        assert_eq!(linux.candidates, [fs::canonicalize(&real).unwrap()]);
    }
}
