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
    } else if cfg!(target_os = "linux") {
        Ok(linux_xdg_directory("XDG_DATA_HOME", ".local/share")?.join("hoi4-mod-setup"))
    } else {
        Err(AppError::UnsupportedPlatform(
            "application data is supported only on Windows and macOS".into(),
        ))
    }
}

pub fn settings_root() -> Result<PathBuf, AppError> {
    if cfg!(target_os = "windows") {
        Ok(required_platform_root("APPDATA")?.join("HOI4 Mod Setup"))
    } else if cfg!(target_os = "linux") {
        Ok(linux_xdg_directory("XDG_CONFIG_HOME", ".config")?.join("hoi4-mod-setup"))
    } else {
        application_data_root()
    }
}

pub fn cache_root() -> Result<PathBuf, AppError> {
    if cfg!(target_os = "windows") {
        Ok(application_data_root()?.join("cache"))
    } else if cfg!(target_os = "macos") {
        Ok(required_platform_root("HOME")?.join("Library/Caches/HOI4 Mod Setup"))
    } else if cfg!(target_os = "linux") {
        Ok(linux_xdg_directory("XDG_CACHE_HOME", ".cache")?.join("hoi4-mod-setup"))
    } else {
        Err(AppError::UnsupportedPlatform(
            "source caching is supported only on Windows and macOS".into(),
        ))
    }
}

/// XDG paths are configuration, never shell input. Relative values are ignored
/// as required by the XDG specification; traversal and links are rejected by
/// the filesystem boundary before these destinations are used.
pub(crate) fn linux_xdg_directory(variable: &str, fallback: &str) -> Result<PathBuf, AppError> {
    xdg_directory(
        env::var_os(variable).as_deref(),
        &required_platform_root("HOME")?,
        fallback,
    )
}

fn xdg_directory(
    value: Option<&std::ffi::OsStr>,
    home: &Path,
    fallback: &str,
) -> Result<PathBuf, AppError> {
    let path = value
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .unwrap_or_else(|| home.join(fallback));
    if !path.is_absolute()
        || path
            .components()
            .any(|part| matches!(part, std::path::Component::ParentDir))
    {
        return Err(AppError::PathSecurity(
            "XDG directory must be absolute without traversal".into(),
        ));
    }
    Ok(path)
}

pub(crate) fn linux_npm_prefix() -> Result<PathBuf, AppError> {
    Ok(linux_xdg_directory("XDG_DATA_HOME", ".local/share")?.join("hoi4-mod-setup/npm"))
}

/// The Windows source bootstrap installs into this exact current-user prefix,
/// independently of existing npm commands on PATH. Resolution does not create it.
pub(crate) fn windows_npm_prefix() -> Result<PathBuf, AppError> {
    let prefix = required_platform_root("APPDATA")?.join("npm");
    if prefix
        .components()
        .any(|part| matches!(part, std::path::Component::ParentDir))
        || crate::security::path_has_link_component(&prefix)
    {
        return Err(AppError::PathSecurity(
            "the current-user npm prefix contains traversal or a link".into(),
        ));
    }
    Ok(prefix)
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
    #[cfg(target_os = "linux")]
    {
        let home = required_platform_root("HOME")?;
        let data = linux_xdg_directory("XDG_DATA_HOME", ".local/share")?;
        existing_unambiguous_mod_directory(&linux_mod_candidates(&home, &data))
    }
    #[cfg(not(target_os = "linux"))]
    {
        let documents = documents_directory()?;
        let candidate = documents.join("Paradox Interactive/Hearts of Iron IV/mod");
        validate_project_root(&candidate).map_err(|_| {
            AppError::PathSecurity(format!(
                "the standard HOI4 mod folder was not found at {}",
                candidate.display()
            ))
        })
    }
}

#[cfg(any(target_os = "linux", test))]
fn linux_mod_candidates(home: &Path, data: &Path) -> Vec<PathBuf> {
    let relative = "Paradox Interactive/Hearts of Iron IV/mod";
    let flatpak = home.join(".var/app/com.valvesoftware.Steam");
    vec![
        data.join(relative),
        flatpak.join("data").join(relative),
        flatpak.join(".local/share").join(relative),
    ]
}

