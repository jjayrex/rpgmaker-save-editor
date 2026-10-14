//! RPG Maker MV save files (`fileNN.rpgsave`).
//!
//! Where the RGSS engines dump a Ruby object graph, MV writes the same game
//! objects as JSON through its own `JsonEx`, then compresses the text. The
//! shape is otherwise familiar: one member per global (`$gameParty` is
//! `party`, `$gameActors` is `actors`), each holding the engine's own private
//! fields under their JavaScript names.
//!
//! `JsonEx` adds three members of its own, which this editor passes through
//! untouched: `@` names the class an object is rebuilt as, `@c` gives it an
//! identity, and `@r` refers back to one. Arrays are wrapped in an object so
//! that they can carry an identity too, which is why reading an array here
//! goes through [`Json::elements`].

use std::path::{Path, PathBuf};

use std::cell::RefCell;

use super::json::{self, Json, ParseError, Step};
use super::lzstring;

/// Members at the top level, as `DataManager.makeSaveContents` writes them.
pub mod top {
    pub const SYSTEM: &str = "system";
    pub const SWITCHES: &str = "switches";
    pub const VARIABLES: &str = "variables";
    pub const SELF_SWITCHES: &str = "selfSwitches";
    pub const ACTORS: &str = "actors";
    pub const PARTY: &str = "party";
    pub const MAP: &str = "map";
    pub const PLAYER: &str = "player";
}

/// `Game_System`.
pub mod system {
    pub const SAVE_COUNT: &str = "_saveCount";
    pub const FRAMES_ON_SAVE: &str = "_framesOnSave";
    pub const BATTLE_COUNT: &str = "_battleCount";
    pub const WIN_COUNT: &str = "_winCount";
    pub const ESCAPE_COUNT: &str = "_escapeCount";
}

/// `Game_Party`.
pub mod party {
    pub const GOLD: &str = "_gold";
    pub const STEPS: &str = "_steps";
    pub const ACTORS: &str = "_actors";
    pub const ITEMS: &str = "_items";
    pub const WEAPONS: &str = "_weapons";
    pub const ARMORS: &str = "_armors";
}

/// `Game_Actor`, and the `Game_Battler` fields it inherits.
pub mod actor {
    pub const ACTOR_ID: &str = "_actorId";
    pub const NAME: &str = "_name";
    pub const NICKNAME: &str = "_nickname";
    pub const CLASS_ID: &str = "_classId";
    pub const LEVEL: &str = "_level";
    pub const EXP: &str = "_exp";
    pub const HP: &str = "_hp";
    pub const MP: &str = "_mp";
    pub const TP: &str = "_tp";
    pub const PARAM_PLUS: &str = "_paramPlus";
    pub const SKILLS: &str = "_skills";
    pub const STATES: &str = "_states";
    pub const STATE_TURNS: &str = "_stateTurns";
    pub const STATE_STEPS: &str = "_stateSteps";
    pub const EQUIPS: &str = "_equips";
    pub const CHARACTER_NAME: &str = "_characterName";
    pub const FACE_NAME: &str = "_faceName";
    pub const PROFILE: &str = "_profile";
}

/// `Game_Item`, which is how an equipped thing is stored.
pub mod item {
    pub const DATA_CLASS: &str = "_dataClass";
    pub const ITEM_ID: &str = "_itemId";
}

/// The private field every collection keeps its contents in.
const DATA: &str = "_data";

/// MV counts play time in frames at its fixed sixty a second.
pub const FRAME_RATE: i64 = 60;

/// `Game_Party.maxGold`.
pub const MAX_GOLD: i64 = 99_999_999;

/// `Game_Party.maxItems`.
pub const MAX_ITEM_COUNT: i64 = 99;

/// The eight parameters, in the order the engine indexes them.
pub const PARAMS: [&str; 8] =
    ["Max HP", "Max MP", "Attack", "Defense", "M.Attack", "M.Defense", "Agility", "Luck"];

/// The engine's default equipment slots, used when the database is not at hand
/// to name the game's own.
pub const EQUIP_SLOTS: [&str; 5] = ["Weapon", "Shield", "Head", "Body", "Accessory"];

#[derive(Debug)]
pub enum OpenError {
    Io(std::io::Error),
    NotBase64,
    NotCompressed,
    LooksLikeMz,
    Json(ParseError),
    NotASave { found: Vec<String> },
}

