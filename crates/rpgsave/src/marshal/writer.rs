//! Encoder for Ruby's `Marshal` format, version 4.8.
//!
//! The output is byte-compatible with what Ruby 1.8/1.9 (and therefore RGSS)
//! produces, including symbol links and object back-references, so an
//! untouched save file re-serialises to the bytes it came from.

use std::collections::HashMap;

use super::reader::{MAJOR, MINOR};
use super::value::{Heap, NodeId, NodeKind, SymId, Value};

struct Writer<'h> {
    out: Vec<u8>,
    heap: &'h Heap,
    symbols: HashMap<SymId, usize>,
    objects: HashMap<NodeId, usize>,
    /// Next index Ruby's loader will assign in its object table.
    next_obj: usize,
}

impl<'h> Writer<'h> {
    fn new(heap: &'h Heap) -> Self {
        Writer {
            out: Vec::new(),
            heap,
            symbols: HashMap::new(),
            objects: HashMap::new(),
            next_obj: 0,
        }
    }

    fn byte(&mut self, b: u8) {
        self.out.push(b);
    }

    /// Ruby's packed integer encoding (`w_long` in marshal.c).
    fn long(&mut self, x: i64) {
        if x == 0 {
            self.byte(0);
            return;
        }
        if 0 < x && x < 123 {
            self.byte((x + 5) as u8);
            return;
        }
        if -124 < x && x < 0 {
            self.byte(((x - 5) & 0xff) as u8);
            return;
        }
        let mut buf = [0u8; 9];
        let mut v = x;
        let mut len = 1usize;
        for i in 1..=8 {
            buf[i] = (v & 0xff) as u8;
            v >>= 8;
            len = i;
            if v == 0 {
                buf[0] = i as u8;
                break;
            }
            if v == -1 {
                buf[0] = (-(i as i64) & 0xff) as u8;
                break;
            }
        }
        self.out.extend_from_slice(&buf[..=len]);
    }

    fn bytes(&mut self, data: &[u8]) {
        self.long(data.len() as i64);
        self.out.extend_from_slice(data);
    }

    fn symbol(&mut self, sym: SymId) {
        if let Some(&idx) = self.symbols.get(&sym) {
            self.byte(b';');
            self.long(idx as i64);
            return;
        }
        let idx = self.symbols.len();
        self.symbols.insert(sym, idx);
        self.byte(b':');
        let name: &'h str = self.heap.sym(sym);
        self.bytes(name.as_bytes());
    }

    fn remember(&mut self, id: NodeId) {
        self.objects.insert(id, self.next_obj);
        self.next_obj += 1;
    }

    fn write_pairs(&mut self, pairs: &[(SymId, Value)]) {
        self.long(pairs.len() as i64);
        for (k, v) in pairs {
            self.symbol(*k);
            self.write_value(*v);
        }
    }

    fn write_value(&mut self, v: Value) {
        match v {
            Value::Nil => self.byte(b'0'),
            Value::Bool(true) => self.byte(b'T'),
            Value::Bool(false) => self.byte(b'F'),
            Value::Sym(s) => self.symbol(s),
            Value::Int(i) => self.write_int(i),
            Value::Ref(id) => self.write_node(id),
        }
    }

    fn write_int(&mut self, i: i64) {
        // Ruby stores integers outside the 31-bit tagged range as Bignums, and
        // RGSS (32-bit Ruby) refuses to load a wider `i`.
        let tagged = i.wrapping_shl(1) | 1;
        if tagged >> 31 == 0 || tagged >> 31 == -1 {
            self.byte(b'i');
            self.long(i);
        } else {
            self.next_obj += 1;
            self.write_bignum_parts(i < 0, &int_to_words(i.unsigned_abs()));
        }
    }

    fn write_bignum_parts(&mut self, negative: bool, words: &[u16]) {
        self.byte(b'l');
        self.byte(if negative { b'-' } else { b'+' });
        self.long(words.len() as i64);
        for w in words {
            self.out.extend_from_slice(&w.to_le_bytes());
        }
    }

