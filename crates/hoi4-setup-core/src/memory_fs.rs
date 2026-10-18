//! A project tree held in memory: a folder dropped into the web edition, or a
//! fake project in tests.

use crate::adapters::{EntryKind, FsEntry, ProjectFs};
use crate::text::normalize_relative_path;
use crate::AppError;
use std::collections::{BTreeMap, BTreeSet};

/// Files by normalized project-relative path. Directories are implied by the
/// files below them, and an empty directory can be added explicitly.
#[derive(Debug, Default, Clone)]
pub struct MemoryFs {
    files: BTreeMap<String, Vec<u8>>,
    directories: BTreeSet<String>,
}

impl MemoryFs {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a file. The path must be a safe project-relative path, and a path
    /// already used by a file or directory is refused.
    pub fn add_file(&mut self, path: &str, bytes: Vec<u8>) -> Result<(), AppError> {
        let path = normalize_relative_path(path)?;
        if self.directories.contains(&path) || self.files.contains_key(&path) {
            return Err(AppError::PathSecurity(format!(
                "duplicate project path: {path}"
            )));
        }
        let mut parent = path.as_str();
        while let Some((directory, _)) = parent.rsplit_once('/') {
            if self.files.contains_key(directory) {
                return Err(AppError::PathSecurity(format!(
                    "a file is used as a folder: {directory}"
                )));
            }
            self.directories.insert(directory.to_owned());
            parent = directory;
        }
        self.files.insert(path, bytes);
        Ok(())
    }

    pub fn add_directory(&mut self, path: &str) -> Result<(), AppError> {
        let path = normalize_relative_path(path)?;
        if self.files.contains_key(&path) {
            return Err(AppError::PathSecurity(format!(
                "duplicate project path: {path}"
            )));
        }
        let mut current = path.as_str();
        loop {
            self.directories.insert(current.to_owned());
            match current.rsplit_once('/') {
                Some((parent, _)) => current = parent,
                None => break,
            }
        }
        Ok(())
    }

    pub fn file_count(&self) -> usize {
        self.files.len()
    }

    pub fn total_bytes(&self) -> u64 {
        self.files.values().map(|bytes| bytes.len() as u64).sum()
    }
}

fn parent_of(path: &str) -> &str {
    path.rsplit_once('/').map_or("", |(parent, _)| parent)
}

impl ProjectFs for MemoryFs {
    fn list(&self, directory: &str) -> Result<Vec<FsEntry>, AppError> {
        let directory = if directory.is_empty() {
            String::new()
        } else {
            let normalized = normalize_relative_path(directory)?;
            if !self.directories.contains(&normalized) {
                return Err(AppError::Scan(format!("folder not found: {normalized}")));
            }
            normalized
        };
        let mut entries = self
            .directories
            .iter()
            .filter(|path| parent_of(path) == directory)
            .map(|path| FsEntry {
                path: path.clone(),
                kind: EntryKind::Directory,
                size: 0,
            })
            .chain(
                self.files
                    .iter()
                    .filter(|(path, _)| parent_of(path) == directory)
                    .map(|(path, bytes)| FsEntry {
                        path: path.clone(),
                        kind: EntryKind::File,
                        size: bytes.len() as u64,
                    }),
            )
            .collect::<Vec<_>>();
        entries.sort_by(|left, right| left.path.cmp(&right.path));
        Ok(entries)
    }

    fn read(&self, path: &str, limit: u64) -> Result<Vec<u8>, AppError> {
        let path = normalize_relative_path(path)?;
        let bytes = self
            .files
            .get(&path)
            .ok_or_else(|| AppError::Scan(format!("file not found: {path}")))?;
        if bytes.len() as u64 > limit {
            return Err(AppError::Scan(format!(
                "{path} is larger than the {limit}-byte read limit"
            )));
        }
        Ok(bytes.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_dropped_tree_lists_and_reads_within_bounds() {
        let mut fs = MemoryFs::new();
        fs.add_file("descriptor.mod", b"name=\"Demo\"".to_vec())
            .unwrap();
        fs.add_file(
            "common\\national_focus\\demo.txt",
            b"focus_tree = {}".to_vec(),
        )
        .unwrap();
        fs.add_directory("gfx/empty").unwrap();
        let root = fs.list("").unwrap();
        assert_eq!(
            root.iter()
                .map(|entry| entry.path.as_str())
                .collect::<Vec<_>>(),
            ["common", "descriptor.mod", "gfx"]
        );
        assert_eq!(root[1].kind, EntryKind::File);
        assert_eq!(root[1].size, 11);
        assert_eq!(fs.list("common").unwrap()[0].path, "common/national_focus");
        assert!(fs.list("gfx/empty").unwrap().is_empty());
        assert_eq!(
            fs.read("common/national_focus/demo.txt", 64).unwrap(),
            b"focus_tree = {}"
        );
        assert!(
            fs.read("descriptor.mod", 4).is_err(),
            "the read limit is enforced"
        );
        assert!(fs.read("missing.txt", 64).is_err());
        assert!(fs.list("missing").is_err());
        assert_eq!(fs.file_count(), 2);
        assert_eq!(fs.total_bytes(), 26);
    }

    #[test]
    fn unsafe_and_conflicting_paths_are_refused() {
        let mut fs = MemoryFs::new();
        for path in [
            "../escape.txt",
            "/abs.txt",
            "C:/drive.txt",
            "a/./../b",
            "con.txt",
            "",
        ] {
            assert!(fs.add_file(path, Vec::new()).is_err(), "{path}");
        }
        fs.add_file("events/demo.txt", Vec::new()).unwrap();
        assert!(fs.add_file("events/demo.txt", Vec::new()).is_err());
        assert!(
            fs.add_file("events", Vec::new()).is_err(),
            "a folder cannot become a file"
        );
        assert!(fs
            .add_file("events/demo.txt/inner.txt", Vec::new())
            .is_err());
    }
}