impl std::fmt::Display for OpenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OpenError::Io(e) => write!(f, "{e}"),
            OpenError::NotBase64 => {
                write!(f, "This is not an RPG Maker MV save: the file is not base64 text.")
            }
            OpenError::NotCompressed => write!(
                f,
                "This file is base64 but not an RPG Maker MV save: it does not unpack as \
                 lz-string, which is what MV compresses its saves with."
            ),
            OpenError::LooksLikeMz => write!(
                f,
                "This looks like an RPG Maker MZ save: it is base64 of a zlib stream, which is \
                 how MZ stores one, rather than the lz-string MV uses. MZ is not supported yet."
            ),
            OpenError::Json(e) => write!(f, "The save unpacked but its contents are not JSON: {e}"),
            OpenError::NotASave { found } => write!(
                f,
                "This is an RPG Maker MV file but not a save slot: it holds {} rather than a \
                 party and its actors. `global.rpgsave` and `config.rpgsave` are the game's own \
                 bookkeeping, not savegames.",
                if found.is_empty() { "nothing".to_owned() } else { found.join(", ") }
            ),
        }
    }
}

impl std::error::Error for OpenError {}

/// A decoded `.rpgsave`.
pub struct MvSave {
    pub path: PathBuf,
    pub size_bytes: usize,
    pub dirty: bool,
    pub notes: Vec<String>,
    /// The save's contents, which is what every method here reads and writes.
    pub root: Json,
    /// The file as it was read, and the text that came out of it.
    ///
    /// Compressing is not quite canonical — the tools people edit these saves
    /// with pad the last byte differently from the game — so a save that comes
    /// out of the editor unchanged is written back as the bytes it arrived as,
    /// rather than as a re-compression that would differ for no reason.
    original_text: String,
    original_bytes: Vec<u8>,
    /// Paths the raw browser has been given numbers for.
    ///
    /// That browser addresses values by number, and a number has to still
    /// mean the same value on the next request. Numbering by position in the
    /// tree would shift the moment an edit added an element, so each number
    /// is an index into this list of the paths handed out so far.
    browse: RefCell<Vec<Vec<Step>>>,
}

