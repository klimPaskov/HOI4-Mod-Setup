#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

#[cfg(feature = "desktop")]
fn main() {
    hoi4_mod_setup::run_desktop();
}

#[cfg(not(feature = "desktop"))]
fn main() {
    // Keeping the binary buildable without desktop dependencies lets CI run the
    // core tests on a clean host. The Tauri entry point is enabled by the
    // `desktop` feature used by `pnpm tauri dev` and release builds.
}