#[cfg(any(target_os = "linux", test))]
fn existing_unambiguous_mod_directory(candidates: &[PathBuf]) -> Result<PathBuf, AppError> {
    let mut found = Vec::new();
    for path in candidates {
        if path.exists() {
            let validated = validate_project_root(path)?;
            if !found.contains(&validated) {
                found.push(validated);
            }
        }
    }
    match found.as_slice() {
        [path] => Ok(path.clone()),
        [] => Err(AppError::PathSecurity("the HOI4 user mod folder was not found; choose its location".into())),
        _ => Err(AppError::PathSecurity("multiple native Steam or Flatpak HOI4 mod folders were found; choose the active location".into())),
    }
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
        validate_export_directory(&PathBuf::from(value.ok_or_else(|| {
            AppError::PathSecurity("Windows returned an invalid Downloads folder".into())
        })?))
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
        validate_export_directory(&path)
    }

    #[cfg(target_os = "linux")]
    {
        let home = required_platform_root("HOME")?;
        let config = linux_xdg_directory("XDG_CONFIG_HOME", ".config")?;
        let configured = crate::flatten::read_bounded_regular_file_no_follow_under_root(
            &config,
            "user-dirs.dirs",
            64 * 1024,
        )
        .ok()
        .and_then(|bytes| String::from_utf8(bytes).ok())
        .and_then(|text| linux_downloads_from_user_dirs(&text, &home));
        validate_export_directory(&configured.unwrap_or_else(|| home.join("Downloads")))
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        Err(AppError::UnsupportedPlatform(
            "the Downloads folder is supported only on Windows and macOS".into(),
        ))
    }
}

#[cfg(any(target_os = "linux", test))]
fn linux_downloads_from_user_dirs(text: &str, home: &Path) -> Option<PathBuf> {
    text.lines().find_map(|line| {
        let value = line.trim().strip_prefix("XDG_DOWNLOAD_DIR=")?.trim();
        let value = value.strip_prefix('"')?.strip_suffix('"')?;
        let path = if value == "$HOME" {
            home.to_path_buf()
        } else if let Some(relative) = value.strip_prefix("$HOME/") {
            home.join(relative)
        } else {
            PathBuf::from(value)
        };
        let remainder = value.strip_prefix("$HOME").unwrap_or(value);
        if remainder
            .chars()
            .any(|c| c.is_control() || matches!(c, '$' | '`' | '"' | '\\'))
            || !path.is_absolute()
            || path
                .components()
                .any(|part| matches!(part, std::path::Component::ParentDir))
        {
            return None;
        }
        Some(path)
    })
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

#[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
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

/// Append one technical line to the bounded diagnostics log in application
/// data. User-facing messages stay short; this file keeps the underlying
/// cause for troubleshooting. Failures to log are ignored, and unit tests
/// never write to the real application data folder.
pub fn record_diagnostic(context: &str, detail: &str) {
    if cfg!(test) {
        return;
    }
    if let Ok(root) = application_data_root() {
        append_diagnostic(&root.join("logs"), context, detail);
    }
}

fn append_diagnostic(directory: &Path, context: &str, detail: &str) {
    const MAX_BYTES: u64 = 512 * 1024;
    if fs::create_dir_all(directory).is_err() {
        return;
    }
    let path = directory.join("diagnostics.log");
    if fs::metadata(&path).is_ok_and(|metadata| metadata.len() > MAX_BYTES) {
        let _ = fs::rename(&path, directory.join("diagnostics.previous.log"));
    }
    let line = format!(
        "{} {} {}\n",
        chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        context,
        crate::security::redact_secrets(&detail.replace(['\r', '\n'], " "), &[])
    );
    if let Ok(mut file) = fs::OpenOptions::new().create(true).append(true).open(path) {
        use std::io::Write;
        let _ = file.write_all(line.as_bytes());
    }
}

/// The diagnostics log location shown to users.
pub fn diagnostics_log_path() -> Option<PathBuf> {
    application_data_root()
        .ok()
        .map(|root| root.join("logs").join("diagnostics.log"))
}

/// Where Hearts of Iron IV and its Workshop content live on this computer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GameInstall {
    /// The game folder, written with forward slashes.
    pub game_dir: String,
    /// The Workshop content folder for Hearts of Iron IV (app 394360).
    pub workshop_dir: String,
    /// False when no install was found and these are the platform defaults.
    pub detected: bool,
}

const HOI4_STEAM_FOLDER: &str = "steamapps/common/Hearts of Iron IV";
const HOI4_WORKSHOP_FOLDER: &str = "steamapps/workshop/content/394360";

