//! The compression RPG Maker MV wraps its saves in.
//!
//! A `.rpgsave` file is `LZString.compressToBase64(JSON.stringify(contents))`:
//! an LZW variant over the JavaScript string's UTF-16 code units, packed six
//! bits to a base64 character. This is a faithful port of lz-string, because
//! the game reads back only what lz-string wrote.

use std::collections::{HashMap, HashSet};

/// lz-string's own base64 alphabet, which is the standard one.
const KEY: &[u8; 65] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/=";

/// Six bits per character, so a character's top bit is 1 << 5.
const BITS_PER_CHAR: u32 = 6;
const RESET_VALUE: u32 = 1 << (BITS_PER_CHAR - 1);

/// The codes the format reserves: a literal 8-bit character, a literal 16-bit
/// character, and the end of the stream.
const LITERAL_8: u32 = 0;
const LITERAL_16: u32 = 1;
const END_OF_STREAM: u32 = 2;

/// Reads the six-bit values back out, most significant bit first, assembling
/// each code from its low bit up.
struct Reader<'a> {
    values: &'a [u32],
    value: u32,
    mask: u32,
    index: usize,
}

impl<'a> Reader<'a> {
    fn new(values: &'a [u32]) -> Option<Reader<'a>> {
        Some(Reader { values, value: *values.first()?, mask: RESET_VALUE, index: 1 })
    }

    fn read(&mut self, count: u32) -> Option<u32> {
        let mut bits = 0;
        let mut power = 1;
        for _ in 0..count {
            let set = self.value & self.mask != 0;
            self.mask >>= 1;
            if self.mask == 0 {
                self.mask = RESET_VALUE;
                // Running out of input mid-code means the file is truncated.
                self.value = *self.values.get(self.index)?;
                self.index += 1;
            }
            if set {
                bits |= power;
            }
            power <<= 1;
        }
        Some(bits)
    }
}

/// Packs codes into six-bit characters, low bit of each code first.
struct Writer {
    chars: Vec<u8>,
    value: u32,
    position: u32,
}

impl Writer {
    fn new(capacity: usize) -> Writer {
        Writer { chars: Vec::with_capacity(capacity), value: 0, position: 0 }
    }

    fn write(&mut self, code: u32, count: u32) {
        let mut code = code;
        for _ in 0..count {
            self.value = (self.value << 1) | (code & 1);
            if self.position == BITS_PER_CHAR - 1 {
                self.position = 0;
                self.chars.push(KEY[self.value as usize]);
                self.value = 0;
            } else {
                self.position += 1;
            }
            code >>= 1;
        }
    }

    /// Pads the part-written character out and returns the base64 text.
    fn finish(mut self) -> String {
        loop {
            self.value <<= 1;
            if self.position == BITS_PER_CHAR - 1 {
                self.chars.push(KEY[self.value as usize]);
                break;
            }
            self.position += 1;
        }
        match self.chars.len() % 4 {
            1 => self.chars.extend_from_slice(b"==="),
            2 => self.chars.extend_from_slice(b"=="),
            3 => self.chars.push(b'='),
            _ => {}
        }
        // Every byte came from KEY, so this is ASCII by construction.
        String::from_utf8(self.chars).expect("base64 is ASCII")
    }
}

/// True when `text` is nothing but base64, which is all a `.rpgsave` holds.
pub fn looks_like_base64(text: &str) -> bool {
    let text = text.trim();
    !text.is_empty()
        && text
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'+' || b == b'/' || b == b'=')
}

/// Decodes the first three bytes a base64 text stands for, which is enough to
/// recognise what a file is when it turns out not to be an lz-string stream.
pub fn first_bytes(text: &str) -> Option<[u8; 3]> {
    let mut bytes = text.trim().bytes();
    let mut bits = 0u32;
    for _ in 0..4 {
        let c = bytes.next()?;
        // The padding stands for no bits, so a text this short decodes to
        // nothing worth looking at.
        let value = KEY[..64].iter().position(|k| *k == c)? as u32;
        bits = bits << 6 | value;
    }
    Some([(bits >> 16) as u8, (bits >> 8) as u8, bits as u8])
}

/// Unpacks what `compress_to_base64` wrote. `None` for anything that is not an
/// lz-string stream.
pub fn decompress_from_base64(text: &str) -> Option<String> {
    let mut lookup = [u32::MAX; 128];
    for (i, c) in KEY.iter().enumerate() {
        lookup[*c as usize] = i as u32;
    }

    let mut values = Vec::with_capacity(text.len());
    for b in text.trim().bytes() {
        // The padding carries no bits, and lz-string never emits one mid-stream.
        if b == b'=' {
            break;
        }
        let value = *lookup.get(b as usize)?;
        if value == u32::MAX {
            return None;
        }
        values.push(value);
    }

    let mut reader = Reader::new(&values)?;
    let units = decompress(&mut reader)?;
    String::from_utf16(&units).ok()
}

