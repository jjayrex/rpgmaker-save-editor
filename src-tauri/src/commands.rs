//! Everything the interface can ask the backend to do.

use tauri::State;
use tauri_plugin_dialog::DialogExt;

use rpgsave::marshal::{NodeKind, Value};
use rpgsave::rpg::actor::LevelChange;
use rpgsave::rpg::save::ItemKind;
use rpgsave_protocol::*;

use crate::state::{assign_scalar, to_path, Editor, SharedEditor};
use crate::views;

type Reply<T> = Result<T, String>;

fn kind_of(kind: &str) -> Reply<ItemKind> {
    ItemKind::from_id(kind).ok_or_else(|| format!("Unknown item kind {kind:?}."))
}

// ------------------------------------------------------------- file dialogs

#[tauri::command(async)]
pub fn pick_save_file<R: tauri::Runtime>(app: tauri::AppHandle<R>) -> Reply<Option<String>> {
    let picked = app
        .dialog()
        .file()
        .set_title("Open an RPG Maker save")
        .add_filter("VX Ace save", &["rvdata2"])
        .add_filter("VX save", &["rvdata"])
        .add_filter("All files", &["*"])
        .blocking_pick_file();
    into_path(picked)
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
    Ok(views::summary(editor.save()?, editor.data()))
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
    Ok(views::summary(editor.save()?, editor.data()))
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
        None => save.path.clone(),
    };
    if target.as_os_str().is_empty() {
        return Err("This save has no path yet; use Save As.".to_owned());
    }
    let backup = save
        .write(&target, backup)
        .map_err(|e| format!("Could not write {}: {e}", target.display()))?;
    Ok(WriteResult {
        path: target.to_string_lossy().into_owned(),
        bytes: save.size_bytes,
        backup: backup.map(|b| b.to_string_lossy().into_owned()),
    })
}

#[tauri::command]
pub fn set_data_dir(path: String, editor: State<'_, SharedEditor>) -> Reply<Summary> {
    let mut editor = lock(&editor)?;
    editor.load_data_dir(&to_path(&path))?;
    Ok(views::summary(editor.save()?, editor.data()))
}

// ------------------------------------------------------------------- party

#[tauri::command]
pub fn set_gold(value: i64, editor: State<'_, SharedEditor>) -> Reply<Summary> {
    let mut editor = lock(&editor)?;
    require(editor.save_mut()?.set_gold(value), "This save has no gold field.")?;
    Ok(views::summary(editor.save()?, editor.data()))
}

#[tauri::command]
pub fn set_steps(value: i64, editor: State<'_, SharedEditor>) -> Reply<Summary> {
    let mut editor = lock(&editor)?;
    require(editor.save_mut()?.set_steps(value), "This save has no step count.")?;
    Ok(views::summary(editor.save()?, editor.data()))
}

#[tauri::command]
pub fn set_playtime(seconds: i64, editor: State<'_, SharedEditor>) -> Reply<Summary> {
    let mut editor = lock(&editor)?;
    let frames = seconds.max(0) * rpgsave::rpg::save::FRAME_RATE;
    require(
        editor.save_mut()?.set_playtime_frames(frames),
        "This save does not record play time.",
    )?;
    Ok(views::summary(editor.save()?, editor.data()))
}

#[tauri::command]
pub fn set_position(x: i64, y: i64, editor: State<'_, SharedEditor>) -> Reply<Summary> {
    let mut editor = lock(&editor)?;
    require(
        editor.save_mut()?.set_player_position(x, y),
        "This save has no player position.",
    )?;
    Ok(views::summary(editor.save()?, editor.data()))
}

#[tauri::command]
pub fn set_party(ids: Vec<i64>, editor: State<'_, SharedEditor>) -> Reply<Summary> {
    let mut editor = lock(&editor)?;
    require(
        editor.save_mut()?.set_party_member_ids(&ids),
        "This save has no party list.",
    )?;
    Ok(views::summary(editor.save()?, editor.data()))
}

// ------------------------------------------------------------------ actors

#[tauri::command]
pub fn get_actor(actor_id: i64, editor: State<'_, SharedEditor>) -> Reply<ActorView> {
    let editor = lock(&editor)?;
    views::actor_view(editor.save()?, editor.data(), actor_id)
        .ok_or_else(|| format!("Actor {actor_id} is not stored in this save."))
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
    let changed = editor
        .save_mut()?
        .update_actor(actor_id, |save, actor| save.set_actor_string(actor, &ivar, &text));
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
    let changed = editor.save_mut()?.update_actor(actor_id, |save, actor| {
        if save.heap.ivar(actor, &ivar).is_none() {
            return false;
        }
        save.heap.set_ivar(actor, &ivar, Value::Int(value));
        save.dirty = true;
        true
    });
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
    editor.save_mut()?.update_actor(actor_id, |save, actor| {
        let previous = save.heap.ivar(actor, &ivar);
        // Each copy keeps its own string node, as the engine wrote them.
        let new = assign_scalar(&mut save.heap, save.engine, previous, &value);
        save.heap.set_ivar(actor, &ivar, new);
        save.dirty = true;
        true
    });
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
    let changed = save.update_actor(actor_id, |save, actor| {
        save.set_actor_level(actor, level, data) != LevelChange::Failed
    });
    require(changed, "This actor has no level field.")?;
    actor_view(&editor, actor_id)
}

