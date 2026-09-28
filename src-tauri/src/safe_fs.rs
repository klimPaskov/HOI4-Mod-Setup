//! Retained, no-follow directory operations for transaction data.
//!
//! Paths are used only to establish and verify a root. After that, Unix
//! operations are relative to retained directory descriptors, and Windows
//! operations use retained directory handles that do not share delete access.

use crate::security::{is_link_metadata, normalize_relative_path, path_has_link_component};
use crate::AppError;
#[cfg(unix)]
use std::ffi::CString;
use std::ffi::{OsStr, OsString};
use std::fs::{self, File, Permissions};
use std::io::{self, Read, Write};
use std::path::{Component, Path, PathBuf};
use uuid::Uuid;

#[cfg(unix)]
use std::os::fd::{AsRawFd, FromRawFd};
#[cfg(unix)]
use std::os::unix::ffi::{OsStrExt, OsStringExt};
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
#[cfg(windows)]
use std::os::windows::ffi::OsStrExt;
#[cfg(windows)]
use std::os::windows::io::{AsRawHandle, FromRawHandle};

#[cfg(windows)]
use windows_sys::Win32::Foundation::{HANDLE, INVALID_HANDLE_VALUE};
#[cfg(windows)]
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, FileDispositionInfoEx, FileRenameInfo, FindClose, FindFirstFileW, FindNextFileW,
    GetFileInformationByHandle, ReplaceFileW, SetFileInformationByHandle,
    BY_HANDLE_FILE_INFORMATION, CREATE_NEW, DELETE, FILE_APPEND_DATA, FILE_ATTRIBUTE_DIRECTORY,
    FILE_ATTRIBUTE_REPARSE_POINT, FILE_ATTRIBUTE_TEMPORARY, FILE_DISPOSITION_FLAG_DELETE,
    FILE_DISPOSITION_FLAG_POSIX_SEMANTICS, FILE_DISPOSITION_INFO_EX, FILE_FLAG_BACKUP_SEMANTICS,
    FILE_FLAG_OPEN_REPARSE_POINT, FILE_GENERIC_READ, FILE_GENERIC_WRITE, FILE_LIST_DIRECTORY,
    FILE_READ_ATTRIBUTES, FILE_RENAME_INFO, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE,
    OPEN_EXISTING, SYNCHRONIZE, WIN32_FIND_DATAW,
};

/// A directory capability rooted at one canonical directory. The retained
/// ancestor handles prevent Windows path components from being renamed while
/// operations are in flight; Unix operations resolve every child with `*at`.
pub(crate) struct RootedDir {
    handle: File,
    ancestors: Vec<File>,
    path: PathBuf,
    deny_delete_share: bool,
}

impl RootedDir {
    pub(crate) fn open(path: &Path) -> Result<Self, AppError> {
        Self::open_with_share_policy(path, true)
    }

    pub(crate) fn open_read(path: &Path) -> Result<Self, AppError> {
        Self::open_with_share_policy(path, false)
    }

    fn open_with_share_policy(path: &Path, deny_delete_share: bool) -> Result<Self, AppError> {
        if !path.is_absolute() {
            return Err(AppError::PathSecurity(
                "filesystem root must be absolute".into(),
            ));
        }
        let observed = fs::symlink_metadata(path)?;
        if is_link_metadata(&observed) || !observed.is_dir() || path_has_link_component(path) {
            return Err(AppError::PathSecurity(
                "filesystem root is not a link-free directory".into(),
            ));
        }
        let canonical = fs::canonicalize(path)?;
        if path_has_link_component(&canonical) {
            return Err(AppError::PathSecurity(
                "canonical filesystem root contains a link".into(),
            ));
        }
        let filesystem_root = absolute_filesystem_root(&canonical)?;
        let root_handle = open_directory_path(&filesystem_root, deny_delete_share)?;
        validate_directory_handle(&root_handle)?;
        let mut current = Self {
            handle: root_handle,
            ancestors: Vec::new(),
            path: filesystem_root.clone(),
            deny_delete_share,
        };

        let remainder = canonical
            .strip_prefix(&filesystem_root)
            .map_err(|_| AppError::PathSecurity("filesystem root changed during open".into()))?;
        for component in remainder.components() {
            match component {
                Component::Normal(name) => current = current.open_child_dir(name)?,
                Component::CurDir => {}
                _ => {
                    return Err(AppError::PathSecurity(
                        "canonical filesystem root has an invalid component".into(),
                    ))
                }
            }
        }
        current.verify_bound_to_path()?;
        #[cfg(unix)]
        if !same_file_identity(&observed, &current.handle.metadata()?) {
            return Err(AppError::PathSecurity(
                "filesystem root changed while its handle was opened".into(),
            ));
        }
        #[cfg(windows)]
        current.verify_bound_to_path()?;
        Ok(current)
    }

    pub(crate) fn open_or_create(path: &Path) -> Result<Self, AppError> {
        if !path.is_absolute() {
            return Err(AppError::PathSecurity(
                "filesystem directory must be absolute".into(),
            ));
        }
        let mut missing = Vec::<OsString>::new();
        let mut existing = path.to_path_buf();
        loop {
            match fs::symlink_metadata(&existing) {
                Ok(metadata) => {
                    if is_link_metadata(&metadata) || !metadata.is_dir() {
                        return Err(AppError::PathSecurity(
                            "filesystem parent is not a link-free directory".into(),
                        ));
                    }
                    break;
                }
                Err(error) if error.kind() == io::ErrorKind::NotFound => {
                    let leaf = existing.file_name().ok_or_else(|| {
                        AppError::PathSecurity("no existing filesystem ancestor".into())
                    })?;
                    missing.push(leaf.to_os_string());
                    if !existing.pop() {
                        return Err(AppError::PathSecurity(
                            "no existing filesystem ancestor".into(),
                        ));
                    }
                }
                Err(error) => return Err(error.into()),
            }
        }
        let mut current = Self::open(&existing)?;
        for segment in missing.iter().rev() {
            current = current.ensure_child_dir(segment)?;
        }
        Ok(current)
    }

