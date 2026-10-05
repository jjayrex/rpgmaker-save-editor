//! RPG Maker 2000 and 2003 behind the common interface.
//!
//! These saves carry ids and no names — the names live in `RPG_RT.ldb`, which
//! this editor does not read — so lists show ids and anything the format does
//! not store (nicknames on the RGSS model, self switches, maximum HP) is
//! reported as absent rather than invented.

use std::path::{Path, PathBuf};

use rpgsave::lcf::save::{
    actor as actor_tag, LcfSave, EQUIP_SLOTS, FRAME_RATE, MAX_GOLD, MAX_ITEM_COUNT,
};
use rpgsave::rpg::{format_playtime, GameData};
use rpgsave_protocol::*;

use crate::backend::{Backend, Reply};

/// Stat bonuses, in the order the save stores them.
const MODS: [(&str, u32); 6] = [
    ("Max HP", actor_tag::HP_MOD),
    ("Max SP", actor_tag::SP_MOD),
    ("Attack", actor_tag::ATTACK_MOD),
    ("Defense", actor_tag::DEFENSE_MOD),
    ("Spirit", actor_tag::SPIRIT_MOD),
    ("Agility", actor_tag::AGILITY_MOD),
];

/// Maps the field names the interface uses onto chunk numbers.
fn field_tag(field: &str) -> Option<u32> {
    match field.trim_start_matches('@') {
        "name" => Some(actor_tag::NAME),
        "nickname" | "title" => Some(actor_tag::TITLE),
        "level" => Some(actor_tag::LEVEL),
        "exp" => Some(actor_tag::EXP),
        "hp" => Some(actor_tag::CURRENT_HP),
        "sp" | "mp" => Some(actor_tag::CURRENT_SP),
        "class_id" => Some(actor_tag::CLASS_ID),
        "sprite_name" => Some(actor_tag::SPRITE_NAME),
        other => MODS
            .iter()
            .find(|(label, _)| label.to_lowercase().replace(' ', "_") == other)
            .map(|(_, tag)| *tag),
    }
}

/// A renamed actor keeps the name the game gave it; otherwise the database
/// has it, and failing that there is only the id.
fn actor_label(save: &LcfSave, data: Option<&GameData>, id: i64) -> String {
    save.actor_name(id)
        .or_else(|| data?.actor(id).map(|a| a.name.clone()).filter(|n| !n.is_empty()))
        .unwrap_or_else(|| format!("Actor {id}"))
}

/// An actor's class: the save records one only when the game has changed it,
/// so the database holds it the rest of the time.
fn actor_class_id(save: &LcfSave, data: Option<&GameData>, id: i64) -> Option<i64> {
    save.actor_int(id, actor_tag::CLASS_ID)
        .filter(|c| *c != 0)
        .or_else(|| data?.actor(id).map(|a| a.class_id).filter(|c| *c != 0))
}

/// Which of the interface's three inventory tabs an item belongs on.
///
/// These engines keep one table for everything carried and mark each entry
/// with what it is, so the tabs are filled by item type rather than by
/// separate tables.
fn item_kind(data: Option<&GameData>, id: i64) -> &'static str {
    let Some(entry) = data.and_then(|d| d.items.iter().find(|e| e.id == id)) else {
        return "item";
    };
    match entry.etype_id {
        1 => "weapon",
        2..=5 => "armor",
        _ => "item",
    }
}

fn item_entry(data: Option<&GameData>, id: i64) -> Option<&rpgsave::rpg::gamedata::Entry> {
    data?.items.iter().find(|e| e.id == id)
}