fn forward_slashes(path: &Path) -> String {
    path.display().to_string().replace('\\', "/")
}

/// Library roots listed in Steam's `libraryfolders.vdf` (`"path" "..."` rows).
pub(crate) fn steam_library_paths(vdf: &str) -> Vec<PathBuf> {
    vdf.lines()
        .filter_map(|line| {
            let mut quoted = line.split('"').skip(1).step_by(2);
            match (quoted.next(), quoted.next()) {
                (Some(key), Some(value)) if key.eq_ignore_ascii_case("path") => {
                    Some(PathBuf::from(value.replace("\\\\", "\\")))
                }
                _ => None,
            }
        })
        .collect()
}

fn steam_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if cfg!(target_os = "windows") {
        for variable in ["ProgramFiles(x86)", "ProgramFiles"] {
            if let Some(base) = env::var_os(variable) {
                roots.push(PathBuf::from(base).join("Steam"));
            }
        }
        if let Some(system_root) = env::var_os("SystemRoot") {
            let reg = PathBuf::from(system_root).join("System32").join("reg.exe");
            if reg.is_file() {
                let mut command = std::process::Command::new(reg);
                command.args(["query", r"HKCU\Software\Valve\Steam", "/v", "SteamPath"]);
                crate::process::configure_child_no_console_window(&mut command);
                if let Ok(output) = command.output() {
                    let text = String::from_utf8_lossy(&output.stdout);
                    if let Some(value) = text
                        .lines()
                        .find(|line| line.contains("SteamPath"))
                        .and_then(|line| line.split("REG_SZ").nth(1))
                    {
                        roots.insert(0, PathBuf::from(value.trim()));
                    }
                }
            }
        }
    } else if cfg!(target_os = "linux") {
        if let Ok(home) = required_platform_root("HOME") {
            let data = linux_xdg_directory("XDG_DATA_HOME", ".local/share")
                .unwrap_or_else(|_| home.join(".local/share"));
            roots.extend(linux_steam_candidates(&home, &data));
        }
    } else if let Some(home) = env::var_os("HOME") {
        roots.push(PathBuf::from(home).join("Library/Application Support/Steam"));
    }
    roots
}

fn linux_steam_candidates(home: &Path, data: &Path) -> Vec<PathBuf> {
    let flatpak = home.join(".var/app/com.valvesoftware.Steam");
    vec![
        data.join("Steam"),
        home.join(".steam/steam"),
        home.join(".steam/root"),
        flatpak.join("data/Steam"),
        flatpak.join(".local/share/Steam"),
    ]
}

/// Find the installed game through the Steam libraries, falling back to the
/// default Steam location of the platform so instructions stay usable.
pub fn detect_game_install() -> GameInstall {
    let mut libraries = Vec::new();
    for root in steam_roots() {
        // Steam intentionally publishes launcher-directory symlinks. Resolve
        // this bounded discovery candidate once; project containment remains
        // strict and never follows links in a selected project.
        let root = fs::canonicalize(&root).unwrap_or(root);
        if !libraries.contains(&root) {
            libraries.push(root.clone());
        }
        if let Ok(bytes) = crate::flatten::read_bounded_regular_file_no_follow_under_root(
            &root,
            "steamapps/libraryfolders.vdf",
            2 * 1024 * 1024,
        ) {
            let Ok(text) = String::from_utf8(bytes) else {
                continue;
            };
            for library in steam_library_paths(&text) {
                if library.is_absolute()
                    && !library
                        .components()
                        .any(|part| matches!(part, std::path::Component::ParentDir))
                    && !libraries.contains(&library)
                {
                    libraries.push(library);
                }
            }
        }
    }
    for library in &libraries {
        let game = library.join(HOI4_STEAM_FOLDER);
        if game.is_dir() {
            return GameInstall {
                game_dir: forward_slashes(&game),
                workshop_dir: forward_slashes(&library.join(HOI4_WORKSHOP_FOLDER)),
                detected: true,
            };
        }
    }
    let default_library = if cfg!(target_os = "windows") {
        PathBuf::from("C:/Program Files (x86)/Steam")
    } else if cfg!(target_os = "linux") {
        libraries
            .first()
            .cloned()
            .unwrap_or_else(|| PathBuf::from("~/.local/share/Steam"))
    } else {
        libraries
            .first()
            .cloned()
            .unwrap_or_else(|| PathBuf::from("~/Library/Application Support/Steam"))
    };
    GameInstall {
        game_dir: forward_slashes(&default_library.join(HOI4_STEAM_FOLDER)),
        workshop_dir: forward_slashes(&default_library.join(HOI4_WORKSHOP_FOLDER)),
        detected: false,
    }
}