    pub(crate) fn verify_bound_to_path(&self) -> Result<(), AppError> {
        if path_has_link_component(&self.path) {
            return Err(AppError::PathSecurity(
                "retained filesystem root is no longer reachable without links".into(),
            ));
        }
        #[cfg(unix)]
        {
            let path_metadata = fs::symlink_metadata(&self.path).map_err(|_| {
                AppError::PathSecurity("retained filesystem root path changed".into())
            })?;
            if is_link_metadata(&path_metadata)
                || !path_metadata.is_dir()
                || !same_file_identity(&path_metadata, &self.handle.metadata()?)
            {
                return Err(AppError::PathSecurity(
                    "retained filesystem root path no longer identifies the opened directory"
                        .into(),
                ));
            }
        }
        #[cfg(windows)]
        {
            let path_handle = open_directory_path(&self.path, self.deny_delete_share)?;
            if windows_file_identity(&path_handle)? != windows_file_identity(&self.handle)? {
                return Err(AppError::PathSecurity(
                    "retained filesystem root path no longer identifies the opened directory"
                        .into(),
                ));
            }
        }
        Ok(())
    }

    pub(crate) fn open_dir(&self, relative: &str) -> Result<Self, AppError> {
        let relative = normalize_relative_path(relative)?;
        let mut current = self.duplicate()?;
        for component in relative.split('/') {
            current = current.open_child_dir(OsStr::new(component))?;
        }
        current.verify_bound_to_path()?;
        Ok(current)
    }

    pub(crate) fn ensure_dir(&self, relative: &str) -> Result<Self, AppError> {
        let relative = normalize_relative_path(relative)?;
        let mut current = self.duplicate()?;
        for component in relative.split('/') {
            current = current.ensure_child_dir(OsStr::new(component))?;
        }
        current.verify_bound_to_path()?;
        Ok(current)
    }

