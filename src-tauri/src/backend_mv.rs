//! RPG Maker MV behind the common interface.
//!
//! MV is the first engine here that is not Ruby, but the editor's model of a
//! save fits it almost unchanged: the ids, counters and tables mean the same
//! things under JavaScript names. What differs is named below — MV holds
//! experience per class, marks each equipped thing with what kind it is, and
//! keeps its database as JSON beside the save.

use std::path::{Path, PathBuf};

use rpgsave::mv::database;
use rpgsave::mv::json::Json;
use rpgsave::mv::save::{
    actor as field, EQUIP_SLOTS, FRAME_RATE, MAX_GOLD, MAX_ITEM_COUNT, PARAMS,
};
use rpgsave::mv::MvSave;
use rpgsave::rpg::gamedata::{ClassInfo, Entry};
use rpgsave::rpg::{format_playtime, GameData, ItemKind};
use rpgsave_protocol::*;

use crate::backend::{Backend, Reply};

/// The actor fields that have an editor of their own above "Other fields".
const DEDICATED: [&str; 11] = [
    field::ACTOR_ID,
    field::NAME,
    field::NICKNAME,
    field::CLASS_ID,
    field::LEVEL,
    field::EXP,
    field::HP,
    field::MP,
    field::TP,
    field::PARAM_PLUS,
    field::EQUIPS,
];

fn item_kind(kind: &str) -> Option<ItemKind> {
    match kind {
        "item" => Some(ItemKind::Item),
        "weapon" => Some(ItemKind::Weapon),
        "armor" => Some(ItemKind::Armor),
        _ => None,
    }
}

/// Maps the names the interface uses onto the engine's own field names, so
/// that both `@hp` from the shared parts of the interface and `_hp` from this
/// backend's own views arrive at the same place.
fn field_key(name: &str) -> String {
    // One marker at most, so a field whose own name begins with an underscore
    // is not renamed on its way through.
    let bare = name.strip_prefix('@').unwrap_or(name);
    let bare = bare.strip_prefix('_').unwrap_or(bare);
    match bare {
        "hp" => field::HP.to_owned(),
        "mp" | "sp" => field::MP.to_owned(),
        "tp" => field::TP.to_owned(),
        "name" => field::NAME.to_owned(),
        "nickname" => field::NICKNAME.to_owned(),
        "class_id" => field::CLASS_ID.to_owned(),
        "level" => field::LEVEL.to_owned(),
        // Anything else is one of the engine's own fields, which the raw
        // browser and the "Other fields" list name exactly.
        _ => format!("_{bare}"),
    }
}

/// An actor's name as the save has it; the save always carries one, because
/// the engine copies it from the database when the actor is created.
fn actor_label(save: &MvSave, data: Option<&GameData>, id: i64) -> String {
    save.actor_text(id, field::NAME)
        .filter(|name| !name.is_empty())
        .map(str::to_owned)
        .or_else(|| data?.actor(id).map(|a| a.name.clone()).filter(|n| !n.is_empty()))
        .unwrap_or_else(|| format!("Actor {id}"))
}

fn class_of<'a>(
    save: &MvSave,
    data: Option<&'a GameData>,
    id: i64,
) -> Option<&'a ClassInfo> {
    data?.class(save.actor_int(id, field::CLASS_ID)?)
}

/// A parameter's base value at an actor's level, from the class curve.
fn param_base(save: &MvSave, data: Option<&GameData>, id: i64, index: usize) -> Option<i64> {
    let level = save.actor_int(id, field::LEVEL)?;
    class_of(save, data, id)?.param(index as u32, level)
}

fn max_param(save: &MvSave, data: Option<&GameData>, id: i64, index: usize) -> Option<i64> {
    let base = param_base(save, data, id, index)?;
    Some(base + save.actor_param_plus(id, index).unwrap_or(0))
}

fn entry_of<'a>(data: Option<&'a GameData>, kind: &str, id: i64) -> Option<&'a Entry> {
    let kind = item_kind(kind)?;
    data?.catalog(kind).iter().find(|e| e.id == id)
}

