//! What the editor has open, and the helpers commands share.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use rpgsave::marshal::{Heap, NodeKind, Value};
use rpgsave::rpg::{Engine, GameData, SaveFile};
use rpgsave_protocol::Scalar;

#[derive(Default)]
pub struct Editor {
    pub save: Option<SaveFile>,
    pub data: Option<GameData>,
}

pub type SharedEditor = Mutex<Editor>;

impl Editor {
    pub fn save(&self) -> Result<&SaveFile, String> {
        self.save.as_ref().ok_or_else(|| "No save file is open.".to_owned())
    }

    pub fn save_mut(&mut self) -> Result<&mut SaveFile, String> {
        self.save.as_mut().ok_or_else(|| "No save file is open.".to_owned())
    }

    /// The open save plus the database, which commands usually need together.
    pub fn both(&mut self) -> Result<(&mut SaveFile, Option<&GameData>), String> {
        let data = self.data.as_ref();
        let save = self.save.as_mut().ok_or_else(|| "No save file is open.".to_owned())?;
        Ok((save, data))
    }

    pub fn data(&self) -> Option<&GameData> {
        self.data.as_ref()
    }

    pub fn open(&mut self, path: &Path) -> Result<(), String> {
        let save = SaveFile::open(path).map_err(|e| e.to_string())?;
        self.data = save.guess_data_dir().map(|dir| GameData::load(&dir, save.engine));
        self.save = Some(save);
        Ok(())
    }

    pub fn load_data_dir(&mut self, dir: &Path) -> Result<(), String> {
        let engine = self.save()?.engine;
        let data = GameData::load(dir, engine);
        if data.is_empty() {
            return Err(format!(
                "No {} database files were found in {}.",
                engine.label(),
                dir.display()
            ));
        }
        self.data = Some(data);
        Ok(())
    }

    /// Looks up an actor by id, or explains that it is not in the save.
    pub fn actor(&self, actor_id: i64) -> Result<Value, String> {
        self.save()?
            .actor(actor_id)
            .ok_or_else(|| format!("Actor {actor_id} is not stored in this save."))
    }

    /// Fails early when an actor id does not exist, so that the fan-out to
    /// every stored copy of it can report "no such field" instead.
    pub fn require_actor(&self, actor_id: i64) -> Result<(), String> {
        self.actor(actor_id).map(|_| ())
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
/// convention: RGSS3 tags every string with its encoding, RGSS2 does not.
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