/// The detected install, looked up once per app run.
pub fn game_install() -> &'static GameInstall {
    static INSTALL: std::sync::OnceLock<GameInstall> = std::sync::OnceLock::new();
    INSTALL.get_or_init(detect_game_install)
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_diagnostic_line_is_one_redacted_line_and_the_log_rolls_over() {
        let directory = tempfile::tempdir().unwrap();
        let logs = directory.path().join("logs");
        append_diagnostic(&logs, "plan", "first line\nsecond line");
        let text = fs::read_to_string(logs.join("diagnostics.log")).unwrap();
        assert_eq!(text.lines().count(), 1);
        assert!(text.contains(" plan first line second line"));
        fs::write(logs.join("diagnostics.log"), vec![b'x'; 600 * 1024]).unwrap();
        append_diagnostic(&logs, "plan", "after rollover");
        assert!(logs.join("diagnostics.previous.log").exists());
        let text = fs::read_to_string(logs.join("diagnostics.log")).unwrap();
        assert!(text.contains("after rollover"));
        assert!(text.len() < 1024);
    }

    #[test]
    fn steam_library_rows_are_read_with_escaped_backslashes() {
        let vdf = "\"libraryfolders\"\n{\n\t\"0\"\n\t{\n\t\t\"path\"\t\t\"C:\\\\Program Files (x86)\\\\Steam\"\n\t}\n\t\"1\"\n\t{\n\t\t\"path\"\t\t\"D:\\\\SteamLibrary\"\n\t\t\"apps\"\n\t}\n}\n";
        assert_eq!(
            steam_library_paths(vdf),
            vec![
                PathBuf::from("C:\\Program Files (x86)\\Steam"),
                PathBuf::from("D:\\SteamLibrary")
            ]
        );
    }

    #[test]
    fn xdg_paths_ignore_relative_values_and_reject_traversal() {
        let home = tempdir().unwrap();
        assert_eq!(
            xdg_directory(
                Some(std::ffi::OsStr::new("relative")),
                home.path(),
                ".local/share"
            )
            .unwrap(),
            home.path().join(".local/share")
        );
        assert!(xdg_directory(
            Some(home.path().join("../escape").as_os_str()),
            home.path(),
            ".cache"
        )
        .is_err());
    }

    #[test]
    fn downloads_configuration_never_evaluates_shell_expressions() {
        let home = tempdir().unwrap();
        assert_eq!(
            linux_downloads_from_user_dirs("XDG_DOWNLOAD_DIR=\"$HOME/My Downloads\"", home.path()),
            Some(home.path().join("My Downloads"))
        );
        for text in [
            "XDG_DOWNLOAD_DIR=\"$(touch sentinel)\"",
            "XDG_DOWNLOAD_DIR=\"$HOME/../escape\"",
            "XDG_DOWNLOAD_DIR=\"`touch sentinel`\"",
        ] {
            assert!(linux_downloads_from_user_dirs(text, home.path()).is_none());
        }
    }

    #[test]
    fn native_and_both_flatpak_mod_routes_are_bounded_and_ambiguity_is_visible() {
        let home = tempdir().unwrap();
        let data = home.path().join("custom-data");
        let candidates = linux_mod_candidates(home.path(), &data);
        assert_eq!(candidates.len(), 3);
        assert!(existing_unambiguous_mod_directory(&candidates).is_err());
        fs::create_dir_all(&candidates[1]).unwrap();
        assert_eq!(
            existing_unambiguous_mod_directory(&candidates).unwrap(),
            candidates[1].canonicalize().unwrap()
        );
        fs::create_dir_all(&candidates[0]).unwrap();
        assert!(existing_unambiguous_mod_directory(&candidates)
            .unwrap_err()
            .to_string()
            .contains("multiple"));
    }

    #[test]
    fn the_game_install_always_names_a_game_and_workshop_folder() {
        let install = detect_game_install();
        assert!(install
            .game_dir
            .ends_with("steamapps/common/Hearts of Iron IV"));
        assert!(install
            .workshop_dir
            .ends_with("steamapps/workshop/content/394360"));
        assert!(!install.game_dir.contains('\\'));
    }

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
