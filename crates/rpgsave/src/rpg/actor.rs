//! Reading and editing `Game_Actor` objects.
//!
//! VX and VX Ace store an actor's state quite differently — Ace keeps
//! experience per class and equipment as `Game_BaseItem` objects, VX keeps a
//! single experience total and five id fields — so everything here is written
//! against whichever instance variables the save actually has.

use crate::marshal::{NodeKind, Value};

use super::engine::Engine;
use super::gamedata::GameData;
use super::save::{ItemKind, SaveFile};

/// One equipment slot as stored in the save.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EquipRef {
    pub slot: usize,
    pub kind: ItemKind,
    pub item_id: i64,
}

/// What `set_actor_level` managed to keep consistent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LevelChange {
    /// Level and experience were both updated.
    LevelAndExp,
    /// Only the level changed; the experience curve was not available.
    LevelOnly,
    Failed,
}

/// Parameter names, in engine order.
pub const ACE_PARAMS: [&str; 8] = [
    "Max HP", "Max MP", "Attack", "Defense", "M.Attack", "M.Defense", "Agility", "Luck",
];
pub const VX_PARAMS: [(&str, &str); 6] = [
    ("Max HP", "@maxhp_plus"),
    ("Max MP", "@maxmp_plus"),
    ("Attack", "@atk_plus"),
    ("Defense", "@def_plus"),
    ("Spirit", "@spi_plus"),
    ("Agility", "@agi_plus"),
];
/// XP's six parameters, in the order its database tables store them.
pub const XP_PARAMS: [(&str, &str); 6] = [
    ("Max HP", "@maxhp_plus"),
    ("Max SP", "@maxsp_plus"),
    ("Strength", "@str_plus"),
    ("Dexterity", "@dex_plus"),
    ("Agility", "@agi_plus"),
    ("Intelligence", "@int_plus"),
];

/// Default equipment slot names, used when the database is not available.
pub const DEFAULT_SLOTS: [&str; 5] = ["Weapon", "Shield", "Head", "Body", "Accessory"];

impl SaveFile {
    pub fn actor_string(&self, actor: Value, ivar: &str) -> Option<String> {
        self.heap.ivar(actor, ivar).and_then(|v| self.heap.string(v))
    }

    /// Assigns a fresh Ruby String, mirroring `@name = "..."` rather than
    /// mutating a string the save may share with the file header.
    pub fn set_actor_string(&mut self, actor: Value, ivar: &str, text: &str) -> bool {
        let keep_ivars = self
            .heap
            .ivar(actor, ivar)
            .and_then(|v| v.as_ref())
            .map(|id| self.heap.node(id).ivars.clone());

        let new = match keep_ivars {
            Some(ivars) if !ivars.is_empty() => {
                let value = self.heap.new_str(text);
                if let Some(id) = value.as_ref() {
                    self.heap.node_mut(id).ivars = ivars;
                }
                value
            }
            // No previous string to copy from: follow the engine's convention.
            _ => super::save::new_engine_string(&mut self.heap, self.engine, text),
        };
        self.dirty = true;
        self.heap.set_ivar(actor, ivar, new)
    }

    pub fn actor_level(&self, actor: Value) -> Option<i64> {
        self.heap.ivar_int(actor, "@level")
    }

    pub fn actor_class_id(&self, actor: Value) -> Option<i64> {
        self.heap.ivar_int(actor, "@class_id")
    }

    /// Total experience. Ace keeps one total per class; VX keeps a single one.
    pub fn actor_exp(&self, actor: Value) -> Option<i64> {
        let exp = self.heap.ivar(actor, "@exp")?;
        match exp {
            Value::Int(i) => Some(i),
            _ => {
                let class_id = self.actor_class_id(actor)?;
                self.heap.hash_get(exp, Value::Int(class_id))?.as_int()
            }
        }
    }

    pub fn set_actor_exp(&mut self, actor: Value, exp: i64) -> bool {
        let exp = exp.max(0);
        let Some(current) = self.heap.ivar(actor, "@exp") else { return false };
        self.dirty = true;
        match current {
            Value::Int(_) | Value::Nil => self.heap.set_ivar(actor, "@exp", Value::Int(exp)),
            _ => {
                let Some(class_id) = self.actor_class_id(actor) else { return false };
                self.heap.hash_set(current, Value::Int(class_id), Value::Int(exp))
            }
        }
    }

    /// Experience needed to reach `level` in this actor's current class.
    ///
    /// VX stores the whole curve on the actor (`@exp_list`); Ace computes it
    /// from the class's four curve parameters, which live in the database.
    pub fn exp_for_level(&self, actor: Value, level: i64, data: Option<&GameData>) -> Option<i64> {
        if let Some(list) = self.heap.ivar(actor, "@exp_list").and_then(|l| self.heap.array(l)) {
            return list.get(level.max(0) as usize).and_then(|v| v.as_int());
        }
        let class_id = self.actor_class_id(actor)?;
        data?.class(class_id)?.exp_for_level(level)
    }

