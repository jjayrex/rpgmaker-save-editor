//! Ruby `Marshal` 4.8 codec.
//!
//! RPG Maker XP, VX and VX Ace store save games as one or more back-to-back
//! `Marshal.dump` documents, so reading and writing them faithfully — object
//! links, symbol links and all — is the whole foundation of this editor.

pub mod error;
pub mod reader;
pub mod value;
pub mod writer;

pub use error::{MarshalError, Result};
pub use reader::{load, load_prefix, load_stream};
pub use value::{decode_ruby_string, Heap, Ivars, Node, NodeId, NodeKind, SymId, Value};
pub use writer::{dump, dump_stream};
