//! The save file itself: locating the well-known game objects inside the
//! decoded Marshal documents, reading them, and writing the file back.

use std::io;
use std::path::{Path, PathBuf};

use crate::marshal::{self, Heap, MarshalError, NodeKind, Value};

use super::engine::{Engine, UnsupportedReason};

/// Where VX Ace keeps the play-time frame counter. `@frames_on_save` is the
/// name RGSS3's own `Game_System#on_before_save` uses; the rest cover scripts
/// and engines that named it differently.
const PLAYTIME_IVARS: [&str; 4] = ["@frames_on_save", "@framecount", "@frame_count", "@playtime"];

/// Hard limits the engines themselves enforce.
pub const MAX_GOLD: i64 = 99_999_999;
pub const MAX_ITEM_COUNT: i64 = 99;

#[derive(Debug)]
pub enum OpenError {
    Io(io::Error),
    Marshal(MarshalError),
    Unsupported(UnsupportedReason),
}

impl std::fmt::Display for OpenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OpenError::Io(e) => write!(f, "{e}"),
            OpenError::Marshal(e) => write!(f, "This file is not readable as Ruby Marshal data: {e}"),
            OpenError::Unsupported(r) => write!(f, "{}", r.message()),
        }
    }
}

impl std::error::Error for OpenError {}

impl From<io::Error> for OpenError {
    fn from(e: io::Error) -> Self {
        OpenError::Io(e)
    }
}

/// One of the inventory containers a party carries.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ItemKind {
    Item,
    Weapon,
    Armor,
}

impl ItemKind {
    pub fn ivar(self) -> &'static str {
        match self {
            ItemKind::Item => "@items",
            ItemKind::Weapon => "@weapons",
            ItemKind::Armor => "@armors",
        }
    }

    pub fn id(self) -> &'static str {
        match self {
            ItemKind::Item => "item",
            ItemKind::Weapon => "weapon",
            ItemKind::Armor => "armor",
        }
    }

    /// The Ruby class VX Ace stores in `Game_BaseItem#@class`.
    pub fn ruby_class(self) -> &'static str {
        match self {
            ItemKind::Item => "RPG::Item",
            ItemKind::Weapon => "RPG::Weapon",
            ItemKind::Armor => "RPG::Armor",
        }
    }

    pub fn from_id(s: &str) -> Option<ItemKind> {
        match s {
            "item" => Some(ItemKind::Item),
            "weapon" => Some(ItemKind::Weapon),
            "armor" => Some(ItemKind::Armor),
            _ => None,
        }
    }

    pub fn all() -> [ItemKind; 3] {
        [ItemKind::Item, ItemKind::Weapon, ItemKind::Armor]
    }
}

/// A decoded save file, kept in the form it was read so that anything the
/// editor does not understand survives the round trip untouched.
pub struct SaveFile {
    pub path: PathBuf,
    pub engine: Engine,
    pub heap: Heap,
    /// Top-level Marshal documents, in file order.
    pub documents: Vec<Value>,
    pub dirty: bool,
    /// Things worth telling the user about this particular file.
    pub notes: Vec<String>,
    /// Size of the file as last read or written.
    pub size_bytes: usize,
    /// VX Ace: index of the save-contents hash — the one the game loads.
    contents_doc: Option<usize>,
    /// Index of the file-select header document.
    header_doc: Option<usize>,
    /// Whether play time was changed, and so the header string needs rewriting.
    playtime_edited: bool,
    /// VX: index of the bare `Graphics.frame_count` document.
    frame_doc: Option<usize>,
}

impl SaveFile {
    pub fn open(path: &Path) -> Result<SaveFile, OpenError> {
        let bytes = std::fs::read(path)?;
        let mut file = SaveFile::from_bytes(&bytes, Engine::from_path(path))?;
        file.path = path.to_path_buf();
        Ok(file)
    }