    pub fn actor_max_level(&self, actor: Value, data: Option<&GameData>) -> i64 {
        if let Some(list) = self.heap.ivar(actor, "@exp_list").and_then(|l| self.heap.array(l)) {
            return (list.len() as i64 - 1).max(1);
        }
        let from_data = self
            .heap
            .ivar_int(actor, "@actor_id")
            .and_then(|id| data?.actor(id).map(|a| a.max_level));
        from_data.unwrap_or(99)
    }

    /// Sets the level, keeping experience in step when the curve is known.
    pub fn set_actor_level(
        &mut self,
        actor: Value,
        level: i64,
        data: Option<&GameData>,
    ) -> LevelChange {
        if self.heap.ivar(actor, "@level").is_none() {
            return LevelChange::Failed;
        }
        let level = level.clamp(1, self.actor_max_level(actor, data));
        self.heap.set_ivar(actor, "@level", Value::Int(level));
        self.dirty = true;

        match self.exp_for_level(actor, level, data) {
            Some(exp) if self.set_actor_exp(actor, exp) => LevelChange::LevelAndExp,
            _ => LevelChange::LevelOnly,
        }
    }

    /// Base value of a parameter at the actor's level, from the database.
    pub fn actor_param_base(
        &self,
        actor: Value,
        param_id: u32,
        data: Option<&GameData>,
    ) -> Option<i64> {
        let data = data?;
        let level = self.actor_level(actor)?;
        // XP stores each actor's stat curve on the actor itself; VX and VX Ace
        // put it on the class.
        if let Some(params) = self
            .heap
            .ivar_int(actor, "@actor_id")
            .and_then(|id| data.actor(id))
            .and_then(|info| info.params.as_ref())
            && let Some(value) = params.get(param_id, level.max(0) as u32, 0)
        {
            return Some(i64::from(value));
        }
        data.class(self.actor_class_id(actor)?)?.param(param_id, level)
    }

    /// `(label, ivar, index, value)` for each stat bonus the actor carries.
    pub fn actor_param_plus(&self, actor: Value) -> Vec<(&'static str, String, usize, i64)> {
        if let Some(list) = self.heap.ivar(actor, "@param_plus").and_then(|p| self.heap.array(p)) {
            return list
                .iter()
                .enumerate()
                .map(|(i, v)| {
                    let label = ACE_PARAMS.get(i).copied().unwrap_or("Parameter");
                    (label, "@param_plus".to_owned(), i, v.as_int().unwrap_or(0))
                })
                .collect();
        }
        let table: &[(&str, &str)] = match self.engine {
            Engine::Xp => &XP_PARAMS,
            _ => &VX_PARAMS,
        };
        table
            .iter()
            .enumerate()
            .filter_map(|(i, (label, ivar))| {
                let value = self.heap.ivar_int(actor, ivar)?;
                Some((*label, (*ivar).to_owned(), i, value))
            })
            .collect()
    }

    pub fn set_actor_param_plus(&mut self, actor: Value, index: usize, value: i64) -> bool {
        if let Some(list) = self.heap.ivar(actor, "@param_plus") {
            let Some(items) = self.heap.array_mut(list) else { return false };
            if index >= items.len() {
                return false;
            }
            items[index] = Value::Int(value);
            self.dirty = true;
            return true;
        }
        let table: &[(&str, &str)] = match self.engine {
            Engine::Xp => &XP_PARAMS,
            _ => &VX_PARAMS,
        };
        let Some((_, ivar)) = table.get(index) else { return false };
        if self.heap.ivar(actor, ivar).is_none() {
            return false;
        }
        self.dirty = true;
        self.heap.set_ivar(actor, ivar, Value::Int(value))
    }

