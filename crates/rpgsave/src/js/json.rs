//! JSON that survives a round trip.
//!
//! A save is not a document the editor owns: it is the game's, and anything
//! the editor rewrites that it did not mean to change is a bug waiting to
//! corrupt somebody's playthrough. So this is not a general-purpose JSON
//! library. It keeps every member in the order the engine wrote it, and keeps
//! the original text of every number and string, which is replayed verbatim
//! until something actually edits that value. What comes out of an untouched
//! save is byte-for-byte what went in.

use std::fmt;

/// A JSON value, with enough of its source kept to write it back unchanged.
#[derive(Clone, Debug, PartialEq)]
pub enum Json {
    Null,
    Bool(bool),
    Number(Number),
    Str(Text),
    Array(Vec<Json>),
    /// Members in source order. JavaScript objects are ordered, and the engine
    /// relies on it: `JsonEx` writes its `@` class marker last.
    Object(Vec<(Text, Json)>),
}

/// A number, and how it was spelled. JavaScript has one numeric type, so
/// `1`, `1.0` and `1e0` all parse to the same value but must not be confused
/// when writing back.
#[derive(Clone, Debug)]
pub struct Number {
    value: f64,
    source: Option<Box<str>>,
}

/// A string, and the escaped source it was read from.
#[derive(Clone, Debug)]
pub struct Text {
    value: String,
    source: Option<Box<str>>,
}

/// One hop into a value: a member of an object or an element of an array.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Step {
    Key(String),
    Index(usize),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParseError {
    pub message: String,
    pub offset: usize,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} at byte {}", self.message, self.offset)
    }
}

impl std::error::Error for ParseError {}

/// How deep a value may nest. Event interpreters nest a few levels per call,
/// so this is far above anything a game writes and far below a blown stack.
const MAX_DEPTH: usize = 256;

impl PartialEq for Number {
    /// Two numbers are equal when they mean the same thing, however spelled.
    fn eq(&self, other: &Self) -> bool {
        self.value == other.value
    }
}

impl PartialEq for Text {
    fn eq(&self, other: &Self) -> bool {
        self.value == other.value
    }
}

impl Number {
    pub fn new(value: f64) -> Number {
        Number { value, source: None }
    }

    pub fn value(&self) -> f64 {
        self.value
    }
}

impl Text {
    pub fn new(value: impl Into<String>) -> Text {
        Text { value: value.into(), source: None }
    }

    pub fn as_str(&self) -> &str {
        &self.value
    }
}

impl Json {
    // ---- building ---------------------------------------------------------

    pub fn int(value: i64) -> Json {
        Json::Number(Number::new(value as f64))
    }

    pub fn float(value: f64) -> Json {
        Json::Number(Number::new(value))
    }

    pub fn text(value: impl Into<String>) -> Json {
        Json::Str(Text::new(value))
    }

    // ---- reading ----------------------------------------------------------