    pub(crate) fn read_file(&self, relative: &str) -> Result<Vec<u8>, AppError> {
        let mut file = self.open_regular_file(relative)?;
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes)?;
        Ok(bytes)
    }

    pub(crate) fn read_dir_names(&self) -> Result<Vec<OsString>, AppError> {
        self.verify_bound_to_path()?;
        let entries = read_dir_names_impl(self)?;
        self.verify_bound_to_path()?;
        Ok(entries)
    }

    pub(crate) fn is_directory(&self, relative: &str) -> Result<bool, AppError> {
        match self.entry_metadata(relative) {
            Ok(metadata) if metadata.is_link => Err(AppError::PathSecurity(
                "filesystem entry is a symlink or reparse point".into(),
            )),
            Ok(metadata) => Ok(metadata.is_dir),
            Err(error) if error_is_missing(&error) => Ok(false),
            Err(error) => Err(error),
        }
    }

    pub(crate) fn is_regular_file(&self, relative: &str) -> Result<bool, AppError> {
        match self.entry_metadata(relative) {
            Ok(metadata) if metadata.is_link => Err(AppError::PathSecurity(
                "filesystem entry is a symlink or reparse point".into(),
            )),
            Ok(metadata) => Ok(metadata.is_file),
            Err(error) if error_is_missing(&error) => Ok(false),
            Err(error) => Err(error),
        }
    }

    pub(crate) fn exists(&self, relative: &str) -> Result<bool, AppError> {
        match self.entry_metadata(relative) {
            Ok(_) => Ok(true),
            Err(error) if error_is_missing(&error) => Ok(false),
            Err(error) => Err(error),
        }
    }

    pub(crate) fn remove_tree(&self, relative: &str) -> Result<(), AppError> {
        let tree = self.open_dir(relative)?;
        let mut count = 0;
        tree.remove_tree_contents(0, &mut count)?;
        drop(tree);
        self.remove_dir(relative)
    }

    fn remove_tree_contents(&self, depth: usize, count: &mut usize) -> Result<(), AppError> {
        const MAX_TREE_DEPTH: usize = 256;
        const MAX_TREE_ENTRIES: usize = 100_000;
        if depth > MAX_TREE_DEPTH {
            return Err(AppError::PathSecurity(
                "transaction staging tree exceeds its depth limit".into(),
            ));
        }
        for name in self.read_dir_names()? {
            *count = count.saturating_add(1);
            if *count > MAX_TREE_ENTRIES {
                return Err(AppError::PathSecurity(
                    "transaction staging tree exceeds its entry limit".into(),
                ));
            }
            let metadata = entry_metadata(self, &name)?;
            if metadata.is_link {
                return Err(AppError::PathSecurity(
                    "refusing to recursively remove a staging link or reparse point".into(),
                ));
            }
            if metadata.is_file {
                let name = name.to_str().ok_or_else(|| {
                    AppError::PathSecurity("staging tree contains a non-UTF-8 file name".into())
                })?;
                self.remove_file(name)?;
            } else if metadata.is_dir {
                let name = name.to_str().ok_or_else(|| {
                    AppError::PathSecurity(
                        "staging tree contains a non-UTF-8 directory name".into(),
                    )
                })?;
                let child = self.open_dir(name)?;
                child.remove_tree_contents(depth + 1, count)?;
                drop(child);
                self.remove_dir(name)?;
            } else {
                return Err(AppError::PathSecurity(
                    "staging tree contains a special filesystem object".into(),
                ));
            }
        }
        Ok(())
    }

    pub(crate) fn hash_file(&self, relative: &str) -> Result<String, AppError> {
        use sha2::{Digest, Sha256};

        let mut file = self.open_regular_file(relative)?;
        let mut hasher = Sha256::new();
        let mut buffer = [0_u8; 64 * 1024];
        loop {
            let read = file.read(&mut buffer)?;
            if read == 0 {
                break;
            }
            hasher.update(&buffer[..read]);
        }
        Ok(hex::encode(hasher.finalize()))
    }

    pub(crate) fn write_atomic(&self, relative: &str, bytes: &[u8]) -> Result<(), AppError> {
        self.write_atomic_from(relative, &mut &bytes[..], None)
    }

    pub(crate) fn append_file(
        &self,
        relative: &str,
        bytes: &[u8],
        sync: bool,
    ) -> Result<(), AppError> {
        let (parent, leaf) = self.open_parent(relative, false)?;
        parent.verify_bound_to_path()?;
        let mut file = open_append_file_at(&parent, &leaf)?;
        file.write_all(bytes)?;
        if sync {
            file.sync_data()?;
        }
        parent.verify_bound_to_path()
    }

    pub(crate) fn copy_file_atomic_to(
        &self,
        source_relative: &str,
        destination: &RootedDir,
        destination_relative: &str,
    ) -> Result<(), AppError> {
        let source = self.open_regular_file(source_relative)?;
        let permissions = source.metadata()?.permissions();
        destination.write_atomic_from(destination_relative, &mut &source, Some(permissions))
    }

    pub(crate) fn remove_file(&self, relative: &str) -> Result<(), AppError> {
        let (parent, leaf) = self.open_parent(relative, false)?;
        parent.verify_bound_to_path()?;
        let metadata = entry_metadata(&parent, &leaf)?;
        if metadata.is_link || !metadata.is_file {
            return Err(AppError::PathSecurity(
                "refusing to remove a link or non-regular file".into(),
            ));
        }
        remove_file_at(&parent, &leaf)?;
        parent.sync_directory()?;
        parent.verify_bound_to_path()
    }

    #[cfg(unix)]
    pub(crate) fn set_executable(&self, relative: &str, executable: bool) -> Result<(), AppError> {
        let file = self.open_regular_file(relative)?;
        let mut permissions = file.metadata()?.permissions();
        let mut mode = permissions.mode();
        if executable {
            mode |= 0o111;
        } else {
            mode &= !0o111;
        }
        permissions.set_mode(mode);
        file.set_permissions(permissions)?;
        Ok(())
    }

    #[cfg(unix)]
    pub(crate) fn observed_executable(&self, relative: &str) -> Result<Option<bool>, AppError> {
        use std::os::unix::fs::PermissionsExt;
        let file = self.open_regular_file(relative)?;
        Ok(Some(file.metadata()?.permissions().mode() & 0o111 != 0))
    }

    pub(crate) fn create_dir(&self, relative: &str) -> Result<Self, AppError> {
        let (parent, leaf) = self.open_parent(relative, false)?;
        parent.verify_bound_to_path()?;
        create_directory_at(&parent, &leaf)?;
        parent.open_child_dir(&leaf)
    }

    pub(crate) fn remove_dir(&self, relative: &str) -> Result<(), AppError> {
        if self.remove_dir_if_empty(relative)? {
            Ok(())
        } else {
            Err(AppError::Transaction(
                "directory is missing or not empty".into(),
            ))
        }
    }

    pub(crate) fn remove_dir_if_empty(&self, relative: &str) -> Result<bool, AppError> {
        let (parent, leaf) = self.open_parent(relative, false)?;
        parent.verify_bound_to_path()?;
        match entry_metadata(&parent, &leaf) {
            Err(error) if error_is_missing(&error) => return Ok(false),
            Err(error) => return Err(error),
            Ok(metadata) if metadata.is_link || !metadata.is_dir => {
                return Err(AppError::PathSecurity(
                    "refusing to remove a link or non-directory".into(),
                ));
            }
            Ok(_) => {}
        }
        let _directory = parent.open_child_dir(&leaf)?;
        drop(_directory);
        match remove_directory_at(&parent, &leaf) {
            Ok(()) => {
                parent.sync_directory()?;
                parent.verify_bound_to_path()?;
                Ok(true)
            }
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::NotFound | io::ErrorKind::DirectoryNotEmpty
                ) =>
            {
                Ok(false)
            }
            Err(error) => Err(error.into()),
        }
    }

    fn write_atomic_from<R: Read>(
        &self,
        relative: &str,
        reader: &mut R,
        permissions: Option<Permissions>,
    ) -> Result<(), AppError> {
        let (parent, leaf) = self.open_parent(relative, true)?;
        parent.verify_bound_to_path()?;
        match entry_metadata(&parent, &leaf) {
            Ok(metadata) if metadata.is_link || !metadata.is_file => {
                return Err(AppError::PathSecurity(
                    "atomic write destination is not a regular file".into(),
                ));
            }
            Ok(_) => {}
            Err(error) if error_is_missing(&error) => {}
            Err(error) => return Err(error),
        }
        let temporary = OsString::from(format!(".hoi4ms-{}.tmp", Uuid::new_v4()));
        let mut file = open_new_file_at(&parent, &temporary)?;
        let mut result = (|| {
            io::copy(reader, &mut file)?;
            if let Some(permissions) = permissions {
                file.set_permissions(permissions)?;
            }
            file.sync_all()?;
            parent.verify_bound_to_path()
        })();
        drop(file);
        if result.is_ok() {
            result = (|| {
                rename_file_at(&parent, &temporary, &leaf, true)?;
                parent.sync_directory()?;
                parent.verify_bound_to_path()
            })();
        }
        if result.is_err() {
            let _ = remove_file_at(&parent, &temporary);
        }
        result
    }

    fn open_regular_file(&self, relative: &str) -> Result<File, AppError> {
        let (parent, leaf) = self.open_parent(relative, false)?;
        parent.verify_bound_to_path()?;
        let file = open_regular_file_at(&parent, &leaf)?;
        if is_link_metadata(&file.metadata()?) {
            return Err(AppError::PathSecurity(
                "opened file is a link or reparse point".into(),
            ));
        }
        Ok(file)
    }

    fn open_parent(&self, relative: &str, create: bool) -> Result<(Self, OsString), AppError> {
        let normalized = normalize_relative_path(relative)?;
        let (parent_relative, leaf) = normalized.rsplit_once('/').unwrap_or(("", &normalized));
        let parent = if parent_relative.is_empty() {
            self.duplicate()?
        } else if create {
            self.ensure_dir(parent_relative)?
        } else {
            self.open_dir(parent_relative)?
        };
        Ok((parent, OsString::from(leaf)))
    }

    fn entry_metadata(&self, relative: &str) -> Result<EntryMetadata, AppError> {
        let (parent, leaf) = self.open_parent(relative, false)?;
        entry_metadata(&parent, &leaf)
    }

    fn open_child_dir(&self, name: &OsStr) -> Result<Self, AppError> {
        validate_component(name)?;
        self.verify_bound_to_path()?;
        let child_path = self.path.join(name);
        let child = open_child_directory(&self.handle, &child_path, name, self.deny_delete_share)?;
        let metadata = child.metadata()?;
        if is_link_metadata(&metadata) || !metadata.is_dir() {
            return Err(AppError::PathSecurity(
                "opened directory is a link, reparse point, or non-directory".into(),
            ));
        }
        let mut ancestors = self
            .ancestors
            .iter()
            .map(File::try_clone)
            .collect::<io::Result<Vec<_>>>()?;
        ancestors.push(self.handle.try_clone()?);
        Ok(Self {
            handle: child,
            ancestors,
            path: child_path,
            deny_delete_share: self.deny_delete_share,
        })
    }

    fn ensure_child_dir(&self, name: &OsStr) -> Result<Self, AppError> {
        match self.open_child_dir(name) {
            Ok(dir) => Ok(dir),
            Err(open_error) => match entry_metadata(self, name) {
                Err(error) if error_is_missing(&error) => match create_directory_at(self, name) {
                    Ok(()) => self.open_child_dir(name),
                    Err(error) if error_is_exists(&error) => self.open_child_dir(name),
                    Err(error) => Err(error),
                },
                Ok(metadata) if metadata.is_link => Err(AppError::PathSecurity(
                    "refusing to traverse a symlink or reparse directory".into(),
                )),
                Ok(metadata) if !metadata.is_dir => Err(AppError::PathSecurity(
                    "path component is not a directory".into(),
                )),
                _ => Err(open_error),
            },
        }
    }

    fn duplicate(&self) -> Result<Self, AppError> {
        Ok(Self {
            handle: self.handle.try_clone()?,
            ancestors: self
                .ancestors
                .iter()
                .map(File::try_clone)
                .collect::<io::Result<Vec<_>>>()?,
            path: self.path.clone(),
            deny_delete_share: self.deny_delete_share,
        })
    }

    fn sync_directory(&self) -> Result<(), AppError> {
        match self.handle.sync_all() {
            Ok(()) => Ok(()),
            Err(error) if cfg!(windows) && error.kind() == io::ErrorKind::PermissionDenied => {
                Ok(())
            }
            Err(error) => Err(error.into()),
        }
    }
}