/// Turns one of a variable's values into something the interface can edit.
fn scalar_of(value: Option<&Json>) -> (Scalar, Option<String>) {
    match value {
        None | Some(Json::Null) => (Scalar::Nil, None),
        Some(Json::Bool(b)) => (Scalar::Bool(*b), None),
        Some(Json::Number(n)) => {
            let value = n.value();
            if value.fract() == 0.0 && value.abs() < 9e15 {
                (Scalar::Int(value as i64), None)
            } else {
                (Scalar::Float(value), None)
            }
        }
        Some(Json::Str(text)) => (Scalar::Str(text.as_str().to_owned()), None),
        // A game that stored a list or an object in a variable is doing
        // something this editor should show but not flatten into a box.
        Some(Json::Array(items)) => (Scalar::Nil, Some(format!("array of {}", items.len()))),
        Some(value @ Json::Object(_)) => (
            Scalar::Nil,
            Some(match value.class_name() {
                Some(class) => class.to_owned(),
                None => "object".to_owned(),
            }),
        ),
    }
}

fn json_of(value: &Scalar) -> Option<Json> {
    match value {
        Scalar::Nil => Some(Json::Null),
        Scalar::Bool(b) => Some(Json::Bool(*b)),
        Scalar::Int(i) => Some(Json::int(*i)),
        Scalar::Float(f) => Some(Json::float(*f)),
        Scalar::Str(s) => Some(Json::text(s.clone())),
        Scalar::Sym(_) => None,
    }
}