    pub fn is_null(&self) -> bool {
        matches!(self, Json::Null)
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Json::Number(n) => Some(n.value),
            _ => None,
        }
    }

    /// Numbers that are whole, as the engine's counters and ids all are.
    pub fn as_i64(&self) -> Option<i64> {
        let value = self.as_f64()?;
        value.is_finite().then(|| value.trunc() as i64)
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Json::Bool(b) => Some(*b),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Json::Str(text) => Some(&text.value),
            _ => None,
        }
    }

    pub fn members(&self) -> Option<&[(Text, Json)]> {
        match self {
            Json::Object(members) => Some(members),
            _ => None,
        }
    }

    pub fn get(&self, key: &str) -> Option<&Json> {
        self.members()?
            .iter()
            .find(|(name, _)| name.value == key)
            .map(|(_, value)| value)
    }

    pub fn get_mut(&mut self, key: &str) -> Option<&mut Json> {
        match self {
            Json::Object(members) => members
                .iter_mut()
                .find(|(name, _)| name.value == key)
                .map(|(_, value)| value),
            _ => None,
        }
    }

    pub fn int_at(&self, key: &str) -> Option<i64> {
        self.get(key)?.as_i64()
    }

    pub fn str_at(&self, key: &str) -> Option<&str> {
        self.get(key)?.as_str()
    }

    /// The elements of an array, seeing through the object `JsonEx` wraps
    /// arrays in so that they can carry an identity (`{"@a":[…],"@c":7}`).
    pub fn elements(&self) -> Option<&[Json]> {
        match self {
            Json::Array(items) => Some(items),
            Json::Object(_) => match self.get(ARRAY_KEY)? {
                Json::Array(items) => Some(items),
                _ => None,
            },
            _ => None,
        }
    }

    pub fn elements_mut(&mut self) -> Option<&mut Vec<Json>> {
        match self {
            Json::Array(items) => Some(items),
            Json::Object(_) => match self.get_mut(ARRAY_KEY)? {
                Json::Array(items) => Some(items),
                _ => None,
            },
            _ => None,
        }
    }

    /// The element at `index`, or `None` for a gap or a short array.
    pub fn element(&self, index: usize) -> Option<&Json> {
        self.elements()?.get(index).filter(|v| !v.is_null())
    }

    /// The class `JsonEx` recorded for this object, for objects that have one.
    pub fn class_name(&self) -> Option<&str> {
        self.str_at(CLASS_KEY)
    }

    /// The value a path leads to, if it still leads anywhere.
    pub fn at(&self, path: &[Step]) -> Option<&Json> {
        let mut value = self;
        for step in path {
            value = match step {
                Step::Key(key) => value.get(key)?,
                Step::Index(index) => value.elements()?.get(*index)?,
            };
        }
        Some(value)
    }

    pub fn at_mut(&mut self, path: &[Step]) -> Option<&mut Json> {
        let mut value = self;
        for step in path {
            value = match step {
                Step::Key(key) => value.get_mut(key)?,
                Step::Index(index) => value.elements_mut()?.get_mut(*index)?,
            };
        }
        Some(value)
    }

    /// Whether this value holds others, and so can be opened in the browser.
    pub fn is_container(&self) -> bool {
        matches!(self, Json::Array(_) | Json::Object(_))
    }

    // ---- changing ---------------------------------------------------------

    /// Sets a member, keeping its position when it is already there.
    ///
    /// A new member goes in before `JsonEx`'s own `@c` and `@` markers, which
    /// the engine writes last.
    pub fn set(&mut self, key: &str, value: Json) -> bool {
        let Json::Object(members) = self else { return false };
        if let Some(slot) = members.iter_mut().find(|(name, _)| name.value == key) {
            slot.1 = value;
            return true;
        }
        let at = members
            .iter()
            .position(|(name, _)| name.value.starts_with('@'))
            .unwrap_or(members.len());
        members.insert(at, (Text::new(key), value));
        true
    }

    pub fn remove(&mut self, key: &str) -> Option<Json> {
        let Json::Object(members) = self else { return None };
        let at = members.iter().position(|(name, _)| name.value == key)?;
        Some(members.remove(at).1)
    }

    /// Writes an element, growing the array with nulls when it has to — the
    /// engine's own arrays are sparse in exactly this way.
    pub fn set_element(&mut self, index: usize, value: Json) -> bool {
        let Some(items) = self.elements_mut() else { return false };
        if items.len() <= index {
            items.resize(index + 1, Json::Null);
        }
        items[index] = value;
        true
    }

    // ---- writing ----------------------------------------------------------

    pub fn to_text(&self) -> String {
        let mut out = String::new();
        self.write(&mut out);
        out
    }

    pub fn write(&self, out: &mut String) {
        match self {
            Json::Null => out.push_str("null"),
            Json::Bool(true) => out.push_str("true"),
            Json::Bool(false) => out.push_str("false"),
            Json::Number(number) => match &number.source {
                Some(source) => out.push_str(source),
                None => out.push_str(&format_number(number.value)),
            },
            Json::Str(text) => write_text(text, out),
            Json::Array(items) => {
                out.push('[');
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    item.write(out);
                }
                out.push(']');
            }
            Json::Object(members) => {
                out.push('{');
                for (i, (key, value)) in members.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    write_text(key, out);
                    out.push(':');
                    value.write(out);
                }
                out.push('}');
            }
        }
    }
}

