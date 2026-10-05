//! RPG Maker 2000 and 2003 save files (`SaveNN.lsd`).
//!
//! Both engines write the same LCF format, so one reader serves both. Chunk
//! numbers follow liblcf, EasyRPG's reference implementation of the format.
//!
//! A field left at its default value is simply absent from the file, so
//! reading means "this tag, or the default" and writing often means adding a
//! chunk that was never there.

use std::path::{Path, PathBuf};

use super::chunk::{Array, Chunks, Truncated};

/// Chunk numbers at the top level of a save.
mod top {
    pub const TITLE: u32 = 0x64;
    pub const SYSTEM: u32 = 0x65;
    pub const PARTY_LOCATION: u32 = 0x68;
    pub const ACTORS: u32 = 0x6C;
    pub const INVENTORY: u32 = 0x6D;
}

/// `SaveSystem`.
mod system {
    pub const FRAME_COUNT: u32 = 0x0B;
    pub const SWITCH_COUNT: u32 = 0x1F;
    pub const SWITCHES: u32 = 0x20;
    pub const VARIABLE_COUNT: u32 = 0x21;
    pub const VARIABLES: u32 = 0x22;
    pub const SAVE_COUNT: u32 = 0x83;
}

/// `SaveInventory`.
mod inventory {
    pub const PARTY_COUNT: u32 = 0x01;
    pub const PARTY: u32 = 0x02;
    pub const ITEM_COUNT: u32 = 0x0B;
    pub const ITEM_IDS: u32 = 0x0C;
    pub const ITEM_COUNTS: u32 = 0x0D;
    pub const ITEM_USAGE: u32 = 0x0E;
    pub const GOLD: u32 = 0x15;
    pub const STEPS: u32 = 0x2A;
}

/// `SaveMapEventBase`, which the party's location extends.
mod location {
    pub const MAP_ID: u32 = 0x0B;
    pub const X: u32 = 0x0C;
    pub const Y: u32 = 0x0D;
}

/// `SaveActor`.
pub mod actor {
    pub const NAME: u32 = 0x01;
    pub const TITLE: u32 = 0x02;
    pub const SPRITE_NAME: u32 = 0x0B;
    pub const LEVEL: u32 = 0x1F;
    pub const EXP: u32 = 0x20;
    pub const HP_MOD: u32 = 0x21;
    pub const SP_MOD: u32 = 0x22;
    pub const ATTACK_MOD: u32 = 0x29;
    pub const DEFENSE_MOD: u32 = 0x2A;
    pub const SPIRIT_MOD: u32 = 0x2B;
    pub const AGILITY_MOD: u32 = 0x2C;
    pub const SKILL_COUNT: u32 = 0x33;
    pub const SKILLS: u32 = 0x34;
    pub const EQUIPPED: u32 = 0x3D;
    pub const CURRENT_HP: u32 = 0x47;
    pub const CURRENT_SP: u32 = 0x48;
    pub const CLASS_ID: u32 = 0x5A;
}

/// liblcf's marker for "this name has not been changed from the database".
pub const UNCHANGED_NAME: &[u8] = b"\x01";

/// The signature every save file starts with.
pub const SIGNATURE: &str = "LcfSaveData";

/// Frames per second the engines count play time in.
pub const FRAME_RATE: i64 = 60;

/// The most gold these engines will hold.
pub const MAX_GOLD: i64 = 999_999;

/// How many pieces of one item the party may hold.
pub const MAX_ITEM_COUNT: i64 = 99;

/// Equipment slots, in the order `SaveActor::equipped` stores them.
pub const EQUIP_SLOTS: [&str; 5] = ["Weapon", "Shield", "Armour", "Helmet", "Accessory"];

#[derive(Debug)]
pub enum OpenError {
    Io(std::io::Error),
    NotASave { found: String },
    Malformed(Truncated),
}

