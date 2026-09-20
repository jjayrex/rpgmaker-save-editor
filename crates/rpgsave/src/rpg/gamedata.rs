//! Names pulled from the game's own `Data/` directory.
//!
//! A save file stores nothing but ids: item 7, switch 42, class 3. Reading the
//! project's database turns those into "Potion", "Met the innkeeper" and
//! "Paladin". Every file here is optional — games that ship their data inside
//! an encrypted archive simply get id-only labels.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::marshal::{self, Heap, Value};

use super::engine::Engine;
use super::rgss::Table;

/// One row of an RPG Maker database table.
#[derive(Clone, Debug, Default, Serialize)]
pub struct Entry {
    pub id: i64,
    pub name: String,
    pub icon_index: i64,
    pub description: String,
    pub price: i64,
    /// Equipment slot type for armors, weapon type for weapons.
    pub etype_id: i64,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct ActorInfo {
    pub id: i64,
    pub name: String,
    pub nickname: String,
    pub class_id: i64,
    pub initial_level: i64,
    pub max_level: i64,
}

#[derive(Clone, Debug, Default)]
pub struct ClassInfo {
    pub id: i64,
    pub name: String,
    /// VX Ace `[basis, extra, acceleration A, acceleration B]`.
    pub exp_params: Option<[f64; 4]>,
    /// 8 × 100 grid of base parameters per level.
    pub params: Option<Table>,
}

impl ClassInfo {
    /// Total experience required to reach `level`, using VX Ace's curve.
    pub fn exp_for_level(&self, level: i64) -> Option<i64> {
        let [basis, extra, acc_a, acc_b] = self.exp_params?;
        if level <= 1 {
            return Some(0);
        }
        let lv = level as f64;
        let value = basis * (lv - 1.0).powf(0.9 + acc_a / 250.0) * lv * (lv + 1.0)
            / (6.0 + lv.powi(2) / 50.0 / acc_b)
            + (lv - 1.0) * extra;
        Some(value.round() as i64)
    }

    /// Base value of a parameter at a level (0 = MaxHP, 1 = MaxMP, …).
    pub fn param(&self, param_id: u32, level: i64) -> Option<i64> {
        let table = self.params.as_ref()?;
        table.get(param_id, level.max(0) as u32, 0).map(i64::from)
    }
}

/// Everything the editor could read out of a project's database.
#[derive(Clone, Debug, Default)]
pub struct GameData {
    pub dir: PathBuf,
    pub game_title: Option<String>,
    pub currency: Option<String>,
    /// Indexed by switch / variable id; index 0 is unused, as in the engine.
    pub switch_names: Vec<String>,
    pub variable_names: Vec<String>,
    /// Slot names from the database terms ("Weapon", "Shield", …).
    pub equip_types: Vec<String>,
    pub items: Vec<Entry>,
    pub weapons: Vec<Entry>,
    pub armors: Vec<Entry>,
    pub skills: Vec<Entry>,
    pub states: Vec<Entry>,
    pub actors: Vec<ActorInfo>,
    pub classes: Vec<ClassInfo>,
    pub maps: BTreeMap<i64, String>,
    /// Database files that were found, and ones that were looked for in vain.
    pub loaded: Vec<String>,
    pub missing: Vec<String>,
}

impl GameData {
    /// Reads whatever database files exist in `dir`.
    pub fn load(dir: &Path, engine: Engine) -> GameData {
        let ext = engine.data_extension();
        let mut data = GameData { dir: dir.to_path_buf(), ..Default::default() };

        let read = |stem: &str| -> Option<(Heap, Value)> {
            let path = dir.join(format!("{stem}.{ext}"));
            let bytes = std::fs::read(&path).ok()?;
            let mut heap = Heap::new();
            match marshal::load(&bytes, &mut heap) {
                Ok(value) => Some((heap, value)),
                Err(_) => None,
            }
        };

        for (stem, target) in [
            ("Items", 0usize),
            ("Weapons", 1),
            ("Armors", 2),
            ("Skills", 3),
            ("States", 4),
        ] {
            match read(stem) {
                Some((heap, value)) => {
                    let entries = read_entries(&heap, value);
                    match target {
                        0 => data.items = entries,
                        1 => data.weapons = entries,
                        2 => data.armors = entries,
                        3 => data.skills = entries,
                        _ => data.states = entries,
                    }
                    data.loaded.push(stem.to_owned());
                }
                None => data.missing.push(stem.to_owned()),
            }
        }

        match read("Actors") {
            Some((heap, value)) => {
                data.actors = read_actors(&heap, value);
                data.loaded.push("Actors".to_owned());
            }
            None => data.missing.push("Actors".to_owned()),
        }

        match read("Classes") {
            Some((heap, value)) => {
                data.classes = read_classes(&heap, value);
                data.loaded.push("Classes".to_owned());
            }
            None => data.missing.push("Classes".to_owned()),
        }

        match read("System") {
            Some((heap, value)) => {
                data.switch_names = read_name_list(&heap, heap.ivar(value, "@switches"));
                data.variable_names = read_name_list(&heap, heap.ivar(value, "@variables"));
                data.game_title = heap.ivar(value, "@game_title").and_then(|v| heap.string(v));
                data.currency = heap.ivar(value, "@currency_unit").and_then(|v| heap.string(v));
                if let Some(terms) = heap.ivar(value, "@terms") {
                    data.equip_types = read_name_list(&heap, heap.ivar(terms, "@etypes"));
                }
                data.loaded.push("System".to_owned());
            }
            None => data.missing.push("System".to_owned()),
        }

        match read("MapInfos") {
            Some((heap, value)) => {
                if let Some(entries) = heap.hash_entries(value) {
                    for (key, info) in entries {
                        let Some(id) = key.as_int() else { continue };
                        let name = heap.ivar(*info, "@name").and_then(|v| heap.string(v));
                        data.maps.insert(id, name.unwrap_or_default());
                    }
                }
                data.loaded.push("MapInfos".to_owned());
            }
            None => data.missing.push("MapInfos".to_owned()),
        }

        if data.game_title.is_none() {
            // VX keeps the title in Game.ini rather than the database.
            data.game_title = dir.parent().and_then(read_ini_title);
        }
        data
    }

