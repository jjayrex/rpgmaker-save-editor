mod commands;
pub mod state;
pub mod views;

use tauri::Manager;
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons};

use state::{Editor, SharedEditor};

pub fn builder<R: tauri::Runtime>(builder: tauri::Builder<R>) -> tauri::Builder<R> {
    builder
        .plugin(tauri_plugin_dialog::init())
        .manage(state::SharedEditor::new(Editor::default()))
        .invoke_handler(tauri::generate_handler![
            commands::pick_save_file,
            commands::pick_save_target,
            commands::pick_data_dir,
            commands::open_save,
            commands::close_save,
            commands::summary,
            commands::write_save,
            commands::set_data_dir,
            commands::set_gold,
            commands::set_steps,
            commands::set_playtime,
            commands::set_position,
            commands::set_party,
            commands::get_actor,
            commands::set_actor_text,
            commands::set_actor_int,
            commands::set_actor_scalar,
            commands::set_actor_level,
            commands::set_actor_exp,
            commands::set_actor_param,
            commands::set_actor_equip,
            commands::set_actor_skills,
            commands::set_actor_states,
            commands::full_heal,
            commands::get_inventory,
            commands::set_item_count,
            commands::get_catalog,
            commands::get_switch_page,
            commands::set_switch,
            commands::set_switches,
            commands::get_variable_page,
            commands::set_variable,
            commands::get_self_switches,
            commands::set_self_switch,
            commands::remove_self_switch,
            commands::raw_root,
            commands::raw_node,
            commands::set_raw_scalar,
        ])
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    builder(tauri::Builder::default())
        .setup(|app| {
            // `rpgmaker-save-editor <path>` opens that save straight away, so the
            // editor can be wired up as the handler for .rvdata2 files.
            if let Some(path) = std::env::args().nth(1) {
                let path = std::path::PathBuf::from(path);
                if let Ok(mut editor) = app.state::<SharedEditor>().lock()
                    && let Err(message) = editor.open(&path)
                {
                    eprintln!("{}: {message}", path.display());
                }
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            // Closing with unsaved edits silently would be the one unrecoverable
            // mistake this editor could make, so ask first.
            let tauri::WindowEvent::CloseRequested { api, .. } = event else {
                return;
            };
            let dirty = window
                .state::<SharedEditor>()
                .lock()
                .map(|editor| editor.save.as_ref().is_some_and(|s| s.dirty))
                .unwrap_or(false);
            if !dirty {
                return;
            }
            api.prevent_close();
            let window = window.clone();
            window
                .clone()
                .dialog()
                .message("This save has changes that have not been written to disk.")
                .title("Close without saving?")
                .buttons(MessageDialogButtons::OkCancelCustom(
                    "Discard and close".to_owned(),
                    "Keep editing".to_owned(),
                ))
                .show(move |discard| {
                    if discard {
                        let _ = window.destroy();
                    }
                });
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