#[tauri::command]
pub fn set_actor_exp(actor_id: i64, exp: i64, editor: State<'_, SharedEditor>) -> Reply<ActorView> {
    let mut editor = lock(&editor)?;
    editor.require_actor(actor_id)?;
    let changed = editor
        .save_mut()?
        .update_actor(actor_id, |save, actor| save.set_actor_exp(actor, exp));
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
    let changed = editor
        .save_mut()?
        .update_actor(actor_id, |save, actor| save.set_actor_param_plus(actor, index, value));
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
    let kind = kind_of(&kind)?;
    let mut editor = lock(&editor)?;
    editor.require_actor(actor_id)?;
    let changed = editor
        .save_mut()?
        .update_actor(actor_id, |save, actor| save.set_actor_equip(actor, slot, kind, item_id));
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
    let changed = editor
        .save_mut()?
        .update_actor(actor_id, |save, actor| save.set_actor_id_list(actor, "@skills", &ids));
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
    let changed = editor
        .save_mut()?
        .update_actor(actor_id, |save, actor| save.set_actor_states(actor, &ids));
    require(changed, "This actor has no state list.")?;
    actor_view(&editor, actor_id)
}

#[tauri::command]
pub fn full_heal(actor_id: i64, editor: State<'_, SharedEditor>) -> Reply<ActorView> {
    let mut editor = lock(&editor)?;
    editor.require_actor(actor_id)?;
    let (save, data) = editor.both()?;
    save.update_actor(actor_id, |save, actor| save.full_heal_actor(actor, data));
    actor_view(&editor, actor_id)
}

fn actor_view(editor: &Editor, actor_id: i64) -> Reply<ActorView> {
    views::actor_view(editor.save()?, editor.data(), actor_id)
        .ok_or_else(|| format!("Actor {actor_id} is not stored in this save."))
}

// --------------------------------------------------------------- inventory

#[tauri::command]
pub fn get_inventory(kind: String, editor: State<'_, SharedEditor>) -> Reply<InventoryView> {
    let kind = kind_of(&kind)?;
    let editor = lock(&editor)?;
    Ok(views::inventory(editor.save()?, editor.data(), kind))
}

#[tauri::command]
pub fn set_item_count(
    kind: String,
    id: i64,
    count: i64,
    editor: State<'_, SharedEditor>,
) -> Reply<InventoryView> {
    let kind = kind_of(&kind)?;
    let mut editor = lock(&editor)?;
    require(
        editor.save_mut()?.set_item_count(kind, id, count),
        "This save has no such inventory container.",
    )?;
    Ok(views::inventory(editor.save()?, editor.data(), kind))
}

#[tauri::command]
pub fn get_catalog(
    kind: String,
    query: String,
    editor: State<'_, SharedEditor>,
) -> Reply<Vec<CatalogEntry>> {
    let editor = lock(&editor)?;
    Ok(views::catalog(editor.save()?, editor.data(), &kind, &query))
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
    Ok(views::switch_page(editor.save()?, editor.data(), &query, offset, limit))
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
    Ok(views::variable_page(editor.save()?, editor.data(), &query, offset, limit))
}

#[tauri::command]
pub fn set_variable(id: i64, value: Scalar, editor: State<'_, SharedEditor>) -> Reply<Applied> {
    let mut editor = lock(&editor)?;
    let save = editor.save_mut()?;
    let previous = save.variable(id);
    let new = assign_scalar(&mut save.heap, save.engine, previous, &value);
    require(save.set_variable(id, new), "This save has no variable list.")?;
    Ok(Applied::ok())
}

#[tauri::command]
pub fn get_self_switches(editor: State<'_, SharedEditor>) -> Reply<Vec<SelfSwitchRow>> {
    let editor = lock(&editor)?;
    Ok(views::self_switches(editor.save()?, editor.data()))
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
    require(
        editor.save_mut()?.set_self_switch(map_id, event_id, &letter, on),
        "This save has no self-switch table.",
    )?;
    Ok(views::self_switches(editor.save()?, editor.data()))
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
    Ok(views::self_switches(editor.save()?, editor.data()))
}

// -------------------------------------------------------------- raw editor

#[tauri::command]
pub fn raw_root(editor: State<'_, SharedEditor>) -> Reply<NodeView> {
    let editor = lock(&editor)?;
    Ok(views::raw_root(editor.save()?))
}