impl std::fmt::Display for OpenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OpenError::Io(e) => write!(f, "{e}"),
            OpenError::NotASave { found } => write!(
                f,
                "This is not an RPG Maker 2000/2003 save: it begins {found:?} rather than \
                 {SIGNATURE:?}."
            ),
            OpenError::Malformed(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for OpenError {}

/// A decoded `.lsd` save.
pub struct LcfSave {
    pub path: PathBuf,
    pub size_bytes: usize,
    pub dirty: bool,
    pub notes: Vec<String>,
    signature: String,
    chunks: Chunks,
}

impl LcfSave {
    pub fn open(path: &Path) -> Result<LcfSave, OpenError> {
        let bytes = std::fs::read(path).map_err(OpenError::Io)?;
        let mut save = LcfSave::from_bytes(&bytes)?;
        save.path = path.to_path_buf();
        Ok(save)
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<LcfSave, OpenError> {
        let length = *bytes.first().ok_or(OpenError::NotASave { found: String::new() })? as usize;
        let signature = bytes
            .get(1..1 + length)
            .map(|s| String::from_utf8_lossy(s).into_owned())
            .unwrap_or_default();
        if signature != SIGNATURE {
            return Err(OpenError::NotASave { found: signature });
        }

        let chunks =
            Chunks::parse_to_end(&bytes[1 + length..]).map_err(OpenError::Malformed)?;
        Ok(LcfSave {
            path: PathBuf::new(),
            size_bytes: bytes.len(),
            dirty: false,
            notes: Vec::new(),
            signature,
            chunks,
        })
    }

    /// True when the data begins with the save signature.
    pub fn looks_like_save(bytes: &[u8]) -> bool {
        let length = bytes.first().copied().unwrap_or(0) as usize;
        bytes.get(1..1 + length).is_some_and(|s| s == SIGNATURE.as_bytes())
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = vec![self.signature.len() as u8];
        out.extend_from_slice(self.signature.as_bytes());
        // The outermost stream has no terminator.
        out.extend_from_slice(&self.chunks.to_bytes(false));
        out
    }

    // ---- sub-structures ---------------------------------------------------

    /// The raw payload of a top-level chunk.
    pub fn chunk(&self, tag: u32) -> Option<&[u8]> {
        self.chunks.get(tag)
    }

    /// Edits the fields of a top-level structure.
    pub fn update_struct(&mut self, tag: u32, edit: impl FnOnce(&mut Chunks)) {
        self.update_sub(tag, edit);
    }

    /// Edits the fields of one entry of a top-level array.
    pub fn update_array_entry(
        &mut self,
        tag: u32,
        at: usize,
        edit: impl FnOnce(&mut Chunks),
    ) -> bool {
        let Some(mut array) = self.chunk(tag).and_then(|d| Array::parse(d).ok()) else {
            return false;
        };
        let Some((_, chunks)) = array.entries.get_mut(at) else { return false };
        edit(chunks);
        self.chunks.set(tag, array.to_bytes());
        self.dirty = true;
        true
    }

    fn sub(&self, tag: u32) -> Chunks {
        self.chunks
            .get(tag)
            .map(|data| {
                let mut pos = 0;
                Chunks::parse_stream(data, &mut pos).unwrap_or_default()
            })
            .unwrap_or_default()
    }

    fn update_sub(&mut self, tag: u32, edit: impl FnOnce(&mut Chunks)) {
        let mut chunks = self.sub(tag);
        edit(&mut chunks);
        self.chunks.set(tag, chunks.to_bytes(true));
        self.dirty = true;
    }

    fn actors_array(&self) -> Array {
        self.chunks
            .get(top::ACTORS)
            .and_then(|d| Array::parse(d).ok())
            .unwrap_or_default()
    }

    // ---- party ------------------------------------------------------------

    pub fn gold(&self) -> i64 {
        i64::from(self.sub(top::INVENTORY).int(inventory::GOLD).unwrap_or(0))
    }

    pub fn set_gold(&mut self, gold: i64) {
        let gold = gold.clamp(0, MAX_GOLD) as u32;
        self.update_sub(top::INVENTORY, |inv| inv.set_int(inventory::GOLD, gold));
    }

    pub fn steps(&self) -> i64 {
        i64::from(self.sub(top::INVENTORY).int(inventory::STEPS).unwrap_or(0))
    }

    pub fn set_steps(&mut self, steps: i64) {
        let steps = steps.max(0) as u32;
        self.update_sub(top::INVENTORY, |inv| inv.set_int(inventory::STEPS, steps));
    }

    pub fn party_member_ids(&self) -> Vec<i64> {
        self.sub(top::INVENTORY)
            .vec_i16(inventory::PARTY)
            .into_iter()
            .map(i64::from)
            .collect()
    }

    pub fn set_party_member_ids(&mut self, ids: &[i64]) {
        let members: Vec<i16> = ids.iter().map(|id| *id as i16).collect();
        self.update_sub(top::INVENTORY, |inv| {
            // The count is its own chunk and has to follow the data.
            inv.set_int(inventory::PARTY_COUNT, members.len() as u32);
            inv.set_vec_i16(inventory::PARTY, &members);
        });
    }

    // ---- inventory --------------------------------------------------------

    /// `(item id, how many)` for everything the party is carrying.
    pub fn items(&self) -> Vec<(i64, i64)> {
        let inv = self.sub(top::INVENTORY);
        let ids = inv.vec_i16(inventory::ITEM_IDS);
        let counts = inv.vec_u8(inventory::ITEM_COUNTS);
        ids.into_iter()
            .enumerate()
            .map(|(i, id)| (i64::from(id), counts.get(i).copied().unwrap_or(0) as i64))
            .collect()
    }

    /// Sets how many of an item the party holds. Zero removes it, as the
    /// engine itself does.
    pub fn set_item_count(&mut self, item_id: i64, count: i64) {
        let count = count.clamp(0, MAX_ITEM_COUNT);
        self.update_sub(top::INVENTORY, |inv| {
            let mut ids = inv.vec_i16(inventory::ITEM_IDS);
            let mut counts = inv.vec_u8(inventory::ITEM_COUNTS);
            let mut usage = inv.vec_u8(inventory::ITEM_USAGE);
            counts.resize(ids.len(), 0);
            usage.resize(ids.len(), 0);

            match ids.iter().position(|id| i64::from(*id) == item_id) {
                Some(at) if count == 0 => {
                    ids.remove(at);
                    counts.remove(at);
                    usage.remove(at);
                }
                Some(at) => counts[at] = count as u8,
                None if count == 0 => {}
                None => {
                    // The engine keeps this list in item order.
                    let at = ids
                        .iter()
                        .position(|id| i64::from(*id) > item_id)
                        .unwrap_or(ids.len());
                    ids.insert(at, item_id as i16);
                    counts.insert(at, count as u8);
                    usage.insert(at, 0);
                }
            }

            inv.set_int(inventory::ITEM_COUNT, ids.len() as u32);
            inv.set_vec_i16(inventory::ITEM_IDS, &ids);
            inv.set_vec_u8(inventory::ITEM_COUNTS, &counts);
            inv.set_vec_u8(inventory::ITEM_USAGE, &usage);
        });
    }

    // ---- switches and variables -------------------------------------------

    pub fn switch_count(&self) -> usize {
        self.sub(top::SYSTEM).vec_u8(system::SWITCHES).len()
    }

    pub fn switch(&self, id: i64) -> bool {
        let index = match usize::try_from(id - 1) {
            Ok(i) => i,
            Err(_) => return false,
        };
        self.sub(top::SYSTEM)
            .vec_u8(system::SWITCHES)
            .get(index)
            .is_some_and(|v| *v != 0)
    }

    pub fn set_switch(&mut self, id: i64, on: bool) -> bool {
        let Ok(index) = usize::try_from(id - 1) else { return false };
        self.update_sub(top::SYSTEM, |sys| {
            let mut switches = sys.vec_u8(system::SWITCHES);
            if index >= switches.len() {
                switches.resize(index + 1, 0);
            }
            switches[index] = u8::from(on);
            sys.set_int(system::SWITCH_COUNT, switches.len() as u32);
            sys.set_vec_u8(system::SWITCHES, &switches);
        });
        true
    }

    pub fn variable_count(&self) -> usize {
        self.sub(top::SYSTEM).vec_i32(system::VARIABLES).len()
    }

    pub fn variable(&self, id: i64) -> i64 {
        let Ok(index) = usize::try_from(id - 1) else { return 0 };
        self.sub(top::SYSTEM)
            .vec_i32(system::VARIABLES)
            .get(index)
            .map(|v| i64::from(*v))
            .unwrap_or(0)
    }

    pub fn set_variable(&mut self, id: i64, value: i64) -> bool {
        let Ok(index) = usize::try_from(id - 1) else { return false };
        let value = value.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32;
        self.update_sub(top::SYSTEM, |sys| {
            let mut variables = sys.vec_i32(system::VARIABLES);
            if index >= variables.len() {
                variables.resize(index + 1, 0);
            }
            variables[index] = value;
            sys.set_int(system::VARIABLE_COUNT, variables.len() as u32);
            sys.set_vec_i32(system::VARIABLES, &variables);
        });
        true
    }

    // ---- play time and location -------------------------------------------

    pub fn playtime_frames(&self) -> i64 {
        i64::from(self.sub(top::SYSTEM).int(system::FRAME_COUNT).unwrap_or(0))
    }

    pub fn set_playtime_seconds(&mut self, seconds: i64) {
        let frames = (seconds.max(0) * FRAME_RATE) as u32;
        self.update_sub(top::SYSTEM, |sys| sys.set_int(system::FRAME_COUNT, frames));
    }

    pub fn save_count(&self) -> Option<i64> {
        self.sub(top::SYSTEM).int(system::SAVE_COUNT).map(i64::from)
    }

    pub fn map_id(&self) -> i64 {
        i64::from(self.sub(top::PARTY_LOCATION).int(location::MAP_ID).unwrap_or(0))
    }

    pub fn player_position(&self) -> (i64, i64) {
        let place = self.sub(top::PARTY_LOCATION);
        (
            i64::from(place.int(location::X).unwrap_or(0)),
            i64::from(place.int(location::Y).unwrap_or(0)),
        )
    }

    pub fn set_player_position(&mut self, x: i64, y: i64) {
        self.update_sub(top::PARTY_LOCATION, |place| {
            place.set_int(location::X, x.max(0) as u32);
            place.set_int(location::Y, y.max(0) as u32);
        });
    }

    pub fn set_map_id(&mut self, map_id: i64) {
        self.update_sub(top::PARTY_LOCATION, |place| {
            place.set_int(location::MAP_ID, map_id.max(0) as u32)
        });
    }

    // ---- actors -----------------------------------------------------------

    pub fn actor_ids(&self) -> Vec<i64> {
        self.actors_array().entries.iter().map(|(id, _)| i64::from(*id)).collect()
    }

    pub fn actor(&self, id: i64) -> Option<Chunks> {
        self.actors_array().get(id as u32).cloned()
    }

    pub fn update_actor(&mut self, id: i64, edit: impl FnOnce(&mut Chunks)) -> bool {
        let mut array = self.actors_array();
        let Some(chunks) = array.get_mut(id as u32) else { return false };
        edit(chunks);
        self.chunks.set(top::ACTORS, array.to_bytes());
        self.dirty = true;
        true
    }

    /// An actor's name, or `None` when it is still whatever the database says.
    pub fn actor_name(&self, id: i64) -> Option<String> {
        let name = self.actor(id)?.string(actor::NAME)?.to_vec();
        if name == UNCHANGED_NAME {
            return None;
        }
        Some(crate::marshal::decode_ruby_string(&name))
    }

    pub fn set_actor_name(&mut self, id: i64, name: &str) -> bool {
        let bytes = name.as_bytes().to_vec();
        self.update_actor(id, |chunks| chunks.set(actor::NAME, bytes))
    }

    pub fn actor_int(&self, id: i64, tag: u32) -> Option<i64> {
        self.actor(id)?.int(tag).map(i64::from)
    }

    pub fn set_actor_int(&mut self, id: i64, tag: u32, value: i64) -> bool {
        let value = value.clamp(0, i64::from(u32::MAX)) as u32;
        self.update_actor(id, |chunks| chunks.set_int(tag, value))
    }

    pub fn actor_equipment(&self, id: i64) -> Vec<i64> {
        self.actor(id)
            .map(|c| c.vec_i16(actor::EQUIPPED).into_iter().map(i64::from).collect())
            .unwrap_or_default()
    }

    pub fn set_actor_equipment(&mut self, id: i64, slot: usize, item_id: i64) -> bool {
        self.update_actor(id, |chunks| {
            let mut equipped = chunks.vec_i16(actor::EQUIPPED);
            if equipped.len() < EQUIP_SLOTS.len() {
                equipped.resize(EQUIP_SLOTS.len(), 0);
            }
            if let Some(entry) = equipped.get_mut(slot) {
                *entry = item_id as i16;
            }
            chunks.set_vec_i16(actor::EQUIPPED, &equipped);
        })
    }

    pub fn actor_skills(&self, id: i64) -> Vec<i64> {
        self.actor(id)
            .map(|c| c.vec_i16(actor::SKILLS).into_iter().map(i64::from).collect())
            .unwrap_or_default()
    }

    pub fn set_actor_skills(&mut self, id: i64, skills: &[i64]) -> bool {
        let values: Vec<i16> = skills.iter().map(|s| *s as i16).collect();
        self.update_actor(id, |chunks| {
            chunks.set_int(actor::SKILL_COUNT, values.len() as u32);
            chunks.set_vec_i16(actor::SKILLS, &values);
        })
    }

    // ---- the save-menu header ---------------------------------------------

    /// The name shown on the file-select screen.
    pub fn title_hero_name(&self) -> Option<String> {
        let name = self.sub(top::TITLE).string(0x0B)?.to_vec();
        Some(crate::marshal::decode_ruby_string(&name))
    }

    /// Brings the file-select header in step with the party, the way the
    /// engine rewrites it on every save.
    pub fn refresh_title(&mut self) {
        let Some(&leader) = self.party_member_ids().first() else { return };
        let name = self.actor_name(leader);
        let level = self.actor_int(leader, actor::LEVEL);
        let hp = self.actor_int(leader, actor::CURRENT_HP);

        self.update_sub(top::TITLE, |title| {
            if let Some(name) = name {
                title.set(0x0B, name.into_bytes());
            }
            if let Some(level) = level {
                title.set_int(0x0C, level as u32);
            }
            if let Some(hp) = hp {
                title.set_int(0x0D, hp as u32);
            }
        });
    }
}