/// The member `JsonEx` puts an array's elements in.
pub const ARRAY_KEY: &str = "@a";
/// The member `JsonEx` records a class name in.
pub const CLASS_KEY: &str = "@";

/// Spells a number the way `JSON.stringify` does, which is the shortest text
/// that reads back as the same value.
fn format_number(value: f64) -> String {
    if !value.is_finite() {
        // JavaScript has no way to write these, and writes null instead.
        return "null".to_owned();
    }
    if value == 0.0 {
        // Including -0.0, which JavaScript writes as 0.
        return "0".to_owned();
    }
    if value.fract() == 0.0 && value.abs() < 1e21 {
        return format!("{value:.0}");
    }
    format!("{value}")
}

fn write_text(text: &Text, out: &mut String) {
    out.push('"');
    match &text.source {
        Some(source) => out.push_str(source),
        None => escape_into(&text.value, out),
    }
    out.push('"');
}

/// Escapes what JSON requires and nothing else, as `JSON.stringify` does:
/// non-ASCII characters stay as themselves.
fn escape_into(value: &str, out: &mut String) {
    for c in value.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
}

// --------------------------------------------------------------- the parser

pub fn parse(text: &str) -> Result<Json, ParseError> {
    let mut parser = Parser { bytes: text.as_bytes(), text, pos: 0 };
    parser.skip_whitespace();
    let value = parser.value(0)?;
    parser.skip_whitespace();
    if parser.pos != parser.bytes.len() {
        return Err(parser.error("trailing text after the value"));
    }
    Ok(value)
}

struct Parser<'a> {
    bytes: &'a [u8],
    text: &'a str,
    pos: usize,
}

impl<'a> Parser<'a> {
    fn error(&self, message: &str) -> ParseError {
        ParseError { message: message.to_owned(), offset: self.pos }
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    fn skip_whitespace(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.pos += 1;
        }
    }

    fn expect(&mut self, byte: u8) -> Result<(), ParseError> {
        if self.peek() == Some(byte) {
            self.pos += 1;
            Ok(())
        } else {
            Err(self.error(&format!("expected `{}`", byte as char)))
        }
    }

    fn literal(&mut self, word: &str) -> Result<(), ParseError> {
        if self.text[self.pos..].starts_with(word) {
            self.pos += word.len();
            Ok(())
        } else {
            Err(self.error(&format!("expected `{word}`")))
        }
    }

    fn value(&mut self, depth: usize) -> Result<Json, ParseError> {
        if depth > MAX_DEPTH {
            return Err(self.error("nested too deeply"));
        }
        match self.peek() {
            Some(b'{') => self.object(depth),
            Some(b'[') => self.array(depth),
            Some(b'"') => Ok(Json::Str(self.string()?)),
            Some(b't') => self.literal("true").map(|()| Json::Bool(true)),
            Some(b'f') => self.literal("false").map(|()| Json::Bool(false)),
            Some(b'n') => self.literal("null").map(|()| Json::Null),
            Some(b'-' | b'0'..=b'9') => self.number(),
            Some(_) => Err(self.error("not a value")),
            None => Err(self.error("the value is missing")),
        }
    }

    fn object(&mut self, depth: usize) -> Result<Json, ParseError> {
        self.expect(b'{')?;
        let mut members = Vec::new();
        self.skip_whitespace();
        if self.peek() == Some(b'}') {
            self.pos += 1;
            return Ok(Json::Object(members));
        }
        loop {
            self.skip_whitespace();
            let key = self.string()?;
            self.skip_whitespace();
            self.expect(b':')?;
            self.skip_whitespace();
            let value = self.value(depth + 1)?;
            members.push((key, value));
            self.skip_whitespace();
            match self.peek() {
                Some(b',') => self.pos += 1,
                Some(b'}') => {
                    self.pos += 1;
                    return Ok(Json::Object(members));
                }
                _ => return Err(self.error("expected `,` or `}`")),
            }
        }
    }

