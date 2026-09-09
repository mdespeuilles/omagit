//! omagit — a Git client for Omarchy and macOS.

// A console window on Windows would be wrong for a GUI. Windows is not a
// supported target (SPEC §2, amended), but the attribute costs nothing and its
// absence would be a bug the day someone builds there.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    omagit_lib::run();
}
