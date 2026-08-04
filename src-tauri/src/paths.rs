use crate::AppError;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct TransactionRoots {
    pub transaction: PathBuf,
    pub backup: PathBuf,
    pub staging: PathBuf,
}

fn required_platform_root(variable: &str) -> Result<PathBuf, AppError> {
    let root = env::var_os(variable)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .ok_or_else(|| {
            AppError::PathSecurity(format!(
                "required platform directory {variable} is unavailable"
            ))
        })?;
    if !root.is_absolute() {
        return Err(AppError::PathSecurity(format!(
            "required platform directory {variable} is not absolute"
        )));
    }
    Ok(root)
}

pub fn application_data_root() -> Result<PathBuf, AppError> {
    if cfg!(target_os = "windows") {
        Ok(required_platform_root("LOCALAPPDATA")?.join("HOI4 Mod Setup"))
    } else if cfg!(target_os = "macos") {
        Ok(required_platform_root("HOME")?.join("Library/Application Support/HOI4 Mod Setup"))
    } else {
        Err(AppError::UnsupportedPlatform(
            "application data is supported only on Windows and macOS".into(),
        ))
    }
}

pub fn settings_root() -> Result<PathBuf, AppError> {
    if cfg!(target_os = "windows") {
        Ok(required_platform_root("APPDATA")?.join("HOI4 Mod Setup"))
    } else {
        application_data_root()
    }
}

pub fn cache_root() -> Result<PathBuf, AppError> {
    if cfg!(target_os = "windows") {
        Ok(application_data_root()?.join("cache"))
    } else if cfg!(target_os = "macos") {
        Ok(required_platform_root("HOME")?.join("Library/Caches/HOI4 Mod Setup"))
    } else {
        Err(AppError::UnsupportedPlatform(
            "source caching is supported only on Windows and macOS".into(),
        ))
    }
}

pub fn transaction_root(app_root: &Path, transaction_id: Uuid) -> TransactionRoots {
    let transaction = app_root
        .join("transactions")
        .join(transaction_id.to_string());
    TransactionRoots {
        backup: app_root.join("backups").join(transaction_id.to_string()),
        staging: app_root.join("staging").join(transaction_id.to_string()),
        transaction,
    }
}

pub fn project_metadata_root(project_root: &Path) -> PathBuf {
    project_root.join(".hoi4-mod-setup")
}

pub fn lock_path(project_root: &Path) -> PathBuf {
    project_metadata_root(project_root).join("install.lock.json")
}

pub fn state_path(project_root: &Path) -> PathBuf {
    project_metadata_root(project_root).join("state.json")
}

/// Formats a validated path for editable UI fields without exposing the
/// Windows verbatim-path implementation prefix. Internal filesystem work keeps
/// using the original `Path` value.
pub fn user_facing_path(path: &Path) -> String {
    let value = path.to_string_lossy().into_owned();
    #[cfg(target_os = "windows")]
    {
        if let Some(rest) = value.strip_prefix(r"\\?\UNC\") {
            return format!(r"\\{rest}");
        }
        if let Some(rest) = value.strip_prefix(r"\\?\") {
            return rest.to_owned();
        }
    }
    value
}

pub fn hoi4_user_mod_directory() -> Result<PathBuf, AppError> {
    let documents = documents_directory()?;
    let candidate = documents.join("Paradox Interactive/Hearts of Iron IV/mod");
    validate_project_root(&candidate).map_err(|_| {
        AppError::PathSecurity(format!(
            "the standard HOI4 mod folder was not found at {}",
            candidate.display()
        ))
    })
}