    /// Equipment, whichever way this engine stores it.
    pub fn actor_equips(&self, actor: Value) -> Vec<EquipRef> {
        if let Some(equips) = self.heap.ivar(actor, "@equips").and_then(|e| self.heap.array(e)) {
            return equips
                .iter()
                .enumerate()
                .map(|(slot, item)| {
                    let kind = self
                        .heap
                        .ivar(*item, "@class")
                        .and_then(|c| self.base_item_kind(c))
                        .unwrap_or(if slot == 0 { ItemKind::Weapon } else { ItemKind::Armor });
                    EquipRef {
                        slot,
                        kind,
                        item_id: self.heap.ivar_int(*item, "@item_id").unwrap_or(0),
                    }
                })
                .collect();
        }

        // VX: one weapon field and four armor fields.
        let dual_wield = self
            .heap
            .ivar(actor, "@two_swords_style")
            .is_some_and(Value::truthy);
        let mut out = Vec::new();
        if let Some(id) = self.heap.ivar_int(actor, "@weapon_id") {
            out.push(EquipRef { slot: 0, kind: ItemKind::Weapon, item_id: id });
        }
        for slot in 1..=4 {
            let ivar = format!("@armor{slot}_id");
            let Some(id) = self.heap.ivar_int(actor, &ivar) else { continue };
            let kind = if slot == 1 && dual_wield { ItemKind::Weapon } else { ItemKind::Armor };
            out.push(EquipRef { slot, kind, item_id: id });
        }
        out
    }

    fn base_item_kind(&self, class_value: Value) -> Option<ItemKind> {
        let NodeKind::Class(name) = self.heap.kind(class_value.as_ref()?) else {
            return None;
        };
        match name.as_str() {
            "RPG::Weapon" => Some(ItemKind::Weapon),
            "RPG::Armor" => Some(ItemKind::Armor),
            "RPG::Item" => Some(ItemKind::Item),
            _ => None,
        }
    }

    /// Puts an item in an equipment slot. An id of 0 empties the slot.
    pub fn set_actor_equip(&mut self, actor: Value, slot: usize, kind: ItemKind, id: i64) -> bool {
        if let Some(equips) = self.heap.ivar(actor, "@equips") {
            let Some(item) = self.heap.array(equips).and_then(|e| e.get(slot).copied()) else {
                return false;
            };
            self.heap.set_ivar(item, "@item_id", Value::Int(id.max(0)));
            let class_value = if id <= 0 {
                Value::Nil
            } else {
                Value::Ref(self.heap.alloc_kind(NodeKind::Class(kind.ruby_class().to_owned())))
            };
            self.heap.set_ivar(item, "@class", class_value);
            self.dirty = true;
            return true;
        }

        let ivar = if slot == 0 {
            "@weapon_id".to_owned()
        } else {
            format!("@armor{slot}_id")
        };
        if self.heap.ivar(actor, &ivar).is_none() {
            return false;
        }
        self.dirty = true;
        self.heap.set_ivar(actor, &ivar, Value::Int(id.max(0)))
    }

    pub fn actor_id_list(&self, actor: Value, ivar: &str) -> Vec<i64> {
        self.heap
            .ivar(actor, ivar)
            .and_then(|v| self.heap.array(v))
            .map(|items| items.iter().filter_map(|v| v.as_int()).collect())
            .unwrap_or_default()
    }

    pub fn set_actor_id_list(&mut self, actor: Value, ivar: &str, ids: &[i64]) -> bool {
        let Some(list) = self.heap.ivar(actor, ivar) else { return false };
        let Some(items) = self.heap.array_mut(list) else { return false };
        *items = ids.iter().map(|id| Value::Int(*id)).collect();
        self.dirty = true;
        true
    }

    /// Replaces the actor's states, dropping any stale turn counters with them.
    pub fn set_actor_states(&mut self, actor: Value, ids: &[i64]) -> bool {
        if !self.set_actor_id_list(actor, "@states", ids) {
            return false;
        }
        for ivar in ["@state_turns", "@state_steps"] {
            let Some(hash) = self.heap.ivar(actor, ivar) else { continue };
            let Some(entries) = self.heap.hash_entries(hash) else { continue };
            let stale: Vec<Value> = entries
                .iter()
                .map(|(k, _)| *k)
                .filter(|k| !k.as_int().is_some_and(|id| ids.contains(&id)))
                .collect();
            for key in stale {
                self.heap.hash_remove(hash, key);
            }
        }
        true
    }

    /// Restores HP and MP and clears every state.
    pub fn full_heal_actor(&mut self, actor: Value, data: Option<&GameData>) -> bool {
        let plus = self.actor_param_plus(actor);
        let bonus = |index: usize| plus.get(index).map(|p| p.3).unwrap_or(0);
        let max_hp = self.actor_param_base(actor, 0, data).map(|b| b + bonus(0));
        let max_mp = self.actor_param_base(actor, 1, data).map(|b| b + bonus(1));

        // Without the database the engine clamps an over-large value on its
        // next refresh, so a generous number is safe and still heals fully.
        let mp_ivar = self.engine.mp_ivar();
        self.heap.set_ivar(actor, "@hp", Value::Int(max_hp.unwrap_or(9_999)));
        self.heap.set_ivar(actor, mp_ivar, Value::Int(max_mp.unwrap_or(9_999)));
        self.set_actor_states(actor, &[]);
        self.dirty = true;
        true
    }
}