impl Backend for MvSave {
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
        self.dirty = false;
        MvSave::to_bytes(self)
    }

    fn byte_size(&self) -> usize {
        self.size_bytes
    }

    fn set_byte_size(&mut self, size: usize) {
        self.size_bytes = size;
    }

    fn guess_data_dir(&self) -> Option<PathBuf> {
        database::find_data_dir(self.path.parent()?)
    }

    fn load_game_data(&self, dir: &Path) -> Option<GameData> {
        // Pointed at a game folder rather than its `data`, look inside.
        database::load(dir).or_else(|| database::load(&database::find_data_dir(dir)?))
    }

    fn summary(&self, data: Option<&GameData>) -> Summary {
        let in_party = self.party_member_ids();
        let party = in_party
            .iter()
            .map(|id| PartyMember {
                actor_id: *id,
                name: actor_label(self, data, *id),
                level: self.actor_int(*id, field::LEVEL).unwrap_or(0),
                class_name: class_of(self, data, *id).map(|c| c.name.clone()),
                hp: self.actor_int(*id, field::HP),
                mp: self.actor_int(*id, field::MP),
            })
            .collect();

        let roster = self
            .actor_ids()
            .into_iter()
            .map(|id| RosterEntry {
                actor_id: id,
                name: actor_label(self, data, id),
                in_save: true,
                in_party: in_party.contains(&id),
            })
            .collect();

        let seconds = self.playtime_frames() / FRAME_RATE;
        let (x, y) = self.player_position();
        Summary {
            path: self.path.to_string_lossy().into_owned(),
            file_name: self
                .path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
            engine: "mv".to_owned(),
            engine_label: "MV".to_owned(),
            dirty: self.dirty,
            file_size: self.size_bytes,
            document_count: 1,
            notes: self.notes.clone(),

            game_title: data.and_then(|d| d.game_title.clone()),
            data_dir: data.map(|d| d.dir.to_string_lossy().into_owned()),
            data_loaded: data.map(|d| d.loaded.clone()).unwrap_or_default(),
            data_missing: data.map(|d| d.missing.clone()).unwrap_or_default(),

            has_playtime: true,
            playtime_seconds: seconds,
            playtime_text: format_playtime(seconds),
            frame_rate: FRAME_RATE,
            save_count: self.save_count(),

            gold: Some(self.gold()),
            max_gold: MAX_GOLD,
            currency: data
                .and_then(|d| d.currency.clone())
                .unwrap_or_else(|| "G".to_owned()),
            steps: Some(self.steps()),

            map_id: Some(self.map_id()),
            map_name: data
                .and_then(|d| d.map_name(self.map_id()))
                .map(str::to_owned),
            player_x: Some(x),
            player_y: Some(y),

            party,
            roster,

            switch_count: self.switch_count(),
            variable_count: self.variable_count(),
            self_switch_count: self.self_switches().len(),
        }
    }

    fn actor_view(&self, data: Option<&GameData>, actor_id: i64) -> Option<ActorView> {
        let actor = self.actor(actor_id)?;
        let class_id = actor.int_at(field::CLASS_ID);
        let class = class_of(self, data, actor_id);
        let level = actor.int_at(field::LEVEL);

        let params = PARAMS
            .iter()
            .enumerate()
            .map(|(index, label)| ParamView {
                index,
                label: (*label).to_owned(),
                plus: self.actor_param_plus(actor_id, index).unwrap_or(0),
                base: param_base(self, data, actor_id, index),
                ivar: format!("{}[{index}]", field::PARAM_PLUS),
            })
            .collect();

        let equips = self
            .actor_equipment(actor_id)
            .into_iter()
            .enumerate()
            .map(|(slot, (kind, item_id))| EquipSlot {
                slot,
                // The game names its slots in its database; the engine's
                // default order is weapon, shield, head, body, accessory.
                label: data
                    .and_then(|d| d.equip_type_name(slot))
                    .map(str::to_owned)
                    .unwrap_or_else(|| {
                        EQUIP_SLOTS.get(slot).copied().unwrap_or("Equipment").to_owned()
                    }),
                item_name: entry_of(data, &kind, item_id).map(|e| e.name.clone()),
                kind,
                item_id,
            })
            .collect();

        let named = |ids: Vec<i64>, name: &dyn Fn(i64) -> Option<String>, what: &str| {
            ids.into_iter()
                .map(|id| NamedId {
                    id,
                    name: name(id).unwrap_or_else(|| format!("{what} {id}")),
                })
                .collect::<Vec<_>>()
        };

        // Everything else the actor carries, so that fields this editor has no
        // dedicated control for — including whatever plugins added — can still
        // be read and changed.
        let extra = actor
            .members()
            .map(|members| {
                members
                    .iter()
                    .filter(|(key, _)| !key.as_str().starts_with('@'))
                    .filter(|(key, _)| !DEDICATED.contains(&key.as_str()))
                    .filter(|(_, value)| !value.is_container())
                    .map(|(key, value)| FieldView {
                        ivar: key.as_str().to_owned(),
                        label: key
                            .as_str()
                            .strip_prefix('_')
                            .unwrap_or(key.as_str())
                            .to_owned(),
                        value: scalar_of(Some(value)).0,
                    })
                    .collect()
            })
            .unwrap_or_default();

        Some(ActorView {
            actor_id,
            name: actor_label(self, data, actor_id),
            nickname: actor
                .str_at(field::NICKNAME)
                .filter(|n| !n.is_empty())
                .map(str::to_owned),
            class_id,
            class_name: class.map(|c| c.name.clone()),
            classes: data
                .map(|d| {
                    d.classes
                        .iter()
                        .map(|c| NamedId { id: c.id, name: c.name.clone() })
                        .collect()
                })
                .unwrap_or_default(),
            in_party: self.party_member_ids().contains(&actor_id),

            level,
            max_level: data
                .and_then(|d| d.actor(actor_id))
                .map(|a| a.max_level)
                .unwrap_or(99),
            exp: self.actor_exp(actor_id),
            exp_this_level: class.zip(level).and_then(|(c, l)| c.exp_for_level(l)),
            exp_next_level: class.zip(level).and_then(|(c, l)| c.exp_for_level(l + 1)),
            // The curve is in the class, so levelling can fix experience up
            // only when the database is at hand.
            exp_follows_level: class.is_some_and(|c| c.exp_params.is_some()),

            hp: actor.int_at(field::HP),
            mp: actor.int_at(field::MP),
            tp: actor.get(field::TP).and_then(Json::as_f64),
            max_hp: max_param(self, data, actor_id, 0),
            max_mp: max_param(self, data, actor_id, 1),
            mp_label: "MP".to_owned(),
            mp_ivar: field::MP.to_owned(),

            params,
            equips,
            skills: named(
                self.actor_skills(actor_id),
                &|id| data?.skill_name(id).map(str::to_owned),
                "Skill",
            ),
            states: named(
                self.actor_states(actor_id),
                &|id| data?.state_name(id).map(str::to_owned),
                "State",
            ),
            extra,
        })
    }

    fn inventory(&self, data: Option<&GameData>, kind: &str) -> Reply<InventoryView> {
        let item_kind = item_kind(kind).ok_or_else(|| format!("No {kind} table."))?;
        let rows = self
            .items(item_kind)
            .into_iter()
            .map(|(id, count)| {
                let entry = entry_of(data, kind, id);
                ItemRow {
                    id,
                    count,
                    name: entry.map(|e| e.name.clone()),
                    icon_index: entry.map(|e| e.icon_index).unwrap_or(0),
                    description: entry.map(|e| e.description.clone()).unwrap_or_default(),
                    price: entry.map(|e| e.price).unwrap_or(0),
                }
            })
            .collect();
        Ok(InventoryView { kind: kind.to_owned(), rows, max_count: MAX_ITEM_COUNT })
    }

    fn catalog(&self, data: Option<&GameData>, kind: &str, query: &str) -> Vec<CatalogEntry> {
        let Some((data, item_kind)) = data.zip(item_kind(kind)) else { return Vec::new() };
        let held = self.items(item_kind);
        let query = query.trim().to_lowercase();
        data.catalog(item_kind)
            .iter()
            // Every MV database carries unnamed rows for the ids a project has
            // not filled in.
            .filter(|e| !e.name.is_empty())
            .filter(|e| {
                query.is_empty()
                    || e.name.to_lowercase().contains(&query)
                    || e.id.to_string() == query
            })
            .map(|e| CatalogEntry {
                id: e.id,
                name: e.name.clone(),
                icon_index: e.icon_index,
                description: e.description.clone(),
                price: e.price,
                etype_id: e.etype_id,
                held: held.iter().find(|(id, _)| *id == e.id).map(|(_, c)| *c).unwrap_or(0),
            })
            .collect()
    }

    fn switch_page(
        &self,
        data: Option<&GameData>,
        query: &str,
        offset: usize,
        limit: usize,
    ) -> SwitchPage {
        // A save only has room for the switches a game has touched, while the
        // database knows every one the project defines.
        let total = self
            .switch_count()
            .max(data.map(|d| d.switch_names.len().saturating_sub(1)).unwrap_or(0));
        let name = |id: i64| data.and_then(|d| d.switch_name(id)).map(str::to_owned);
        let query = query.trim().to_lowercase();
        let matches: Vec<i64> = (1..=total as i64)
            .filter(|id| {
                query.is_empty()
                    || id.to_string().contains(&query)
                    || name(*id).is_some_and(|n| n.to_lowercase().contains(&query))
            })
            .collect();
        SwitchPage {
            total,
            matched: matches.len(),
            rows: matches
                .into_iter()
                .skip(offset)
                .take(limit)
                .map(|id| SwitchRow { id, name: name(id), on: self.switch(id) })
                .collect(),
        }
    }

    fn variable_page(
        &self,
        data: Option<&GameData>,
        query: &str,
        offset: usize,
        limit: usize,
    ) -> VariablePage {
        let total = self
            .variable_count()
            .max(data.map(|d| d.variable_names.len().saturating_sub(1)).unwrap_or(0));
        let name = |id: i64| data.and_then(|d| d.variable_name(id)).map(str::to_owned);
        let query = query.trim().to_lowercase();
        let matches: Vec<i64> = (1..=total as i64)
            .filter(|id| {
                query.is_empty()
                    || id.to_string().contains(&query)
                    || name(*id).is_some_and(|n| n.to_lowercase().contains(&query))
            })
            .collect();
        VariablePage {
            total,
            matched: matches.len(),
            rows: matches
                .into_iter()
                .skip(offset)
                .take(limit)
                .map(|id| {
                    let (value, complex) = scalar_of(self.variable(id));
                    VariableRow { id, name: name(id), value, complex }
                })
                .collect(),
        }
    }

    fn self_switches(&self, data: Option<&GameData>) -> Vec<SelfSwitchRow> {
        MvSave::self_switches(self)
            .into_iter()
            .map(|(map_id, event_id, letter, on)| SelfSwitchRow {
                map_id,
                map_name: data.and_then(|d| d.map_name(map_id)).map(str::to_owned),
                event_id,
                letter,
                on,
            })
            .collect()
    }

    fn raw_root(&self) -> NodeView {
        crate::views_mv::raw_root(self)
    }

    fn raw_node(&self, node: u32) -> Option<NodeView> {
        crate::views_mv::raw_node(self, node)
    }

    fn set_raw_scalar(&mut self, node: Option<u32>, slot: &Slot, value: &Scalar) -> Reply<()> {
        crate::views_mv::set_raw_scalar(self, node, slot, value)
    }

    fn set_gold(&mut self, value: i64) -> bool {
        MvSave::set_gold(self, value)
    }

    fn set_steps(&mut self, value: i64) -> bool {
        MvSave::set_steps(self, value)
    }

    fn set_playtime_seconds(&mut self, seconds: i64) -> bool {
        MvSave::set_playtime_seconds(self, seconds)
    }

    fn set_position(&mut self, x: i64, y: i64) -> bool {
        self.set_player_position(x, y)
    }

    fn set_party(&mut self, ids: &[i64]) -> Reply<()> {
        if let Some(missing) = ids.iter().find(|id| self.actor(**id).is_none()) {
            return Err(format!("This save holds no actor {missing}."));
        }
        self.set_party_member_ids(ids);
        Ok(())
    }

    fn has_actor(&self, actor_id: i64) -> bool {
        self.actor(actor_id).is_some()
    }

    fn set_actor_text(&mut self, actor_id: i64, field: &str, text: &str) -> bool {
        self.set_actor_field(actor_id, &field_key(field), Json::text(text))
    }

    fn set_actor_int(&mut self, actor_id: i64, field: &str, value: i64) -> bool {
        self.set_actor_field(actor_id, &field_key(field), Json::int(value))
    }

    fn set_actor_scalar(&mut self, actor_id: i64, field: &str, value: &Scalar) -> bool {
        let Some(value) = json_of(value) else { return false };
        self.set_actor_field(actor_id, &field_key(field), value)
    }

    fn set_actor_level(&mut self, actor_id: i64, level: i64, data: Option<&GameData>) -> bool {
        let max = data
            .and_then(|d| d.actor(actor_id))
            .map(|a| a.max_level)
            .unwrap_or(99);
        let level = level.clamp(1, max.max(1));
        if !self.set_actor_field(actor_id, field::LEVEL, Json::int(level)) {
            return false;
        }
        // Experience and level are two records of the same thing, and the
        // engine trusts experience: a level without the experience to match
        // snaps back the next time the actor gains any.
        if let Some(exp) = class_of(self, data, actor_id).and_then(|c| c.exp_for_level(level)) {
            self.set_actor_exp(actor_id, exp);
        }
        true
    }

    fn set_actor_exp(&mut self, actor_id: i64, exp: i64) -> bool {
        MvSave::set_actor_exp(self, actor_id, exp)
    }

    fn set_actor_param(&mut self, actor_id: i64, index: usize, value: i64) -> bool {
        self.set_actor_param_plus(actor_id, index, value)
    }

    fn set_actor_equip(&mut self, actor_id: i64, slot: usize, kind: &str, item_id: i64) -> bool {
        self.set_actor_equipment(actor_id, slot, kind, item_id)
    }

    fn set_actor_skills(&mut self, actor_id: i64, ids: &[i64]) -> bool {
        MvSave::set_actor_skills(self, actor_id, ids)
    }

    fn set_actor_states(&mut self, actor_id: i64, ids: &[i64]) -> bool {
        MvSave::set_actor_states(self, actor_id, ids)
    }

    fn full_heal(&mut self, actor_id: i64, data: Option<&GameData>) -> bool {
        // Without the database there is no curve to read a maximum from; the
        // engine clamps an over-large value the next time it refreshes.
        let hp = max_param(self, data, actor_id, 0).unwrap_or(9_999);
        let mp = max_param(self, data, actor_id, 1).unwrap_or(9_999);
        let healed = self.set_actor_field(actor_id, field::HP, Json::int(hp))
            && self.set_actor_field(actor_id, field::MP, Json::int(mp));
        healed && MvSave::set_actor_states(self, actor_id, &[])
    }

    fn set_item_count(&mut self, kind: &str, id: i64, count: i64) -> bool {
        let Some(kind) = item_kind(kind) else { return false };
        MvSave::set_item_count(self, kind, id, count)
    }

    fn set_switch(&mut self, id: i64, on: bool) -> bool {
        MvSave::set_switch(self, id, on)
    }

    fn set_variable(&mut self, id: i64, value: &Scalar) -> bool {
        let Some(value) = json_of(value) else { return false };
        MvSave::set_variable(self, id, value)
    }

    fn set_self_switch(&mut self, map_id: i64, event_id: i64, letter: &str, on: bool) -> bool {
        MvSave::set_self_switch(self, map_id, event_id, letter, on)
    }

    fn remove_self_switch(&mut self, map_id: i64, event_id: i64, letter: &str) -> bool {
        MvSave::remove_self_switch(self, map_id, event_id, letter)
    }
}