    pub fn is_empty(&self) -> bool {
        self.loaded.is_empty()
    }

    pub fn catalog(&self, kind: super::save::ItemKind) -> &[Entry] {
        match kind {
            super::save::ItemKind::Item => &self.items,
            super::save::ItemKind::Weapon => &self.weapons,
            super::save::ItemKind::Armor => &self.armors,
        }
    }

    pub fn entry_name(&self, kind: super::save::ItemKind, id: i64) -> Option<&str> {
        find(self.catalog(kind), id).map(|e| e.name.as_str())
    }

    pub fn skill_name(&self, id: i64) -> Option<&str> {
        find(&self.skills, id).map(|e| e.name.as_str())
    }

    pub fn state_name(&self, id: i64) -> Option<&str> {
        find(&self.states, id).map(|e| e.name.as_str())
    }

    pub fn class(&self, id: i64) -> Option<&ClassInfo> {
        self.classes.iter().find(|c| c.id == id)
    }

    pub fn class_name(&self, id: i64) -> Option<&str> {
        self.class(id).map(|c| c.name.as_str())
    }

    pub fn actor(&self, id: i64) -> Option<&ActorInfo> {
        self.actors.iter().find(|a| a.id == id)
    }

    pub fn map_name(&self, id: i64) -> Option<&str> {
        self.maps.get(&id).map(String::as_str)
    }

    pub fn switch_name(&self, id: i64) -> Option<&str> {
        self.switch_names
            .get(id as usize)
            .map(String::as_str)
            .filter(|s| !s.is_empty())
    }

    pub fn variable_name(&self, id: i64) -> Option<&str> {
        self.variable_names
            .get(id as usize)
            .map(String::as_str)
            .filter(|s| !s.is_empty())
    }