pub fn downloads_directory() -> Result<PathBuf, AppError> {
    #[cfg(target_os = "windows")]
    {
        use std::ffi::OsString;
        use std::os::windows::ffi::OsStringExt;
        use windows_sys::Win32::System::Com::CoTaskMemFree;
        use windows_sys::Win32::UI::Shell::{FOLDERID_Downloads, SHGetKnownFolderPath};

        let mut raw = std::ptr::null_mut::<u16>();
        let result =
            unsafe { SHGetKnownFolderPath(&FOLDERID_Downloads, 0, std::ptr::null_mut(), &mut raw) };
        if result < 0 || raw.is_null() {
            return Err(AppError::PathSecurity(
                "Windows could not resolve the Downloads folder".into(),
            ));
        }
        let length = unsafe {
            let mut length = 0usize;
            while length < 32_768 && *raw.add(length) != 0 {
                length += 1;
            }
            length
        };
        let value = if length == 32_768 {
            None
        } else {
            Some(unsafe { OsString::from_wide(std::slice::from_raw_parts(raw, length)) })
        };
        unsafe { CoTaskMemFree(raw.cast()) };
        return validate_export_directory(&PathBuf::from(value.ok_or_else(|| {
            AppError::PathSecurity("Windows returned an invalid Downloads folder".into())
        })?));
    }

    #[cfg(target_os = "macos")]
    {
        use objc2_foundation::{
            NSSearchPathDirectory, NSSearchPathDomainMask, NSSearchPathForDirectoriesInDomains,
        };

        let candidates = NSSearchPathForDirectoriesInDomains(
            NSSearchPathDirectory::DownloadsDirectory,
            NSSearchPathDomainMask::UserDomainMask,
            true,
        );
        let path = candidates
            .firstObject()
            .map(|value| PathBuf::from(value.to_string()))
            .ok_or_else(|| {
                AppError::PathSecurity("macOS could not resolve the Downloads folder".into())
            })?;
        return validate_export_directory(&path);
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        Err(AppError::UnsupportedPlatform(
            "the Downloads folder is supported only on Windows and macOS".into(),
        ))
    }
}

pub fn validate_export_directory(path: &Path) -> Result<PathBuf, AppError> {
    if !path.is_absolute()
        || path
            .components()
            .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        return Err(AppError::PathSecurity(
            "export folder must be an absolute path without traversal".into(),
        ));
    }
    if crate::security::path_has_link_component(path) {
        return Err(AppError::PathSecurity(
            "export folder contains a symlink or junction".into(),
        ));
    }
    let canonical = fs::canonicalize(path).map_err(|error| {
        AppError::PathSecurity(format!("export folder is not accessible: {error}"))
    })?;
    if !canonical.is_dir() {
        return Err(AppError::PathSecurity(
            "export folder must be a directory".into(),
        ));
    }
    if crate::security::path_has_link_component(&canonical) {
        return Err(AppError::PathSecurity(
            "export folder contains a symlink or junction".into(),
        ));
    }
    Ok(canonical)
}

#[cfg(target_os = "windows")]
fn documents_directory() -> Result<PathBuf, AppError> {
    use std::ffi::OsString;
    use std::os::windows::ffi::OsStringExt;
    use windows_sys::Win32::System::Com::CoTaskMemFree;
    use windows_sys::Win32::UI::Shell::{FOLDERID_Documents, SHGetKnownFolderPath};

    let mut raw = std::ptr::null_mut::<u16>();
    let result =
        unsafe { SHGetKnownFolderPath(&FOLDERID_Documents, 0, std::ptr::null_mut(), &mut raw) };
    if result < 0 || raw.is_null() {
        return Err(AppError::PathSecurity(
            "Windows could not resolve the Documents folder".into(),
        ));
    }
    let length = unsafe {
        let mut length = 0usize;
        while length < 32_768 && *raw.add(length) != 0 {
            length += 1;
        }
        length
    };
    let value = if length == 32_768 {
        None
    } else {
        Some(unsafe { OsString::from_wide(std::slice::from_raw_parts(raw, length)) })
    };
    unsafe { CoTaskMemFree(raw.cast()) };
    let path = PathBuf::from(value.ok_or_else(|| {
        AppError::PathSecurity("Windows returned an invalid Documents folder".into())
    })?);
    if !path.is_absolute() {
        return Err(AppError::PathSecurity(
            "Windows returned a non-absolute Documents folder".into(),
        ));
    }
    Ok(path)
}

