//! Arena-backed representation of a Ruby object graph.
//!
//! Ruby's `Marshal` format preserves object identity: the second time an object
//! is emitted it becomes a back-reference (`@`) into a table of everything
//! written so far. To round-trip a save file we have to preserve that sharing,
//! so composite values live in a [`Heap`] and are referred to by [`NodeId`].
//! Immediate values (nil, booleans, fixnums, floats, symbols) are stored inline
//! in [`Value`], exactly as Ruby stores them.

use std::collections::HashMap;

/// Handle to a composite value inside a [`Heap`].
pub type NodeId = u32;

/// Handle to an interned symbol inside a [`Heap`].
pub type SymId = u32;

/// A Ruby value: either an immediate or a reference into the heap.
///
/// The immediates are exactly the ones Ruby 1.8 and 1.9 have. Floats are *not*
/// among them — those Rubies allocate every Float on the heap, and RGSS saves
/// do contain several references to one shared Float — so they live in the
/// heap here too, and keep their identity through a round trip.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Value {
    Nil,
    Bool(bool),
    /// Ruby `Fixnum`/`Integer` small enough for the `i` encoding.
    Int(i64),
    Sym(SymId),
    Ref(NodeId),
}

impl Value {
    pub fn as_int(self) -> Option<i64> {
        match self {
            Value::Int(i) => Some(i),
            _ => None,
        }
    }

    pub fn as_ref(self) -> Option<NodeId> {
        match self {
            Value::Ref(id) => Some(id),
            _ => None,
        }
    }

    pub fn is_nil(self) -> bool {
        matches!(self, Value::Nil)
    }

    /// Ruby truthiness: everything except `nil` and `false`.
    pub fn truthy(self) -> bool {
        !matches!(self, Value::Nil | Value::Bool(false))
    }
}

/// Instance variables, in declaration order (Marshal preserves it).
pub type Ivars = Vec<(SymId, Value)>;

/// A composite Ruby value.
#[derive(Clone, Debug)]
pub struct Node {
    pub kind: NodeKind,
    pub ivars: Ivars,
}

#[derive(Clone, Debug)]
pub enum NodeKind {
    /// `f` — the value, plus the exact text it was read from.
    ///
    /// Every Ruby version has formatted floats differently (1.8 used C's `%g`,
    /// 1.9 switched to shortest-round-trip), so replaying the original text is
    /// the only way to reproduce a file from any engine byte for byte.
    Float { value: f64, source: Option<Box<str>> },
    /// `"` — a Ruby String. Kept as raw bytes: RGSS1/2 saves are not UTF-8.
    Str(Vec<u8>),
    /// `[`
    Array(Vec<Value>),
    /// `{` / `}`
    Hash {
        entries: Vec<(Value, Value)>,
        default: Option<Value>,
    },
    /// `o` — a plain object; state lives entirely in `ivars`.
    Object { class: SymId },
    /// `u` — a class with `_dump`/`_load` (RGSS `Table`, `Color`, `Tone`, `Rect`).
    UserDef { class: SymId, data: Vec<u8> },
    /// `U` — a class with `marshal_dump`/`marshal_load`.
    UserMarshal { class: SymId, value: Value },
    /// `C` — a subclass of String/Array/Hash wrapping a builtin value.
    UserClass { class: SymId, inner: Value },
    /// `S` — a Struct.
    Struct { name: SymId, fields: Vec<(SymId, Value)> },
    /// `l` — arbitrary-precision integer, stored as Ruby stores it.
    Bignum { negative: bool, words: Vec<u16> },
    /// `c`
    Class(String),
    /// `m` / `M`
    Module(String),
    /// `/`
    Regexp { source: Vec<u8>, options: u8 },
    /// `e` — `obj.extend(Module)`; a wrapper around the real value.
    Extended { module: SymId, inner: Value },
}

impl Node {
    pub fn new(kind: NodeKind) -> Self {
        Node { kind, ivars: Vec::new() }
    }
}