    fn array(&mut self, depth: usize) -> Result<Json, ParseError> {
        self.expect(b'[')?;
        let mut items = Vec::new();
        self.skip_whitespace();
        if self.peek() == Some(b']') {
            self.pos += 1;
            return Ok(Json::Array(items));
        }
        loop {
            self.skip_whitespace();
            items.push(self.value(depth + 1)?);
            self.skip_whitespace();
            match self.peek() {
                Some(b',') => self.pos += 1,
                Some(b']') => {
                    self.pos += 1;
                    return Ok(Json::Array(items));
                }
                _ => return Err(self.error("expected `,` or `]`")),
            }
        }
    }

    fn string(&mut self) -> Result<Text, ParseError> {
        self.expect(b'"')?;
        let start = self.pos;
        let mut escaped = false;
        loop {
            match self.peek() {
                Some(b'"') => break,
                Some(b'\\') => {
                    escaped = true;
                    self.pos += 2;
                    if self.pos > self.bytes.len() {
                        return Err(self.error("the string ends inside an escape"));
                    }
                }
                Some(_) => {
                    // Walk by characters, so that the source slice stays valid
                    // UTF-8 and multi-byte characters are not cut in half.
                    let rest = &self.text[self.pos..];
                    let c = rest.chars().next().expect("not at the end");
                    self.pos += c.len_utf8();
                }
                None => return Err(self.error("the string is unterminated")),
            }
        }
        let source = &self.text[start..self.pos];
        self.pos += 1;
        let value = if escaped { unescape(source, start)? } else { source.to_owned() };
        Ok(Text { value, source: Some(source.into()) })
    }

    fn number(&mut self) -> Result<Json, ParseError> {
        let start = self.pos;
        if self.peek() == Some(b'-') {
            self.pos += 1;
        }
        while matches!(self.peek(), Some(b'0'..=b'9')) {
            self.pos += 1;
        }
        if self.peek() == Some(b'.') {
            self.pos += 1;
            while matches!(self.peek(), Some(b'0'..=b'9')) {
                self.pos += 1;
            }
        }
        if matches!(self.peek(), Some(b'e' | b'E')) {
            self.pos += 1;
            if matches!(self.peek(), Some(b'+' | b'-')) {
                self.pos += 1;
            }
            while matches!(self.peek(), Some(b'0'..=b'9')) {
                self.pos += 1;
            }
        }
        let source = &self.text[start..self.pos];
        let value: f64 = source.parse().map_err(|_| ParseError {
            message: format!("`{source}` is not a number"),
            offset: start,
        })?;
        Ok(Json::Number(Number { value, source: Some(source.into()) }))
    }
}

fn unescape(source: &str, offset: usize) -> Result<String, ParseError> {
    let mut out = String::with_capacity(source.len());
    let mut chars = source.char_indices();
    while let Some((i, c)) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        let error = |message: &str| ParseError { message: message.to_owned(), offset: offset + i };
        match chars.next().map(|(_, c)| c) {
            Some('"') => out.push('"'),
            Some('\\') => out.push('\\'),
            Some('/') => out.push('/'),
            Some('b') => out.push('\u{8}'),
            Some('f') => out.push('\u{c}'),
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some('t') => out.push('\t'),
            Some('u') => {
                let high = hex4(&mut chars).ok_or_else(|| error("a broken \\u escape"))?;
                // Characters above the basic plane are written as a surrogate
                // pair, which has to be put back together.
                let c = match char::from_u32(u32::from(high)) {
                    Some(c) => c,
                    None => {
                        let mut peek = chars.clone();
                        let low = match (peek.next(), peek.next()) {
                            (Some((_, '\\')), Some((_, 'u'))) => hex4(&mut peek),
                            _ => None,
                        };
                        match low.and_then(|low| {
                            char::decode_utf16([high, low]).next()?.ok()
                        }) {
                            Some(c) => {
                                chars = peek;
                                c
                            }
                            // A lone surrogate is not text; the source slice is
                            // kept, so writing it back loses nothing.
                            None => char::REPLACEMENT_CHARACTER,
                        }
                    }
                };
                out.push(c);
            }
            _ => return Err(error("an unknown escape")),
        }
    }
    Ok(out)
}

fn hex4(chars: &mut std::str::CharIndices<'_>) -> Option<u16> {
    let mut value = 0u16;
    for _ in 0..4 {
        let digit = chars.next()?.1.to_digit(16)?;
        value = value << 4 | digit as u16;
    }
    Some(value)
}