    pub fn from_bytes(bytes: &[u8], hint: Option<Engine>) -> Result<SaveFile, OpenError> {
        let mut heap = Heap::new();
        let documents = marshal::load_stream(bytes, &mut heap).map_err(OpenError::Marshal)?;

        let engine = Engine::from_documents(&heap, &documents)
            .or(hint)
            .ok_or(OpenError::Unsupported(UnsupportedReason::NotASave))?;

        let mut notes = Vec::new();
        let (header_doc, contents_doc) = match engine {
            Engine::VxAce => {
                let header = documents.iter().position(|d| {
                    heap.hash_entries(*d).is_some_and(|e| has_symbol_key(&heap, e, "characters"))
                });
                let contents = find_contents_doc(&heap, &documents, header);
                if contents.is_none() {
                    notes.push(
                        "No save-contents hash was found; this file may have been written by a \
                         script that changes the save format."
                            .to_owned(),
                    );
                }
                (header, contents)
            }
            Engine::Vx | Engine::Xp => (Some(0), None),
        };
        // VX and XP dump `Graphics.frame_count` as a document of its own.
        let frame_doc = match engine {
            Engine::Vx | Engine::Xp => documents.iter().position(|d| matches!(d, Value::Int(_))),
            Engine::VxAce => None,
        };

        let mut file = SaveFile {
            path: PathBuf::new(),
            engine,
            heap,
            documents,
            dirty: false,
            notes,
            size_bytes: bytes.len(),
            contents_doc,
            header_doc,
            playtime_edited: false,
            frame_doc,
        };
        if file.root("Game_Party").is_none() {
            file.notes.push(
                "No Game_Party object was found, so party and inventory editing is unavailable."
                    .to_owned(),
            );
        } else if file.roots("Game_Party").len() > 1 {
            file.notes.push(
                "This game mirrors its whole state into the save header for the load screen. \
                 Edits are written to every copy so the two stay in step."
                    .to_owned(),
            );
        }
        Ok(file)
    }

    // ---- locating the game objects ---------------------------------------

    /// Every value that could be a top-level game object, paired with the
    /// document it came from. The contents document comes first, so that
    /// reads see the state the game will actually load.
    fn candidates(&self) -> Vec<(usize, Value)> {
        let mut order: Vec<usize> = (0..self.documents.len()).collect();
        if let Some(contents) = self.contents_doc {
            order.retain(|i| *i != contents);
            order.insert(0, contents);
        }

        let mut out = Vec::new();
        for index in order {
            let document = self.documents[index];
            match self.heap.hash_entries(document) {
                Some(entries) => out.extend(entries.iter().map(|(_, v)| (index, *v))),
                None => out.push((index, document)),
            }
        }
        out
    }

    /// Finds a top-level game object by its Ruby class name.
    ///
    /// Searching by class rather than by position or hash key keeps working on
    /// saves written by games whose scripts reorder or extend the contents.
    pub fn root(&self, class: &str) -> Option<Value> {
        self.candidates()
            .into_iter()
            .find(|(_, v)| self.heap.class_name(*v) == Some(class))
            .map(|(_, v)| v)
    }

    /// Every top-level copy of a class, the one the game loads first.
    ///
    /// Stock saves hold exactly one of each. Some games mirror the entire game
    /// state into the file header so the load screen can show gold, play time
    /// and party without loading the save proper — leaving those copies behind
    /// would make the load screen disagree with the game.
    pub fn roots(&self, class: &str) -> Vec<Value> {
        self.candidates()
            .into_iter()
            .filter(|(_, v)| self.heap.class_name(*v) == Some(class))
            .map(|(_, v)| v)
            .collect()
    }

    /// Applies a change to every top-level copy of a class.
    fn update_roots(
        &mut self,
        class: &str,
        mut apply: impl FnMut(&mut Heap, Value) -> bool,
    ) -> bool {
        let roots = self.roots(class);
        let mut changed = false;
        for root in roots {
            changed |= apply(&mut self.heap, root);
        }
        self.dirty |= changed;
        changed
    }

    pub fn system(&self) -> Option<Value> {
        self.root("Game_System")
    }