impl NodeKind {
    /// Short tag used by the UI to pick an editor for this node.
    pub fn tag(&self) -> &'static str {
        match self {
            NodeKind::Float { .. } => "float",
            NodeKind::Str(_) => "string",
            NodeKind::Array(_) => "array",
            NodeKind::Hash { .. } => "hash",
            NodeKind::Object { .. } => "object",
            NodeKind::UserDef { .. } => "userdef",
            NodeKind::UserMarshal { .. } => "usermarshal",
            NodeKind::UserClass { .. } => "userclass",
            NodeKind::Struct { .. } => "struct",
            NodeKind::Bignum { .. } => "bignum",
            NodeKind::Class(_) => "class",
            NodeKind::Module(_) => "module",
            NodeKind::Regexp { .. } => "regexp",
            NodeKind::Extended { .. } => "extended",
        }
    }
}

/// Arena holding every composite value plus the symbol table.
///
/// One heap holds all the Marshal documents of a single save file, so values
/// can be moved between them freely.
#[derive(Clone, Debug, Default)]
pub struct Heap {
    nodes: Vec<Node>,
    symbols: Vec<String>,
    sym_lookup: HashMap<String, SymId>,
}

impl Heap {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    pub fn alloc(&mut self, node: Node) -> NodeId {
        self.nodes.push(node);
        (self.nodes.len() - 1) as NodeId
    }

    pub fn alloc_kind(&mut self, kind: NodeKind) -> NodeId {
        self.alloc(Node::new(kind))
    }

    pub fn node(&self, id: NodeId) -> &Node {
        &self.nodes[id as usize]
    }

    pub fn node_mut(&mut self, id: NodeId) -> &mut Node {
        &mut self.nodes[id as usize]
    }

    pub fn get(&self, id: NodeId) -> Option<&Node> {
        self.nodes.get(id as usize)
    }

    pub fn kind(&self, id: NodeId) -> &NodeKind {
        &self.nodes[id as usize].kind
    }

    // ---- symbols ----------------------------------------------------------

    pub fn intern(&mut self, name: &str) -> SymId {
        if let Some(&id) = self.sym_lookup.get(name) {
            return id;
        }
        self.symbols.push(name.to_owned());
        let id = (self.symbols.len() - 1) as SymId;
        self.sym_lookup.insert(name.to_owned(), id);
        id
    }

    pub fn sym(&self, id: SymId) -> &str {
        &self.symbols[id as usize]
    }

    /// Symbol id for `name`, if it has ever been interned in this heap.
    pub fn sym_id(&self, name: &str) -> Option<SymId> {
        self.sym_lookup.get(name).copied()
    }

    // ---- convenience constructors ----------------------------------------

    pub fn new_str(&mut self, s: &str) -> Value {
        let id = self.alloc_kind(NodeKind::Str(s.as_bytes().to_vec()));
        Value::Ref(id)
    }

    /// A Ruby 1.9+ string: carries the `:E => true` UTF-8 encoding ivar that
    /// RGSS3 writes for every string.
    pub fn new_utf8_str(&mut self, s: &str) -> Value {
        let e = self.intern("E");
        let id = self.alloc_kind(NodeKind::Str(s.as_bytes().to_vec()));
        self.node_mut(id).ivars.push((e, Value::Bool(true)));
        Value::Ref(id)
    }

    pub fn new_float(&mut self, value: f64) -> Value {
        Value::Ref(self.alloc_kind(NodeKind::Float { value, source: None }))
    }

    /// A float read from a stream, remembering how it was written.
    pub fn new_float_from(&mut self, value: f64, source: &str) -> Value {
        let source = Some(source.into());
        Value::Ref(self.alloc_kind(NodeKind::Float { value, source }))
    }

    pub fn new_array(&mut self, items: Vec<Value>) -> Value {
        Value::Ref(self.alloc_kind(NodeKind::Array(items)))
    }

    pub fn new_sym(&mut self, name: &str) -> Value {
        Value::Sym(self.intern(name))
    }

    // ---- readers ----------------------------------------------------------

