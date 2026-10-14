//! RPG Maker MV save files.
//!
//! MV left Ruby behind: a save is a JSON object, serialised by the engine's
//! own `JsonEx` and then compressed into base64 text. The modules here are
//! that stack from the bottom up — [`lzstring`] unwraps the file, [`json`]
//! parses what is inside without losing a byte of it, and [`save`] gives the
//! result the shape the rest of the editor expects.

pub mod database;
pub mod json;
pub mod lzstring;
pub mod save;

pub use json::Json;
pub use save::{MvSave, OpenError as MvOpenError};

