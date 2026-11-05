mod commands;
mod format;
mod history;
mod search;
mod term_page;
mod tree;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(commands::AppState::default())
        .invoke_handler(tauri::generate_handler![
            commands::open_project,
            commands::tree_children,
            commands::term_page,
            commands::file_text,
            commands::apply_edits,
            commands::save_file,
            commands::undo,
            commands::redo,
            commands::preview_form,
            commands::apply_form,
            commands::search_names,
            commands::search_text,
            commands::run_query,
            commands::format_file,
            commands::format_project_preview,
            commands::format_project,
            commands::create_file
        ])
        .setup(|app| {
            if cfg!(debug_assertions) {
                app.handle().plugin(
                    tauri_plugin_log::Builder::default()
                        .level(log::LevelFilter::Info)
                        .build(),
                )?;
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("Cannot start the application.");
}