impl MvSave {
    pub fn open(path: &Path) -> Result<MvSave, OpenError> {
        let bytes = std::fs::read(path).map_err(OpenError::Io)?;
        let mut save = MvSave::from_bytes(&bytes)?;
        save.path = path.to_path_buf();
        Ok(save)
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<MvSave, OpenError> {
        let text = std::str::from_utf8(bytes).map_err(|_| OpenError::NotBase64)?;
        if !lzstring::looks_like_base64(text) {
            return Err(OpenError::NotBase64);
        }
        // MV's successor writes the same kind of file with a different
        // compression, and is worth telling apart by name. This is checked
        // before unpacking because lz-string will happily turn any base64
        // into some string or other; only the JSON inside says it was a save.
        if looks_like_zlib(text) {
            return Err(OpenError::LooksLikeMz);
        }
        let payload = lzstring::decompress_from_base64(text).ok_or(OpenError::NotCompressed)?;
        let root = json::parse(&payload).map_err(OpenError::Json)?;

        // `global.rpgsave` and `config.rpgsave` are the same format without
        // any game state in them, and are worth telling apart by name.
        if root.get(top::PARTY).is_none() || root.get(top::ACTORS).is_none() {
            let found = root
                .members()
                .map(|m| {
                    m.iter()
                        .map(|(key, _)| key.as_str().to_owned())
                        .filter(|key| !key.starts_with('@'))
                        .collect()
                })
                .unwrap_or_default();
            return Err(OpenError::NotASave { found });
        }

        Ok(MvSave {
            path: PathBuf::new(),
            size_bytes: bytes.len(),
            dirty: false,
            notes: Vec::new(),
            root,
            original_text: payload,
            original_bytes: bytes.to_vec(),
            browse: RefCell::new(Vec::new()),
        })
    }

    /// True for a file that is nothing but base64, which is all MV writes.
    ///
    /// A Ruby `Marshal` stream starts with two bytes that are not base64, and
    /// an LCF save with a length byte, so this is enough to tell the formats
    /// apart before committing to unpacking one.
    pub fn looks_like_save(bytes: &[u8]) -> bool {
        std::str::from_utf8(bytes).is_ok_and(lzstring::looks_like_base64)
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let text = self.root.to_text();
        if text == self.original_text {
            return self.original_bytes.clone();
        }
        lzstring::compress_to_base64(&text).into_bytes()
    }

    /// The JSON the save holds, for callers that want to look at it whole.
    pub fn to_json_text(&self) -> String {
        self.root.to_text()
    }

    // ---- browsing ---------------------------------------------------------

    /// The number that stands for a path, the same one every time.
    pub fn browse_id(&self, path: &[Step]) -> u32 {
        let mut paths = self.browse.borrow_mut();
        match paths.iter().position(|known| known == path) {
            Some(at) => at as u32,
            None => {
                paths.push(path.to_vec());
                paths.len() as u32 - 1
            }
        }
    }

    pub fn browse_path(&self, id: u32) -> Option<Vec<Step>> {
        self.browse.borrow().get(id as usize).cloned()
    }

    // ---- the globals ------------------------------------------------------

    fn global(&self, name: &str) -> Option<&Json> {
        self.root.get(name)
    }

    fn global_mut(&mut self, name: &str) -> Option<&mut Json> {
        self.root.get_mut(name)
    }

    /// The contents of one of the engine's collections (`switches._data`).
    fn data_of(&self, name: &str) -> Option<&Json> {
        self.global(name)?.get(DATA)
    }

    fn data_of_mut(&mut self, name: &str) -> Option<&mut Json> {
        self.global_mut(name)?.get_mut(DATA)
    }

    fn set_in(&mut self, global: &str, key: &str, value: Json) -> bool {
        self.dirty = true;
        self.global_mut(global).is_some_and(|g| g.set(key, value))
    }

    // ---- the overview -----------------------------------------------------

    pub fn gold(&self) -> i64 {
        self.global(top::PARTY).and_then(|p| p.int_at(party::GOLD)).unwrap_or(0)
    }

    pub fn set_gold(&mut self, value: i64) -> bool {
        self.set_in(top::PARTY, party::GOLD, Json::int(value.clamp(0, MAX_GOLD)))
    }

    pub fn steps(&self) -> i64 {
        self.global(top::PARTY).and_then(|p| p.int_at(party::STEPS)).unwrap_or(0)
    }

    pub fn set_steps(&mut self, value: i64) -> bool {
        self.set_in(top::PARTY, party::STEPS, Json::int(value.max(0)))
    }

    pub fn playtime_frames(&self) -> i64 {
        self.global(top::SYSTEM)
            .and_then(|s| s.int_at(system::FRAMES_ON_SAVE))
            .unwrap_or(0)
    }

    pub fn set_playtime_seconds(&mut self, seconds: i64) -> bool {
        let frames = seconds.max(0).saturating_mul(FRAME_RATE);
        self.set_in(top::SYSTEM, system::FRAMES_ON_SAVE, Json::int(frames))
    }

    pub fn save_count(&self) -> Option<i64> {
        self.global(top::SYSTEM)?.int_at(system::SAVE_COUNT)
    }

    pub fn map_id(&self) -> i64 {
        self.global(top::MAP).and_then(|m| m.int_at("_mapId")).unwrap_or(0)
    }

    pub fn player_position(&self) -> (i64, i64) {
        let player = self.global(top::PLAYER);
        let at = |key: &str| player.and_then(|p| p.int_at(key)).unwrap_or(0);
        (at("_x"), at("_y"))
    }

    /// Moves the player, keeping the camera where it was in relation to them.
    ///
    /// The engine does not recentre the view when a save is loaded, so moving
    /// the player without moving the view with them would start the game with
    /// the party off screen.
    pub fn set_player_position(&mut self, x: i64, y: i64) -> bool {
        let (old_x, old_y) = self.player_position();
        let player = match self.global_mut(top::PLAYER) {
            Some(player) => player,
            None => return false,
        };
        for (key, value) in [("_x", x), ("_y", y), ("_realX", x), ("_realY", y)] {
            player.set(key, Json::int(value));
        }

        if let Some(map) = self.global_mut(top::MAP) {
            for (key, from, to) in [("_displayX", old_x, x), ("_displayY", old_y, y)] {
                if let Some(display) = map.get(key).and_then(Json::as_f64) {
                    let moved = display + (to - from) as f64;
                    map.set(key, Json::float(moved.max(0.0)));
                }
            }
        }
        self.dirty = true;
        true
    }

    // ---- the party --------------------------------------------------------

    pub fn party_member_ids(&self) -> Vec<i64> {
        self.global(top::PARTY)
            .and_then(|p| p.get(party::ACTORS))
            .and_then(Json::elements)
            .map(|ids| ids.iter().filter_map(Json::as_i64).collect())
            .unwrap_or_default()
    }

    pub fn set_party_member_ids(&mut self, ids: &[i64]) -> bool {
        self.dirty = true;
        let Some(members) = self
            .global_mut(top::PARTY)
            .and_then(|p| p.get_mut(party::ACTORS))
            .and_then(Json::elements_mut)
        else {
            return false;
        };
        *members = ids.iter().map(|id| Json::int(*id)).collect();
        true
    }

    // ---- what the party is carrying ---------------------------------------

    /// Which member of `Game_Party` holds a kind of thing.
    fn container_name(kind: ItemKind) -> &'static str {
        match kind {
            ItemKind::Item => party::ITEMS,
            ItemKind::Weapon => party::WEAPONS,
            ItemKind::Armor => party::ARMORS,
        }
    }

