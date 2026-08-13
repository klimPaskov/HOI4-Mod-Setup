//! Build the optional flat source folder intended for a ChatGPT Chat project.
//!
//! This is a generated view of the verified setup inputs. It never removes the
//! normal `.agents/skills` or `.codex/agents` trees and every output remains a
//! normal transaction operation with conflict and rollback evidence.

use crate::models::{GeneratedArtifact, PreparedFile};
use crate::security::{
    canonical_relative_key, contains_credential_shaped_content, is_link_metadata,
    normalize_relative_path, safe_join,
};
use crate::AppError;
use std::collections::BTreeMap;
#[cfg(any(unix, test))]
use std::fs;
use std::fs::OpenOptions;
use std::io::Read;
use std::path::{Path, PathBuf};

#[cfg(unix)]
use std::ffi::CString;
#[cfg(windows)]
use std::ffi::OsString;
#[cfg(unix)]
use std::os::fd::{AsRawFd, FromRawFd};
#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;
#[cfg(windows)]
use std::os::windows::ffi::OsStringExt;
#[cfg(windows)]
use std::os::windows::fs::OpenOptionsExt;
#[cfg(windows)]
use std::os::windows::io::AsRawHandle;

#[cfg(windows)]
use windows_sys::Win32::Storage::FileSystem::{GetFinalPathNameByHandleW, FILE_NAME_NORMALIZED};

pub const FLAT_DESTINATION_ROOT: &str = "chatgpt_project_sources";
const MAX_FLAT_FILES: usize = 512;
const MAX_FLAT_FILE_BYTES: u64 = 8 * 1024 * 1024;
const MAX_FLAT_TOTAL_BYTES: u64 = 64 * 1024 * 1024;

pub fn build_artifacts(
    prepared: &[PreparedFile],
    generated: &[GeneratedArtifact],
    project_root: &Path,
) -> Result<Vec<GeneratedArtifact>, AppError> {
    let mut sources = BTreeMap::<String, Vec<u8>>::new();
    for file in prepared {
        if !is_flatten_source_candidate(&file.destination) {
            continue;
        }
        insert_source(
            &mut sources,
            &file.destination.replace('\\', "/"),
            file.bytes.clone(),
        )?;
    }
    for file in generated {
        if !matches!(
            file.destination.replace('\\', "/").as_str(),
            "AGENTS.md" | "README.md"
        ) {
            continue;
        }
        insert_source(
            &mut sources,
            &file.destination.replace('\\', "/"),
            file.bytes
                .clone()
                .unwrap_or_else(|| file.content.as_bytes().to_vec()),
        )?;
    }
    for required in ["AGENTS.md", "README.md"] {
        if !sources.contains_key(required) {
            let path = safe_join(project_root, required)?;
            if path.is_file() {
                insert_source(
                    &mut sources,
                    required,
                    read_regular_file_no_follow_under_root(project_root, required)?,
                )?;
            }
        }
    }
    let mut outputs = BTreeMap::<String, (String, Vec<u8>)>::new();
    let mut skill_count = 0usize;
    let mut subagent_count = 0usize;
    for (source, bytes) in &sources {
        if source == "AGENTS.md" || source == "README.md" {
            std::str::from_utf8(bytes).map_err(|_| {
                AppError::Serialization(format!(
                    "flattened required file is not valid UTF-8: {source}"
                ))
            })?;
            insert_output(&mut outputs, source, bytes.clone())?;
            continue;
        }
        let parts = source.split('/').collect::<Vec<_>>();
        if parts.len() == 4
            && parts[0] == ".agents"
            && parts[1] == "skills"
            && parts[3] == "SKILL.md"
        {
            let skill = parts[parts.len() - 2];
            std::str::from_utf8(bytes).map_err(|_| {
                AppError::Serialization(format!("flattened skill is not valid UTF-8: {source}"))
            })?;
            let destination = format!("{skill}.md");
            insert_output(&mut outputs, &destination, bytes.clone())?;
            skill_count += 1;
        } else if parts.len() == 3
            && parts[0] == ".codex"
            && parts[1] == "agents"
            && parts[2].ends_with(".toml")
        {
            std::str::from_utf8(bytes).map_err(|_| {
                AppError::Serialization(format!("flattened subagent is not valid UTF-8: {source}"))
            })?;
            insert_output(&mut outputs, parts[2], bytes.clone())?;
            subagent_count += 1;
        }
    }
    if !outputs.contains_key(&canonical_relative_key("AGENTS.md")?)
        || !outputs.contains_key(&canonical_relative_key("README.md")?)
    {
        return Err(AppError::InvalidInput(
            "flattened Chat sources require AGENTS.md and README.md".into(),
        ));
    }
    if skill_count == 0 || subagent_count == 0 {
        return Err(AppError::InvalidInput(
            "flattened Chat sources require at least one skill and one subagent".into(),
        ));
    }

    if outputs.len() > MAX_FLAT_FILES {
        return Err(AppError::InvalidInput(
            "flattened Chat sources exceed the aggregate file limit".into(),
        ));
    }
    let total_bytes = outputs
        .values()
        .try_fold(0_u64, |total, (_, bytes)| {
            total.checked_add(bytes.len() as u64)
        })
        .ok_or_else(|| AppError::InvalidInput("flattened Chat source size overflows".into()))?;
    if total_bytes > MAX_FLAT_TOTAL_BYTES {
        return Err(AppError::InvalidInput(
            "flattened Chat sources exceed the aggregate size limit".into(),
        ));
    }
    Ok(outputs
        .into_iter()
        .map(|(_, (destination, bytes))| GeneratedArtifact {
            component_id: "codex.chat_flatten".into(),
            destination: format!("{FLAT_DESTINATION_ROOT}/{destination}"),
            expected_sha256: crate::security::sha256_bytes(&bytes),
            content: String::from_utf8(bytes.clone())
                .unwrap_or_else(|_| "[binary source; hash-verified by the core]".into()),
            external: false,
            bytes: (!bytes.is_empty() && String::from_utf8(bytes.clone()).is_err())
                .then_some(bytes),
        })
        .collect())
}

