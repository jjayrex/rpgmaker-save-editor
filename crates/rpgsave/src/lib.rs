//! Reading, editing and writing RPG Maker save games.
//!
//! [`marshal`] is a faithful Ruby `Marshal` 4.8 codec; [`rpg`] layers the
//! RPG Maker save-file structure (VX and VX Ace) on top of it.

// `as_chunks`, which clippy suggests here, is only stable from Rust 1.88;
// `chunks_exact` keeps the crate buildable on older toolchains.
#![allow(clippy::chunks_exact_to_as_chunks)]

pub mod marshal;
pub mod rpg;

pub use marshal::{Heap, MarshalError, Node, NodeId, NodeKind, SymId, Value};
pub use rpg::{Engine, GameData, ItemKind, SaveFile};
