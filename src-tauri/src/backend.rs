//! The operations the interface performs on an open save, whichever engine
//! wrote it.
//!
//! The RGSS engines and RPG Maker 2000/2003 share nothing below this line —
//! one stores a Ruby object graph, the other a tree of binary chunks — so the
//! command layer talks to them through this trait and never learns which it
//! has.

use std::path::{Path, PathBuf};

use rpgsave::rpg::GameData;
use rpgsave_protocol::*;

pub type Reply<T> = Result<T, String>;

pub trait Backend: Send {
    // ---- the file ---------------------------------------------------------
    fn path(&self) -> &Path;
    fn set_path(&mut self, path: PathBuf);
    fn is_dirty(&self) -> bool;
    fn mark_saved(&mut self);
    fn encode(&mut self) -> Vec<u8>;
    fn byte_size(&self) -> usize;
    fn set_byte_size(&mut self, size: usize);
    /// Where this engine keeps the database that gives ids their names.
    fn guess_data_dir(&self) -> Option<PathBuf>;
    fn load_game_data(&self, dir: &Path) -> Option<GameData>;

    // ---- what the interface displays --------------------------------------
    fn summary(&self, data: Option<&GameData>) -> Summary;
    fn actor_view(&self, data: Option<&GameData>, actor_id: i64) -> Option<ActorView>;
    fn inventory(&self, data: Option<&GameData>, kind: &str) -> Reply<InventoryView>;
    fn catalog(&self, data: Option<&GameData>, kind: &str, query: &str) -> Vec<CatalogEntry>;
    fn switch_page(
        &self,
        data: Option<&GameData>,
        query: &str,
        offset: usize,
        limit: usize,
    ) -> SwitchPage;
    fn variable_page(
        &self,
        data: Option<&GameData>,
        query: &str,
        offset: usize,
        limit: usize,
    ) -> VariablePage;
    fn self_switches(&self, data: Option<&GameData>) -> Vec<SelfSwitchRow>;
    fn raw_root(&self) -> NodeView;
    fn raw_node(&self, node: u32) -> Option<NodeView>;
    fn set_raw_scalar(&mut self, node: Option<u32>, slot: &Slot, value: &Scalar) -> Reply<()>;

    // ---- what the interface changes ---------------------------------------
    fn set_gold(&mut self, value: i64) -> bool;
    fn set_steps(&mut self, value: i64) -> bool;
    fn set_playtime_seconds(&mut self, seconds: i64) -> bool;
    fn set_position(&mut self, x: i64, y: i64) -> bool;
    fn set_party(&mut self, ids: &[i64]) -> Reply<()>;

    fn set_actor_text(&mut self, actor_id: i64, field: &str, text: &str) -> bool;
    fn set_actor_int(&mut self, actor_id: i64, field: &str, value: i64) -> bool;
    fn set_actor_scalar(&mut self, actor_id: i64, field: &str, value: &Scalar) -> bool;
    fn set_actor_level(&mut self, actor_id: i64, level: i64, data: Option<&GameData>) -> bool;
    fn set_actor_exp(&mut self, actor_id: i64, exp: i64) -> bool;
    fn set_actor_param(&mut self, actor_id: i64, index: usize, value: i64) -> bool;
    fn set_actor_equip(&mut self, actor_id: i64, slot: usize, kind: &str, item_id: i64) -> bool;
    fn set_actor_skills(&mut self, actor_id: i64, ids: &[i64]) -> bool;
    fn set_actor_states(&mut self, actor_id: i64, ids: &[i64]) -> bool;
    fn full_heal(&mut self, actor_id: i64, data: Option<&GameData>) -> bool;
    fn has_actor(&self, actor_id: i64) -> bool;

    fn set_item_count(&mut self, kind: &str, id: i64, count: i64) -> bool;
    fn set_switch(&mut self, id: i64, on: bool) -> bool;
    fn set_variable(&mut self, id: i64, value: &Scalar) -> bool;
    fn set_self_switch(&mut self, map_id: i64, event_id: i64, letter: &str, on: bool) -> bool;
    fn remove_self_switch(&mut self, map_id: i64, event_id: i64, letter: &str) -> bool;
}
