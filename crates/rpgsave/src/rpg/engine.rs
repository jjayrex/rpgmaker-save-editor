//! Which RPG Maker generation a file came from.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::marshal::{Heap, Value};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Engine {
    /// RPG Maker VX Ace (RGSS3), `SaveN.rvdata2`.
    VxAce,
    /// RPG Maker VX (RGSS2), `SaveN.rvdata`.
    Vx,
}

impl Engine {
    /// Extension of this engine's save files, without the dot.
    pub fn save_extension(self) -> &'static str {
        match self {
            Engine::VxAce => "rvdata2",
            Engine::Vx => "rvdata",
        }
    }

    /// Extension of this engine's `Data/` files, without the dot.
    pub fn data_extension(self) -> &'static str {
        self.save_extension()
    }

    pub fn label(self) -> &'static str {
        match self {
            Engine::VxAce => "VX Ace",
            Engine::Vx => "VX",
        }
    }

    pub fn id(self) -> &'static str {
        match self {
            Engine::VxAce => "vxace",
            Engine::Vx => "vx",
        }
    }

    /// Guess from a file name. `None` for extensions we do not handle.
    pub fn from_path(path: &Path) -> Option<Engine> {
        match path
            .extension()
            .and_then(|e| e.to_str())
            .map(str::to_ascii_lowercase)
            .as_deref()
        {
            Some("rvdata2") => Some(Engine::VxAce),
            Some("rvdata") => Some(Engine::Vx),
            _ => None,
        }
    }

    /// Decide from the shape of the decoded documents. VX Ace writes a header
    /// hash followed by a contents hash; VX writes a run of separate objects.
    pub fn from_documents(heap: &Heap, documents: &[Value]) -> Option<Engine> {
        if documents.len() == 2 && heap.hash_entries(documents[1]).is_some() {
            return Some(Engine::VxAce);
        }
        if documents
            .iter()
            .any(|d| heap.class_name(*d) == Some("Game_System"))
        {
            return Some(Engine::Vx);
        }
        None
    }
}

/// Why a file could not be opened as a VX / VX Ace save.
#[derive(Debug, Clone)]
pub enum UnsupportedReason {
    /// An RPG Maker XP save; the format is close but not handled yet.
    RpgMakerXp,
    /// Decoded, but nothing in it looks like a save game.
    NotASave,
}

impl UnsupportedReason {
    pub fn message(&self) -> &'static str {
        match self {
            UnsupportedReason::RpgMakerXp => {
                "This is an RPG Maker XP save (.rxdata). Only VX and VX Ace are supported so far."
            }
            UnsupportedReason::NotASave => {
                "This file is valid Ruby Marshal data, but does not contain a Game_System or \
                 save contents hash, so it is not an RPG Maker save game."
            }
        }
    }
}