    /// Class name of an object-like value (`o`, `u`, `U`, `C`, `S`).
    pub fn class_name(&self, v: Value) -> Option<&str> {
        let id = v.as_ref()?;
        match &self.node(id).kind {
            NodeKind::Object { class }
            | NodeKind::UserDef { class, .. }
            | NodeKind::UserMarshal { class, .. }
            | NodeKind::UserClass { class, .. } => Some(self.sym(*class)),
            NodeKind::Struct { name, .. } => Some(self.sym(*name)),
            NodeKind::Extended { inner, .. } => self.class_name(*inner),
            _ => None,
        }
    }

    /// Follows `e`/`C` wrappers to the value they decorate.
    pub fn unwrap_value(&self, v: Value) -> Value {
        let mut cur = v;
        loop {
            let Some(id) = cur.as_ref() else { return cur };
            match self.node(id).kind {
                NodeKind::Extended { inner, .. } => cur = inner,
                _ => return cur,
            }
        }
    }

    pub fn ivar(&self, v: Value, name: &str) -> Option<Value> {
        let id = self.unwrap_value(v).as_ref()?;
        let sym = self.sym_id(name)?;
        self.node(id)
            .ivars
            .iter()
            .find(|(k, _)| *k == sym)
            .map(|(_, v)| *v)
    }

    pub fn ivar_int(&self, v: Value, name: &str) -> Option<i64> {
        self.ivar(v, name).and_then(Value::as_int)
    }

    /// Numeric value of an Integer or a Float.
    pub fn number(&self, v: Value) -> Option<f64> {
        match v {
            Value::Int(i) => Some(i as f64),
            Value::Ref(id) => match self.node(id).kind {
                NodeKind::Float { value, .. } => Some(value),
                _ => None,
            },
            _ => None,
        }
    }

    pub fn ivar_number(&self, v: Value, name: &str) -> Option<f64> {
        self.ivar(v, name).and_then(|v| self.number(v))
    }

    /// Sets an instance variable, appending it if the object does not have it.
    pub fn set_ivar(&mut self, v: Value, name: &str, new: Value) -> bool {
        let Some(id) = self.unwrap_value(v).as_ref() else { return false };
        let sym = self.intern(name);
        let node = self.node_mut(id);
        match node.ivars.iter_mut().find(|(k, _)| *k == sym) {
            Some(slot) => slot.1 = new,
            None => node.ivars.push((sym, new)),
        }
        true
    }

    pub fn ivar_names(&self, v: Value) -> Vec<String> {
        let Some(id) = self.unwrap_value(v).as_ref() else { return Vec::new() };
        self.node(id)
            .ivars
            .iter()
            .map(|(k, _)| self.sym(*k).to_owned())
            .collect()
    }

    pub fn array(&self, v: Value) -> Option<&[Value]> {
        let id = self.unwrap_value(v).as_ref()?;
        match &self.node(id).kind {
            NodeKind::Array(items) => Some(items),
            NodeKind::UserClass { inner, .. } => self.array(*inner),
            _ => None,
        }
    }

    pub fn array_mut(&mut self, v: Value) -> Option<&mut Vec<Value>> {
        let id = self.unwrap_value(v).as_ref()?;
        let id = match self.node(id).kind {
            NodeKind::UserClass { inner, .. } => inner.as_ref()?,
            _ => id,
        };
        match &mut self.node_mut(id).kind {
            NodeKind::Array(items) => Some(items),
            _ => None,
        }
    }

    pub fn hash_entries(&self, v: Value) -> Option<&[(Value, Value)]> {
        let id = self.unwrap_value(v).as_ref()?;
        match &self.node(id).kind {
            NodeKind::Hash { entries, .. } => Some(entries),
            NodeKind::UserClass { inner, .. } => self.hash_entries(*inner),
            _ => None,
        }
    }

    pub fn hash_entries_mut(&mut self, v: Value) -> Option<&mut Vec<(Value, Value)>> {
        let id = self.unwrap_value(v).as_ref()?;
        let id = match self.node(id).kind {
            NodeKind::UserClass { inner, .. } => inner.as_ref()?,
            _ => id,
        };
        match &mut self.node_mut(id).kind {
            NodeKind::Hash { entries, .. } => Some(entries),
            _ => None,
        }
    }