    /// What the party is carrying, as `(id, count)` in id order. The engine
    /// keys these by the id written out as text.
    pub fn items(&self, kind: ItemKind) -> Vec<(i64, i64)> {
        let Some(container) = self.global(top::PARTY).and_then(|p| p.get(Self::container_name(kind)))
        else {
            return Vec::new();
        };
        let Some(members) = container.members() else { return Vec::new() };
        let mut held: Vec<(i64, i64)> = members
            .iter()
            .filter(|(key, _)| !key.as_str().starts_with('@'))
            .filter_map(|(key, count)| Some((key.as_str().parse().ok()?, count.as_i64()?)))
            .filter(|(_, count)| *count != 0)
            .collect();
        held.sort_unstable();
        held
    }

    pub fn item_count(&self, kind: ItemKind, id: i64) -> i64 {
        self.global(top::PARTY)
            .and_then(|p| p.get(Self::container_name(kind)))
            .and_then(|c| c.int_at(&id.to_string()))
            .unwrap_or(0)
    }

    /// Sets how many of one thing the party is carrying. A count of zero
    /// removes the entry, which is what the engine itself does.
    pub fn set_item_count(&mut self, kind: ItemKind, id: i64, count: i64) -> bool {
        self.dirty = true;
        let name = Self::container_name(kind);
        let Some(container) = self.global_mut(top::PARTY).and_then(|p| p.get_mut(name)) else {
            return false;
        };
        let count = count.clamp(0, MAX_ITEM_COUNT);
        if count == 0 {
            container.remove(&id.to_string());
            return true;
        }
        container.set(&id.to_string(), Json::int(count))
    }

    // ---- switches and variables -------------------------------------------

    /// How many switches the save has room for. The engine grows the array as
    /// a game sets them, so this is the highest one touched, not the number
    /// the project defines.
    pub fn switch_count(&self) -> usize {
        self.data_of(top::SWITCHES)
            .and_then(Json::elements)
            .map(|d| d.len().saturating_sub(1))
            .unwrap_or(0)
    }

    pub fn variable_count(&self) -> usize {
        self.data_of(top::VARIABLES)
            .and_then(Json::elements)
            .map(|d| d.len().saturating_sub(1))
            .unwrap_or(0)
    }

    pub fn switch(&self, id: i64) -> bool {
        usize::try_from(id)
            .ok()
            .and_then(|id| self.data_of(top::SWITCHES)?.element(id)?.as_bool())
            .unwrap_or(false)
    }

    pub fn set_switch(&mut self, id: i64, on: bool) -> bool {
        let Ok(index) = usize::try_from(id) else { return false };
        self.dirty = true;
        self.data_of_mut(top::SWITCHES)
            .is_some_and(|d| d.set_element(index, Json::Bool(on)))
    }

    /// A variable's value. These hold whatever a game put in them, which is
    /// usually a number but can be anything JSON can carry.
    pub fn variable(&self, id: i64) -> Option<&Json> {
        let index = usize::try_from(id).ok()?;
        self.data_of(top::VARIABLES)?.element(index)
    }

    pub fn set_variable(&mut self, id: i64, value: Json) -> bool {
        let Ok(index) = usize::try_from(id) else { return false };
        self.dirty = true;
        self.data_of_mut(top::VARIABLES)
            .is_some_and(|d| d.set_element(index, value))
    }

    // ---- self switches ----------------------------------------------------