#[tauri::command]
pub fn raw_node(node: u32, editor: State<'_, SharedEditor>) -> Reply<NodeView> {
    let editor = lock(&editor)?;
    views::node_view(&editor.save()?.heap, node)
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
    let save = editor.save_mut()?;
    let engine = save.engine;

    // Read the old value first so a replaced string can keep its encoding.
    let previous = read_slot(save, node, &slot);
    let new = assign_scalar(&mut save.heap, engine, previous, &value);
    write_slot(save, node, &slot, new)?;
    save.dirty = true;

    match node {
        Some(id) => views::node_view(&save.heap, id)
            .ok_or_else(|| format!("There is no value {id} in this save.")),
        None => Ok(views::raw_root(save)),
    }
}

fn read_slot(
    save: &rpgsave::rpg::SaveFile,
    node: Option<u32>,
    slot: &Slot,
) -> Option<Value> {
    if let Slot::Document(i) = slot {
        return save.documents.get(*i).copied();
    }
    let id = node?;
    let node = save.heap.get(id)?;
    match (slot, &node.kind) {
        (Slot::Ivar(name), _) => node
            .ivars
            .iter()
            .find(|(k, _)| save.heap.sym(*k) == name)
            .map(|(_, v)| *v),
        (Slot::Index(i), NodeKind::Array(items)) => items.get(*i).copied(),
        (Slot::Entry(i), NodeKind::Hash { entries, .. }) => entries.get(*i).map(|(_, v)| *v),
        (Slot::EntryKey(i), NodeKind::Hash { entries, .. }) => entries.get(*i).map(|(k, _)| *k),
        (Slot::Default, NodeKind::Hash { default, .. }) => *default,
        (Slot::Field(name), NodeKind::Struct { fields, .. }) => fields
            .iter()
            .find(|(k, _)| save.heap.sym(*k) == name)
            .map(|(_, v)| *v),
        (Slot::Inner, NodeKind::UserMarshal { value, .. }) => Some(*value),
        (Slot::Inner, NodeKind::UserClass { inner, .. }) => Some(*inner),
        (Slot::Inner, NodeKind::Extended { inner, .. }) => Some(*inner),
        _ => None,
    }
}

fn write_slot(
    save: &mut rpgsave::rpg::SaveFile,
    node: Option<u32>,
    slot: &Slot,
    new: Value,
) -> Reply<()> {
    if let Slot::Document(i) = slot {
        let doc = save
            .documents
            .get_mut(*i)
            .ok_or_else(|| format!("There is no document {i} in this save."))?;
        *doc = new;
        return Ok(());
    }
    let id = node.ok_or("This value has no parent to write into.")?;

    // Both of these name their slot with a symbol, which has to be interned
    // before the node is borrowed mutably.
    if let Slot::Ivar(name) = slot {
        let sym = save.heap.intern(name);
        let node = save.heap.node_mut(id);
        match node.ivars.iter_mut().find(|(k, _)| *k == sym) {
            Some(entry) => entry.1 = new,
            None => node.ivars.push((sym, new)),
        }
        return Ok(());
    }
    if let Slot::Field(name) = slot {
        let sym = save.heap.intern(name);
        if let NodeKind::Struct { fields, .. } = &mut save.heap.node_mut(id).kind
            && let Some(entry) = fields.iter_mut().find(|(k, _)| *k == sym)
        {
            entry.1 = new;
            return Ok(());
        }
        return Err(format!("This value has no field {name}."));
    }

    let node = save.heap.node_mut(id);
    let ok = match (slot, &mut node.kind) {
        (Slot::Index(i), NodeKind::Array(items)) => items.get_mut(*i).map(|v| *v = new).is_some(),
        (Slot::Entry(i), NodeKind::Hash { entries, .. }) => {
            entries.get_mut(*i).map(|e| e.1 = new).is_some()
        }
        (Slot::EntryKey(i), NodeKind::Hash { entries, .. }) => {
            entries.get_mut(*i).map(|e| e.0 = new).is_some()
        }
        (Slot::Default, NodeKind::Hash { default, .. }) => {
            *default = Some(new);
            true
        }
        (Slot::Inner, NodeKind::UserMarshal { value, .. }) => {
            *value = new;
            true
        }
        (Slot::Inner, NodeKind::UserClass { inner, .. }) => {
            *inner = new;
            true
        }
        (Slot::Inner, NodeKind::Extended { inner, .. }) => {
            *inner = new;
            true
        }
        _ => false,
    };
    if ok {
        Ok(())
    } else {
        Err("That value cannot be edited here.".to_owned())
    }
}

// ------------------------------------------------------------------ shared

fn lock<'a>(
    editor: &'a State<'_, SharedEditor>,
) -> Reply<std::sync::MutexGuard<'a, Editor>> {
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
