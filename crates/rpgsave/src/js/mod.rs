//! RPG Maker MV and MZ save files.
//!
//! These two engines left Ruby for JavaScript, and a save is the game's own
//! objects written as JSON by the engine's `JsonEx`. Everything inside is
//! shared between them — the same globals under the same names — so one model
//! serves both; what differs is the wrapper the text arrives in. MV compresses
//! with lz-string into base64 ([`lzstring`]), MZ deflates and writes the
//! result as text ([`zlib`]).
//!
//! [`json`] is the parser that keeps a save byte-identical through a round
//! trip, [`save`] is the model, and [`database`] reads the `data` folder both
//! engines keep their names in.

pub mod database;
pub mod json;
pub mod lzstring;
pub mod save;
pub mod zlib;

pub use json::Json;
pub use save::{Container, JsonSave, OpenError as JsonSaveError};