    /// Structural equality, enough for the key types RGSS uses as hash keys
    /// (integers, symbols, strings and arrays of those).
    pub fn value_eq(&self, a: Value, b: Value) -> bool {
        match (a, b) {
            (Value::Nil, Value::Nil) => true,
            (Value::Bool(x), Value::Bool(y)) => x == y,
            (Value::Int(x), Value::Int(y)) => x == y,
            (Value::Sym(x), Value::Sym(y)) => x == y,
            (Value::Ref(x), Value::Ref(y)) => {
                if x == y {
                    return true;
                }
                match (&self.node(x).kind, &self.node(y).kind) {
                    (
                        NodeKind::Float { value: a, .. },
                        NodeKind::Float { value: b, .. },
                    ) => a == b,
                    (NodeKind::Str(a), NodeKind::Str(b)) => a == b,
                    (NodeKind::Array(a), NodeKind::Array(b)) => {
                        a.len() == b.len()
                            && a.iter().zip(b).all(|(x, y)| self.value_eq(*x, *y))
                    }
                    _ => false,
                }
            }
            _ => false,
        }
    }

    pub fn hash_get(&self, hash: Value, key: Value) -> Option<Value> {
        self.hash_entries(hash)?
            .iter()
            .find(|(k, _)| self.value_eq(*k, key))
            .map(|(_, v)| *v)
    }

    /// Inserts or updates `key`. Returns false if `hash` is not a Hash.
    pub fn hash_set(&mut self, hash: Value, key: Value, val: Value) -> bool {
        let Some(entries) = self.hash_entries(hash) else { return false };
        let pos = entries.iter().position(|(k, _)| self.value_eq(*k, key));
        let entries = self.hash_entries_mut(hash).expect("checked above");
        match pos {
            Some(i) => entries[i].1 = val,
            None => entries.push((key, val)),
        }
        true
    }

    pub fn hash_remove(&mut self, hash: Value, key: Value) -> bool {
        let Some(entries) = self.hash_entries(hash) else { return false };
        let Some(pos) = entries.iter().position(|(k, _)| self.value_eq(*k, key)) else {
            return false;
        };
        self.hash_entries_mut(hash).expect("checked above").remove(pos);
        true
    }

    pub fn bytes(&self, v: Value) -> Option<&[u8]> {
        let id = self.unwrap_value(v).as_ref()?;
        match &self.node(id).kind {
            NodeKind::Str(b) => Some(b),
            NodeKind::UserClass { inner, .. } => self.bytes(*inner),
            _ => None,
        }
    }

    /// Decodes a Ruby string for display. RGSS1/2 (Ruby 1.8) strings carry no
    /// encoding, so fall back to the legacy Japanese/Western code pages before
    /// giving up and replacing bad bytes.
    pub fn string(&self, v: Value) -> Option<String> {
        self.bytes(v).map(decode_ruby_string)
    }

    pub fn set_string(&mut self, v: Value, s: &str) -> bool {
        let Some(id) = self.unwrap_value(v).as_ref() else { return false };
        let id = match self.node(id).kind {
            NodeKind::UserClass { inner, .. } => match inner.as_ref() {
                Some(i) => i,
                None => return false,
            },
            _ => id,
        };
        match &mut self.node_mut(id).kind {
            NodeKind::Str(b) => {
                *b = s.as_bytes().to_vec();
                true
            }
            _ => false,
        }
    }
}

/// Best-effort decode of a Ruby string to UTF-8 for display purposes.
pub fn decode_ruby_string(bytes: &[u8]) -> String {
    if let Ok(s) = std::str::from_utf8(bytes) {
        return s.to_owned();
    }
    // RGSS1/RGSS2 run on Ruby 1.8: strings are raw bytes in the code page the
    // game was authored in. Shift_JIS covers the Japanese originals; if that
    // decodes cleanly prefer it, otherwise fall back to Windows-1252.
    let (text, _, had_errors) = encoding_rs::SHIFT_JIS.decode(bytes);
    if !had_errors {
        return text.into_owned();
    }
    let (text, _, _) = encoding_rs::WINDOWS_1252.decode(bytes);
    text.into_owned()
}
