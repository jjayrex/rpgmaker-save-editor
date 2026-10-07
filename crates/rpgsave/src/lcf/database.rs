//! Reading `RPG_RT.ldb`, the RPG Maker 2000/2003 database.
//!
//! The save file stores ids and almost no text, so without this the interface
//! can only list numbers. The database is the same LCF chunk format as a save;
//! only the field numbers differ, and those follow liblcf.

use std::path::Path;

use crate::marshal::decode_ruby_string;
use crate::rpg::gamedata::{ActorInfo, ClassInfo, Entry, GameData};

use super::chunk::{Array, Chunks};

/// The signature of a database file.
pub const SIGNATURE: &str = "LcfDataBase";

/// The file a game keeps its database in.
pub const FILE_NAME: &str = "RPG_RT.ldb";

/// The signature of a map tree.
pub const MAP_TREE_SIGNATURE: &str = "LcfMapTree";

/// The file holding the list of maps and their names.
pub const MAP_TREE_FILE_NAME: &str = "RPG_RT.lmt";

/// Sections of the database.
mod section {
    pub const ACTORS: u32 = 0x0B;
    pub const SKILLS: u32 = 0x0C;
    pub const ITEMS: u32 = 0x0D;
    pub const STATES: u32 = 0x12;
    pub const TERMS: u32 = 0x15;
    pub const SWITCHES: u32 = 0x17;
    pub const VARIABLES: u32 = 0x18;
    pub const CLASSES: u32 = 0x1E;
}

/// Fields shared by most database entries.
mod entry {
    pub const NAME: u32 = 0x01;
    pub const DESCRIPTION: u32 = 0x02;
}

mod item {
    pub const TYPE: u32 = 0x03;
    pub const PRICE: u32 = 0x05;
}

mod db_actor {
    pub const TITLE: u32 = 0x02;
    pub const INITIAL_LEVEL: u32 = 0x07;
    pub const FINAL_LEVEL: u32 = 0x08;
    pub const CLASS_ID: u32 = 0x39;
}

mod terms {
    pub const GOLD: u32 = 0x5F;
    pub const SLOTS: [u32; 5] = [0x88, 0x89, 0x8A, 0x8B, 0x8C];
}

/// Finds one of a game's files, whatever case its name is in.
///
/// Games made on Windows are routinely unpacked onto case-sensitive file
/// systems, where `RPG_RT.LDB` and `rpg_rt.ldb` both turn up.
pub fn find_file(dir: &Path, name: &str) -> Option<std::path::PathBuf> {
    let wanted = name.to_ascii_lowercase();
    std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .find(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.to_ascii_lowercase() == wanted)
        })
}

pub fn find_database(dir: &Path) -> Option<std::path::PathBuf> {
    find_file(dir, FILE_NAME)
}

/// Reads the database in a directory, if there is one this editor understands.
pub fn load(dir: &Path) -> Option<GameData> {
    let path = find_database(dir)?;
    let bytes = std::fs::read(&path).ok()?;
    let mut data = parse(&bytes)?;
    data.dir = dir.to_path_buf();
    // The game's name lives in the launcher's ini file rather than the database.
    data.game_title = read_ini_title(dir);

    // Map names are in a file of their own.
    match load_map_names(dir) {
        Some(maps) => {
            data.maps = maps;
            data.loaded.push(MAP_TREE_FILE_NAME.to_owned());
        }
        None => data.missing.push(MAP_TREE_FILE_NAME.to_owned()),
    }
    Some(data)
}

pub fn parse(bytes: &[u8]) -> Option<GameData> {
    let length = *bytes.first()? as usize;
    if bytes.get(1..1 + length)? != SIGNATURE.as_bytes() {
        return None;
    }
    let sections = Chunks::parse_to_end(&bytes[1 + length..]).ok()?;

    let mut data = GameData {
        items: entries(&sections, section::ITEMS, true),
        skills: entries(&sections, section::SKILLS, false),
        states: entries(&sections, section::STATES, false),
        actors: actors(&sections),
        classes: classes(&sections),
        switch_names: names_by_id(&sections, section::SWITCHES),
        variable_names: names_by_id(&sections, section::VARIABLES),
        ..GameData::default()
    };

    if let Some(terms) = structure(&sections, section::TERMS) {
        data.currency = text(&terms, terms::GOLD);
        data.equip_types = terms::SLOTS
            .iter()
            .map(|tag| text(&terms, *tag).unwrap_or_default())
            .collect();
    }

    data.loaded.push(FILE_NAME.to_owned());
    Some(data)
}