fn error_is_missing(error: &AppError) -> bool {
    match error {
        AppError::Transaction(message) => {
            let message = message.to_ascii_lowercase();
            message.contains("no such file")
                || message.contains("not found")
                || message.contains("cannot find the file")
                || message.contains("cannot find the path")
        }
        _ => false,
    }
}

fn error_is_exists(error: &AppError) -> bool {
    match error {
        AppError::Transaction(message) => message.to_ascii_lowercase().contains("exist"),
        _ => false,
    }
}

fn absolute_filesystem_root(path: &Path) -> Result<PathBuf, AppError> {
    let mut root = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(prefix) => root.push(prefix.as_os_str()),
            Component::RootDir => root.push(component.as_os_str()),
            Component::Normal(_) => break,
            Component::CurDir => {}
            Component::ParentDir => {
                return Err(AppError::PathSecurity(
                    "canonical path contains parent traversal".into(),
                ))
            }
        }
    }
    if root.as_os_str().is_empty() {
        return Err(AppError::PathSecurity(
            "absolute path has no filesystem root".into(),
        ));
    }
    Ok(root)
}

fn validate_component(name: &OsStr) -> Result<(), AppError> {
    let path = Path::new(name);
    let mut components = path.components();
    match (components.next(), components.next()) {
        (Some(Component::Normal(_)), None)
            if name != OsStr::new(".") && name != OsStr::new("..") =>
        {
            Ok(())
        }
        _ => Err(AppError::PathSecurity(
            "filesystem operation requires one normalized path component".into(),
        )),
    }
}

#[cfg(unix)]
fn open_directory_path(path: &Path, _deny_delete_share: bool) -> Result<File, AppError> {
    use std::os::unix::ffi::OsStrExt;
    let name = CString::new(path.as_os_str().as_bytes())
        .map_err(|_| AppError::PathSecurity("filesystem path contains NUL".into()))?;
    let fd = unsafe {
        libc::open(
            name.as_ptr(),
            libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
        )
    };
    if fd < 0 {
        return Err(io::Error::last_os_error().into());
    }
    // SAFETY: `open` returned a newly owned file descriptor.
    Ok(unsafe { File::from_raw_fd(fd) })
}

#[cfg(windows)]
fn open_directory_path(path: &Path, deny_delete_share: bool) -> Result<File, AppError> {
    let file = create_windows_file(
        path,
        FILE_LIST_DIRECTORY | FILE_READ_ATTRIBUTES | SYNCHRONIZE,
        OPEN_EXISTING,
        FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT,
        FILE_SHARE_READ
            | FILE_SHARE_WRITE
            | if deny_delete_share {
                0
            } else {
                FILE_SHARE_DELETE
            },
    )
    .map_err(|error| {
        AppError::Transaction(format!(
            "could not open directory root {}: {error}",
            path.display()
        ))
    })?;
    validate_windows_directory_handle(&file)?;
    Ok(file)
}

#[cfg(unix)]
fn open_child_directory(
    parent: &File,
    _display_path: &Path,
    name: &OsStr,
    _deny_delete_share: bool,
) -> Result<File, AppError> {
    let name = CString::new(name.as_bytes())
        .map_err(|_| AppError::PathSecurity("filesystem component contains NUL".into()))?;
    let fd = unsafe {
        libc::openat(
            parent.as_raw_fd(),
            name.as_ptr(),
            libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
        )
    };
    if fd < 0 {
        return Err(io::Error::last_os_error().into());
    }
    // SAFETY: `openat` returned a newly owned file descriptor.
    Ok(unsafe { File::from_raw_fd(fd) })
}

#[cfg(windows)]
fn open_child_directory(
    _parent: &File,
    display_path: &Path,
    _name: &OsStr,
    deny_delete_share: bool,
) -> Result<File, AppError> {
    let file = create_windows_file(
        display_path,
        FILE_LIST_DIRECTORY | FILE_READ_ATTRIBUTES | SYNCHRONIZE,
        OPEN_EXISTING,
        FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT,
        FILE_SHARE_READ
            | FILE_SHARE_WRITE
            | if deny_delete_share {
                0
            } else {
                FILE_SHARE_DELETE
            },
    )
    .map_err(|error| {
        AppError::Transaction(format!(
            "could not open directory component {}: {error}",
            display_path.display()
        ))
    })?;
    validate_windows_directory_handle(&file)?;
    Ok(file)
}

fn validate_directory_handle(file: &File) -> Result<(), AppError> {
    let metadata = file.metadata()?;
    if is_link_metadata(&metadata) || !metadata.is_dir() {
        return Err(AppError::PathSecurity(
            "opened directory is a link, reparse point, or non-directory".into(),
        ));
    }
    Ok(())
}

