//! Everything the interface can ask the backend to do.

use tauri::State;
use tauri_plugin_dialog::DialogExt;

use rpgsave::rpg::save::write_file_safely;
use rpgsave::rpg::Engine;
use rpgsave_protocol::*;

use crate::backend::Reply;
use crate::state::{to_path, Editor, SharedEditor};

// ------------------------------------------------------------- file dialogs

#[tauri::command(async)]
pub fn pick_save_file<R: tauri::Runtime>(app: tauri::AppHandle<R>) -> Reply<Option<String>> {
    let mut dialog = app.dialog().file().set_title("Open an RPG Maker save");
    for (name, extensions) in save_file_filters() {
        dialog = dialog.add_filter(name, &extensions);
    }
    into_path(dialog.blocking_pick_file())
}

/// What RPG Maker 2000 and 2003 call their saves.
const LCF_EXTENSION: &str = "lsd";

/// Filters for the open dialog: every supported save first, so the default
/// view shows all of them, then one per engine.
///
/// Built from [`Engine::ALL`] so that adding an engine cannot leave its saves
/// invisible in the file picker.
fn save_file_filters() -> Vec<(String, Vec<&'static str>)> {
    let mut every: Vec<&'static str> = Engine::ALL.iter().map(|e| e.save_extension()).collect();
    every.push(LCF_EXTENSION);
    let mut filters = vec![("RPG Maker save".to_owned(), every)];
    filters.extend(
        Engine::ALL
            .into_iter()
            .map(|engine| (format!("{} save", engine.label()), vec![engine.save_extension()])),
    );
    filters.push(("2000/2003 save".to_owned(), vec![LCF_EXTENSION]));
    filters.push(("All files".to_owned(), vec!["*"]));
    filters
}

#[tauri::command(async)]
pub fn pick_save_target<R: tauri::Runtime>(app: tauri::AppHandle<R>, file_name: String) -> Reply<Option<String>> {
    let picked = app
        .dialog()
        .file()
        .set_title("Save a copy as")
        .set_file_name(&file_name)
        .blocking_save_file();
    into_path(picked)
}

#[tauri::command(async)]
pub fn pick_data_dir<R: tauri::Runtime>(app: tauri::AppHandle<R>) -> Reply<Option<String>> {
    let picked = app
        .dialog()
        .file()
        .set_title("Choose the game's Data folder")
        .blocking_pick_folder();
    into_path(picked)
}

fn into_path(picked: Option<tauri_plugin_dialog::FilePath>) -> Reply<Option<String>> {
    match picked {
        None => Ok(None),
        Some(file) => file
            .into_path()
            .map(|p| Some(p.to_string_lossy().into_owned()))
            .map_err(|e| e.to_string()),
    }
}

// ---------------------------------------------------------------- the file

#[tauri::command]
pub fn open_save(path: String, editor: State<'_, SharedEditor>) -> Reply<Summary> {
    let mut editor = lock(&editor)?;
    editor.open(&to_path(&path))?;
    Ok(editor.save()?.summary(editor.data()))
}

#[tauri::command]
pub fn close_save(editor: State<'_, SharedEditor>) -> Reply<()> {
    let mut editor = lock(&editor)?;
    editor.save = None;
    editor.data = None;
    Ok(())
}

#[tauri::command]
pub fn summary(editor: State<'_, SharedEditor>) -> Reply<Summary> {
    let editor = lock(&editor)?;
    Ok(editor.save()?.summary(editor.data()))
}

#[tauri::command]
pub fn write_save(
    path: Option<String>,
    backup: bool,
    editor: State<'_, SharedEditor>,
) -> Reply<WriteResult> {
    let mut editor = lock(&editor)?;
    let save = editor.save_mut()?;
    let target = match path {
        Some(p) => to_path(&p),
        None => save.path().to_path_buf(),
    };
    if target.as_os_str().is_empty() {
        return Err("This save has no path yet; use Save As.".to_owned());
    }

    let bytes = save.encode();
    let backup = write_file_safely(&target, &bytes, backup)
        .map_err(|e| format!("Could not write {}: {e}", target.display()))?;
    save.set_byte_size(bytes.len());
    save.set_path(target.clone());
    save.mark_saved();

    Ok(WriteResult {
        path: target.to_string_lossy().into_owned(),
        bytes: bytes.len(),
        backup: backup.map(|b| b.to_string_lossy().into_owned()),
    })
}

