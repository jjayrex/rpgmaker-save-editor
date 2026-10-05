//! RPG Maker 2000 and 2003 save files.
//!
//! These engines predate RGSS and store saves in the LCF format: a signature
//! followed by nested chunks, rather than a Ruby object graph. Chunk numbers
//! follow liblcf, EasyRPG's reference implementation.

pub mod chunk;
pub mod save;

pub use chunk::{Array, Chunk, Chunks};
pub use save::{LcfSave, OpenError as LcfOpenError};