    pub fn equip_type_name(&self, slot: usize) -> Option<&str> {
        self.equip_types
            .get(slot)
            .map(String::as_str)
            .filter(|s| !s.is_empty())
    }
}

fn find(entries: &[Entry], id: i64) -> Option<&Entry> {
    entries.iter().find(|e| e.id == id)
}

/// Database tables are arrays indexed by id, with a `nil` in slot 0.
fn read_entries(heap: &Heap, value: Value) -> Vec<Entry> {
    let Some(items) = heap.array(value) else { return Vec::new() };
    items
        .iter()
        .enumerate()
        .filter(|(_, v)| !v.is_nil())
        .map(|(i, v)| Entry {
            id: heap.ivar_int(*v, "@id").unwrap_or(i as i64),
            name: heap.ivar(*v, "@name").and_then(|n| heap.string(n)).unwrap_or_default(),
            icon_index: heap
                .ivar_int(*v, "@icon_index")
                .or_else(|| heap.ivar_int(*v, "@icon_name"))
                .unwrap_or(0),
            description: heap
                .ivar(*v, "@description")
                .and_then(|n| heap.string(n))
                .unwrap_or_default(),
            price: heap.ivar_int(*v, "@price").unwrap_or(0),
            etype_id: heap
                .ivar_int(*v, "@etype_id")
                .or_else(|| heap.ivar_int(*v, "@kind"))
                .unwrap_or(0),
        })
        .collect()
}

fn read_actors(heap: &Heap, value: Value) -> Vec<ActorInfo> {
    let Some(items) = heap.array(value) else { return Vec::new() };
    items
        .iter()
        .enumerate()
        .filter(|(_, v)| !v.is_nil())
        .map(|(i, v)| ActorInfo {
            id: heap.ivar_int(*v, "@id").unwrap_or(i as i64),
            name: heap.ivar(*v, "@name").and_then(|n| heap.string(n)).unwrap_or_default(),
            nickname: heap
                .ivar(*v, "@nickname")
                .and_then(|n| heap.string(n))
                .unwrap_or_default(),
            class_id: heap.ivar_int(*v, "@class_id").unwrap_or(0),
            initial_level: heap.ivar_int(*v, "@initial_level").unwrap_or(1),
            max_level: heap
                .ivar_int(*v, "@max_level")
                .or_else(|| heap.ivar_int(*v, "@final_level"))
                .unwrap_or(99),
        })
        .collect()
}

fn read_classes(heap: &Heap, value: Value) -> Vec<ClassInfo> {
    let Some(items) = heap.array(value) else { return Vec::new() };
    items
        .iter()
        .enumerate()
        .filter(|(_, v)| !v.is_nil())
        .map(|(i, v)| {
            let exp_params = heap.ivar(*v, "@exp_params").and_then(|p| {
                let values = heap.array(p)?;
                if values.len() < 4 {
                    return None;
                }
                Some([
                    heap.number(values[0])?,
                    heap.number(values[1])?,
                    heap.number(values[2])?,
                    heap.number(values[3])?,
                ])
            });
            let params = heap
                .ivar(*v, "@params")
                .or_else(|| heap.ivar(*v, "@parameters"))
                .and_then(|p| {
                let id = p.as_ref()?;
                match heap.kind(id) {
                    crate::marshal::NodeKind::UserDef { data, .. } => Table::parse(data),
                    _ => None,
                }
            });
            ClassInfo {
                id: heap.ivar_int(*v, "@id").unwrap_or(i as i64),
                name: heap.ivar(*v, "@name").and_then(|n| heap.string(n)).unwrap_or_default(),
                exp_params,
                params,
            }
        })
        .collect()
}

fn read_name_list(heap: &Heap, value: Option<Value>) -> Vec<String> {
    let Some(value) = value else { return Vec::new() };
    let Some(items) = heap.array(value) else { return Vec::new() };
    items
        .iter()
        .map(|v| heap.string(*v).unwrap_or_default())
        .collect()
}

/// VX stores the window title in `Game.ini` instead of the database.
fn read_ini_title(game_dir: &Path) -> Option<String> {
    let text = std::fs::read(game_dir.join("Game.ini")).ok()?;
    let text = marshal::decode_ruby_string(&text);
    text.lines()
        .find_map(|line| line.trim().strip_prefix("Title="))
        .map(|t| t.trim().to_owned())
        .filter(|t| !t.is_empty())
}
