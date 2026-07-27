use crate::AppError;
use std::env;
use std::path::{Path, PathBuf};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct TransactionRoots {
    pub transaction: PathBuf,
    pub backup: PathBuf,
    pub staging: PathBuf,
}

pub fn application_data_root() -> PathBuf {
    if cfg!(target_os = "windows") {
        env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(env::temp_dir)
            .join("HOI4 Mod Setup")
    } else {
        env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(env::temp_dir)
            .join("Library/Application Support/HOI4 Mod Setup")
    }
}

pub fn settings_root() -> PathBuf {
    if cfg!(target_os = "windows") {
        env::var_os("APPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(env::temp_dir)
            .join("HOI4 Mod Setup")
    } else {
        application_data_root()
    }
}

pub fn cache_root() -> PathBuf {
    if cfg!(target_os = "windows") {
        application_data_root().join("cache")
    } else {
        env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(env::temp_dir)
            .join("Library/Caches/HOI4 Mod Setup")
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
