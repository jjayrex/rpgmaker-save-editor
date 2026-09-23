//! The wire format between the editor's backend and its user interface.
//!
//! Both sides depend on this crate, so a change to a field is a compile error
//! rather than a silently missing value at runtime.

use serde::{Deserialize, Serialize};

/// A single editable value, whatever Ruby type it happens to be.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum Scalar {
    #[default]
    Nil,
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(String),
    Sym(String),
}

impl Scalar {
    pub fn type_name(&self) -> &'static str {
        match self {
            Scalar::Nil => "nil",
            Scalar::Bool(_) => "bool",
            Scalar::Int(_) => "int",
            Scalar::Float(_) => "float",
            Scalar::Str(_) => "string",
            Scalar::Sym(_) => "symbol",
        }
    }
}

/// Where a value sits inside its parent, so the backend can write it back.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Slot {
    /// A top-level Marshal document.
    Document(usize),
    /// An instance variable, by name.
    Ivar(String),
    /// An array element.
    Index(usize),
    /// A hash entry's value, by position in the entry list.
    Entry(usize),
    /// A hash entry's key, by position.
    EntryKey(usize),
    /// A struct field, by name.
    Field(String),
    /// The value a `U` (marshal_dump) object wraps.
    Inner,
    /// A hash's default value.
    Default,
}

// ---------------------------------------------------------------- overview

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Summary {
    pub path: String,
    pub file_name: String,
    pub engine: String,
    pub engine_label: String,
    pub dirty: bool,
    pub file_size: usize,
    pub document_count: usize,
    pub notes: Vec<String>,

    pub game_title: Option<String>,
    pub data_dir: Option<String>,
    pub data_loaded: Vec<String>,
    pub data_missing: Vec<String>,

    pub has_playtime: bool,
    pub playtime_seconds: i64,
    /// Frames per second the engine counts play time in.
    pub frame_rate: i64,
    pub playtime_text: String,
    pub save_count: Option<i64>,

    pub gold: Option<i64>,
    pub currency: String,
    pub steps: Option<i64>,

    pub map_id: Option<i64>,
    pub map_name: Option<String>,
    pub player_x: Option<i64>,
    pub player_y: Option<i64>,

    pub party: Vec<PartyMember>,
    pub roster: Vec<RosterEntry>,

    pub switch_count: usize,
    pub variable_count: usize,
    pub self_switch_count: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PartyMember {
    pub actor_id: i64,
    pub name: String,
    pub level: i64,
    pub class_name: Option<String>,
    pub hp: Option<i64>,
    pub mp: Option<i64>,
}

/// Every actor the editor knows about: those stored in the save, plus any the
/// database defines that the save has not instantiated yet.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct RosterEntry {
    pub actor_id: i64,
    pub name: String,
    pub in_save: bool,
    pub in_party: bool,
}

// ------------------------------------------------------------------ actors

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ActorView {
    pub actor_id: i64,
    pub name: String,
    pub nickname: Option<String>,
    pub class_id: Option<i64>,
    pub class_name: Option<String>,
    pub classes: Vec<NamedId>,
    pub in_party: bool,

    pub level: Option<i64>,
    pub max_level: i64,
    pub exp: Option<i64>,
    pub exp_this_level: Option<i64>,
    pub exp_next_level: Option<i64>,
    /// True when changing the level can also fix up experience.
    pub exp_follows_level: bool,

    pub hp: Option<i64>,
    pub mp: Option<i64>,
    pub tp: Option<f64>,
    pub max_hp: Option<i64>,
    pub max_mp: Option<i64>,
    /// What this engine calls the secondary pool, and where it keeps it.
    pub mp_label: String,
    pub mp_ivar: String,

    pub params: Vec<ParamView>,
    pub equips: Vec<EquipSlot>,
    pub skills: Vec<NamedId>,
    pub states: Vec<NamedId>,
    /// Plain ivars that have no dedicated editor above.
    pub extra: Vec<FieldView>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct NamedId {
    pub id: i64,
    pub name: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ParamView {
    pub index: usize,
    pub label: String,
    pub plus: i64,
    pub base: Option<i64>,
    /// The ivar this maps to, so VX's separate `@atk_plus` fields work too.
    pub ivar: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct EquipSlot {
    pub slot: usize,
    pub label: String,
    /// "weapon" or "armor".
    pub kind: String,
    pub item_id: i64,
    pub item_name: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct FieldView {
    pub ivar: String,
    pub label: String,
    pub value: Scalar,
}

// --------------------------------------------------------------- inventory

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct InventoryView {
    pub kind: String,
    pub rows: Vec<ItemRow>,
    pub max_count: i64,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ItemRow {
    pub id: i64,
    pub count: i64,
    pub name: Option<String>,
    pub icon_index: i64,
    pub description: String,
    pub price: i64,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct CatalogEntry {
    pub id: i64,
    pub name: String,
    pub icon_index: i64,
    pub description: String,
    pub price: i64,
    pub etype_id: i64,
    pub held: i64,
}

// ---------------------------------------------------- switches / variables

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SwitchPage {
    /// Highest id the save has a slot for.
    pub total: usize,
    /// How many ids matched the current filter.
    pub matched: usize,
    pub rows: Vec<SwitchRow>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SwitchRow {
    pub id: i64,
    pub name: Option<String>,
    pub on: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct VariablePage {
    pub total: usize,
    pub matched: usize,
    pub rows: Vec<VariableRow>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct VariableRow {
    pub id: i64,
    pub name: Option<String>,
    pub value: Scalar,
    /// Set when the value is something no simple editor can represent.
    pub complex: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SelfSwitchRow {
    pub map_id: i64,
    pub map_name: Option<String>,
    pub event_id: i64,
    pub letter: String,
    pub on: bool,
}

// -------------------------------------------------------------- raw editor

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct NodeView {
    pub node: Option<u32>,
    pub title: String,
    pub kind: String,
    pub summary: String,
    pub children: Vec<ChildView>,
    /// Set for `Table`, `Color` and other `_dump` payloads we can decode.
    pub binary: Option<BinaryView>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ChildView {
    pub slot: Slot,
    pub key: String,
    pub preview: String,
    /// Present when the child can be expanded.
    pub node: Option<u32>,
    /// Present when the child can be edited in place.
    pub scalar: Option<Scalar>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct BinaryView {
    pub class: String,
    pub description: String,
    pub bytes: usize,
    /// First values of a `Table`, for a quick look at map data.
    pub preview: Vec<i64>,
}

// ----------------------------------------------------------------- results

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct WriteResult {
    pub path: String,
    pub bytes: usize,
    pub backup: Option<String>,
}

/// Returned by every mutating command so the UI can refresh in one round trip.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Applied {
    pub ok: bool,
    pub message: Option<String>,
}

impl Applied {
    pub fn ok() -> Self {
        Applied { ok: true, message: None }
    }

    pub fn note(message: impl Into<String>) -> Self {
        Applied { ok: true, message: Some(message.into()) }
    }
}