fn is_flatten_source_candidate(destination: &str) -> bool {
    let normalized = destination.replace('\\', "/");
    if matches!(normalized.as_str(), "AGENTS.md" | "README.md") {
        return true;
    }
    let parts = normalized.split('/').collect::<Vec<_>>();
    (parts.len() == 4 && parts[0] == ".agents" && parts[1] == "skills" && parts[3] == "SKILL.md")
        || (parts.len() == 3
            && parts[0] == ".codex"
            && parts[1] == "agents"
            && parts[2].ends_with(".toml"))
}

pub(crate) fn read_regular_file_no_follow_under_root(
    root: &Path,
    relative: &str,
) -> Result<Vec<u8>, AppError> {
    read_bounded_regular_file_no_follow_under_root(root, relative, MAX_FLAT_FILE_BYTES)
}

pub(crate) fn read_bounded_regular_file_no_follow_under_root(
    root: &Path,
    relative: &str,
    max_bytes: u64,
) -> Result<Vec<u8>, AppError> {
    read_bounded_regular_file_no_follow_under_root_with_check(root, relative, max_bytes, || false)
}

pub(crate) fn read_bounded_regular_file_no_follow_under_root_with_check<F>(
    root: &Path,
    relative: &str,
    max_bytes: u64,
    should_stop: F,
) -> Result<Vec<u8>, AppError>
where
    F: FnMut() -> bool,
{
    let root = BoundedReadRoot::open(root)?;
    root.read_bounded_with_check(relative, max_bytes, should_stop)
}

/// An opened, no-follow root used to bind every bounded read to the directory
/// that was approved at the start of an operation. On Unix, descendants are
/// opened relative to this retained descriptor. On Windows, the retained
/// handle deliberately denies delete sharing, preventing rename/replacement of
/// the root while path-based descendant handles are opened.
#[derive(Debug)]
pub(crate) struct BoundedReadRoot {
    canonical: PathBuf,
    #[cfg_attr(windows, allow(dead_code))]
    handle: std::fs::File,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FileSystemIdentity {
    volume: u64,
    file: u64,
}

impl BoundedReadRoot {
    pub(crate) fn open(root: &Path) -> Result<Self, AppError> {
        let expected_identity = path_identity_no_follow(root).ok_or_else(|| {
            AppError::PathSecurity("selected directory identity could not be captured".into())
        })?;
        Self::open_with_expected_identity(root, expected_identity)
    }

    fn open_with_expected_identity(
        root: &Path,
        expected_identity: FileSystemIdentity,
    ) -> Result<Self, AppError> {
        if crate::security::path_has_link_component(root) {
            return Err(AppError::PathSecurity(
                "selected directory contains a symlink or junction".into(),
            ));
        }
        let handle = open_directory_no_follow(root).map_err(|error| {
            AppError::PathSecurity(format!("selected directory is not accessible: {error}"))
        })?;
        let metadata = handle.metadata()?;
        if !metadata.is_dir() || is_link_metadata(&metadata) {
            return Err(AppError::PathSecurity(
                "selected directory is not a regular directory".into(),
            ));
        }
        if handle_identity(&handle) != Some(expected_identity) {
            return Err(AppError::PathSecurity(
                "selected directory changed identity while it was opened".into(),
            ));
        }
        let canonical = open_directory_path(&handle, root)?;
        if crate::security::path_has_link_component(&canonical) {
            return Err(AppError::PathSecurity(
                "selected directory resolved through a symlink or junction".into(),
            ));
        }
        Ok(Self { canonical, handle })
    }

    pub(crate) fn canonical(&self) -> &Path {
        &self.canonical
    }

    pub(crate) fn directory_handle(&self) -> &std::fs::File {
        &self.handle
    }

