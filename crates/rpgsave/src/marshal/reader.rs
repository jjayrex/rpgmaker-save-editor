//! Decoder for Ruby's `Marshal` format, version 4.8.

use super::error::{MarshalError, Result};
use super::value::{Heap, Node, NodeKind, SymId, Value};

pub const MAJOR: u8 = 4;
pub const MINOR: u8 = 8;

struct Reader<'a, 'h> {
    data: &'a [u8],
    pos: usize,
    heap: &'h mut Heap,
    /// Symbol table for this document, in the order Ruby assigned it.
    symbols: Vec<SymId>,
    /// Object table for this document; `@` links index into it.
    objects: Vec<Value>,
}

impl<'a, 'h> Reader<'a, 'h> {
    fn byte(&mut self) -> Result<u8> {
        let b = *self
            .data
            .get(self.pos)
            .ok_or(MarshalError::UnexpectedEof { pos: self.pos })?;
        self.pos += 1;
        Ok(b)
    }

    fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        let end = self
            .pos
            .checked_add(n)
            .ok_or(MarshalError::UnexpectedEof { pos: self.pos })?;
        if end > self.data.len() {
            return Err(MarshalError::UnexpectedEof { pos: self.pos });
        }
        let slice = &self.data[self.pos..end];
        self.pos = end;
        Ok(slice)
    }

    /// Ruby's packed integer encoding (`r_long` in marshal.c).
    fn long(&mut self) -> Result<i64> {
        let c = self.byte()? as i8;
        if c == 0 {
            return Ok(0);
        }
        if c > 0 {
            if (5..=127).contains(&c) {
                return Ok(c as i64 - 5);
            }
            let n = c as usize;
            if n > 8 {
                return Err(MarshalError::BadLength { value: n as i64, pos: self.pos });
            }
            let mut x: i64 = 0;
            for i in 0..n {
                x |= (self.byte()? as i64) << (8 * i);
            }
            Ok(x)
        } else {
            if (-128..=-5).contains(&c) {
                return Ok(c as i64 + 5);
            }
            let n = -(c as i64) as usize;
            if n > 8 {
                return Err(MarshalError::BadLength { value: n as i64, pos: self.pos });
            }
            let mut x: i64 = -1;
            for i in 0..n {
                x &= !(0xffi64 << (8 * i));
                x |= (self.byte()? as i64) << (8 * i);
            }
            Ok(x)
        }
    }

    fn length(&mut self) -> Result<usize> {
        let n = self.long()?;
        if n < 0 || n as usize > self.data.len() - self.pos + 1 {
            return Err(MarshalError::BadLength { value: n, pos: self.pos });
        }
        Ok(n as usize)
    }

    fn raw_bytes(&mut self) -> Result<Vec<u8>> {
        let n = self.length()?;
        Ok(self.take(n)?.to_vec())
    }

    /// Registers a node in the object table before its contents are read, so
    /// that self-referential structures resolve correctly.
    fn register(&mut self, id: u32) {
        self.objects.push(Value::Ref(id));
    }

    fn read_symbol(&mut self) -> Result<SymId> {
        let bytes = self.raw_bytes()?;
        let name = String::from_utf8_lossy(&bytes).into_owned();
        let id = self.heap.intern(&name);
        self.symbols.push(id);
        Ok(id)
    }

    /// A symbol in a position where Ruby writes either `:sym`, `;link`, or an
    /// ivar-decorated symbol.
    fn read_symbol_ref(&mut self) -> Result<SymId> {
        let pos = self.pos;
        match self.byte()? {
            b':' => self.read_symbol(),
            b';' => {
                let idx = self.long()?;
                self.symbols
                    .get(idx as usize)
                    .copied()
                    .ok_or(MarshalError::BadLink { index: idx, pos })
            }
            b'I' => {
                // Non-ASCII symbol carrying an encoding; the encoding is not
                // meaningful for the identifiers RGSS uses.
                let sym = self.read_symbol_ref()?;
                self.skip_ivars()?;
                Ok(sym)
            }
            b => Err(MarshalError::UnknownType { byte: b, pos }),
        }
    }

    fn skip_ivars(&mut self) -> Result<()> {
        let n = self.length()?;
        for _ in 0..n {
            self.read_symbol_ref()?;
            self.read_value()?;
        }
        Ok(())
    }

    fn read_ivars(&mut self) -> Result<Vec<(SymId, Value)>> {
        let n = self.length()?;
        let mut out = Vec::with_capacity(n);
        for _ in 0..n {
            let name = self.read_symbol_ref()?;
            let value = self.read_value()?;
            out.push((name, value));
        }
        Ok(out)
    }

    fn read_value(&mut self) -> Result<Value> {
        let pos = self.pos;
        let t = self.byte()?;
        match t {
            b'0' => Ok(Value::Nil),
            b'T' => Ok(Value::Bool(true)),
            b'F' => Ok(Value::Bool(false)),
            b'i' => Ok(Value::Int(self.long()?)),
            b':' => Ok(Value::Sym(self.read_symbol()?)),
            b';' => {
                let idx = self.long()?;
                self.symbols
                    .get(idx as usize)
                    .copied()
                    .map(Value::Sym)
                    .ok_or(MarshalError::BadLink { index: idx, pos })
            }
            b'@' => {
                let idx = self.long()?;
                self.objects
                    .get(idx as usize)
                    .copied()
                    .ok_or(MarshalError::BadLink { index: idx, pos })
            }
            b'f' => {
                let bytes = self.raw_bytes()?;
                let text = String::from_utf8_lossy(&bytes).into_owned();
                let id = self.heap.alloc_kind(NodeKind::Float {
                    value: parse_ruby_float(&bytes),
                    source: Some(text.into()),
                });
                // Ruby gives floats a slot in the object table, and save files
                // do link back to them.
                self.register(id);
                Ok(Value::Ref(id))
            }
            b'l' => {
                let sign = self.byte()?;
                let n = self.length()?;
                let bytes = self.take(n * 2)?;
                let words = bytes
                    .chunks_exact(2)
                    .map(|c| u16::from_le_bytes([c[0], c[1]]))
                    .collect();
                let id = self.heap.alloc_kind(NodeKind::Bignum {
                    negative: sign == b'-',
                    words,
                });
                self.register(id);
                Ok(Value::Ref(id))
            }
            b'"' => {
                let id = self.heap.alloc_kind(NodeKind::Str(Vec::new()));
                self.register(id);
                let bytes = self.raw_bytes()?;
                self.heap.node_mut(id).kind = NodeKind::Str(bytes);
                Ok(Value::Ref(id))
            }
            b'[' => {
                let id = self.heap.alloc_kind(NodeKind::Array(Vec::new()));
                self.register(id);
                let n = self.length()?;
                let mut items = Vec::with_capacity(n.min(1024));
                for _ in 0..n {
                    items.push(self.read_value()?);
                }
                self.heap.node_mut(id).kind = NodeKind::Array(items);
                Ok(Value::Ref(id))
            }
            b'{' | b'}' => {
                let id = self.heap.alloc_kind(NodeKind::Hash {
                    entries: Vec::new(),
                    default: None,
                });
                self.register(id);
                let n = self.length()?;
                let mut entries = Vec::with_capacity(n.min(1024));
                for _ in 0..n {
                    let k = self.read_value()?;
                    let v = self.read_value()?;
                    entries.push((k, v));
                }
                let default = if t == b'}' { Some(self.read_value()?) } else { None };
                self.heap.node_mut(id).kind = NodeKind::Hash { entries, default };
                Ok(Value::Ref(id))
            }
            b'o' => {
                let class = self.read_symbol_ref()?;
                let id = self.heap.alloc_kind(NodeKind::Object { class });
                self.register(id);
                let ivars = self.read_ivars()?;
                self.heap.node_mut(id).ivars = ivars;
                Ok(Value::Ref(id))
            }
            b'u' => {
                let class = self.read_symbol_ref()?;
                let data = self.raw_bytes()?;
                let id = self.heap.alloc_kind(NodeKind::UserDef { class, data });
                self.register(id);
                Ok(Value::Ref(id))
            }
            b'U' => {
                let class = self.read_symbol_ref()?;
                let id = self.heap.alloc_kind(NodeKind::UserMarshal {
                    class,
                    value: Value::Nil,
                });
                self.register(id);
                let value = self.read_value()?;
                self.heap.node_mut(id).kind = NodeKind::UserMarshal { class, value };
                Ok(Value::Ref(id))
            }
            b'C' => {
                // Wrapper: the wrapped builtin takes the object-table slot.
                let class = self.read_symbol_ref()?;
                let inner = self.read_value()?;
                let id = self.heap.alloc_kind(NodeKind::UserClass { class, inner });
                Ok(Value::Ref(id))
            }
            b'e' => {
                let module = self.read_symbol_ref()?;
                let inner = self.read_value()?;
                let id = self.heap.alloc_kind(NodeKind::Extended { module, inner });
                Ok(Value::Ref(id))
            }
            b'S' => {
                let name = self.read_symbol_ref()?;
                let id = self.heap.alloc_kind(NodeKind::Struct {
                    name,
                    fields: Vec::new(),
                });
                self.register(id);
                let n = self.length()?;
                let mut fields = Vec::with_capacity(n);
                for _ in 0..n {
                    let k = self.read_symbol_ref()?;
                    let v = self.read_value()?;
                    fields.push((k, v));
                }
                self.heap.node_mut(id).kind = NodeKind::Struct { name, fields };
                Ok(Value::Ref(id))
            }
            b'c' => {
                let name = String::from_utf8_lossy(&self.raw_bytes()?).into_owned();
                let id = self.heap.alloc_kind(NodeKind::Class(name));
                self.register(id);
                Ok(Value::Ref(id))
            }
            b'm' | b'M' => {
                let name = String::from_utf8_lossy(&self.raw_bytes()?).into_owned();
                let id = self.heap.alloc_kind(NodeKind::Module(name));
                self.register(id);
                Ok(Value::Ref(id))
            }
            b'/' => {
                let id = self.heap.alloc_kind(NodeKind::Regexp {
                    source: Vec::new(),
                    options: 0,
                });
                self.register(id);
                let source = self.raw_bytes()?;
                let options = self.byte()?;
                self.heap.node_mut(id).kind = NodeKind::Regexp { source, options };
                Ok(Value::Ref(id))
            }
            b'I' => {
                let inner = self.read_value()?;
                let ivars = self.read_ivars()?;
                match inner {
                    Value::Ref(id) => {
                        let node: &mut Node = self.heap.node_mut(id);
                        node.ivars.extend(ivars);
                        Ok(inner)
                    }
                    // `I` around a symbol is handled in read_symbol_ref; other
                    // immediates cannot carry state.
                    Value::Sym(_) => Ok(inner),
                    _ => Err(MarshalError::IvarsOnImmediate { pos }),
                }
            }
            b'd' => Err(MarshalError::Unsupported { what: "Data (`d`)", pos }),
            b => Err(MarshalError::UnknownType { byte: b, pos }),
        }
    }
}

