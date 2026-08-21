#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

#[cfg(feature = "desktop")]
fn main() {
    if let Some(code) = hoi4_mod_setup::meshy::run_cli_if_requested() {
        std::process::exit(code);
    }
    hoi4_mod_setup::run_desktop();
}

#[cfg(not(feature = "desktop"))]
fn main() {
    if let Some(code) = hoi4_mod_setup::meshy::run_cli_if_requested() {
        std::process::exit(code);
    }
    // Keeping the binary buildable without desktop dependencies lets CI run the
    // core tests on a clean host. The Tauri entry point is enabled by the
    // `desktop` feature used by `pnpm tauri dev` and release builds.
}