fn decompress(reader: &mut Reader) -> Option<Vec<u16>> {
    // Entries 0, 1 and 2 are the reserved codes above and never read as text.
    let mut dictionary: Vec<Vec<u16>> = vec![Vec::new(), Vec::new(), Vec::new()];
    let mut enlarge_in = 4u32;
    let mut numbits = 3u32;

    let first = match reader.read(2)? {
        LITERAL_8 => reader.read(8)? as u16,
        LITERAL_16 => reader.read(16)? as u16,
        // An empty string compresses to the end marker on its own.
        END_OF_STREAM => return Some(Vec::new()),
        _ => return None,
    };

    dictionary.push(vec![first]);
    let mut result = vec![first];
    let mut w = vec![first];

    loop {
        let mut code = reader.read(numbits)?;
        match code {
            LITERAL_8 | LITERAL_16 => {
                let width = if code == LITERAL_8 { 8 } else { 16 };
                let unit = reader.read(width)? as u16;
                dictionary.push(vec![unit]);
                code = dictionary.len() as u32 - 1;
                enlarge_in -= 1;
            }
            END_OF_STREAM => return Some(result),
            _ => {}
        }

        if enlarge_in == 0 {
            enlarge_in = 1 << numbits;
            numbits += 1;
        }

        let entry = match dictionary.get(code as usize) {
            Some(entry) if !entry.is_empty() => entry.clone(),
            // The one code a stream may name before it is defined is the next
            // one, which stands for the previous entry plus its own first unit.
            _ if code == dictionary.len() as u32 => {
                let mut entry = w.clone();
                entry.push(w[0]);
                entry
            }
            _ => return None,
        };
        result.extend_from_slice(&entry);

        let mut added = w;
        added.push(entry[0]);
        dictionary.push(added);
        enlarge_in -= 1;

        w = entry;
        if enlarge_in == 0 {
            enlarge_in = 1 << numbits;
            numbits += 1;
        }
    }
}

/// Compresses exactly as lz-string does, so an unedited save re-encodes to the
/// bytes it was read from.
pub fn compress_to_base64(text: &str) -> String {
    let input: Vec<u16> = text.encode_utf16().collect();
    let mut writer = Writer::new(input.len() / 4 + 8);
    if input.is_empty() {
        writer.write(END_OF_STREAM, 2);
        return writer.finish();
    }

    // The dictionary as a trie: single units by value, longer entries by the
    // code of everything but their last unit. lz-string keys it by the string
    // itself, which comes to the same thing and costs far more to build.
    let mut units: HashMap<u16, u32> = HashMap::new();
    let mut pairs: HashMap<(u32, u16), u32> = HashMap::new();
    // Units that have been given a code but not yet written out literally.
    let mut unwritten: HashSet<u16> = HashSet::new();
    let mut dict_size = 3u32;
    let mut numbits = 2u32;
    let mut enlarge_in = 2u32;

    // The string built so far: its code, and the unit it is, when it is one.
    let mut w: Option<(u32, Option<u16>)> = None;

    for &c in &input {
        let code_of_c = match units.get(&c) {
            Some(&code) => code,
            None => {
                units.insert(c, dict_size);
                unwritten.insert(c);
                dict_size += 1;
                dict_size - 1
            }
        };

        let Some((w_code, w_single)) = w else {
            w = Some((code_of_c, Some(c)));
            continue;
        };

        match pairs.get(&(w_code, c)) {
            // The longer string is known, so keep growing it.
            Some(&code) => w = Some((code, None)),
            None => {
                if write_entry(&mut writer, &mut unwritten, w_code, w_single, numbits) {
                    enlarge(&mut numbits, &mut enlarge_in);
                }
                enlarge(&mut numbits, &mut enlarge_in);
                pairs.insert((w_code, c), dict_size);
                dict_size += 1;
                w = Some((code_of_c, Some(c)));
            }
        }
    }

    if let Some((w_code, w_single)) = w {
        if write_entry(&mut writer, &mut unwritten, w_code, w_single, numbits) {
            enlarge(&mut numbits, &mut enlarge_in);
        }
        enlarge(&mut numbits, &mut enlarge_in);
    }

    writer.write(END_OF_STREAM, numbits);
    writer.finish()
}

/// Writes the code for the string built so far. A unit that has not been sent
/// before goes out as itself, so that the reader learns it too; that costs an
/// extra step towards the next code width, which is why this reports it.
fn write_entry(
    writer: &mut Writer,
    unwritten: &mut HashSet<u16>,
    code: u32,
    single: Option<u16>,
    numbits: u32,
) -> bool {
    match single.filter(|unit| unwritten.contains(unit)) {
        Some(unit) => {
            if unit < 256 {
                writer.write(LITERAL_8, numbits);
                writer.write(u32::from(unit), 8);
            } else {
                writer.write(LITERAL_16, numbits);
                writer.write(u32::from(unit), 16);
            }
            unwritten.remove(&unit);
            true
        }
        None => {
            writer.write(code, numbits);
            false
        }
    }
}

/// The code width grows once the dictionary fills, in step on both sides.
fn enlarge(numbits: &mut u32, enlarge_in: &mut u32) {
    *enlarge_in -= 1;
    if *enlarge_in == 0 {
        *enlarge_in = 1 << *numbits;
        *numbits += 1;
    }
}