#[tauri::command]
pub fn set_data_dir(path: String, editor: State<'_, SharedEditor>) -> Reply<Summary> {
    let mut editor = lock(&editor)?;
    editor.load_data_dir(&to_path(&path))?;
    Ok(editor.save()?.summary(editor.data()))
}

// ------------------------------------------------------------------- party

#[tauri::command]
pub fn set_gold(value: i64, editor: State<'_, SharedEditor>) -> Reply<Summary> {
    let mut editor = lock(&editor)?;
    require(editor.save_mut()?.set_gold(value), "This save has no gold field.")?;
    Ok(editor.save()?.summary(editor.data()))
}

#[tauri::command]
pub fn set_steps(value: i64, editor: State<'_, SharedEditor>) -> Reply<Summary> {
    let mut editor = lock(&editor)?;
    require(editor.save_mut()?.set_steps(value), "This save has no step count.")?;
    Ok(editor.save()?.summary(editor.data()))
}

#[tauri::command]
pub fn set_playtime(seconds: i64, editor: State<'_, SharedEditor>) -> Reply<Summary> {
    let mut editor = lock(&editor)?;
    require(
        editor.save_mut()?.set_playtime_seconds(seconds),
        "This save does not record play time.",
    )?;
    Ok(editor.save()?.summary(editor.data()))
}

#[tauri::command]
pub fn set_position(x: i64, y: i64, editor: State<'_, SharedEditor>) -> Reply<Summary> {
    let mut editor = lock(&editor)?;
    require(editor.save_mut()?.set_position(x, y), "This save has no player position.")?;
    Ok(editor.save()?.summary(editor.data()))
}

#[tauri::command]
pub fn set_party(ids: Vec<i64>, editor: State<'_, SharedEditor>) -> Reply<Summary> {
    let mut editor = lock(&editor)?;
    editor.save_mut()?.set_party(&ids)?;
    Ok(editor.save()?.summary(editor.data()))
}

// ------------------------------------------------------------------ actors

#[tauri::command]
pub fn get_actor(actor_id: i64, editor: State<'_, SharedEditor>) -> Reply<ActorView> {
    let editor = lock(&editor)?;
    actor_view(&editor, actor_id)
}

#[tauri::command]
pub fn set_actor_text(
    actor_id: i64,
    ivar: String,
    text: String,
    editor: State<'_, SharedEditor>,
) -> Reply<ActorView> {
    let mut editor = lock(&editor)?;
    editor.require_actor(actor_id)?;
    let changed = editor.save_mut()?.set_actor_text(actor_id, &ivar, &text);
    require(changed, "This actor has no such field.")?;
    actor_view(&editor, actor_id)
}

#[tauri::command]
pub fn set_actor_int(
    actor_id: i64,
    ivar: String,
    value: i64,
    editor: State<'_, SharedEditor>,
) -> Reply<ActorView> {
    let mut editor = lock(&editor)?;
    editor.require_actor(actor_id)?;
    let changed = editor.save_mut()?.set_actor_int(actor_id, &ivar, value);
    require(changed, "This actor has no such field.")?;
    actor_view(&editor, actor_id)
}

#[tauri::command]
pub fn set_actor_scalar(
    actor_id: i64,
    ivar: String,
    value: Scalar,
    editor: State<'_, SharedEditor>,
) -> Reply<ActorView> {
    let mut editor = lock(&editor)?;
    editor.require_actor(actor_id)?;
    editor.save_mut()?.set_actor_scalar(actor_id, &ivar, &value);
    actor_view(&editor, actor_id)
}

#[tauri::command]
pub fn set_actor_level(
    actor_id: i64,
    level: i64,
    editor: State<'_, SharedEditor>,
) -> Reply<ActorView> {
    let mut editor = lock(&editor)?;
    editor.require_actor(actor_id)?;
    let (save, data) = editor.both()?;
    let changed = save.set_actor_level(actor_id, level, data);
    require(changed, "This actor has no level field.")?;
    actor_view(&editor, actor_id)
}

#[tauri::command]
pub fn set_actor_exp(actor_id: i64, exp: i64, editor: State<'_, SharedEditor>) -> Reply<ActorView> {
    let mut editor = lock(&editor)?;
    editor.require_actor(actor_id)?;
    let changed = editor.save_mut()?.set_actor_exp(actor_id, exp);
    require(changed, "This actor has no experience field.")?;
    actor_view(&editor, actor_id)
}

