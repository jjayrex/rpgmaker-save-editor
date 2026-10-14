//! Reading, editing and writing RPG Maker save games.
//!
//! Three formats, one editor. [`marshal`] is a faithful Ruby `Marshal` 4.8
//! codec and [`rpg`] layers the RGSS save-file structure (XP, VX and VX Ace)
//! on top of it; [`lcf`] reads the chunk trees RPG Maker 2000 and 2003 write;
//! [`mv`] reads the compressed JSON of RPG Maker MV.

// `as_chunks`, which clippy suggests here, is only stable from Rust 1.88;
// `chunks_exact` keeps the crate buildable on older toolchains.
#![allow(clippy::chunks_exact_to_as_chunks)]

pub mod lcf;
pub mod marshal;
pub mod mv;
pub mod rpg;

pub use lcf::LcfSave;
pub use marshal::{Heap, MarshalError, Node, NodeId, NodeKind, SymId, Value};
pub use rpg::{Engine, GameData, ItemKind, SaveFile};