    /// The engine keys these by `"mapId,eventId,LETTER"`.
    fn self_switch_key(map_id: i64, event_id: i64, letter: &str) -> String {
        format!("{map_id},{event_id},{letter}")
    }

    /// Every self switch the save holds, as `(map, event, letter, on)`.
    pub fn self_switches(&self) -> Vec<(i64, i64, String, bool)> {
        let Some(members) = self.data_of(top::SELF_SWITCHES).and_then(Json::members) else {
            return Vec::new();
        };
        let mut rows: Vec<(i64, i64, String, bool)> = members
            .iter()
            .filter_map(|(key, value)| {
                let mut parts = key.as_str().split(',');
                let map_id = parts.next()?.parse().ok()?;
                let event_id = parts.next()?.parse().ok()?;
                let letter = parts.next()?.to_owned();
                Some((map_id, event_id, letter, value.as_bool().unwrap_or(false)))
            })
            .collect();
        rows.sort_unstable();
        rows
    }

    pub fn set_self_switch(&mut self, map_id: i64, event_id: i64, letter: &str, on: bool) -> bool {
        self.dirty = true;
        let key = Self::self_switch_key(map_id, event_id, letter);
        self.data_of_mut(top::SELF_SWITCHES)
            .is_some_and(|d| d.set(&key, Json::Bool(on)))
    }

    /// Deletes the entry outright, which is not the same as setting it off:
    /// the engine treats a missing entry as off, and a save that never had one
    /// should not grow one.
    pub fn remove_self_switch(&mut self, map_id: i64, event_id: i64, letter: &str) -> bool {
        let key = Self::self_switch_key(map_id, event_id, letter);
        let removed = self
            .data_of_mut(top::SELF_SWITCHES)
            .and_then(|d| d.remove(&key))
            .is_some();
        self.dirty |= removed;
        removed
    }

    // ---- actors -----------------------------------------------------------

    /// `$gameActors._data` is indexed by actor id, with gaps for actors the
    /// game has not instantiated.
    pub fn actor_ids(&self) -> Vec<i64> {
        let Some(actors) = self.data_of(top::ACTORS).and_then(Json::elements) else {
            return Vec::new();
        };
        actors
            .iter()
            .enumerate()
            .filter(|(_, actor)| !actor.is_null())
            .map(|(index, actor)| actor.int_at(actor::ACTOR_ID).unwrap_or(index as i64))
            .collect()
    }

    pub fn actor(&self, id: i64) -> Option<&Json> {
        let index = usize::try_from(id).ok()?;
        self.data_of(top::ACTORS)?.element(index)
    }

    pub fn actor_mut(&mut self, id: i64) -> Option<&mut Json> {
        let index = usize::try_from(id).ok()?;
        let actors = self.data_of_mut(top::ACTORS)?.elements_mut()?;
        actors.get_mut(index).filter(|actor| !actor.is_null())
    }

    pub fn actor_int(&self, id: i64, field: &str) -> Option<i64> {
        self.actor(id)?.int_at(field)
    }

    pub fn actor_text(&self, id: i64, field: &str) -> Option<&str> {
        self.actor(id)?.str_at(field)
    }

    pub fn set_actor_field(&mut self, id: i64, field: &str, value: Json) -> bool {
        self.dirty = true;
        self.actor_mut(id).is_some_and(|actor| actor.set(field, value))
    }

    /// An actor's experience, which the engine keeps per class so that a
    /// change of class does not lose the progress made in the old one.
    pub fn actor_exp(&self, id: i64) -> Option<i64> {
        let actor = self.actor(id)?;
        let class_id = actor.int_at(actor::CLASS_ID)?;
        actor.get(actor::EXP)?.int_at(&class_id.to_string())
    }

    pub fn set_actor_exp(&mut self, id: i64, exp: i64) -> bool {
        let Some(class_id) = self.actor_int(id, actor::CLASS_ID) else { return false };
        self.dirty = true;
        self.actor_mut(id)
            .and_then(|actor| actor.get_mut(actor::EXP))
            .is_some_and(|table| table.set(&class_id.to_string(), Json::int(exp.max(0))))
    }

    pub fn actor_param_plus(&self, id: i64, index: usize) -> Option<i64> {
        self.actor(id)?.get(actor::PARAM_PLUS)?.element(index)?.as_i64()
    }