    pub fn party(&self) -> Option<Value> {
        self.root("Game_Party")
    }

    pub fn actors_root(&self) -> Option<Value> {
        self.root("Game_Actors")
    }

    pub fn map(&self) -> Option<Value> {
        self.root("Game_Map")
    }

    pub fn player(&self) -> Option<Value> {
        self.root("Game_Player")
    }

    /// The `@data` array or hash behind `Game_Switches`, `Game_Variables`,
    /// `Game_SelfSwitches` and `Game_Actors`.
    pub fn data_of(&self, class: &str) -> Option<Value> {
        self.heap.ivar(self.root(class)?, "@data")
    }

    // ---- play time --------------------------------------------------------

    pub fn playtime_frames(&self) -> Option<i64> {
        match self.engine {
            Engine::Vx | Engine::Xp => self.frame_doc.and_then(|i| self.documents[i].as_int()),
            Engine::VxAce => {
                let system = self.system()?;
                PLAYTIME_IVARS.iter().find_map(|n| self.heap.ivar_int(system, n))
            }
        }
    }

    pub fn set_playtime_frames(&mut self, frames: i64) -> bool {
        let frames = frames.max(0);
        match self.engine {
            Engine::Vx | Engine::Xp => {
                let Some(i) = self.frame_doc else { return false };
                self.documents[i] = Value::Int(frames);
            }
            Engine::VxAce => {
                let Some(system) = self.system() else { return false };
                // Only ever write a field the save already has: inventing one
                // would leave a value the engine never reads.
                let Some(name) = PLAYTIME_IVARS
                    .into_iter()
                    .find(|n| self.heap.ivar(system, n).is_some())
                else {
                    return false;
                };
                let changed = self.update_roots("Game_System", |heap, system| {
                    heap.set_ivar(system, name, Value::Int(frames))
                });
                self.playtime_edited |= changed;
                return changed;
            }
        }
        self.playtime_edited = true;
        self.dirty = true;
        true
    }

    pub fn playtime_seconds(&self) -> Option<i64> {
        self.playtime_frames().map(|f| f / self.engine.frame_rate())
    }

    /// Sets play time in seconds, converting at the engine's frame rate.
    pub fn set_playtime_seconds(&mut self, seconds: i64) -> bool {
        self.set_playtime_frames(seconds.max(0) * self.engine.frame_rate())
    }

    // ---- party ------------------------------------------------------------

    pub fn gold(&self) -> Option<i64> {
        self.heap.ivar_int(self.party()?, "@gold")
    }

    pub fn set_gold(&mut self, gold: i64) -> bool {
        let gold = gold.clamp(0, MAX_GOLD);
        self.update_roots("Game_Party", |heap, party| {
            heap.set_ivar(party, "@gold", Value::Int(gold))
        })
    }

    pub fn steps(&self) -> Option<i64> {
        self.heap.ivar_int(self.party()?, "@steps")
    }

    pub fn set_steps(&mut self, steps: i64) -> bool {
        let steps = steps.max(0);
        self.update_roots("Game_Party", |heap, party| {
            heap.set_ivar(party, "@steps", Value::Int(steps))
        })
    }