    /// A process working-directory path tied to the retained directory handle.
    /// Unix resolves the descriptor before exec; Windows relies on the retained
    /// non-delete-sharing handle to keep the canonical directory in place.
    pub(crate) fn stable_process_path(&self) -> PathBuf {
        #[cfg(all(unix, not(target_os = "macos")))]
        {
            PathBuf::from(format!("/proc/self/fd/{}", self.handle.as_raw_fd()))
        }
        #[cfg(target_os = "macos")]
        {
            self.canonical.clone()
        }
        #[cfg(not(unix))]
        {
            self.canonical.clone()
        }
    }

    pub(crate) fn open_child_directory(&self, relative: &str) -> Result<Self, AppError> {
        let normalized = normalize_relative_path(relative)?;
        let canonical = safe_join(&self.canonical, &normalized)?;
        let handle = open_directory_under_root(self, &normalized).map_err(|error| {
            AppError::PathSecurity(format!(
                "contained directory is not accessible: {relative}: {error}"
            ))
        })?;
        let metadata = handle.metadata()?;
        if !metadata.is_dir() || is_link_metadata(&metadata) {
            return Err(AppError::PathSecurity(format!(
                "contained directory is not a regular directory: {relative}"
            )));
        }
        verify_open_file_contained(&handle, &self.canonical, relative)?;
        Ok(Self { canonical, handle })
    }

    /// Keep the retained directory open while running `operation` and confirm
    /// that the approved path still names the same directory immediately
    /// before and after it. The retained handle prevents replacement on
    /// Windows; the identity fence detects Unix rename/replacement races.
    pub(crate) fn with_stable_path<T>(
        &self,
        operation: impl FnOnce(&Path) -> T,
    ) -> Result<T, AppError> {
        let expected = handle_identity(&self.handle).ok_or_else(|| {
            AppError::PathSecurity("contained directory identity is unavailable".into())
        })?;
        if path_identity_no_follow(&self.canonical) != Some(expected) {
            return Err(AppError::PathSecurity(
                "contained directory changed identity before use".into(),
            ));
        }
        let process_path = self.stable_process_path();
        let output = operation(&process_path);
        if path_identity_no_follow(&self.canonical) != Some(expected) {
            return Err(AppError::PathSecurity(
                "contained directory changed identity during use".into(),
            ));
        }
        Ok(output)
    }

    pub(crate) fn read_bounded_with_check<F>(
        &self,
        relative: &str,
        max_bytes: u64,
        mut should_stop: F,
    ) -> Result<Vec<u8>, AppError>
    where
        F: FnMut() -> bool,
    {
        let normalized = normalize_relative_path(relative)?;
        let path = safe_join(&self.canonical, &normalized)?;
        let mut file = open_regular_file_under_root(self, &normalized).map_err(|error| {
            AppError::InvalidInput(format!(
                "flattened file is not readable: {relative}: {error}"
            ))
        })?;
        verify_open_file_contained(&file, &self.canonical, relative)?;
        let metadata = file.metadata()?;
        if !metadata.is_file() || is_link_metadata(&metadata) {
            return Err(AppError::PathSecurity(format!(
                "flattened file is not a regular file: {relative}"
            )));
        }
        if metadata.len() > max_bytes {
            return Err(AppError::InvalidInput(format!(
                "flattened file exceeds the {} MiB limit: {relative}",
                max_bytes / 1024 / 1024
            )));
        }
        let mut bytes = Vec::with_capacity((metadata.len().min(max_bytes)) as usize);
        let mut chunk = [0_u8; 64 * 1024];
        while bytes.len() as u64 <= max_bytes {
            if should_stop() {
                return Err(AppError::Scan(
                    "bounded file read stopped by the caller".into(),
                ));
            }
            let remaining = max_bytes
                .saturating_add(1)
                .saturating_sub(bytes.len() as u64) as usize;
            let chunk_len = chunk.len().min(remaining);
            let read = file.read(&mut chunk[..chunk_len]).map_err(|error| {
                AppError::InvalidInput(format!(
                    "flattened file could not be read: {relative}: {error}"
                ))
            })?;
            if read == 0 {
                break;
            }
            bytes.extend_from_slice(&chunk[..read]);
        }
        if bytes.len() as u64 > max_bytes {
            return Err(AppError::InvalidInput(format!(
                "flattened file grew beyond the {} MiB limit while it was read: {relative}",
                max_bytes / 1024 / 1024
            )));
        }
        let after = file.metadata()?;
        verify_open_file_contained(&file, &self.canonical, relative)?;
        if !after.is_file()
            || is_link_metadata(&after)
            || after.len() != bytes.len() as u64
            || path_has_link_component_for_flatten(&path)
        {
            return Err(AppError::PathSecurity(format!(
                "flattened file changed or became a link during read: {relative}"
            )));
        }
        Ok(bytes)
    }
}

#[cfg(unix)]
fn identity_from_metadata(metadata: &std::fs::Metadata) -> FileSystemIdentity {
    use std::os::unix::fs::MetadataExt;
    FileSystemIdentity {
        volume: metadata.dev(),
        file: metadata.ino(),
    }
}

#[cfg(unix)]
fn path_identity_no_follow(path: &Path) -> Option<FileSystemIdentity> {
    let metadata = fs::symlink_metadata(path).ok()?;
    (!is_link_metadata(&metadata) && metadata.is_dir()).then(|| identity_from_metadata(&metadata))
}

#[cfg(unix)]
fn handle_identity(file: &std::fs::File) -> Option<FileSystemIdentity> {
    file.metadata()
        .ok()
        .map(|metadata| identity_from_metadata(&metadata))
}

#[cfg(windows)]
fn identity_from_handle(file: &std::fs::File) -> Option<FileSystemIdentity> {
    use windows_sys::Win32::Storage::FileSystem::{
        GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION,
    };
    let mut information = unsafe { std::mem::zeroed::<BY_HANDLE_FILE_INFORMATION>() };
    let success = unsafe {
        GetFileInformationByHandle(file.as_raw_handle() as _, &mut information as *mut _)
    };
    (success != 0).then_some(FileSystemIdentity {
        volume: information.dwVolumeSerialNumber as u64,
        file: ((information.nFileIndexHigh as u64) << 32) | information.nFileIndexLow as u64,
    })
}

#[cfg(windows)]
fn path_identity_no_follow(path: &Path) -> Option<FileSystemIdentity> {
    let file = open_directory_no_follow(path).ok()?;
    identity_from_handle(&file)
}

#[cfg(windows)]
fn handle_identity(file: &std::fs::File) -> Option<FileSystemIdentity> {
    identity_from_handle(file)
}

#[cfg(not(any(unix, windows)))]
fn path_identity_no_follow(path: &Path) -> Option<FileSystemIdentity> {
    let metadata = std::fs::symlink_metadata(path).ok()?;
    (!is_link_metadata(&metadata) && metadata.is_dir()).then_some(FileSystemIdentity {
        volume: 0,
        file: metadata.len(),
    })
}

#[cfg(not(any(unix, windows)))]
fn handle_identity(file: &std::fs::File) -> Option<FileSystemIdentity> {
    let metadata = file.metadata().ok()?;
    Some(FileSystemIdentity {
        volume: 0,
        file: metadata.len(),
    })
}

#[cfg(unix)]
fn open_directory_no_follow(path: &Path) -> Result<std::fs::File, std::io::Error> {
    OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(path)
}

#[cfg(windows)]
fn open_directory_no_follow(path: &Path) -> Result<std::fs::File, std::io::Error> {
    const FILE_SHARE_READ: u32 = 0x0000_0001;
    const FILE_SHARE_WRITE: u32 = 0x0000_0002;
    let mut options = OpenOptions::new();
    options.read(true);
    options.share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE);
    options.custom_flags(0x0200_0000 | 0x0020_0000);
    options.open(path)
}