#[tauri::command]
pub fn set_actor_param(
    actor_id: i64,
    index: usize,
    value: i64,
    editor: State<'_, SharedEditor>,
) -> Reply<ActorView> {
    let mut editor = lock(&editor)?;
    editor.require_actor(actor_id)?;
    let changed = editor.save_mut()?.set_actor_param(actor_id, index, value);
    require(changed, "This actor has no such parameter bonus.")?;
    actor_view(&editor, actor_id)
}

#[tauri::command]
pub fn set_actor_equip(
    actor_id: i64,
    slot: usize,
    kind: String,
    item_id: i64,
    editor: State<'_, SharedEditor>,
) -> Reply<ActorView> {
    let mut editor = lock(&editor)?;
    editor.require_actor(actor_id)?;
    let changed = editor.save_mut()?.set_actor_equip(actor_id, slot, &kind, item_id);
    require(changed, "This actor has no such equipment slot.")?;
    actor_view(&editor, actor_id)
}

#[tauri::command]
pub fn set_actor_skills(
    actor_id: i64,
    ids: Vec<i64>,
    editor: State<'_, SharedEditor>,
) -> Reply<ActorView> {
    let mut editor = lock(&editor)?;
    editor.require_actor(actor_id)?;
    let changed = editor.save_mut()?.set_actor_skills(actor_id, &ids);
    require(changed, "This actor has no skill list.")?;
    actor_view(&editor, actor_id)
}

#[tauri::command]
pub fn set_actor_states(
    actor_id: i64,
    ids: Vec<i64>,
    editor: State<'_, SharedEditor>,
) -> Reply<ActorView> {
    let mut editor = lock(&editor)?;
    editor.require_actor(actor_id)?;
    let changed = editor.save_mut()?.set_actor_states(actor_id, &ids);
    require(changed, "This save does not record states on actors.")?;
    actor_view(&editor, actor_id)
}

#[tauri::command]
pub fn full_heal(actor_id: i64, editor: State<'_, SharedEditor>) -> Reply<ActorView> {
    let mut editor = lock(&editor)?;
    editor.require_actor(actor_id)?;
    let (save, data) = editor.both()?;
    save.full_heal(actor_id, data);
    actor_view(&editor, actor_id)
}

fn actor_view(editor: &Editor, actor_id: i64) -> Reply<ActorView> {
    editor
        .save()?
        .actor_view(editor.data(), actor_id)
        .ok_or_else(|| format!("Actor {actor_id} is not stored in this save."))
}

// --------------------------------------------------------------- inventory

#[tauri::command]
pub fn get_inventory(kind: String, editor: State<'_, SharedEditor>) -> Reply<InventoryView> {
    let editor = lock(&editor)?;
    editor.save()?.inventory(editor.data(), &kind)
}

#[tauri::command]
pub fn set_item_count(
    kind: String,
    id: i64,
    count: i64,
    editor: State<'_, SharedEditor>,
) -> Reply<InventoryView> {
    let mut editor = lock(&editor)?;
    let changed = editor.save_mut()?.set_item_count(&kind, id, count);
    require(changed, "This save has no such inventory container.")?;
    editor.save()?.inventory(editor.data(), &kind)
}

#[tauri::command]
pub fn get_catalog(
    kind: String,
    query: String,
    editor: State<'_, SharedEditor>,
) -> Reply<Vec<CatalogEntry>> {
    let editor = lock(&editor)?;
    Ok(editor.save()?.catalog(editor.data(), &kind, &query))
}

// ---------------------------------------------------- switches / variables

#[tauri::command]
pub fn get_switch_page(
    query: String,
    offset: usize,
    limit: usize,
    editor: State<'_, SharedEditor>,
) -> Reply<SwitchPage> {
    let editor = lock(&editor)?;
    Ok(editor.save()?.switch_page(editor.data(), &query, offset, limit))
}

#[tauri::command]
pub fn set_switch(id: i64, on: bool, editor: State<'_, SharedEditor>) -> Reply<Applied> {
    let mut editor = lock(&editor)?;
    require(editor.save_mut()?.set_switch(id, on), "This save has no switch list.")?;
    Ok(Applied::ok())
}

#[tauri::command]
pub fn set_switches(ids: Vec<i64>, on: bool, editor: State<'_, SharedEditor>) -> Reply<Applied> {
    let mut editor = lock(&editor)?;
    let save = editor.save_mut()?;
    let mut changed = 0;
    for id in &ids {
        if save.set_switch(*id, on) {
            changed += 1;
        }
    }
    Ok(Applied::note(format!(
        "Turned {changed} switch{} {}.",
        if changed == 1 { "" } else { "es" },
        if on { "on" } else { "off" }
    )))
}

