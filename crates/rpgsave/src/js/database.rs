//! Names pulled from an MV project's `data` folder.
//!
//! MV ships its database as the same JSON it saves games in, one file per
//! table, so this reads them with the parser in [`super::json`] and fills the
//! same [`GameData`] the RGSS and LCF readers do. Everything is optional: a
//! game whose folder is not beside the save, or is packed into an archive,
//! simply shows ids.

use std::path::{Path, PathBuf};

use crate::rpg::gamedata::{find_file, ActorInfo, ClassInfo, Entry, GameData};

use super::json::{self, Json};

/// The folder MV keeps its database in.
pub const DATA_DIR: &str = "data";

/// The file every MV project has, used to recognise the folder.
pub const SYSTEM_FILE: &str = "System.json";

/// The tables the editor reads, by file name.
mod file {
    pub const ACTORS: &str = "Actors.json";
    pub const CLASSES: &str = "Classes.json";
    pub const ITEMS: &str = "Items.json";
    pub const WEAPONS: &str = "Weapons.json";
    pub const ARMORS: &str = "Armors.json";
    pub const SKILLS: &str = "Skills.json";
    pub const STATES: &str = "States.json";
    pub const SYSTEM: &str = "System.json";
    pub const MAP_INFOS: &str = "MapInfos.json";
}

/// Finds the `data` folder that belongs to a save.
///
/// A deployed game puts the two side by side under `www`, and the editor is
/// as likely to be pointed at `www/save/file1.rpgsave` as at a save somebody
/// has copied out, so this looks beside the save and then up from it.
pub fn find_data_dir(save_dir: &Path) -> Option<PathBuf> {
    let mut candidate = Some(save_dir.to_path_buf());
    // The save folder itself, the game folder above it, and `www` above that.
    for _ in 0..3 {
        let dir = candidate?;
        if let Some(data) = find_file(&dir, DATA_DIR)
            && find_file(&data, SYSTEM_FILE).is_some()
        {
            return Some(data);
        }
        candidate = dir.parent().map(Path::to_path_buf);
    }
    None
}

/// Reads whatever tables are in `dir`. `None` when it holds none of them.
pub fn load(dir: &Path) -> Option<GameData> {
    let mut data = GameData { dir: dir.to_path_buf(), ..Default::default() };

    let read = |name: &str| -> Option<Json> {
        let text = std::fs::read_to_string(find_file(dir, name)?).ok()?;
        json::parse(&text).ok()
    };

    for (name, target) in [
        (file::ITEMS, 0usize),
        (file::WEAPONS, 1),
        (file::ARMORS, 2),
        (file::SKILLS, 3),
        (file::STATES, 4),
    ] {
        match read(name) {
            Some(table) => {
                let entries = read_entries(&table);
                match target {
                    0 => data.items = entries,
                    1 => data.weapons = entries,
                    2 => data.armors = entries,
                    3 => data.skills = entries,
                    _ => data.states = entries,
                }
                data.loaded.push(name.to_owned());
            }
            None => data.missing.push(name.to_owned()),
        }
    }

    match read(file::ACTORS) {
        Some(table) => {
            data.actors = read_actors(&table);
            data.loaded.push(file::ACTORS.to_owned());
        }
        None => data.missing.push(file::ACTORS.to_owned()),
    }

    match read(file::CLASSES) {
        Some(table) => {
            data.classes = read_classes(&table);
            data.loaded.push(file::CLASSES.to_owned());
        }
        None => data.missing.push(file::CLASSES.to_owned()),
    }

    match read(file::SYSTEM) {
        Some(system) => {
            data.game_title = system.str_at("gameTitle").map(str::to_owned).filter(|t| !t.is_empty());
            data.currency = system.str_at("currencyUnit").map(str::to_owned);
            data.switch_names = read_names(system.get("switches"));
            data.variable_names = read_names(system.get("variables"));
            // Slot 0 of `equipTypes` is the engine's placeholder for "none",
            // and the slots the interface shows start at the weapon.
            data.equip_types = read_names(system.get("equipTypes"))
                .into_iter()
                .skip(1)
                .collect();
            data.loaded.push(file::SYSTEM.to_owned());
        }
        None => data.missing.push(file::SYSTEM.to_owned()),
    }

    match read(file::MAP_INFOS) {
        Some(table) => {
            for info in table.elements().unwrap_or_default() {
                let Some(id) = info.int_at("id") else { continue };
                data.maps.insert(id, info.str_at("name").unwrap_or_default().to_owned());
            }
            data.loaded.push(file::MAP_INFOS.to_owned());
        }
        None => data.missing.push(file::MAP_INFOS.to_owned()),
    }

    (!data.loaded.is_empty()).then_some(data)
}

/// Database tables are arrays indexed by id with a `null` in slot 0.
fn rows(table: &Json) -> impl Iterator<Item = (usize, &Json)> {
    table
        .elements()
        .unwrap_or_default()
        .iter()
        .enumerate()
        .filter(|(_, row)| !row.is_null())
}

fn read_entries(table: &Json) -> Vec<Entry> {
    rows(table)
        .map(|(index, row)| Entry {
            id: row.int_at("id").unwrap_or(index as i64),
            name: row.str_at("name").unwrap_or_default().to_owned(),
            icon_index: row.int_at("iconIndex").unwrap_or(0),
            description: row.str_at("description").unwrap_or_default().to_owned(),
            price: row.int_at("price").unwrap_or(0),
            etype_id: row.int_at("etypeId").unwrap_or(0),
        })
        .collect()
}

fn read_actors(table: &Json) -> Vec<ActorInfo> {
    rows(table)
        .map(|(index, row)| ActorInfo {
            id: row.int_at("id").unwrap_or(index as i64),
            name: row.str_at("name").unwrap_or_default().to_owned(),
            nickname: row.str_at("nickname").unwrap_or_default().to_owned(),
            class_id: row.int_at("classId").unwrap_or(0),
            initial_level: row.int_at("initialLevel").unwrap_or(1),
            max_level: row.int_at("maxLevel").unwrap_or(99),
            // MV keeps its stat curves on the class rather than the actor.
            params: None,
        })
        .collect()
}

fn read_classes(table: &Json) -> Vec<ClassInfo> {
    rows(table)
        .map(|(index, row)| {
            let exp_params = row.get("expParams").and_then(|p| {
                let values = p.elements()?;
                Some([
                    values.first()?.as_f64()?,
                    values.get(1)?.as_f64()?,
                    values.get(2)?.as_f64()?,
                    values.get(3)?.as_f64()?,
                ])
            });
            let param_curves = row.get("params").and_then(|p| {
                Some(
                    p.elements()?
                        .iter()
                        .map(|curve| {
                            curve
                                .elements()
                                .unwrap_or_default()
                                .iter()
                                .filter_map(Json::as_i64)
                                .collect()
                        })
                        .collect(),
                )
            });
            ClassInfo {
                id: row.int_at("id").unwrap_or(index as i64),
                name: row.str_at("name").unwrap_or_default().to_owned(),
                exp_params,
                params: None,
                param_curves,
            }
        })
        .collect()
}

/// Switch and variable names are arrays indexed by id, so they keep their
/// unnamed slots and the unused slot 0.
fn read_names(list: Option<&Json>) -> Vec<String> {
    list.and_then(Json::elements)
        .map(|names| {
            names
                .iter()
                .map(|name| name.as_str().unwrap_or_default().to_owned())
                .collect()
        })
        .unwrap_or_default()
}