#[cfg(all(unix, not(target_os = "macos")))]
fn open_directory_path(file: &std::fs::File, _display: &Path) -> Result<PathBuf, AppError> {
    let descriptor_path = PathBuf::from(format!("/proc/self/fd/{}", file.as_raw_fd()));
    fs::read_link(&descriptor_path).map_err(|error| {
        AppError::PathSecurity(format!(
            "selected directory handle could not be resolved through {}: {error}",
            descriptor_path.display()
        ))
    })
}

#[cfg(target_os = "macos")]
fn open_directory_path(file: &std::fs::File, _display: &Path) -> Result<PathBuf, AppError> {
    use std::ffi::CStr;
    use std::os::unix::ffi::OsStrExt;

    let mut buffer = vec![0_i8; libc::PATH_MAX as usize + 1];
    let result = unsafe { libc::fcntl(file.as_raw_fd(), libc::F_GETPATH, buffer.as_mut_ptr()) };
    if result == -1 {
        return Err(AppError::PathSecurity(format!(
            "selected directory handle could not be resolved: {}",
            std::io::Error::last_os_error()
        )));
    }
    let bytes = unsafe { CStr::from_ptr(buffer.as_ptr()) }.to_bytes();
    Ok(PathBuf::from(std::ffi::OsStr::from_bytes(bytes)))
}

#[cfg(windows)]
fn open_directory_path(file: &std::fs::File, _display: &Path) -> Result<PathBuf, AppError> {
    Ok(PathBuf::from(windows_final_path(file)?))
}

#[cfg(not(any(unix, windows)))]
fn open_directory_no_follow(path: &Path) -> Result<std::fs::File, std::io::Error> {
    OpenOptions::new().read(true).open(path)
}

#[cfg(not(any(unix, windows)))]
fn open_directory_path(_file: &std::fs::File, display: &Path) -> Result<PathBuf, AppError> {
    fs::canonicalize(display).map_err(AppError::from)
}

#[cfg(unix)]
fn open_regular_file_under_root(
    root: &BoundedReadRoot,
    relative: &str,
) -> Result<std::fs::File, std::io::Error> {
    open_relative_under_root(root, relative, false)
}

#[cfg(unix)]
fn open_directory_under_root(
    root: &BoundedReadRoot,
    relative: &str,
) -> Result<std::fs::File, std::io::Error> {
    open_relative_under_root(root, relative, true)
}

