//! The RGSS engines (XP, VX, VX Ace) behind the common interface.

use std::path::{Path, PathBuf};

use rpgsave::marshal::{NodeKind, Value};
use rpgsave::rpg::actor::LevelChange;
use rpgsave::rpg::save::ItemKind;
use rpgsave::rpg::{GameData, SaveFile};
use rpgsave_protocol::*;

use crate::backend::{Backend, Reply};
use crate::state::assign_scalar;
use crate::views;

fn kind_of(kind: &str) -> Option<ItemKind> {
    ItemKind::from_id(kind)
}

impl Backend for SaveFile {
    fn path(&self) -> &Path {
        &self.path
    }

    fn set_path(&mut self, path: PathBuf) {
        self.path = path;
    }

    fn is_dirty(&self) -> bool {
        self.dirty
    }

    fn mark_saved(&mut self) {
        self.dirty = false;
    }

    fn encode(&mut self) -> Vec<u8> {
        self.to_bytes()
    }

    fn byte_size(&self) -> usize {
        self.size_bytes
    }

    fn set_byte_size(&mut self, size: usize) {
        self.size_bytes = size;
    }

    fn guess_data_dir(&self) -> Option<PathBuf> {
        SaveFile::guess_data_dir(self)
    }

    fn load_game_data(&self, dir: &Path) -> Option<GameData> {
        let data = GameData::load(dir, self.engine);
        (!data.is_empty()).then_some(data)
    }

    fn summary(&self, data: Option<&GameData>) -> Summary {
        views::summary(self, data)
    }

    fn actor_view(&self, data: Option<&GameData>, actor_id: i64) -> Option<ActorView> {
        views::actor_view(self, data, actor_id)
    }

    fn inventory(&self, data: Option<&GameData>, kind: &str) -> Reply<InventoryView> {
        let kind = kind_of(kind).ok_or_else(|| format!("Unknown item kind {kind:?}."))?;
        Ok(views::inventory(self, data, kind))
    }

    fn catalog(&self, data: Option<&GameData>, kind: &str, query: &str) -> Vec<CatalogEntry> {
        views::catalog(self, data, kind, query)
    }

    fn switch_page(
        &self,
        data: Option<&GameData>,
        query: &str,
        offset: usize,
        limit: usize,
    ) -> SwitchPage {
        views::switch_page(self, data, query, offset, limit)
    }

    fn variable_page(
        &self,
        data: Option<&GameData>,
        query: &str,
        offset: usize,
        limit: usize,
    ) -> VariablePage {
        views::variable_page(self, data, query, offset, limit)
    }

    fn self_switches(&self, data: Option<&GameData>) -> Vec<SelfSwitchRow> {
        views::self_switches(self, data)
    }

    fn raw_root(&self) -> NodeView {
        views::raw_root(self)
    }

    fn raw_node(&self, node: u32) -> Option<NodeView> {
        views::node_view(&self.heap, node)
    }

    fn set_raw_scalar(&mut self, node: Option<u32>, slot: &Slot, value: &Scalar) -> Reply<()> {
        let engine = self.engine;
        // Read the old value first so a replaced string can keep its encoding.
        let previous = read_slot(self, node, slot);
        let new = assign_scalar(&mut self.heap, engine, previous, value);
        write_slot(self, node, slot, new)?;
        self.dirty = true;
        Ok(())
    }

    fn set_gold(&mut self, value: i64) -> bool {
        SaveFile::set_gold(self, value)
    }

    fn set_steps(&mut self, value: i64) -> bool {
        SaveFile::set_steps(self, value)
    }

    fn set_playtime_seconds(&mut self, seconds: i64) -> bool {
        SaveFile::set_playtime_seconds(self, seconds)
    }

    fn set_position(&mut self, x: i64, y: i64) -> bool {
        self.set_player_position(x, y)
    }

    fn set_party(&mut self, ids: &[i64]) -> Reply<()> {
        // XP holds the actors themselves, so one that the game has never
        // created cannot be put in the party.
        if let Some(missing) = ids.iter().find(|id| !self.party_can_include(**id)) {
            return Err(format!(
                "Actor {missing} has not been created in this save yet. RPG Maker XP stores the \
                 party members themselves rather than their ids, so only actors the game has \
                 already made can join."
            ));
        }
        if self.set_party_member_ids(ids) {
            Ok(())
        } else {
            Err("This save has no party list.".to_owned())
        }
    }

