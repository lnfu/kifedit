// Windows release 版不開額外的主控台視窗；Tauri 範本要求保留
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    kifedit_app_lib::run();
}