    fn write_node(&mut self, id: NodeId) {
        if let Some(&idx) = self.objects.get(&id) {
            self.byte(b'@');
            self.long(idx as i64);
            return;
        }
        // Copying the reference out detaches its lifetime from `self`, so the
        // node can be read while the writer mutates itself.
        let heap: &'h Heap = self.heap;
        let node = heap.node(id);

        // `e` and `C` decorate the value that follows; the decorated value is
        // the one that lands in the object table.
        match node.kind {
            NodeKind::Extended { module, inner } => {
                self.byte(b'e');
                self.symbol(module);
                self.write_value(inner);
                self.alias_wrapper(id, inner);
                return;
            }
            NodeKind::UserClass { class, inner } => {
                self.byte(b'C');
                self.symbol(class);
                self.write_value(inner);
                self.alias_wrapper(id, inner);
                return;
            }
            _ => {}
        }

        // `o` and `S` carry their state natively; every other type needs the
        // `I` wrapper to hold instance variables.
        let state_is_native = matches!(
            node.kind,
            NodeKind::Object { .. } | NodeKind::Struct { .. }
        );
        let wrap_ivars = !node.ivars.is_empty() && !state_is_native;
        if wrap_ivars {
            self.byte(b'I');
        }
        self.remember(id);

        match &node.kind {
            NodeKind::Float { value, source } => {
                self.byte(b'f');
                // Write it back exactly as it was read, when that still says
                // the same number; otherwise format it ourselves.
                match source {
                    Some(text) if parses_back_to(text, *value) => self.bytes(text.as_bytes()),
                    _ => self.bytes(ruby_float_string(*value).as_bytes()),
                }
            }
            NodeKind::Str(bytes) => {
                self.byte(b'"');
                self.bytes(bytes);
            }
            NodeKind::Array(items) => {
                self.byte(b'[');
                self.long(items.len() as i64);
                for it in items {
                    self.write_value(*it);
                }
            }
            NodeKind::Hash { entries, default } => {
                self.byte(if default.is_some() { b'}' } else { b'{' });
                self.long(entries.len() as i64);
                for (k, v) in entries {
                    self.write_value(*k);
                    self.write_value(*v);
                }
                if let Some(d) = default {
                    self.write_value(*d);
                }
            }
            NodeKind::Object { class } => {
                self.byte(b'o');
                self.symbol(*class);
                self.write_pairs(&node.ivars);
            }
            NodeKind::UserDef { class, data } => {
                self.byte(b'u');
                self.symbol(*class);
                self.bytes(data);
            }
            NodeKind::UserMarshal { class, value } => {
                self.byte(b'U');
                self.symbol(*class);
                self.write_value(*value);
            }
            NodeKind::Struct { name, fields } => {
                self.byte(b'S');
                self.symbol(*name);
                self.write_pairs(fields);
            }
            NodeKind::Bignum { negative, words } => self.write_bignum_parts(*negative, words),
            NodeKind::Class(name) => {
                self.byte(b'c');
                self.bytes(name.as_bytes());
            }
            NodeKind::Module(name) => {
                self.byte(b'm');
                self.bytes(name.as_bytes());
            }
            NodeKind::Regexp { source, options } => {
                self.byte(b'/');
                self.bytes(source);
                self.byte(*options);
            }
            NodeKind::Extended { .. } | NodeKind::UserClass { .. } => unreachable!("handled above"),
        }

        if wrap_ivars {
            self.write_pairs(&node.ivars);
        }
    }

    /// A second reference to a wrapper node must link to the slot its wrapped
    /// value took.
    fn alias_wrapper(&mut self, wrapper: NodeId, inner: Value) {
        if let Some(idx) = inner.as_ref().and_then(|i| self.objects.get(&i).copied()) {
            self.objects.insert(wrapper, idx);
        }
    }
}

fn int_to_words(mut n: u64) -> Vec<u16> {
    let mut words = Vec::new();
    while n > 0 {
        words.push((n & 0xffff) as u16);
        n >>= 16;
    }
    if words.is_empty() {
        words.push(0);
    }
    words
}

/// Whether `text` is a faithful spelling of `value`.
fn parses_back_to(text: &str, value: f64) -> bool {
    match text {
        "inf" => value == f64::INFINITY,
        "-inf" => value == f64::NEG_INFINITY,
        "nan" => value.is_nan(),
        _ => text.parse::<f64>().is_ok_and(|parsed| parsed.to_bits() == value.to_bits()),
    }
}

/// Ruby's textual float encoding.
///
/// `Marshal` writes the shortest digit string that round-trips, then chooses
/// between positional and exponential notation by where the decimal point
/// falls — which is why Ruby dumps `100.0` as `1e2` but `255.0` as `255`.
/// Matching it exactly is what lets an untouched save re-encode byte for byte.
fn ruby_float_string(f: f64) -> String {
    if f.is_nan() {
        return "nan".to_owned();
    }
    if f.is_infinite() {
        return if f < 0.0 { "-inf".to_owned() } else { "inf".to_owned() };
    }
    if f == 0.0 {
        return if f.is_sign_negative() { "-0".to_owned() } else { "0".to_owned() };
    }

    // Rust's exponential formatting gives the same shortest round-trip digits
    // Ruby's `ruby_dtoa` produces, in a form that is easy to take apart.
    let sign = if f < 0.0 { "-" } else { "" };
    let repr = format!("{:e}", f.abs());
    let (mantissa, exponent) = repr.split_once('e').expect("exponential form");
    let digits: String = mantissa.chars().filter(|c| *c != '.').collect();
    let digs = digits.len() as i32;
    // Position of the decimal point relative to the start of the digits.
    let decpt = exponent.parse::<i32>().expect("exponent") + 1;

    /// `DECIMAL_DIG` for an IEEE double, the cutoff Ruby uses.
    const DECIMAL_DIG: i32 = 17;

    let outside_positional_range = !(-3..=DECIMAL_DIG).contains(&decpt);
    let body = if outside_positional_range || decpt > digs {
        let mut out = String::from(&digits[..1]);
        if digs > 1 {
            out.push('.');
            out.push_str(&digits[1..]);
        }
        out.push('e');
        out.push_str(&(decpt - 1).to_string());
        out
    } else if decpt == digs {
        digits
    } else if decpt > 0 {
        let split = decpt as usize;
        format!("{}.{}", &digits[..split], &digits[split..])
    } else {
        format!("0.{}{}", "0".repeat((-decpt) as usize), digits)
    };
    format!("{sign}{body}")
}

/// Serialises one value as a complete Marshal document.
pub fn dump(value: Value, heap: &Heap) -> Vec<u8> {
    let mut w = Writer::new(heap);
    w.out.push(MAJOR);
    w.out.push(MINOR);
    w.write_value(value);
    w.out
}

/// Serialises several values as back-to-back Marshal documents, the layout
/// RPG Maker XP and VX use for their save files.
pub fn dump_stream(values: &[Value], heap: &Heap) -> Vec<u8> {
    let mut out = Vec::new();
    for v in values {
        out.extend_from_slice(&dump(*v, heap));
    }
    out
}