    fn has_actor(&self, actor_id: i64) -> bool {
        self.actor(actor_id).is_some()
    }

    fn set_actor_text(&mut self, actor_id: i64, field: &str, text: &str) -> bool {
        self.update_actor(actor_id, |save, actor| save.set_actor_string(actor, field, text))
    }

    fn set_actor_int(&mut self, actor_id: i64, field: &str, value: i64) -> bool {
        self.update_actor(actor_id, |save, actor| {
            if save.heap.ivar(actor, field).is_none() {
                return false;
            }
            save.heap.set_ivar(actor, field, Value::Int(value));
            save.dirty = true;
            true
        })
    }

    fn set_actor_scalar(&mut self, actor_id: i64, field: &str, value: &Scalar) -> bool {
        self.update_actor(actor_id, |save, actor| {
            let previous = save.heap.ivar(actor, field);
            // Each copy keeps its own string node, as the engine wrote them.
            let new = assign_scalar(&mut save.heap, save.engine, previous, value);
            save.heap.set_ivar(actor, field, new);
            save.dirty = true;
            true
        })
    }

    fn set_actor_level(&mut self, actor_id: i64, level: i64, data: Option<&GameData>) -> bool {
        self.update_actor(actor_id, |save, actor| {
            save.set_actor_level(actor, level, data) != LevelChange::Failed
        })
    }

    fn set_actor_exp(&mut self, actor_id: i64, exp: i64) -> bool {
        self.update_actor(actor_id, |save, actor| save.set_actor_exp(actor, exp))
    }

    fn set_actor_param(&mut self, actor_id: i64, index: usize, value: i64) -> bool {
        self.update_actor(actor_id, |save, actor| save.set_actor_param_plus(actor, index, value))
    }

    fn set_actor_equip(&mut self, actor_id: i64, slot: usize, kind: &str, item_id: i64) -> bool {
        let Some(kind) = kind_of(kind) else { return false };
        self.update_actor(actor_id, |save, actor| save.set_actor_equip(actor, slot, kind, item_id))
    }

    fn set_actor_skills(&mut self, actor_id: i64, ids: &[i64]) -> bool {
        self.update_actor(actor_id, |save, actor| save.set_actor_id_list(actor, "@skills", ids))
    }

    fn set_actor_states(&mut self, actor_id: i64, ids: &[i64]) -> bool {
        self.update_actor(actor_id, |save, actor| save.set_actor_states(actor, ids))
    }

    fn full_heal(&mut self, actor_id: i64, data: Option<&GameData>) -> bool {
        self.update_actor(actor_id, |save, actor| save.full_heal_actor(actor, data))
    }

    fn set_item_count(&mut self, kind: &str, id: i64, count: i64) -> bool {
        let Some(kind) = kind_of(kind) else { return false };
        SaveFile::set_item_count(self, kind, id, count)
    }

    fn set_switch(&mut self, id: i64, on: bool) -> bool {
        SaveFile::set_switch(self, id, on)
    }

    fn set_variable(&mut self, id: i64, value: &Scalar) -> bool {
        let previous = self.variable(id);
        let engine = self.engine;
        let new = assign_scalar(&mut self.heap, engine, previous, value);
        SaveFile::set_variable(self, id, new)
    }

    fn set_self_switch(&mut self, map_id: i64, event_id: i64, letter: &str, on: bool) -> bool {
        SaveFile::set_self_switch(self, map_id, event_id, letter, on)
    }

    fn remove_self_switch(&mut self, map_id: i64, event_id: i64, letter: &str) -> bool {
        SaveFile::remove_self_switch(self, map_id, event_id, letter)
    }
}

fn read_slot(save: &SaveFile, node: Option<u32>, slot: &Slot) -> Option<Value> {
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

fn write_slot(save: &mut SaveFile, node: Option<u32>, slot: &Slot, new: Value) -> Reply<()> {
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