    pub fn set_actor_param_plus(&mut self, id: i64, index: usize, value: i64) -> bool {
        if index >= PARAMS.len() {
            return false;
        }
        self.dirty = true;
        self.actor_mut(id)
            .and_then(|actor| actor.get_mut(actor::PARAM_PLUS))
            .is_some_and(|plus| plus.set_element(index, Json::int(value)))
    }

    fn actor_id_list(&self, id: i64, field: &str) -> Vec<i64> {
        self.actor(id)
            .and_then(|actor| actor.get(field))
            .and_then(Json::elements)
            .map(|ids| ids.iter().filter_map(Json::as_i64).collect())
            .unwrap_or_default()
    }

    fn set_actor_id_list(&mut self, id: i64, field: &str, ids: &[i64]) -> bool {
        self.dirty = true;
        self.actor_mut(id)
            .and_then(|actor| actor.get_mut(field))
            .and_then(Json::elements_mut)
            .map(|list| *list = ids.iter().map(|id| Json::int(*id)).collect())
            .is_some()
    }

    pub fn actor_skills(&self, id: i64) -> Vec<i64> {
        self.actor_id_list(id, actor::SKILLS)
    }

    pub fn set_actor_skills(&mut self, id: i64, ids: &[i64]) -> bool {
        self.set_actor_id_list(id, actor::SKILLS, ids)
    }

    pub fn actor_states(&self, id: i64) -> Vec<i64> {
        self.actor_id_list(id, actor::STATES)
    }

    /// Sets which states an actor is under, dropping the turn and step counts
    /// of any that have gone so that nothing is left counting down a state the
    /// actor no longer has.
    pub fn set_actor_states(&mut self, id: i64, ids: &[i64]) -> bool {
        if !self.set_actor_id_list(id, actor::STATES, ids) {
            return false;
        }
        let kept: Vec<String> = ids.iter().map(i64::to_string).collect();
        let Some(actor) = self.actor_mut(id) else { return false };
        for field in [actor::STATE_TURNS, actor::STATE_STEPS] {
            let Some(Json::Object(members)) = actor.get_mut(field) else { continue };
            members.retain(|(key, _)| {
                key.as_str().starts_with('@') || kept.iter().any(|id| id == key.as_str())
            });
        }
        true
    }

    /// What an actor has equipped, slot by slot, as `(kind, id)`. The kind is
    /// the engine's own `_dataClass`, which is how it tells a weapon from a
    /// piece of armour in a slot.
    pub fn actor_equipment(&self, id: i64) -> Vec<(String, i64)> {
        let Some(equips) = self.actor(id).and_then(|a| a.get(actor::EQUIPS)).and_then(Json::elements)
        else {
            return Vec::new();
        };
        equips
            .iter()
            .enumerate()
            .map(|(slot, equip)| {
                let kind = equip.str_at(item::DATA_CLASS).unwrap_or("").to_owned();
                let item_id = equip.int_at(item::ITEM_ID).unwrap_or(0);
                // An empty slot has no class; the first slot is the weapon.
                let kind = match (kind.as_str(), slot) {
                    ("weapon" | "armor", _) => kind,
                    (_, 0) => "weapon".to_owned(),
                    _ => "armor".to_owned(),
                };
                (kind, item_id)
            })
            .collect()
    }

    /// Equips one slot. An id of zero empties it, which the engine records as
    /// an item of no class at all.
    pub fn set_actor_equipment(&mut self, id: i64, slot: usize, kind: &str, item_id: i64) -> bool {
        self.dirty = true;
        let Some(equips) = self
            .actor_mut(id)
            .and_then(|actor| actor.get_mut(actor::EQUIPS))
            .and_then(Json::elements_mut)
        else {
            return false;
        };
        let Some(equip) = equips.get_mut(slot) else { return false };
        let kind = if item_id == 0 { "" } else { kind };
        equip.set(item::DATA_CLASS, Json::text(kind));
        equip.set(item::ITEM_ID, Json::int(item_id.max(0)))
    }
}

/// True for base64 that stands for a zlib stream, which is how RPG Maker MZ
/// compresses a save.
fn looks_like_zlib(text: &str) -> bool {
    // A zlib header is 0x78 and one of four second bytes, by compression level.
    matches!(lzstring::first_bytes(text), Some([0x78, 0x01 | 0x5E | 0x9C | 0xDA, _]))
}

/// The three tables a party carries things in.
pub use crate::rpg::save::ItemKind;
