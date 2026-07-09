//! Lingshu Desktop — 入口点

// Prevents additional console window on Windows in release
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    lingshu_desktop_lib::run()
}