    /// Actor ids in the party, in order.
    ///
    /// VX and VX Ace store ids; XP stores the `Game_Actor` objects themselves.
    pub fn party_member_ids(&self) -> Vec<i64> {
        self.party()
            .and_then(|p| self.heap.ivar(p, "@actors"))
            .and_then(|a| self.heap.array(a))
            .map(|items| {
                items
                    .iter()
                    .filter_map(|v| v.as_int().or_else(|| self.heap.ivar_int(*v, "@actor_id")))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Whether this actor could join the party.
    ///
    /// On XP the party holds actor objects, so only actors the game has already
    /// created can be put in it.
    pub fn party_can_include(&self, actor_id: i64) -> bool {
        !self.engine.party_holds_objects() || self.actor(actor_id).is_some()
    }

    pub fn set_party_member_ids(&mut self, ids: &[i64]) -> bool {
        let members: Vec<Value> = if self.engine.party_holds_objects() {
            let mut resolved = Vec::with_capacity(ids.len());
            for id in ids {
                let Some(actor) = self.actor(*id) else { return false };
                resolved.push(actor);
            }
            resolved
        } else {
            ids.iter().map(|id| Value::Int(*id)).collect()
        };

        self.update_roots("Game_Party", |heap, party| {
            let Some(actors) = heap.ivar(party, "@actors") else { return false };
            let Some(list) = heap.array_mut(actors) else { return false };
            *list = members.clone();
            true
        })
    }

    // ---- inventory --------------------------------------------------------

    /// `(id, count)` pairs from one of the party's containers.
    pub fn inventory(&self, kind: ItemKind) -> Vec<(i64, i64)> {
        let Some(party) = self.party() else { return Vec::new() };
        let Some(container) = self.heap.ivar(party, kind.ivar()) else { return Vec::new() };
        let Some(entries) = self.heap.hash_entries(container) else { return Vec::new() };
        let mut out: Vec<(i64, i64)> = entries
            .iter()
            .filter_map(|(k, v)| Some((k.as_int()?, v.as_int().unwrap_or(0))))
            .collect();
        out.sort_by_key(|(id, _)| *id);
        out
    }

    /// Sets how many of an item the party holds. A count of zero removes the
    /// entry, which is what the engine's own `gain_item` does.
    pub fn set_item_count(&mut self, kind: ItemKind, id: i64, count: i64) -> bool {
        let count = count.clamp(0, MAX_ITEM_COUNT);
        let key = Value::Int(id);
        self.update_roots("Game_Party", |heap, party| {
            let Some(container) = heap.ivar(party, kind.ivar()) else { return false };
            if heap.hash_entries(container).is_none() {
                return false;
            }
            if count == 0 {
                heap.hash_remove(container, key);
            } else {
                heap.hash_set(container, key, Value::Int(count));
            }
            true
        })
    }

    // ---- switches and variables -------------------------------------------

    /// Length of the switch or variable array, i.e. the highest id the save
    /// has ever touched.
    pub fn data_len(&self, class: &str) -> usize {
        self.data_of(class)
            .and_then(|d| self.heap.array(d))
            .map(|a| a.len())
            .unwrap_or(0)
    }

    pub fn switch(&self, id: i64) -> bool {
        self.data_of("Game_Switches")
            .and_then(|d| self.heap.array(d))
            .and_then(|a| a.get(id as usize).copied())
            .map(Value::truthy)
            .unwrap_or(false)
    }

    pub fn set_switch(&mut self, id: i64, on: bool) -> bool {
        self.set_indexed("Game_Switches", id, Value::Bool(on))
    }

    pub fn variable(&self, id: i64) -> Option<Value> {
        self.data_of("Game_Variables")
            .and_then(|d| self.heap.array(d))
            .and_then(|a| a.get(id as usize).copied())
    }

    pub fn set_variable(&mut self, id: i64, value: Value) -> bool {
        self.set_indexed("Game_Variables", id, value)
    }

    /// Writes into an id-indexed `@data` array, growing it with `nil` the way
    /// Ruby does when you assign past the end.
    fn set_indexed(&mut self, class: &str, id: i64, value: Value) -> bool {
        if id < 0 {
            return false;
        }
        let index = id as usize;
        self.update_roots(class, |heap, root| {
            let Some(data) = heap.ivar(root, "@data") else { return false };
            let Some(array) = heap.array_mut(data) else { return false };
            if index >= array.len() {
                array.resize(index + 1, Value::Nil);
            }
            array[index] = value;
            true
        })
    }

    /// `(map_id, event_id, letter, on)` for every self switch that has a value.
    pub fn self_switches(&self) -> Vec<(i64, i64, String, bool)> {
        let Some(data) = self.data_of("Game_SelfSwitches") else { return Vec::new() };
        let Some(entries) = self.heap.hash_entries(data) else { return Vec::new() };
        let mut out = Vec::new();
        for (key, value) in entries {
            let Some(parts) = self.heap.array(*key) else { continue };
            if parts.len() < 3 {
                continue;
            }
            let (Some(map), Some(event)) = (parts[0].as_int(), parts[1].as_int()) else { continue };
            let letter = self.heap.string(parts[2]).unwrap_or_default();
            out.push((map, event, letter, value.truthy()));
        }
        out.sort_by_key(|(map, event, letter, _)| (*map, *event, letter.clone()));
        out
    }

    pub fn set_self_switch(&mut self, map: i64, event: i64, letter: &str, on: bool) -> bool {
        let engine = self.engine;
        self.update_roots("Game_SelfSwitches", |heap, root| {
            let Some(data) = heap.ivar(root, "@data") else { return false };
            let letter_value = new_engine_string(heap, engine, letter);
            let key = heap.new_array(vec![Value::Int(map), Value::Int(event), letter_value]);
            heap.hash_set(data, key, Value::Bool(on))
        })
    }

    pub fn remove_self_switch(&mut self, map: i64, event: i64, letter: &str) -> bool {
        self.update_roots("Game_SelfSwitches", |heap, root| {
            let Some(data) = heap.ivar(root, "@data") else { return false };
            let letter_value = heap.new_str(letter);
            let key = heap.new_array(vec![Value::Int(map), Value::Int(event), letter_value]);
            heap.hash_remove(data, key)
        })
    }

    // ---- actors -----------------------------------------------------------

    /// `(actor_id, actor)` for every actor stored in the save.
    pub fn actors(&self) -> Vec<(i64, Value)> {
        let Some(data) = self.actors_root().and_then(|r| self.heap.ivar(r, "@data")) else {
            return Vec::new();
        };
        let Some(items) = self.heap.array(data) else { return Vec::new() };
        items
            .iter()
            .enumerate()
            .filter(|(_, v)| !v.is_nil())
            .map(|(i, v)| {
                let id = self.heap.ivar_int(*v, "@actor_id").unwrap_or(i as i64);
                (id, *v)
            })
            .collect()
    }

    pub fn actor(&self, id: i64) -> Option<Value> {
        self.actors().into_iter().find(|(a, _)| *a == id).map(|(_, v)| v)
    }

    /// Every stored copy of the actor with this id.
    ///
    /// There is usually one, in `Game_Actors`. XP also keeps the party members
    /// themselves inside `Game_Party`, and because each is written as its own
    /// Marshal document they come back as separate objects — so a party member
    /// exists twice and both have to be kept in step.
    pub fn actor_copies(&self, id: i64) -> Vec<Value> {
        let matches = |actor: &Value, index: usize| {
            !actor.is_nil()
                && self.heap.ivar_int(*actor, "@actor_id").unwrap_or(index as i64) == id
        };

        let mut copies: Vec<Value> = Vec::new();
        let mut push = |value: Value| {
            if !copies.contains(&value) {
                copies.push(value);
            }
        };

        for root in self.roots("Game_Actors") {
            let Some(items) = self.heap.ivar(root, "@data").and_then(|d| self.heap.array(d)) else {
                continue;
            };
            if let Some(actor) = items.iter().enumerate().find(|(i, a)| matches(a, *i)) {
                push(*actor.1);
            }
        }

        for party in self.roots("Game_Party") {
            let Some(members) = self.heap.ivar(party, "@actors").and_then(|a| self.heap.array(a))
            else {
                continue;
            };
            for (index, member) in members.iter().enumerate() {
                // Only the engines that store objects here; ids are not copies.
                if member.as_ref().is_some() && matches(member, index) {
                    push(*member);
                }
            }
        }
        copies
    }

    /// Runs a change against every stored copy of one actor.
    ///
    /// Reading an actor gives the copy the game will load; writing has to
    /// reach the header's mirror as well, or the load screen goes stale.
    pub fn update_actor(
        &mut self,
        id: i64,
        mut apply: impl FnMut(&mut SaveFile, Value) -> bool,
    ) -> bool {
        let copies = self.actor_copies(id);
        let mut changed = false;
        for actor in copies {
            changed |= apply(self, actor);
        }
        changed
    }

    // ---- map / player -----------------------------------------------------

    pub fn map_id(&self) -> Option<i64> {
        self.heap.ivar_int(self.map()?, "@map_id")
    }

    pub fn player_position(&self) -> Option<(i64, i64)> {
        let player = self.player()?;
        Some((
            self.heap.ivar_int(player, "@x")?,
            self.heap.ivar_int(player, "@y")?,
        ))
    }

    /// Moves the player. `@real_x` / `@real_y` are the smooth-scroll position
    /// and must follow, or the sprite slides in from the old tile.
    pub fn set_player_position(&mut self, x: i64, y: i64) -> bool {
        let engine = self.engine;
        self.update_roots("Game_Player", |heap, player| {
            heap.set_ivar(player, "@x", Value::Int(x));
            heap.set_ivar(player, "@y", Value::Int(y));
            match engine.tile_subdivisions() {
                // RGSS3 tracks the position within a tile as a Float.
                None => {
                    let (real_x, real_y) = (heap.new_float(x as f64), heap.new_float(y as f64));
                    heap.set_ivar(player, "@real_x", real_x);
                    heap.set_ivar(player, "@real_y", real_y);
                }
                // The older engines count in fractions of a tile.
                Some(scale) => {
                    heap.set_ivar(player, "@real_x", Value::Int(x * scale));
                    heap.set_ivar(player, "@real_y", Value::Int(y * scale));
                }
            }
            true
        })
    }

    // ---- writing ----------------------------------------------------------

    pub fn to_bytes(&mut self) -> Vec<u8> {
        self.refresh_header();
        marshal::dump_stream(&self.documents, &self.heap)
    }

    /// Rewrites the file-select header so the game's load screen shows the
    /// edited party and play time.
    fn refresh_header(&mut self) {
        let index = self.header_doc.unwrap_or(0);
        let Some(&header) = self.documents.get(index) else { return };

        match self.engine {
            Engine::VxAce => {
                if self.heap.hash_entries(header).is_none() {
                    return;
                }
                let key = self.heap.new_sym("characters");
                if let Some(existing) = self.heap.hash_get(header, key) {
                    let rebuilt = self.build_characters(self.header_face_limit(existing));
                    // Only touch it when the faces would actually change; a game
                    // may store a shape here that this rebuild cannot reproduce.
                    if !self.heap.value_eq(existing, rebuilt) {
                        self.heap.hash_set(header, key, rebuilt);
                    }
                }
                self.refresh_playtime_text(header);
            }
            Engine::Vx | Engine::Xp => {
                // The first document is the characters array itself.
                if self.heap.array(header).is_some() {
                    let rebuilt = self.build_characters(self.header_face_limit(header));
                    if !self.heap.value_eq(header, rebuilt) {
                        self.documents[index] = rebuilt;
                    }
                }
            }
        }
    }

    /// Rewrites the header's play-time string, but only when play time was
    /// actually edited and is actually known.
    ///
    /// The load screen reads this string rather than recomputing it, so writing
    /// a value derived from a field we failed to find would leave the game
    /// showing a play time it never had.
    fn refresh_playtime_text(&mut self, header: Value) {
        if !self.playtime_edited {
            return;
        }
        let Some(seconds) = self.playtime_seconds() else { return };
        let key = self.heap.new_sym("playtime_s");
        let text = format_playtime(seconds);
        match self.heap.hash_get(header, key) {
            // Replacing the bytes in place keeps the string's encoding.
            Some(existing) if self.heap.set_string(existing, &text) => {}
            Some(_) | None => {
                let value = new_engine_string(&mut self.heap, self.engine, &text);
                self.heap.hash_set(header, key, value);
            }
        }
    }

    /// How many faces the load screen shows.
    ///
    /// Both engines default to the four battle members, but a game whose
    /// scripts show more already says so in the header it wrote.
    fn header_face_limit(&self, existing: Value) -> usize {
        self.heap.array(existing).map(<[Value]>::len).unwrap_or(4).max(4)
    }

    /// `[[character_name, character_index], ...]` for the battle members, the
    /// shape both engines store in the save header.
    fn build_characters(&mut self, limit: usize) -> Value {
        let ids = self.party_member_ids();
        let mut rows = Vec::new();
        for id in ids.into_iter().take(limit) {
            let Some(actor) = self.actor(id) else { continue };
            let name = self
                .heap
                .ivar(actor, "@character_name")
                .unwrap_or(Value::Nil);
            let index = self
                .heap
                .ivar(actor, "@character_index")
                .or_else(|| self.heap.ivar(actor, "@character_hue"))
                .unwrap_or(Value::Int(0));
            rows.push(self.heap.new_array(vec![name, index]));
        }
        self.heap.new_array(rows)
    }

    /// Writes the save, keeping a timestamped backup of what was there before.
    ///
    /// The new bytes go to a temporary file in the same directory and are then
    /// renamed over the target, so an interrupted write cannot leave a
    /// half-written save behind.
    pub fn write(&mut self, path: &Path, make_backup: bool) -> io::Result<Option<PathBuf>> {
        let bytes = self.to_bytes();

        let backup = if make_backup && path.exists() {
            let backup = backup_path(path);
            std::fs::copy(path, &backup)?;
            prune_backups(path, 10);
            Some(backup)
        } else {
            None
        };

        self.size_bytes = bytes.len();
        let temp = path.with_extension(format!(
            "{}.tmp",
            path.extension().and_then(|e| e.to_str()).unwrap_or("save")
        ));
        std::fs::write(&temp, &bytes)?;
        std::fs::rename(&temp, path)?;

        self.path = path.to_path_buf();
        self.dirty = false;
        self.playtime_edited = false;
        Ok(backup)
    }

    /// Where the editor expects to find the game's `Data` directory.
    pub fn guess_data_dir(&self) -> Option<PathBuf> {
        let needle = format!("System.{}", self.engine.data_extension());
        let mut dir = self.path.parent()?;
        for _ in 0..3 {
            let candidate = dir.join("Data");
            if candidate.join(&needle).is_file() {
                return Some(candidate);
            }
            if dir.join(&needle).is_file() {
                return Some(dir.to_path_buf());
            }
            dir = dir.parent()?;
        }
        None
    }
}

/// Picks the document the game will actually load.
///
/// Stock VX Ace writes a small header then the contents. Some games mirror the
/// whole state into that header too, so a hash that looks like contents is not
/// enough on its own: prefer one that is not the header, and failing that the
/// last one, which is the order `DataManager` reads them in.
fn find_contents_doc(heap: &Heap, documents: &[Value], header: Option<usize>) -> Option<usize> {
    let candidates: Vec<usize> = documents
        .iter()
        .enumerate()
        .filter(|(_, d)| heap.hash_entries(**d).is_some_and(|e| looks_like_contents(heap, e)))
        .map(|(i, _)| i)
        .collect();
    candidates
        .iter()
        .rev()
        .find(|i| Some(**i) != header)
        .or_else(|| candidates.last())
        .copied()
}

fn has_symbol_key(heap: &Heap, entries: &[(Value, Value)], name: &str) -> bool {
    entries.iter().any(|(k, _)| match k {
        Value::Sym(s) => heap.sym(*s) == name,
        _ => false,
    })
}

/// A hash is the VX Ace save contents if it is keyed by the symbols the engine
/// uses. Two matches is enough to be sure without demanding an exact set,
/// which scripts sometimes extend.
pub(crate) fn looks_like_contents(heap: &Heap, entries: &[(Value, Value)]) -> bool {
    const KEYS: [&str; 6] = ["system", "party", "actors", "switches", "variables", "map"];
    let hits = entries
        .iter()
        .filter(|(k, _)| match k {
            Value::Sym(s) => KEYS.contains(&heap.sym(*s)),
            _ => false,
        })
        .count();
    hits >= 2
}

/// Builds a Ruby String the way this engine's Ruby would have.
pub fn new_engine_string(heap: &mut Heap, engine: Engine, text: &str) -> Value {
    if engine.strings_carry_encoding() {
        heap.new_utf8_str(text)
    } else {
        heap.new_str(text)
    }
}

pub fn format_playtime(seconds: i64) -> String {
    let seconds = seconds.max(0);
    format!("{:02}:{:02}:{:02}", seconds / 3600, seconds / 60 % 60, seconds % 60)
}

fn backup_path(path: &Path) -> PathBuf {
    let stamp = timestamp();
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("save");
    path.with_file_name(format!("{name}.{stamp}.bak"))
}

/// Deletes all but the newest `keep` backups of one save file.
fn prune_backups(path: &Path, keep: usize) {
    let (Some(dir), Some(name)) = (path.parent(), path.file_name().and_then(|n| n.to_str())) else {
        return;
    };
    let prefix = format!("{name}.");
    let mut backups: Vec<PathBuf> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with(&prefix) && n.ends_with(".bak"))
        })
        .collect();
    if backups.len() <= keep {
        return;
    }
    // The timestamp sorts lexicographically, so plain name order is age order.
    backups.sort();
    for old in &backups[..backups.len() - keep] {
        let _ = std::fs::remove_file(old);
    }
}