fn parse_ruby_float(bytes: &[u8]) -> f64 {
    // Ruby appends a NUL plus extra mantissa bits in very old streams; and
    // writes "inf" / "-inf" / "nan" for the specials.
    let text = bytes.split(|b| *b == 0).next().unwrap_or(&[]);
    match std::str::from_utf8(text).unwrap_or("").trim() {
        "inf" => f64::INFINITY,
        "-inf" => f64::NEG_INFINITY,
        "nan" => f64::NAN,
        s => s.parse().unwrap_or(0.0),
    }
}

/// Reads one Marshal document from the front of `data`.
///
/// Returns the value and the number of bytes consumed, so that files holding
/// several documents back to back (RPG Maker XP and VX saves) can be walked.
pub fn load_prefix(data: &[u8], heap: &mut Heap) -> Result<(Value, usize)> {
    if data.len() < 2 {
        return Err(MarshalError::UnexpectedEof { pos: 0 });
    }
    if data[0] != MAJOR || data[1] > MINOR {
        return Err(MarshalError::BadMagic { major: data[0], minor: data[1] });
    }
    let mut r = Reader {
        data,
        pos: 2,
        heap,
        symbols: Vec::new(),
        objects: Vec::new(),
    };
    let v = r.read_value()?;
    Ok((v, r.pos))
}

/// Reads a single Marshal document that spans the whole of `data`.
pub fn load(data: &[u8], heap: &mut Heap) -> Result<Value> {
    load_prefix(data, heap).map(|(v, _)| v)
}

/// Reads every Marshal document in `data`, one after another.
pub fn load_stream(data: &[u8], heap: &mut Heap) -> Result<Vec<Value>> {
    let mut out = Vec::new();
    let mut offset = 0;
    while offset < data.len() {
        // Trailing whitespace/padding some tools leave behind.
        if data[offset..].iter().all(|b| *b == 0) {
            break;
        }
        let (v, used) = load_prefix(&data[offset..], heap)?;
        out.push(v);
        offset += used;
    }
    Ok(out)
}