#[cfg(windows)]
fn validate_windows_directory_handle(file: &File) -> Result<(), AppError> {
    let information = windows_file_information(file)?;
    if information.dwFileAttributes & FILE_ATTRIBUTE_REPARSE_POINT != 0
        || information.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY == 0
    {
        return Err(AppError::PathSecurity(
            "opened directory is a reparse point or non-directory".into(),
        ));
    }
    Ok(())
}

#[cfg(unix)]
fn same_file_identity(left: &fs::Metadata, right: &fs::Metadata) -> bool {
    use std::os::unix::fs::MetadataExt;
    left.dev() == right.dev() && left.ino() == right.ino()
}

#[cfg(unix)]
fn open_regular_file_at(parent: &RootedDir, name: &OsStr) -> Result<File, AppError> {
    validate_component(name)?;
    let name = CString::new(name.as_bytes())
        .map_err(|_| AppError::PathSecurity("filesystem component contains NUL".into()))?;
    let fd = unsafe {
        libc::openat(
            parent.handle.as_raw_fd(),
            name.as_ptr(),
            libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK,
        )
    };
    if fd < 0 {
        return Err(io::Error::last_os_error().into());
    }
    // SAFETY: `openat` returned a newly owned file descriptor.
    let file = unsafe { File::from_raw_fd(fd) };
    let metadata = file.metadata()?;
    if !metadata.is_file() || is_link_metadata(&metadata) {
        return Err(AppError::PathSecurity(
            "opened object is not a regular file".into(),
        ));
    }
    Ok(file)
}

#[cfg(windows)]
fn open_regular_file_at(parent: &RootedDir, name: &OsStr) -> Result<File, AppError> {
    validate_component(name)?;
    let path = parent.path.join(name);
    let file = create_windows_file(
        &path,
        FILE_GENERIC_READ | FILE_READ_ATTRIBUTES | SYNCHRONIZE,
        OPEN_EXISTING,
        FILE_FLAG_OPEN_REPARSE_POINT,
        FILE_SHARE_READ
            | FILE_SHARE_WRITE
            | if parent.deny_delete_share {
                0
            } else {
                FILE_SHARE_DELETE
            },
    )?;
    let information = windows_file_information(&file)?;
    if information.dwFileAttributes & FILE_ATTRIBUTE_REPARSE_POINT != 0
        || information.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY != 0
    {
        return Err(AppError::PathSecurity(
            "opened object is a reparse point or non-file".into(),
        ));
    }
    Ok(file)
}

fn open_new_file_at(parent: &RootedDir, name: &OsStr) -> Result<File, AppError> {
    validate_component(name)?;
    open_new_file_impl(parent, name)
}

fn open_append_file_at(parent: &RootedDir, name: &OsStr) -> Result<File, AppError> {
    validate_component(name)?;
    open_append_file_impl(parent, name)
}

#[cfg(unix)]
fn open_append_file_impl(parent: &RootedDir, name: &OsStr) -> Result<File, AppError> {
    let name = CString::new(name.as_bytes())
        .map_err(|_| AppError::PathSecurity("filesystem component contains NUL".into()))?;
    let fd = unsafe {
        libc::openat(
            parent.handle.as_raw_fd(),
            name.as_ptr(),
            libc::O_WRONLY | libc::O_APPEND | libc::O_NOFOLLOW | libc::O_CLOEXEC,
        )
    };
    if fd < 0 {
        return Err(io::Error::last_os_error().into());
    }
    // SAFETY: `openat` returned a newly owned file descriptor.
    let file = unsafe { File::from_raw_fd(fd) };
    if !file.metadata()?.is_file() {
        return Err(AppError::PathSecurity(
            "append target is not a regular file".into(),
        ));
    }
    Ok(file)
}

#[cfg(windows)]
fn open_append_file_impl(parent: &RootedDir, name: &OsStr) -> Result<File, AppError> {
    let file = create_windows_file(
        &parent.path.join(name),
        FILE_APPEND_DATA | FILE_READ_ATTRIBUTES | SYNCHRONIZE,
        OPEN_EXISTING,
        FILE_FLAG_OPEN_REPARSE_POINT,
        FILE_SHARE_READ | FILE_SHARE_WRITE,
    )?;
    let information = windows_file_information(&file)?;
    if information.dwFileAttributes & FILE_ATTRIBUTE_REPARSE_POINT != 0
        || information.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY != 0
    {
        return Err(AppError::PathSecurity(
            "append target is a reparse point or non-file".into(),
        ));
    }
    Ok(file)
}

#[cfg(unix)]
fn open_new_file_impl(parent: &RootedDir, name: &OsStr) -> Result<File, AppError> {
    let name = CString::new(name.as_bytes())
        .map_err(|_| AppError::PathSecurity("filesystem component contains NUL".into()))?;
    let fd = unsafe {
        libc::openat(
            parent.handle.as_raw_fd(),
            name.as_ptr(),
            libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            0o600,
        )
    };
    if fd < 0 {
        return Err(io::Error::last_os_error().into());
    }
    // SAFETY: `openat` returned a newly owned file descriptor.
    Ok(unsafe { File::from_raw_fd(fd) })
}

#[cfg(windows)]
fn open_new_file_impl(parent: &RootedDir, name: &OsStr) -> Result<File, AppError> {
    let path = parent.path.join(name);
    create_windows_file(
        &path,
        FILE_GENERIC_WRITE | FILE_READ_ATTRIBUTES | DELETE | SYNCHRONIZE,
        CREATE_NEW,
        FILE_ATTRIBUTE_TEMPORARY | FILE_FLAG_OPEN_REPARSE_POINT,
        FILE_SHARE_READ | FILE_SHARE_WRITE,
    )
}

fn create_directory_at(parent: &RootedDir, name: &OsStr) -> Result<(), AppError> {
    validate_component(name)?;
    create_directory_impl(parent, name)
}

#[cfg(unix)]
fn create_directory_impl(parent: &RootedDir, name: &OsStr) -> Result<(), AppError> {
    let name = CString::new(name.as_bytes())
        .map_err(|_| AppError::PathSecurity("filesystem component contains NUL".into()))?;
    let result = unsafe { libc::mkdirat(parent.handle.as_raw_fd(), name.as_ptr(), 0o700) };
    if result == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error().into())
    }
}