impl Backend for LcfSave {
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
        // The file-select screen reads its own copy of the leader's details.
        self.refresh_title();
        self.dirty = false;
        LcfSave::to_bytes(self)
    }

    fn byte_size(&self) -> usize {
        self.size_bytes
    }

    fn set_byte_size(&mut self, size: usize) {
        self.size_bytes = size;
    }

    fn guess_data_dir(&self) -> Option<PathBuf> {
        // The database sits beside the save, in the game's own folder.
        let dir = self.path.parent()?;
        rpgsave::lcf::database::find_database(dir).map(|_| dir.to_path_buf())
    }

    fn load_game_data(&self, dir: &Path) -> Option<GameData> {
        rpgsave::lcf::database::load(dir)
    }

    fn summary(&self, data: Option<&GameData>) -> Summary {
        let party = self
            .party_member_ids()
            .into_iter()
            .map(|id| PartyMember {
                actor_id: id,
                name: actor_label(self, data, id),
                level: self.actor_int(id, actor_tag::LEVEL).unwrap_or(0),
                class_name: actor_class_id(self, data, id)
                    .and_then(|c| data?.class_name(c).map(str::to_owned)),
                hp: self.actor_int(id, actor_tag::CURRENT_HP),
                mp: self.actor_int(id, actor_tag::CURRENT_SP),
            })
            .collect();

        let in_party = self.party_member_ids();
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
            engine: "rm2k".to_owned(),
            engine_label: "2000/2003".to_owned(),
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
            // Map names live in RPG_RT.lmt, which this editor does not read.
            map_name: None,
            player_x: Some(x),
            player_y: Some(y),

            party,
            roster,

            switch_count: self.switch_count(),
            variable_count: self.variable_count(),
            self_switch_count: 0,
        }
    }

    fn actor_view(&self, data: Option<&GameData>, actor_id: i64) -> Option<ActorView> {
        let chunks = self.actor(actor_id)?;
        let class_id = actor_class_id(self, data, actor_id);

        let params = MODS
            .iter()
            .enumerate()
            .map(|(index, (label, tag))| ParamView {
                index,
                label: (*label).to_owned(),
                plus: chunks.int(*tag).map(i64::from).unwrap_or(0),
                base: None,
                ivar: label.to_lowercase().replace(' ', "_"),
            })
            .collect();

        let equips = self
            .actor_equipment(actor_id)
            .into_iter()
            .enumerate()
            .map(|(slot, item_id)| EquipSlot {
                slot,
                // The game names its own equipment slots in the database.
                label: data
                    .and_then(|d| d.equip_type_name(slot))
                    .map(str::to_owned)
                    .unwrap_or_else(|| {
                        EQUIP_SLOTS.get(slot).copied().unwrap_or("Equipment").to_owned()
                    }),
                kind: item_kind(data, item_id).to_owned(),
                item_name: item_entry(data, item_id).map(|e| e.name.clone()),
                item_id,
            })
            .collect();

        let skills = self
            .actor_skills(actor_id)
            .into_iter()
            .map(|id| NamedId {
                id,
                name: data
                    .and_then(|d| d.skill_name(id))
                    .map(str::to_owned)
                    .unwrap_or_else(|| format!("Skill {id}")),
            })
            .collect();

        Some(ActorView {
            actor_id,
            name: actor_label(self, data, actor_id),
            nickname: chunks
                .string(actor_tag::TITLE)
                .filter(|t| *t != rpgsave::lcf::save::UNCHANGED_NAME)
                .map(rpgsave::marshal::decode_ruby_string)
                .or_else(|| data?.actor(actor_id).map(|a| a.nickname.clone()))
                .filter(|t| !t.is_empty()),
            class_id,
            class_name: class_id.and_then(|c| data?.class_name(c).map(str::to_owned)),
            classes: data
                .map(|d| {
                    d.classes
                        .iter()
                        .map(|c| NamedId { id: c.id, name: c.name.clone() })
                        .collect()
                })
                .unwrap_or_default(),
            in_party: self.party_member_ids().contains(&actor_id),

            level: chunks.int(actor_tag::LEVEL).map(i64::from),
            // The engine's ceiling rather than the game's `final_level`: that
            // field is unreliable in practice — this editor's own test file
            // sets it to 1 while shipping the full 99 levels of stat curves.
            max_level: 99,
            exp: chunks.int(actor_tag::EXP).map(i64::from),
            exp_this_level: None,
            exp_next_level: None,
            // The curve lives in RPG_RT.ldb, which this editor does not read.
            exp_follows_level: false,

            hp: chunks.int(actor_tag::CURRENT_HP).map(i64::from),
            mp: chunks.int(actor_tag::CURRENT_SP).map(i64::from),
            tp: None,
            max_hp: None,
            max_mp: None,
            mp_label: "SP".to_owned(),
            mp_ivar: "sp".to_owned(),

            params,
            equips,
            skills,
            states: Vec::new(),
            extra: Vec::new(),
        })
    }

    fn inventory(&self, data: Option<&GameData>, kind: &str) -> Reply<InventoryView> {
        // One table holds everything carried, with each entry marked as what it
        // is, so the three tabs are filled by item type.
        let rows = self
            .items()
            .into_iter()
            .filter(|(id, _)| item_kind(data, *id) == kind)
            .map(|(id, count)| {
                let entry = item_entry(data, id);
                ItemRow {
                    id,
                    count,
                    name: entry.map(|e| e.name.clone()),
                    icon_index: 0,
                    description: entry.map(|e| e.description.clone()).unwrap_or_default(),
                    price: entry.map(|e| e.price).unwrap_or(0),
                }
            })
            .collect();
        Ok(InventoryView { kind: kind.to_owned(), rows, max_count: MAX_ITEM_COUNT })
    }

    fn catalog(&self, data: Option<&GameData>, kind: &str, query: &str) -> Vec<CatalogEntry> {
        let Some(data) = data else { return Vec::new() };
        let held = self.items();
        let query = query.trim().to_lowercase();
        data.items
            .iter()
            // Entries with no name are the unused rows every database carries.
            .filter(|e| !e.name.is_empty())
            .filter(|e| item_kind(Some(data), e.id) == kind)
            .filter(|e| {
                query.is_empty()
                    || e.name.to_lowercase().contains(&query)
                    || e.id.to_string() == query
            })
            .map(|e| CatalogEntry {
                id: e.id,
                name: e.name.clone(),
                icon_index: 0,
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
        // The database knows how many the game defines, which is more than a
        // save that has only touched the first few.
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
                .map(|id| VariableRow {
                    id,
                    name: name(id),
                    value: Scalar::Int(self.variable(id)),
                    complex: None,
                })
                .collect(),
        }
    }

    fn self_switches(&self, _data: Option<&GameData>) -> Vec<SelfSwitchRow> {
        Vec::new()
    }

    fn raw_root(&self) -> NodeView {
        crate::views_lcf::raw_root(self)
    }

    fn raw_node(&self, node: u32) -> Option<NodeView> {
        crate::views_lcf::raw_node(self, node)
    }

    fn set_raw_scalar(&mut self, node: Option<u32>, slot: &Slot, value: &Scalar) -> Reply<()> {
        crate::views_lcf::set_raw_scalar(self, node, slot, value)
    }

    fn set_gold(&mut self, value: i64) -> bool {
        LcfSave::set_gold(self, value);
        true
    }

    fn set_steps(&mut self, value: i64) -> bool {
        LcfSave::set_steps(self, value);
        true
    }

    fn set_playtime_seconds(&mut self, seconds: i64) -> bool {
        LcfSave::set_playtime_seconds(self, seconds);
        true
    }

    fn set_position(&mut self, x: i64, y: i64) -> bool {
        self.set_player_position(x, y);
        true
    }

    fn set_party(&mut self, ids: &[i64]) -> Reply<()> {
        if let Some(missing) = ids.iter().find(|id| !self.has_actor(**id)) {
            return Err(format!("This save holds no actor {missing}."));
        }
        self.set_party_member_ids(ids);
        Ok(())
    }

    fn has_actor(&self, actor_id: i64) -> bool {
        self.actor(actor_id).is_some()
    }

    fn set_actor_text(&mut self, actor_id: i64, field: &str, text: &str) -> bool {
        match field_tag(field) {
            Some(actor_tag::NAME) => self.set_actor_name(actor_id, text),
            Some(tag) => {
                let bytes = text.as_bytes().to_vec();
                self.update_actor(actor_id, |chunks| chunks.set(tag, bytes))
            }
            None => false,
        }
    }

    fn set_actor_int(&mut self, actor_id: i64, field: &str, value: i64) -> bool {
        let Some(tag) = field_tag(field) else { return false };
        LcfSave::set_actor_int(self, actor_id, tag, value)
    }

    fn set_actor_scalar(&mut self, actor_id: i64, field: &str, value: &Scalar) -> bool {
        match value {
            Scalar::Int(v) => Backend::set_actor_int(self, actor_id, field, *v),
            Scalar::Float(v) => Backend::set_actor_int(self, actor_id, field, *v as i64),
            Scalar::Bool(v) => Backend::set_actor_int(self, actor_id, field, i64::from(*v)),
            Scalar::Str(s) | Scalar::Sym(s) => self.set_actor_text(actor_id, field, s),
            Scalar::Nil => false,
        }
    }

    fn set_actor_level(&mut self, actor_id: i64, level: i64, _data: Option<&GameData>) -> bool {
        LcfSave::set_actor_int(self, actor_id, actor_tag::LEVEL, level.clamp(1, 99))
    }

    fn set_actor_exp(&mut self, actor_id: i64, exp: i64) -> bool {
        LcfSave::set_actor_int(self, actor_id, actor_tag::EXP, exp)
    }

    fn set_actor_param(&mut self, actor_id: i64, index: usize, value: i64) -> bool {
        let Some((_, tag)) = MODS.get(index) else { return false };
        LcfSave::set_actor_int(self, actor_id, *tag, value)
    }

    fn set_actor_equip(&mut self, actor_id: i64, slot: usize, _kind: &str, item_id: i64) -> bool {
        self.set_actor_equipment(actor_id, slot, item_id)
    }

    fn set_actor_skills(&mut self, actor_id: i64, ids: &[i64]) -> bool {
        LcfSave::set_actor_skills(self, actor_id, ids)
    }

    fn set_actor_states(&mut self, _actor_id: i64, _ids: &[i64]) -> bool {
        false
    }

    fn full_heal(&mut self, actor_id: i64, _data: Option<&GameData>) -> bool {
        // Without the database there is no maximum to restore to, but the
        // engine clamps an over-large value the next time it refreshes.
        let hp = self.actor_int(actor_id, actor_tag::HP_MOD).unwrap_or(0).max(9_999);
        LcfSave::set_actor_int(self, actor_id, actor_tag::CURRENT_HP, hp)
            && LcfSave::set_actor_int(self, actor_id, actor_tag::CURRENT_SP, 9_999)
    }

    fn set_item_count(&mut self, _kind: &str, id: i64, count: i64) -> bool {
        LcfSave::set_item_count(self, id, count);
        true
    }

    fn set_switch(&mut self, id: i64, on: bool) -> bool {
        LcfSave::set_switch(self, id, on)
    }

    fn set_variable(&mut self, id: i64, value: &Scalar) -> bool {
        let number = match value {
            Scalar::Int(v) => *v,
            Scalar::Float(v) => *v as i64,
            Scalar::Bool(v) => i64::from(*v),
            Scalar::Nil => 0,
            Scalar::Str(s) => s.trim().parse().unwrap_or(0),
            Scalar::Sym(_) => return false,
        };
        LcfSave::set_variable(self, id, number)
    }

    fn set_self_switch(&mut self, _map: i64, _event: i64, _letter: &str, _on: bool) -> bool {
        false
    }

    fn remove_self_switch(&mut self, _map: i64, _event: i64, _letter: &str) -> bool {
        false
    }
}