#[tauri::command]
pub fn get_variable_page(
    query: String,
    offset: usize,
    limit: usize,
    editor: State<'_, SharedEditor>,
) -> Reply<VariablePage> {
    let editor = lock(&editor)?;
    Ok(editor.save()?.variable_page(editor.data(), &query, offset, limit))
}

#[tauri::command]
pub fn set_variable(id: i64, value: Scalar, editor: State<'_, SharedEditor>) -> Reply<Applied> {
    let mut editor = lock(&editor)?;
    require(editor.save_mut()?.set_variable(id, &value), "This save has no variable list.")?;
    Ok(Applied::ok())
}

#[tauri::command]
pub fn get_self_switches(editor: State<'_, SharedEditor>) -> Reply<Vec<SelfSwitchRow>> {
    let editor = lock(&editor)?;
    Ok(editor.save()?.self_switches(editor.data()))
}

#[tauri::command]
pub fn set_self_switch(
    map_id: i64,
    event_id: i64,
    letter: String,
    on: bool,
    editor: State<'_, SharedEditor>,
) -> Reply<Vec<SelfSwitchRow>> {
    let mut editor = lock(&editor)?;
    let changed = editor.save_mut()?.set_self_switch(map_id, event_id, &letter, on);
    require(changed, "This save has no self-switch table.")?;
    Ok(editor.save()?.self_switches(editor.data()))
}

#[tauri::command]
pub fn remove_self_switch(
    map_id: i64,
    event_id: i64,
    letter: String,
    editor: State<'_, SharedEditor>,
) -> Reply<Vec<SelfSwitchRow>> {
    let mut editor = lock(&editor)?;
    editor.save_mut()?.remove_self_switch(map_id, event_id, &letter);
    Ok(editor.save()?.self_switches(editor.data()))
}

// -------------------------------------------------------------- raw editor

#[tauri::command]
pub fn raw_root(editor: State<'_, SharedEditor>) -> Reply<NodeView> {
    let editor = lock(&editor)?;
    Ok(editor.save()?.raw_root())
}

#[tauri::command]
pub fn raw_node(node: u32, editor: State<'_, SharedEditor>) -> Reply<NodeView> {
    let editor = lock(&editor)?;
    editor
        .save()?
        .raw_node(node)
        .ok_or_else(|| format!("There is no value {node} in this save."))
}

#[tauri::command]
pub fn set_raw_scalar(
    node: Option<u32>,
    slot: Slot,
    value: Scalar,
    editor: State<'_, SharedEditor>,
) -> Reply<NodeView> {
    let mut editor = lock(&editor)?;
    editor.save_mut()?.set_raw_scalar(node, &slot, &value)?;

    let save = editor.save()?;
    match node {
        Some(id) => save
            .raw_node(id)
            .ok_or_else(|| format!("There is no value {id} in this save.")),
        None => Ok(save.raw_root()),
    }
}

// ------------------------------------------------------------------ shared

fn lock<'a>(editor: &'a State<'_, SharedEditor>) -> Reply<std::sync::MutexGuard<'a, Editor>> {
    editor
        .lock()
        .map_err(|_| "The editor state was left inconsistent by an earlier error.".to_owned())
}

fn require(ok: bool, message: &str) -> Reply<()> {
    if ok {
        Ok(())
    } else {
        Err(message.to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every engine the editor supports has to be offered by the open dialog.
    #[test]
    fn the_open_dialog_offers_every_supported_engine() {
        let filters = save_file_filters();
        let combined = &filters[0].1;

        for engine in Engine::ALL {
            assert!(
                combined.contains(&engine.save_extension()),
                "{} saves are missing from the combined filter",
                engine.label()
            );
            assert!(
                filters.iter().any(|(name, extensions)| {
                    name.contains(engine.label()) && extensions == &[engine.save_extension()]
                }),
                "{} has no filter of its own",
                engine.label()
            );
        }
        assert!(combined.contains(&LCF_EXTENSION), "RPG Maker 2000/2003 saves are missing");
        assert!(
            filters.iter().any(|(name, ext)| name.contains("2000/2003") && ext == &[LCF_EXTENSION])
        );
        assert_eq!(filters.last().map(|(name, _)| name.as_str()), Some("All files"));
    }
}