#[cfg(windows)]
fn create_directory_impl(parent: &RootedDir, name: &OsStr) -> Result<(), AppError> {
    let path = parent.path.join(name);
    let wide = path
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let result = unsafe {
        windows_sys::Win32::Storage::FileSystem::CreateDirectoryW(wide.as_ptr(), std::ptr::null())
    };
    if result != 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error().into())
    }
}

fn entry_metadata(parent: &RootedDir, name: &OsStr) -> Result<EntryMetadata, AppError> {
    entry_metadata_impl(parent, name)
}

struct EntryMetadata {
    is_file: bool,
    is_dir: bool,
    is_link: bool,
}

#[cfg(unix)]
fn entry_metadata_impl(parent: &RootedDir, name: &OsStr) -> Result<EntryMetadata, AppError> {
    let name = CString::new(name.as_bytes())
        .map_err(|_| AppError::PathSecurity("filesystem component contains NUL".into()))?;
    let mut stat = std::mem::MaybeUninit::<libc::stat>::uninit();
    let result = unsafe {
        libc::fstatat(
            parent.handle.as_raw_fd(),
            name.as_ptr(),
            stat.as_mut_ptr(),
            libc::AT_SYMLINK_NOFOLLOW,
        )
    };
    if result != 0 {
        return Err(io::Error::last_os_error().into());
    }
    // SAFETY: `fstatat` initialized `stat` after returning success.
    let stat = unsafe { stat.assume_init() };
    let file_type = stat.st_mode & libc::S_IFMT;
    Ok(EntryMetadata {
        is_file: file_type == libc::S_IFREG,
        is_dir: file_type == libc::S_IFDIR,
        is_link: file_type == libc::S_IFLNK,
    })
}

#[cfg(windows)]
fn entry_metadata_impl(parent: &RootedDir, name: &OsStr) -> Result<EntryMetadata, AppError> {
    validate_component(name)?;
    let path = parent.path.join(name);
    let file = create_windows_file(
        &path,
        FILE_READ_ATTRIBUTES,
        OPEN_EXISTING,
        FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT,
        FILE_SHARE_READ
            | FILE_SHARE_WRITE
            | if parent.deny_delete_share {
                0
            } else {
                FILE_SHARE_DELETE
            },
    )?;
    let information = windows_file_information(&file)?;
    Ok(EntryMetadata {
        is_file: information.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY == 0
            && information.dwFileAttributes & FILE_ATTRIBUTE_REPARSE_POINT == 0,
        is_dir: information.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY != 0
            && information.dwFileAttributes & FILE_ATTRIBUTE_REPARSE_POINT == 0,
        is_link: information.dwFileAttributes & FILE_ATTRIBUTE_REPARSE_POINT != 0,
    })
}

fn remove_file_at(parent: &RootedDir, name: &OsStr) -> Result<(), AppError> {
    validate_component(name)?;
    remove_file_impl(parent, name)
}

fn remove_directory_at(parent: &RootedDir, name: &OsStr) -> io::Result<()> {
    remove_directory_impl(parent, name)
}

#[cfg(unix)]
fn remove_directory_impl(parent: &RootedDir, name: &OsStr) -> io::Result<()> {
    let name = CString::new(name.as_bytes()).map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "filesystem component contains NUL",
        )
    })?;
    let result =
        unsafe { libc::unlinkat(parent.handle.as_raw_fd(), name.as_ptr(), libc::AT_REMOVEDIR) };
    if result == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

#[cfg(windows)]
fn remove_directory_impl(parent: &RootedDir, name: &OsStr) -> io::Result<()> {
    let handle = create_windows_file(
        &parent.path.join(name),
        DELETE | FILE_READ_ATTRIBUTES | SYNCHRONIZE,
        OPEN_EXISTING,
        FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT,
        FILE_SHARE_READ | FILE_SHARE_WRITE,
    )
    .map_err(app_error_to_io)?;
    let information = windows_file_information(&handle).map_err(app_error_to_io)?;
    if information.dwFileAttributes & FILE_ATTRIBUTE_REPARSE_POINT != 0
        || information.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY == 0
    {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "refusing to remove a reparse point or non-directory",
        ));
    }
    let disposition = FILE_DISPOSITION_INFO_EX {
        Flags: FILE_DISPOSITION_FLAG_DELETE | FILE_DISPOSITION_FLAG_POSIX_SEMANTICS,
    };
    let result = unsafe {
        SetFileInformationByHandle(
            handle.as_raw_handle() as HANDLE,
            FileDispositionInfoEx,
            &disposition as *const _ as *const _,
            std::mem::size_of_val(&disposition) as u32,
        )
    };
    if result == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(windows)]
fn app_error_to_io(error: AppError) -> io::Error {
    io::Error::other(error.to_string())
}

#[cfg(unix)]
fn remove_file_impl(parent: &RootedDir, name: &OsStr) -> Result<(), AppError> {
    let name = CString::new(name.as_bytes())
        .map_err(|_| AppError::PathSecurity("filesystem component contains NUL".into()))?;
    let result = unsafe { libc::unlinkat(parent.handle.as_raw_fd(), name.as_ptr(), 0) };
    if result == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error().into())
    }
}

#[cfg(windows)]
fn remove_file_impl(parent: &RootedDir, name: &OsStr) -> Result<(), AppError> {
    let path = parent.path.join(name);
    let file = create_windows_file(
        &path,
        DELETE | FILE_READ_ATTRIBUTES | SYNCHRONIZE,
        OPEN_EXISTING,
        FILE_FLAG_OPEN_REPARSE_POINT,
        FILE_SHARE_READ | FILE_SHARE_WRITE,
    )?;
    let information = windows_file_information(&file)?;
    if information.dwFileAttributes & FILE_ATTRIBUTE_REPARSE_POINT != 0
        || information.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY != 0
    {
        return Err(AppError::PathSecurity(
            "refusing to remove a reparse point or directory as a file".into(),
        ));
    }
    let disposition = FILE_DISPOSITION_INFO_EX {
        Flags: FILE_DISPOSITION_FLAG_DELETE | FILE_DISPOSITION_FLAG_POSIX_SEMANTICS,
    };
    let result = unsafe {
        SetFileInformationByHandle(
            file.as_raw_handle() as HANDLE,
            FileDispositionInfoEx,
            &disposition as *const _ as *const _,
            std::mem::size_of_val(&disposition) as u32,
        )
    };
    if result == 0 {
        Err(io::Error::last_os_error().into())
    } else {
        Ok(())
    }
}