#[cfg(unix)]
fn open_relative_under_root(
    root: &BoundedReadRoot,
    relative: &str,
    final_directory: bool,
) -> Result<std::fs::File, std::io::Error> {
    let mut directory = root.handle.try_clone()?;
    let parts = relative.split('/').collect::<Vec<_>>();
    for (index, part) in parts.iter().enumerate() {
        let name = CString::new(part.as_bytes()).map_err(|_| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "path segment contains NUL",
            )
        })?;
        let is_final = index + 1 == parts.len();
        let mut flags = libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_CLOEXEC;
        if !is_final || final_directory {
            flags |= libc::O_DIRECTORY;
        }
        let descriptor = unsafe { libc::openat(directory.as_raw_fd(), name.as_ptr(), flags) };
        if descriptor < 0 {
            return Err(std::io::Error::last_os_error());
        }
        let next = unsafe { std::fs::File::from_raw_fd(descriptor) };
        if is_final {
            return Ok(next);
        }
        directory = next;
    }
    Err(std::io::Error::new(
        std::io::ErrorKind::InvalidInput,
        "relative path has no file name",
    ))
}

#[cfg(windows)]
fn open_regular_file_under_root(
    root: &BoundedReadRoot,
    relative: &str,
) -> Result<std::fs::File, std::io::Error> {
    let path = safe_join(&root.canonical, relative).map_err(|error| {
        std::io::Error::new(std::io::ErrorKind::PermissionDenied, error.to_string())
    })?;
    let mut options = OpenOptions::new();
    options.read(true);
    options.custom_flags(0x0020_0000);
    options.open(path)
}

#[cfg(windows)]
fn open_directory_under_root(
    root: &BoundedReadRoot,
    relative: &str,
) -> Result<std::fs::File, std::io::Error> {
    let path = safe_join(&root.canonical, relative).map_err(|error| {
        std::io::Error::new(std::io::ErrorKind::PermissionDenied, error.to_string())
    })?;
    open_directory_no_follow(&path)
}

#[cfg(not(any(unix, windows)))]
fn open_directory_under_root(
    root: &BoundedReadRoot,
    relative: &str,
) -> Result<std::fs::File, std::io::Error> {
    let path = safe_join(&root.canonical, relative).map_err(|error| {
        std::io::Error::new(std::io::ErrorKind::PermissionDenied, error.to_string())
    })?;
    open_directory_no_follow(&path)
}

fn verify_open_file_contained(
    file: &std::fs::File,
    root: &Path,
    display_path: &str,
) -> Result<(), AppError> {
    #[cfg(windows)]
    {
        let final_path = windows_final_path(file)?;
        let root = root
            .to_string_lossy()
            .trim_end_matches(['\\', '/'])
            .to_ascii_lowercase();
        let final_path = final_path
            .to_string_lossy()
            .trim_end_matches(['\\', '/'])
            .to_ascii_lowercase();
        if final_path != root
            && !final_path.starts_with(&(root.clone() + "\\"))
            && !final_path.starts_with(&(root + "/"))
        {
            return Err(AppError::PathSecurity(format!(
                "flattened file handle escaped its root: {display_path}"
            )));
        }
    }
    #[cfg(not(windows))]
    {
        let _ = (file, root, display_path);
    }
    Ok(())
}

#[cfg(windows)]
fn windows_final_path(file: &std::fs::File) -> Result<OsString, AppError> {
    let handle = file.as_raw_handle();
    let mut buffer = vec![0_u16; 512];
    loop {
        let length = unsafe {
            GetFinalPathNameByHandleW(
                handle,
                buffer.as_mut_ptr(),
                buffer.len() as u32,
                FILE_NAME_NORMALIZED,
            )
        };
        if length == 0 {
            return Err(AppError::PathSecurity(format!(
                "cannot resolve flattened file handle: {}",
                std::io::Error::last_os_error()
            )));
        }
        if (length as usize) < buffer.len() {
            return Ok(OsString::from_wide(&buffer[..length as usize]));
        }
        if buffer.len() >= 32 * 1024 {
            return Err(AppError::PathSecurity(
                "flattened file handle path is oversized".into(),
            ));
        }
        buffer.resize(buffer.len() * 2, 0);
    }
}

fn path_has_link_component_for_flatten(path: &Path) -> bool {
    crate::security::path_has_link_component(path)
}

fn insert_output(
    outputs: &mut BTreeMap<String, (String, Vec<u8>)>,
    destination: &str,
    bytes: Vec<u8>,
) -> Result<(), AppError> {
    if bytes.len() as u64 > MAX_FLAT_FILE_BYTES {
        return Err(AppError::InvalidInput(format!(
            "flattened Chat source exceeds the {} MiB per-file limit: {destination}",
            MAX_FLAT_FILE_BYTES / 1024 / 1024
        )));
    }
    let text = String::from_utf8_lossy(&bytes);
    if contains_credential_shaped_content(&text) {
        return Err(AppError::Credential(format!(
            "flattened Chat source contains secret-shaped content: {destination}"
        )));
    }
    let key = canonical_relative_key(destination)?;
    if let Some((existing_destination, _)) = outputs.get(&key) {
        return Err(AppError::Transaction(format!(
            "flattened Chat source name collision: {destination} conflicts with {existing_destination}"
        )));
    }
    outputs.insert(key, (destination.into(), bytes));
    Ok(())
}

