//! What the editor has open, and the helpers commands share.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use rpgsave::lcf::LcfSave;
use rpgsave::marshal::{Heap, NodeKind, Value};
use rpgsave::rpg::{Engine, GameData, SaveFile};
use rpgsave_protocol::Scalar;

use crate::backend::Backend;

#[derive(Default)]
pub struct Editor {
    pub save: Option<Box<dyn Backend>>,
    pub data: Option<GameData>,
}

pub type SharedEditor = Mutex<Editor>;

impl Editor {
    pub fn save(&self) -> Result<&dyn Backend, String> {
        self.save
            .as_deref()
            .ok_or_else(|| "No save file is open.".to_owned())
    }

    pub fn save_mut(&mut self) -> Result<&mut dyn Backend, String> {
        match self.save.as_deref_mut() {
            Some(save) => Ok(save),
            None => Err("No save file is open.".to_owned()),
        }
    }

    /// The open save plus the database, which commands usually need together.
    pub fn both(&mut self) -> Result<(&mut dyn Backend, Option<&GameData>), String> {
        let data = self.data.as_ref();
        let save = self
            .save
            .as_deref_mut()
            .ok_or_else(|| "No save file is open.".to_owned())?;
        Ok((save, data))
    }

    pub fn data(&self) -> Option<&GameData> {
        self.data.as_ref()
    }

    /// Opens whichever format the file turns out to be.
    ///
    /// RPG Maker 2000 and 2003 saves announce themselves in their first bytes;
    /// everything else is a Ruby Marshal stream from one of the RGSS engines.
    pub fn open(&mut self, path: &Path) -> Result<(), String> {
        let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;

        let save: Box<dyn Backend> = if LcfSave::looks_like_save(&bytes) {
            let mut save = LcfSave::from_bytes(&bytes).map_err(|e| e.to_string())?;
            save.path = path.to_path_buf();
            Box::new(save)
        } else {
            Box::new(SaveFile::open(path).map_err(|e| e.to_string())?)
        };

        // Whichever engine it is, look beside the save for the database that
        // turns ids into names.
        self.data = save.guess_data_dir().and_then(|dir| save.load_game_data(&dir));
        self.save = Some(save);
        Ok(())
    }

    pub fn load_data_dir(&mut self, dir: &Path) -> Result<(), String> {
        let save = self.save()?;
        let data = save.load_game_data(dir).ok_or_else(|| {
            format!("No database files this editor understands were found in {}.", dir.display())
        })?;
        self.data = Some(data);
        Ok(())
    }

    /// Fails early when an actor id does not exist, so that the fan-out to
    /// every stored copy of it can report "no such field" instead.
    pub fn require_actor(&self, actor_id: i64) -> Result<(), String> {
        if self.save()?.has_actor(actor_id) {
            Ok(())
        } else {
            Err(format!("Actor {actor_id} is not stored in this save."))
        }
    }
}

/// Converts a Ruby value into something the UI can put in an input box.
/// Composite values other than strings have no scalar form.
pub fn scalar_of(heap: &Heap, value: Value) -> Option<Scalar> {
    match value {
        Value::Nil => Some(Scalar::Nil),
        Value::Bool(b) => Some(Scalar::Bool(b)),
        Value::Int(i) => Some(Scalar::Int(i)),
        Value::Sym(s) => Some(Scalar::Sym(heap.sym(s).to_owned())),
        Value::Ref(id) => match heap.kind(id) {
            NodeKind::Float { value, .. } => Some(Scalar::Float(*value)),
            NodeKind::Str(bytes) => Some(Scalar::Str(rpgsave::marshal::decode_ruby_string(bytes))),
            _ => None,
        },
    }
}

/// Builds a Ruby value from an edited scalar. Strings follow the engine's
/// convention: RGSS3 tags every string with its encoding, RGSS1 and RGSS2 do
/// not.
pub fn value_of(heap: &mut Heap, engine: Engine, scalar: &Scalar) -> Value {
    match scalar {
        Scalar::Nil => Value::Nil,
        Scalar::Bool(b) => Value::Bool(*b),
        Scalar::Int(i) => Value::Int(*i),
        Scalar::Float(f) => heap.new_float(*f),
        Scalar::Sym(s) => heap.new_sym(s),
        Scalar::Str(s) => rpgsave::rpg::save::new_engine_string(heap, engine, s),
    }
}

/// Replaces a string in place when the old value was one, so that any encoding
/// instance variable is preserved; otherwise builds a fresh value.
pub fn assign_scalar(
    heap: &mut Heap,
    engine: Engine,
    previous: Option<Value>,
    scalar: &Scalar,
) -> Value {
    if let (Scalar::Str(text), Some(id)) = (scalar, previous.and_then(|v| v.as_ref()))
        && matches!(heap.kind(id), NodeKind::Str(_))
    {
        let ivars = heap.node(id).ivars.clone();
        let new = heap.new_str(text);
        if let Some(new_id) = new.as_ref() {
            heap.node_mut(new_id).ivars = ivars;
        }
        return new;
    }
    value_of(heap, engine, scalar)
}

pub fn to_path(path: &str) -> PathBuf {
    PathBuf::from(path)
}