fn rename_file_at(
    parent: &RootedDir,
    source: &OsStr,
    destination: &OsStr,
    replace: bool,
) -> Result<(), AppError> {
    validate_component(source)?;
    validate_component(destination)?;
    rename_file_impl(parent, source, destination, replace)
}

#[cfg(unix)]
fn rename_file_impl(
    parent: &RootedDir,
    source: &OsStr,
    destination: &OsStr,
    _replace: bool,
) -> Result<(), AppError> {
    let source = CString::new(source.as_bytes())
        .map_err(|_| AppError::PathSecurity("filesystem component contains NUL".into()))?;
    let destination = CString::new(destination.as_bytes())
        .map_err(|_| AppError::PathSecurity("filesystem component contains NUL".into()))?;
    let result = unsafe {
        libc::renameat(
            parent.handle.as_raw_fd(),
            source.as_ptr(),
            parent.handle.as_raw_fd(),
            destination.as_ptr(),
        )
    };
    if result == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error().into())
    }
}

#[cfg(windows)]
fn rename_file_impl(
    parent: &RootedDir,
    source: &OsStr,
    destination: &OsStr,
    replace: bool,
) -> Result<(), AppError> {
    let source_path = parent.path.join(source);
    let destination_path = parent.path.join(destination);
    let source_file = create_windows_file(
        &source_path,
        DELETE | FILE_READ_ATTRIBUTES | SYNCHRONIZE,
        OPEN_EXISTING,
        FILE_FLAG_OPEN_REPARSE_POINT,
        FILE_SHARE_READ | FILE_SHARE_WRITE,
    )?;
    let information = windows_file_information(&source_file)?;
    if information.dwFileAttributes & FILE_ATTRIBUTE_REPARSE_POINT != 0
        || information.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY != 0
    {
        return Err(AppError::PathSecurity(
            "refusing to rename a reparse point or directory as a file".into(),
        ));
    }
    let destination_exists = match entry_metadata(parent, destination) {
        Ok(metadata) if metadata.is_link || !metadata.is_file => {
            return Err(AppError::PathSecurity(
                "rename destination is not a regular file".into(),
            ));
        }
        Ok(_) => true,
        Err(error) if error_is_missing(&error) => false,
        Err(error) => return Err(error),
    };
    if replace && destination_exists {
        drop(source_file);
        let source_wide = source_path
            .as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect::<Vec<_>>();
        let destination_wide = destination_path
            .as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect::<Vec<_>>();
        let result = unsafe {
            ReplaceFileW(
                destination_wide.as_ptr(),
                source_wide.as_ptr(),
                std::ptr::null(),
                0,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        };
        if result == 0 {
            return Err(io::Error::last_os_error().into());
        }
        return Ok(());
    }
    let destination = destination_path
        .as_os_str()
        .encode_wide()
        .collect::<Vec<_>>();
    let name_offset = std::mem::offset_of!(FILE_RENAME_INFO, FileName);
    let name_bytes = destination
        .len()
        .checked_mul(std::mem::size_of::<u16>())
        .ok_or_else(|| AppError::PathSecurity("rename target name is too long".into()))?;
    // FILE_RENAME_INFO declares one WCHAR in its trailing array. Windows
    // requires sizeof(struct) plus the complete filename byte count, while
    // the data itself starts at the array offset.
    let total_size = std::mem::size_of::<FILE_RENAME_INFO>()
        .checked_add(name_bytes)
        .ok_or_else(|| AppError::PathSecurity("rename target name is too long".into()))?;
    let mut buffer = vec![0_u64; total_size.div_ceil(std::mem::size_of::<u64>())];
    unsafe {
        let buffer_pointer = buffer.as_mut_ptr() as *mut u8;
        let info = buffer_pointer as *mut FILE_RENAME_INFO;
        (*info).Anonymous.ReplaceIfExists = 0;
        (*info).RootDirectory = std::ptr::null_mut();
        (*info).FileNameLength = name_bytes as u32;
        std::ptr::copy_nonoverlapping(
            destination.as_ptr(),
            buffer_pointer.add(name_offset) as *mut u16,
            destination.len(),
        );
        let result = SetFileInformationByHandle(
            source_file.as_raw_handle() as HANDLE,
            FileRenameInfo,
            buffer_pointer as *const _,
            total_size as u32,
        );
        if result == 0 {
            let error = io::Error::last_os_error();
            return Err(AppError::Transaction(format!(
                "handle-relative rename failed (name bytes {name_bytes}, structure bytes {total_size}): {error}"
            )));
        }
    }
    Ok(())
}

#[cfg(windows)]
fn create_windows_file(
    path: &Path,
    access: u32,
    disposition: u32,
    attributes: u32,
    share: u32,
) -> Result<File, AppError> {
    let path = path
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let handle = unsafe {
        CreateFileW(
            path.as_ptr(),
            access,
            share,
            std::ptr::null(),
            disposition,
            attributes,
            std::ptr::null_mut(),
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error().into());
    }
    // SAFETY: `CreateFileW` returned a valid, uniquely owned handle.
    Ok(unsafe { File::from_raw_handle(handle as _) })
}

#[cfg(unix)]
fn read_dir_names_impl(directory: &RootedDir) -> Result<Vec<OsString>, AppError> {
    use std::ffi::CStr;

    let duplicate = unsafe { libc::fcntl(directory.handle.as_raw_fd(), libc::F_DUPFD_CLOEXEC, 0) };
    if duplicate < 0 {
        return Err(io::Error::last_os_error().into());
    }
    // SAFETY: `duplicate` is a newly owned descriptor and `fdopendir` takes ownership.
    let stream = unsafe { libc::fdopendir(duplicate) };
    if stream.is_null() {
        let error = io::Error::last_os_error();
        unsafe { libc::close(duplicate) };
        return Err(error.into());
    }
    let mut entries = Vec::new();
    loop {
        set_errno(0);
        // SAFETY: `stream` remains valid until `closedir` below.
        let entry = unsafe { libc::readdir(stream) };
        if entry.is_null() {
            let errno = current_errno();
            if errno != 0 {
                unsafe { libc::closedir(stream) };
                return Err(io::Error::from_raw_os_error(errno).into());
            }
            break;
        }
        // SAFETY: `d_name` is a NUL-terminated entry name supplied by readdir.
        let name = unsafe { CStr::from_ptr((*entry).d_name.as_ptr()) };
        if name.to_bytes() != b"." && name.to_bytes() != b".." {
            entries.push(OsString::from_vec(name.to_bytes().to_vec()));
        }
    }
    if unsafe { libc::closedir(stream) } != 0 {
        return Err(io::Error::last_os_error().into());
    }
    Ok(entries)
}

#[cfg(any(target_os = "linux", target_os = "android"))]
fn set_errno(value: i32) {
    unsafe { *libc::__errno_location() = value };
}

#[cfg(target_os = "macos")]
fn set_errno(value: i32) {
    unsafe { *libc::__error() = value };
}

#[cfg(any(target_os = "linux", target_os = "android"))]
fn current_errno() -> i32 {
    unsafe { *libc::__errno_location() }
}

#[cfg(target_os = "macos")]
fn current_errno() -> i32 {
    unsafe { *libc::__error() }
}

#[cfg(windows)]
fn read_dir_names_impl(directory: &RootedDir) -> Result<Vec<OsString>, AppError> {
    use std::os::windows::ffi::OsStringExt;
    const ERROR_NO_MORE_FILES: i32 = 18;

    let pattern = directory
        .path
        .join("*")
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let mut data = unsafe { std::mem::zeroed::<WIN32_FIND_DATAW>() };
    let search = unsafe { FindFirstFileW(pattern.as_ptr(), &mut data as *mut _) };
    if search == INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error().into());
    }
    let mut entries = Vec::new();
    let result = loop {
        let length = data
            .cFileName
            .iter()
            .position(|unit| *unit == 0)
            .unwrap_or(data.cFileName.len());
        let name = OsString::from_wide(&data.cFileName[..length]);
        if name != OsStr::new(".") && name != OsStr::new("..") {
            entries.push(name);
        }
        if unsafe { FindNextFileW(search, &mut data as *mut _) } == 0 {
            let error = io::Error::last_os_error();
            if error.raw_os_error() == Some(ERROR_NO_MORE_FILES) {
                break Ok(());
            }
            break Err(error);
        }
    };
    if unsafe { FindClose(search) } == 0 {
        return Err(io::Error::last_os_error().into());
    }
    result?;
    Ok(entries)
}