fn insert_source(
    sources: &mut BTreeMap<String, Vec<u8>>,
    destination: &str,
    bytes: Vec<u8>,
) -> Result<(), AppError> {
    if bytes.len() as u64 > MAX_FLAT_FILE_BYTES {
        return Err(AppError::InvalidInput(format!(
            "flattened source exceeds the {} MiB per-file limit: {destination}",
            MAX_FLAT_FILE_BYTES / 1024 / 1024
        )));
    }
    if let Some(existing) = sources.get(destination) {
        if existing == &bytes {
            return Ok(());
        }
        return Err(AppError::Transaction(format!(
            "flattened source input collision: {destination}"
        )));
    }
    if sources.len() >= MAX_FLAT_FILES {
        return Err(AppError::InvalidInput(
            "flattened Chat sources exceed the aggregate file limit".into(),
        ));
    }
    let current_total = sources.values().try_fold(0_u64, |total, existing| {
        total.checked_add(existing.len() as u64)
    });
    let next_total = current_total
        .and_then(|total| total.checked_add(bytes.len() as u64))
        .ok_or_else(|| AppError::InvalidInput("flattened Chat source size overflows".into()))?;
    if next_total > MAX_FLAT_TOTAL_BYTES {
        return Err(AppError::InvalidInput(
            "flattened Chat sources exceed the aggregate size limit".into(),
        ));
    }
    sources.insert(destination.into(), bytes);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    fn prepared(destination: &str, content: &str) -> PreparedFile {
        PreparedFile {
            operation_id: destination.into(),
            destination: destination.into(),
            bytes: content.as_bytes().to_vec(),
            expected_sha256: crate::security::sha256_bytes(content.as_bytes()),
        }
    }

    #[test]
    fn skills_are_flattened_and_required_files_are_preserved() {
        let project = tempfile::tempdir().unwrap();
        let output = build_artifacts(
            &[
                prepared("AGENTS.md", "adapted agents"),
                prepared(".agents/skills/hoi4-events/SKILL.md", "events"),
                prepared(".codex/agents/hoi4setup_worker.toml", "fork_context=false"),
                prepared("paradox_wiki/AGENTS.md", "wiki instructions"),
            ],
            &[GeneratedArtifact {
                component_id: "project.readme".into(),
                destination: "README.md".into(),
                content: "readme".into(),
                expected_sha256: crate::security::sha256_bytes(b"readme"),
                external: false,
                bytes: None,
            }],
            project.path(),
        )
        .unwrap();
        let destinations = output
            .iter()
            .map(|artifact| artifact.destination.as_str())
            .collect::<BTreeSet<_>>();
        assert!(destinations.contains("chatgpt_project_sources/hoi4-events.md"));
        assert!(destinations.contains("chatgpt_project_sources/hoi4setup_worker.toml"));
        assert!(destinations.contains("chatgpt_project_sources/AGENTS.md"));
        assert!(destinations.contains("chatgpt_project_sources/README.md"));
        assert!(!destinations.contains("chatgpt_project_sources/paradox_wiki/AGENTS.md"));
        assert_eq!(
            output
                .iter()
                .find(|artifact| artifact.destination == "chatgpt_project_sources/AGENTS.md")
                .map(|artifact| artifact.content.as_str()),
            Some("adapted agents")
        );
    }

    #[test]
    fn unrelated_selected_files_do_not_consume_the_flatten_file_limit() {
        let project = tempfile::tempdir().unwrap();
        let mut prepared_files = (0..600)
            .map(|index| prepared(&format!("paradox_wiki/page-{index}.md"), "wiki"))
            .collect::<Vec<_>>();
        prepared_files.push(prepared(".agents/skills/hoi4-events/SKILL.md", "events"));
        prepared_files.push(prepared(
            ".codex/agents/hoi4setup_worker.toml",
            "fork_context=false",
        ));
        let generated = [
            GeneratedArtifact {
                component_id: "core.agents".into(),
                destination: "AGENTS.md".into(),
                content: "agents".into(),
                expected_sha256: crate::security::sha256_bytes(b"agents"),
                external: false,
                bytes: None,
            },
            GeneratedArtifact {
                component_id: "project.readme".into(),
                destination: "README.md".into(),
                content: "readme".into(),
                expected_sha256: crate::security::sha256_bytes(b"readme"),
                external: false,
                bytes: None,
            },
        ];

        let output = build_artifacts(&prepared_files, &generated, project.path()).unwrap();

        assert_eq!(output.len(), 4);
        assert!(output
            .iter()
            .all(|artifact| !artifact.destination.contains("paradox_wiki")));
    }

    #[test]
    fn documented_meshy_placeholder_is_not_treated_as_a_real_credential() {
        let project = tempfile::tempdir().unwrap();
        let output = build_artifacts(
            &[
                prepared(
                    ".agents/skills/hoi4-3d-model-pipeline/SKILL.md",
                    "Use MESHY_API_KEY with msy_your_actual_key_here only as documentation.",
                ),
                prepared(".codex/agents/worker.toml", "fork_context=false"),
            ],
            &[
                GeneratedArtifact {
                    component_id: "core.agents".into(),
                    destination: "AGENTS.md".into(),
                    content: "agents".into(),
                    expected_sha256: crate::security::sha256_bytes(b"agents"),
                    external: false,
                    bytes: None,
                },
                GeneratedArtifact {
                    component_id: "project.readme".into(),
                    destination: "README.md".into(),
                    content: "readme".into(),
                    expected_sha256: crate::security::sha256_bytes(b"readme"),
                    external: false,
                    bytes: None,
                },
            ],
            project.path(),
        )
        .unwrap();

        assert!(output
            .iter()
            .any(|artifact| artifact.destination.ends_with("hoi4-3d-model-pipeline.md")));
    }

    #[test]
    fn real_meshy_shaped_values_remain_blocked_from_flattening() {
        let project = tempfile::tempdir().unwrap();
        let error = build_artifacts(
            &[
                prepared(
                    ".agents/skills/hoi4-3d-model-pipeline/SKILL.md",
                    &format!("MESHY_API_KEY={}{}", "msy_123456", "7890abcdef"),
                ),
                prepared(".codex/agents/worker.toml", "fork_context=false"),
            ],
            &[
                GeneratedArtifact {
                    component_id: "core.agents".into(),
                    destination: "AGENTS.md".into(),
                    content: "agents".into(),
                    expected_sha256: crate::security::sha256_bytes(b"agents"),
                    external: false,
                    bytes: None,
                },
                GeneratedArtifact {
                    component_id: "project.readme".into(),
                    destination: "README.md".into(),
                    content: "readme".into(),
                    expected_sha256: crate::security::sha256_bytes(b"readme"),
                    external: false,
                    bytes: None,
                },
            ],
            project.path(),
        )
        .unwrap_err();

        assert!(error.to_string().contains("secret-shaped content"));
    }

    #[test]
    fn missing_required_flatten_inputs_fail_closed() {
        let project = tempfile::tempdir().unwrap();
        assert!(build_artifacts(
            &[prepared(".agents/skills/one/SKILL.md", "one")],
            &[],
            project.path(),
        )
        .is_err());
        assert!(build_artifacts(
            &[
                prepared(".agents/skills/one/SKILL.md", "one"),
                prepared(".agents/skills/two/SKILL.md", "two"),
            ],
            &[
                GeneratedArtifact {
                    component_id: "core.agents".into(),
                    destination: "AGENTS.md".into(),
                    content: "agents".into(),
                    expected_sha256: crate::security::sha256_bytes(b"agents"),
                    external: false,
                    bytes: None,
                },
                GeneratedArtifact {
                    component_id: "project.readme".into(),
                    destination: "README.md".into(),
                    content: "readme".into(),
                    expected_sha256: crate::security::sha256_bytes(b"readme"),
                    external: false,
                    bytes: None,
                },
            ],
            project.path(),
        )
        .is_err());
    }

    #[test]
    fn case_collisions_fail_closed() {
        let project = tempfile::tempdir().unwrap();
        let generated = vec![
            GeneratedArtifact {
                component_id: "core.agents".into(),
                destination: "AGENTS.md".into(),
                content: "agents".into(),
                expected_sha256: crate::security::sha256_bytes(b"agents"),
                external: false,
                bytes: None,
            },
            GeneratedArtifact {
                component_id: "project.readme".into(),
                destination: "README.md".into(),
                content: "readme".into(),
                expected_sha256: crate::security::sha256_bytes(b"readme"),
                external: false,
                bytes: None,
            },
        ];
        let inputs = [
            prepared(".agents/skills/one/SKILL.md", "one"),
            prepared(".agents/skills/two/SKILL.md", "two"),
            prepared(".agents/skills/ONE/SKILL.md", "one"),
            prepared(".codex/agents/one.toml", "agent"),
        ];
        assert!(build_artifacts(&inputs, &generated, project.path()).is_err());
    }

    #[test]
    fn unselected_existing_skills_and_subagents_are_not_flattened() {
        let project = tempfile::tempdir().unwrap();
        fs::create_dir_all(project.path().join(".agents/skills/unselected")).unwrap();
        fs::create_dir_all(project.path().join(".codex/agents")).unwrap();
        fs::write(
            project.path().join(".agents/skills/unselected/SKILL.md"),
            "unselected",
        )
        .unwrap();
        fs::write(
            project.path().join(".codex/agents/unselected.toml"),
            "unselected",
        )
        .unwrap();
        let output = build_artifacts(
            &[
                prepared(".agents/skills/selected/SKILL.md", "selected"),
                prepared(".codex/agents/selected.toml", "selected"),
            ],
            &[
                GeneratedArtifact {
                    component_id: "core.agents".into(),
                    destination: "AGENTS.md".into(),
                    content: "agents".into(),
                    expected_sha256: crate::security::sha256_bytes(b"agents"),
                    external: false,
                    bytes: None,
                },
                GeneratedArtifact {
                    component_id: "project.readme".into(),
                    destination: "README.md".into(),
                    content: "readme".into(),
                    expected_sha256: crate::security::sha256_bytes(b"readme"),
                    external: false,
                    bytes: None,
                },
            ],
            project.path(),
        )
        .unwrap();
        let destinations = output
            .iter()
            .map(|artifact| artifact.destination.as_str())
            .collect::<BTreeSet<_>>();
        assert!(destinations.contains("chatgpt_project_sources/selected.md"));
        assert!(destinations.contains("chatgpt_project_sources/selected.toml"));
        assert!(!destinations.contains("chatgpt_project_sources/unselected.md"));
        assert!(!destinations.contains("chatgpt_project_sources/unselected.toml"));
    }

    #[test]
    fn regular_reader_binds_file_access_to_the_project_root() {
        let project = tempfile::tempdir().unwrap();
        fs::create_dir_all(project.path().join("nested")).unwrap();
        fs::write(project.path().join("nested/file.txt"), "inside").unwrap();

        let bytes =
            read_regular_file_no_follow_under_root(project.path(), "nested/file.txt").unwrap();

        assert_eq!(bytes, b"inside");
    }

    #[test]
    fn root_open_rejects_a_directory_replaced_after_identity_capture() {
        let container = tempfile::tempdir().unwrap();
        let root = container.path().join("project");
        let original = container.path().join("original");
        fs::create_dir(&root).unwrap();
        let expected = path_identity_no_follow(&root).unwrap();
        fs::rename(&root, &original).unwrap();
        fs::create_dir(&root).unwrap();

        let error = BoundedReadRoot::open_with_expected_identity(&root, expected).unwrap_err();
        assert!(error.to_string().contains("changed identity"));
    }

    #[cfg(unix)]
    #[test]
    fn retained_root_handle_reads_the_original_directory_after_path_replacement() {
        let container = tempfile::tempdir().unwrap();
        let root = container.path().join("project");
        let original = container.path().join("original");
        fs::create_dir(&root).unwrap();
        fs::write(root.join("file.txt"), "original").unwrap();
        let bounded_root = BoundedReadRoot::open(&root).unwrap();

        fs::rename(&root, &original).unwrap();
        fs::create_dir(&root).unwrap();
        fs::write(root.join("file.txt"), "replacement").unwrap();

        let bytes = bounded_root
            .read_bounded_with_check("file.txt", 1024, || false)
            .unwrap();
        assert_eq!(bytes, b"original");
    }

    #[cfg(windows)]
    #[test]
    fn retained_root_handle_blocks_windows_directory_replacement() {
        let container = tempfile::tempdir().unwrap();
        let root = container.path().join("project");
        let moved = container.path().join("moved");
        fs::create_dir(&root).unwrap();
        fs::write(root.join("file.txt"), "original").unwrap();
        let bounded_root = BoundedReadRoot::open(&root).unwrap();

        assert!(fs::rename(&root, &moved).is_err());
        let bytes = bounded_root
            .read_bounded_with_check("file.txt", 1024, || false)
            .unwrap();
        assert_eq!(bytes, b"original");
    }

    #[cfg(windows)]
    #[test]
    fn retained_child_directory_blocks_windows_git_replacement() {
        let container = tempfile::tempdir().unwrap();
        let root = container.path().join("project");
        let moved = container.path().join("moved-git");
        fs::create_dir_all(root.join(".git")).unwrap();
        fs::write(root.join(".git/HEAD"), "ref: refs/heads/main\n").unwrap();
        let bounded_root = BoundedReadRoot::open(&root).unwrap();
        let git_root = bounded_root.open_child_directory(".git").unwrap();

        assert!(fs::rename(root.join(".git"), &moved).is_err());
        let bytes = git_root
            .read_bounded_with_check("HEAD", 1024, || false)
            .unwrap();
        assert_eq!(bytes, b"ref: refs/heads/main\n");
    }

    #[cfg(unix)]
    #[test]
    fn linked_required_sources_are_rejected_before_flattening() {
        use std::os::unix::fs::symlink;

        let project = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        fs::write(outside.path().join("AGENTS.md"), "outside").unwrap();
        symlink(
            outside.path().join("AGENTS.md"),
            project.path().join("AGENTS.md"),
        )
        .unwrap();

        let error = build_artifacts(
            &[
                prepared(".agents/skills/one/SKILL.md", "one"),
                prepared(".codex/agents/worker.toml", "fork_context=false"),
            ],
            &[GeneratedArtifact {
                component_id: "project.readme".into(),
                destination: "README.md".into(),
                content: "readme".into(),
                expected_sha256: crate::security::sha256_bytes(b"readme"),
                external: false,
                bytes: None,
            }],
            project.path(),
        )
        .unwrap_err();
        assert!(error.to_string().contains("link"));
    }
}