#[cfg(target_os = "macos")]
fn documents_directory() -> Result<PathBuf, AppError> {
    use objc2_foundation::{
        NSSearchPathDirectory, NSSearchPathDomainMask, NSSearchPathForDirectoriesInDomains,
    };

    let candidates = NSSearchPathForDirectoriesInDomains(
        NSSearchPathDirectory::DocumentDirectory,
        NSSearchPathDomainMask::UserDomainMask,
        true,
    );
    let path = candidates
        .firstObject()
        .map(|value| PathBuf::from(value.to_string()))
        .ok_or_else(|| {
            AppError::PathSecurity("macOS could not resolve the Documents folder".into())
        })?;
    if !path.is_absolute() {
        return Err(AppError::PathSecurity(
            "macOS returned a non-absolute Documents folder".into(),
        ));
    }
    Ok(path)
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
fn documents_directory() -> Result<PathBuf, AppError> {
    Err(AppError::UnsupportedPlatform(
        "automatic HOI4 folder detection is supported only on Windows and macOS".into(),
    ))
}

pub fn validate_project_root(path: &Path) -> Result<PathBuf, AppError> {
    if crate::security::path_has_link_component(path) {
        return Err(AppError::PathSecurity(
            "project root contains a symlink or junction".into(),
        ));
    }
    let canonical = std::fs::canonicalize(path).map_err(|error| {
        AppError::PathSecurity(format!("project root is not accessible: {error}"))
    })?;
    if !canonical.is_dir() {
        return Err(AppError::PathSecurity(
            "project root must be a directory".into(),
        ));
    }
    if crate::security::path_has_link_component(&canonical) {
        return Err(AppError::PathSecurity(
            "project root contains a symlink or junction".into(),
        ));
    }
    Ok(canonical)
}

pub fn validate_project_root_or_destination(path: &Path) -> Result<(PathBuf, bool), AppError> {
    if path.exists() {
        return validate_project_root(path).map(|canonical| (canonical, true));
    }
    if !path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                std::path::Component::CurDir | std::path::Component::ParentDir
            )
        })
    {
        return Err(AppError::PathSecurity(
            "new project root must be an absolute path without traversal".into(),
        ));
    }
    let leaf = path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| AppError::PathSecurity("new project root has no valid name".into()))?;
    crate::security::normalize_relative_path(leaf)?;
    let parent = path
        .parent()
        .ok_or_else(|| AppError::PathSecurity("new project root has no parent".into()))?;
    let parent = validate_project_root(parent)?;
    let candidate = parent.join(leaf);
    if candidate.exists() {
        return validate_project_root(&candidate).map(|canonical| (canonical, true));
    }
    if crate::security::path_has_link_component(&candidate) {
        return Err(AppError::PathSecurity(
            "new project root contains a symlink or junction".into(),
        ));
    }
    Ok((candidate, false))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn absent_project_destination_is_validated_without_being_created() {
        let parent = tempdir().unwrap();
        let destination = parent.path().join("new_mod");

        let (validated, exists) = validate_project_root_or_destination(&destination).unwrap();

        assert!(!exists);
        assert_eq!(
            validated,
            parent.path().canonicalize().unwrap().join("new_mod")
        );
        assert!(!destination.exists());
    }

    #[test]
    fn project_destination_requires_one_leaf_below_an_existing_parent() {
        let parent = tempdir().unwrap();
        let nested = parent.path().join("missing").join("new_mod");

        assert!(validate_project_root_or_destination(&nested).is_err());
        assert!(!parent.path().join("missing").exists());
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn user_facing_paths_hide_windows_verbatim_prefixes() {
        assert_eq!(
            user_facing_path(Path::new(r"\\?\C:\Users\Example\mod")),
            r"C:\Users\Example\mod"
        );
        assert_eq!(
            user_facing_path(Path::new(r"\\?\UNC\server\mods\example")),
            r"\\server\mods\example"
        );
    }
}