/// Reads the map names out of `RPG_RT.lmt`.
///
/// Unlike the database, the map tree's top level is not a chunk stream: the
/// list of maps is written straight after the signature, so it is read as a
/// bare array.
pub fn load_map_names(dir: &Path) -> Option<std::collections::BTreeMap<i64, String>> {
    let bytes = std::fs::read(find_file(dir, MAP_TREE_FILE_NAME)?).ok()?;
    parse_map_names(&bytes)
}

pub fn parse_map_names(bytes: &[u8]) -> Option<std::collections::BTreeMap<i64, String>> {
    let length = *bytes.first()? as usize;
    if bytes.get(1..1 + length)? != MAP_TREE_SIGNATURE.as_bytes() {
        return None;
    }
    let maps = Array::parse(&bytes[1 + length..]).ok()?;
    Some(
        maps.entries
            .iter()
            // Entry zero is the tree's root, which carries the project's name
            // rather than a map's.
            .filter(|(id, _)| *id != 0)
            .filter_map(|(id, chunks)| Some((i64::from(*id), text(chunks, entry::NAME)?)))
            .collect(),
    )
}

fn structure(sections: &Chunks, tag: u32) -> Option<Chunks> {
    let data = sections.get(tag)?;
    let mut pos = 0;
    Chunks::parse_stream(data, &mut pos).ok()
}

fn array(sections: &Chunks, tag: u32) -> Option<Array> {
    Array::parse(sections.get(tag)?).ok()
}

fn text(chunks: &Chunks, tag: u32) -> Option<String> {
    let bytes = chunks.string(tag)?;
    let text = decode_ruby_string(bytes);
    (!text.is_empty()).then_some(text)
}

fn entries(sections: &Chunks, tag: u32, priced: bool) -> Vec<Entry> {
    let Some(array) = array(sections, tag) else { return Vec::new() };
    array
        .entries
        .iter()
        .map(|(id, chunks)| Entry {
            id: i64::from(*id),
            name: text(chunks, entry::NAME).unwrap_or_default(),
            icon_index: 0,
            description: text(chunks, entry::DESCRIPTION).unwrap_or_default(),
            price: if priced {
                chunks.int(item::PRICE).map(i64::from).unwrap_or(0)
            } else {
                0
            },
            // For items this is what kind of thing it is, which is also what
            // decides the equipment slot it fits.
            etype_id: if priced {
                chunks.int(item::TYPE).map(i64::from).unwrap_or(0)
            } else {
                0
            },
        })
        .collect()
}

fn actors(sections: &Chunks) -> Vec<ActorInfo> {
    let Some(array) = array(sections, section::ACTORS) else { return Vec::new() };
    array
        .entries
        .iter()
        .map(|(id, chunks)| ActorInfo {
            id: i64::from(*id),
            name: text(chunks, entry::NAME).unwrap_or_default(),
            nickname: text(chunks, db_actor::TITLE).unwrap_or_default(),
            class_id: chunks.int(db_actor::CLASS_ID).map(i64::from).unwrap_or(0),
            initial_level: chunks.int(db_actor::INITIAL_LEVEL).map(i64::from).unwrap_or(1),
            max_level: chunks.int(db_actor::FINAL_LEVEL).map(i64::from).unwrap_or(99),
            params: None,
        })
        .collect()
}

fn classes(sections: &Chunks) -> Vec<ClassInfo> {
    let Some(array) = array(sections, section::CLASSES) else { return Vec::new() };
    array
        .entries
        .iter()
        .map(|(id, chunks)| ClassInfo {
            id: i64::from(*id),
            name: text(chunks, entry::NAME).unwrap_or_default(),
            exp_params: None,
            params: None,
        })
        .collect()
}

/// Switches and variables are named lists indexed by id.
fn names_by_id(sections: &Chunks, tag: u32) -> Vec<String> {
    let Some(array) = array(sections, tag) else { return Vec::new() };
    let highest = array.entries.iter().map(|(id, _)| *id).max().unwrap_or(0) as usize;
    let mut names = vec![String::new(); highest + 1];
    for (id, chunks) in &array.entries {
        if let Some(name) = text(chunks, entry::NAME) {
            names[*id as usize] = name;
        }
    }
    names
}

/// `RPG_RT.ini` holds the window title.
fn read_ini_title(dir: &Path) -> Option<String> {
    let wanted = "rpg_rt.ini";
    let path = std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .find(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.to_ascii_lowercase() == wanted)
        })?;
    let text = decode_ruby_string(&std::fs::read(path).ok()?);
    text.lines()
        .find_map(|line| line.trim().strip_prefix("GameTitle="))
        .map(|t| t.trim().to_owned())
        .filter(|t| !t.is_empty())
}