/// `YYYYmmdd-HHMMSS` in UTC, without pulling in a date library.
fn timestamp() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let (days, rem) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    let (y, m, d) = civil_from_days(days);
    format!(
        "{y:04}{m:02}{d:02}-{:02}{:02}{:02}",
        rem / 3600,
        rem / 60 % 60,
        rem % 60
    )
}

/// Howard Hinnant's days-from-epoch to calendar date conversion.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// Pretty one-line description of any value, for list rows and the raw tree.
pub fn preview(heap: &Heap, value: Value) -> String {
    match value {
        Value::Nil => "nil".to_owned(),
        Value::Bool(b) => b.to_string(),
        Value::Int(i) => i.to_string(),
        Value::Sym(s) => format!(":{}", heap.sym(s)),
        Value::Ref(id) => match heap.kind(id) {
            NodeKind::Float { value, .. } => format!("{value}"),
            NodeKind::Str(bytes) => {
                let text = crate::marshal::decode_ruby_string(bytes);
                if text.chars().count() > 60 {
                    let short: String = text.chars().take(57).collect();
                    format!("\"{short}…\"")
                } else {
                    format!("\"{text}\"")
                }
            }
            NodeKind::Array(items) => format!("Array ({} items)", items.len()),
            NodeKind::Hash { entries, .. } => format!("Hash ({} keys)", entries.len()),
            NodeKind::Object { class } => heap.sym(*class).to_owned(),
            NodeKind::UserDef { class, data } => super::rgss::describe(heap.sym(*class), data),
            NodeKind::UserMarshal { class, .. } => heap.sym(*class).to_owned(),
            NodeKind::UserClass { class, .. } => heap.sym(*class).to_owned(),
            NodeKind::Struct { name, fields } => {
                format!("{} ({} fields)", heap.sym(*name), fields.len())
            }
            NodeKind::Bignum { negative, words } => {
                let mut n: i128 = 0;
                for w in words.iter().rev().take(8) {
                    n = (n << 16) | *w as i128;
                }
                format!("{}{n}", if *negative { "-" } else { "" })
            }
            NodeKind::Class(name) => name.clone(),
            NodeKind::Module(name) => name.clone(),
            NodeKind::Regexp { source, .. } => {
                format!("/{}/", String::from_utf8_lossy(source))
            }
            NodeKind::Extended { module, .. } => format!("extended by {}", heap.sym(*module)),
        },
    }
}
