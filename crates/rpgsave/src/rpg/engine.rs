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
    /// RPG Maker XP (RGSS1), `SaveN.rxdata`.
    Xp,
}

impl Engine {
    pub const ALL: [Engine; 3] = [Engine::VxAce, Engine::Vx, Engine::Xp];

    /// Extension of this engine's save files, without the dot.
    pub fn save_extension(self) -> &'static str {
        match self {
            Engine::VxAce => "rvdata2",
            Engine::Vx => "rvdata",
            Engine::Xp => "rxdata",
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
            Engine::Xp => "XP",
        }
    }

    pub fn id(self) -> &'static str {
        match self {
            Engine::VxAce => "vxace",
            Engine::Vx => "vx",
            Engine::Xp => "xp",
        }
    }

    /// Frames per second the engine runs at, which is what turns the stored
    /// frame count into a play time. RGSS1 ran at 40; RGSS2 and RGSS3 at 60.
    pub fn frame_rate(self) -> i64 {
        match self {
            Engine::VxAce | Engine::Vx => 60,
            Engine::Xp => 40,
        }
    }

    /// How `@real_x` / `@real_y` relate to a tile coordinate.
    ///
    /// RGSS3 counts in whole tiles as a float; the older engines count in
    /// fractions of a tile as an integer.
    pub fn tile_subdivisions(self) -> Option<i64> {
        match self {
            Engine::VxAce => None,
            Engine::Vx => Some(256),
            Engine::Xp => Some(128),
        }
    }

    /// The instance variable holding an actor's secondary pool. XP calls it
    /// spirit points, and named the field to match.
    pub fn mp_ivar(self) -> &'static str {
        match self {
            Engine::VxAce | Engine::Vx => "@mp",
            Engine::Xp => "@sp",
        }
    }

    /// What to call that pool in the interface.
    pub fn mp_label(self) -> &'static str {
        match self {
            Engine::VxAce | Engine::Vx => "MP",
            Engine::Xp => "SP",
        }
    }

    /// Whether this engine's Ruby tags strings with their encoding.
    ///
    /// RGSS3 runs Ruby 1.9, where every String carries an `:E` instance
    /// variable; RGSS1 and RGSS2 run Ruby 1.8, where strings are plain bytes.
    pub fn strings_carry_encoding(self) -> bool {
        matches!(self, Engine::VxAce)
    }

    /// XP stores the party as `Game_Actor` objects rather than actor ids, so it
    /// can only hold actors the game has already created.
    pub fn party_holds_objects(self) -> bool {
        matches!(self, Engine::Xp)
    }

    /// Guess from a file name. `None` for extensions we do not handle.
    pub fn from_path(path: &Path) -> Option<Engine> {
        let extension = path.extension()?.to_str()?.to_ascii_lowercase();
        Engine::ALL
            .into_iter()
            .find(|engine| engine.save_extension() == extension)
    }

    /// Decide from the shape of the decoded documents.
    ///
    /// VX Ace writes a header hash followed by a contents hash. The two older
    /// engines write a run of separate objects, and differ in which ones: XP
    /// saves `Game_Screen` at the top level, while VX moved the screen inside
    /// `Game_Map` and added `Game_Message`.
    pub fn from_documents(heap: &Heap, documents: &[Value]) -> Option<Engine> {
        let has = |class: &str| {
            documents
                .iter()
                .any(|d| heap.class_name(*d) == Some(class))
        };

        // Only VX Ace collects the game objects into a hash.
        let has_contents_hash = documents.iter().any(|d| {
            heap.hash_entries(*d)
                .is_some_and(|entries| super::save::looks_like_contents(heap, entries))
        });
        if has_contents_hash {
            return Some(Engine::VxAce);
        }
        if has("Game_Screen") {
            return Some(Engine::Xp);
        }
        if has("Game_System") {
            return Some(Engine::Vx);
        }
        None
    }
}

/// Why a file could not be opened as a save.
#[derive(Debug, Clone)]
pub enum UnsupportedReason {
    /// Decoded, but nothing in it looks like a save game.
    NotASave,
}

impl UnsupportedReason {
    pub fn message(&self) -> &'static str {
        match self {
            UnsupportedReason::NotASave => {
                "This file is valid Ruby Marshal data, but does not contain a Game_System or \
                 save contents hash, so it is not an RPG Maker save game."
            }
        }
    }
}