#[cfg(windows)]
fn windows_file_information(file: &File) -> Result<BY_HANDLE_FILE_INFORMATION, AppError> {
    let mut information = unsafe { std::mem::zeroed::<BY_HANDLE_FILE_INFORMATION>() };
    let result = unsafe {
        GetFileInformationByHandle(file.as_raw_handle() as HANDLE, &mut information as *mut _)
    };
    if result == 0 {
        Err(io::Error::last_os_error().into())
    } else {
        Ok(information)
    }
}

#[cfg(windows)]
fn windows_file_identity(file: &File) -> Result<(u32, u64), AppError> {
    let information = windows_file_information(file)?;
    let index = ((information.nFileIndexHigh as u64) << 32) | information.nFileIndexLow as u64;
    Ok((information.dwVolumeSerialNumber, index))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn atomic_write_rejects_a_static_link_leaf() {
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let sentinel = outside.path().join("sentinel.txt");
        fs::write(&sentinel, b"user bytes").unwrap();
        let project = RootedDir::open(root.path()).unwrap();

        #[cfg(unix)]
        std::os::unix::fs::symlink(&sentinel, root.path().join("managed.txt")).unwrap();
        #[cfg(windows)]
        std::os::windows::fs::symlink_file(&sentinel, root.path().join("managed.txt")).unwrap();

        assert!(project.write_atomic("managed.txt", b"replacement").is_err());
        assert_eq!(fs::read(&sentinel).unwrap(), b"user bytes");
    }

    #[test]
    fn ancestor_link_swap_cannot_redirect_a_write_outside_the_root() {
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let outside_sentinel = outside.path().join("sentinel.txt");
        fs::write(&outside_sentinel, b"user bytes").unwrap();
        let managed = root.path().join("managed");
        fs::create_dir(&managed).unwrap();
        let project = RootedDir::open(root.path()).unwrap();

        #[cfg(unix)]
        {
            let moved = root.path().join("managed-original");
            fs::rename(&managed, &moved).unwrap();
            std::os::unix::fs::symlink(outside.path(), &managed).unwrap();
        }
        #[cfg(windows)]
        {
            let moved = root.path().join("managed-original");
            fs::rename(&managed, &moved).unwrap();
            let output = std::process::Command::new("cmd.exe")
                .args(["/d", "/c", "mklink", "/J"])
                .arg(&managed)
                .arg(outside.path())
                .output()
                .expect("cmd.exe is present on Windows");
            assert!(
                output.status.success(),
                "junction creation failed: {output:?}"
            );
        }

        assert!(project
            .write_atomic("managed/sentinel.txt", b"replacement")
            .is_err());
        assert_eq!(fs::read(&outside_sentinel).unwrap(), b"user bytes");
    }

    #[test]
    fn recursive_staging_removal_refuses_link_entries() {
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let sentinel = outside.path().join("sentinel.txt");
        fs::write(&sentinel, b"keep").unwrap();
        let app = RootedDir::open(root.path()).unwrap();
        let staging = app.ensure_dir("staging/transaction").unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(&sentinel, staging.path.join("linked.txt")).unwrap();
        #[cfg(windows)]
        std::os::windows::fs::symlink_file(&sentinel, staging.path.join("linked.txt")).unwrap();

        assert!(app.remove_tree("staging/transaction").is_err());
        assert_eq!(fs::read(&sentinel).unwrap(), b"keep");
    }

    #[cfg(windows)]
    #[test]
    fn read_only_root_can_open_the_system_codex_test_executable_directory() {
        let path = std::env::var_os("SystemRoot")
            .map(PathBuf::from)
            .expect("SystemRoot is set")
            .join("System32/WindowsPowerShell/v1.0");
        RootedDir::open_read(&path).unwrap();
    }
}
