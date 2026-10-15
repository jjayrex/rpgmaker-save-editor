//! The wrapper RPG Maker MZ puts around a save.
//!
//! MZ deflates the JSON with pako and writes what comes back straight to the
//! file. What comes back is a JavaScript string of one character per byte, and
//! writing a string writes UTF-8, so every byte above 0x7F arrives on disk as
//! two. The engine reads the file back as UTF-8 and takes the low byte of each
//! character, which is what [`from_text`] and [`to_text`] do here.
//!
//! The compression itself is DEFLATE in a zlib wrapper, which is standard and
//! not RPG Maker's own invention, so this hands it to `miniz_oxide` rather
//! than implementing it again.

/// The most a save is allowed to unpack to. MZ refuses to write one over
/// 20 MB, so this is well clear of any real save while still putting a bound
/// on what a damaged or hostile file can ask for.
const MAX_UNPACKED: usize = 64 * 1024 * 1024;

/// How hard to compress when writing. The engine itself uses the fastest
/// setting; this one is slower to write and smaller on disk, and any level
/// reads back the same.
const LEVEL: u8 = 6;

/// Turns the file's text back into the bytes the engine deflated.
///
/// `None` when the text is not what the engine writes: either it is not UTF-8,
/// or it holds a character that cannot have come from a single byte.
pub fn from_text(bytes: &[u8]) -> Option<Vec<u8>> {
    let text = std::str::from_utf8(bytes).ok()?;
    text.chars().map(|c| u8::try_from(u32::from(c)).ok()).collect()
}

/// Writes bytes the way the engine does, one character each.
pub fn to_text(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len() * 2);
    let mut buffer = [0u8; 4];
    for byte in data {
        out.extend_from_slice(char::from(*byte).encode_utf8(&mut buffer).as_bytes());
    }
    out
}

/// True for bytes that begin with a zlib header.
///
/// The low nibble of the first byte names the compression method, which is 8
/// for DEFLATE, and its high nibble the window size; the engine always writes
/// 0x78, the largest. The second byte carries the level, and the pair is a
/// multiple of 31 as a check on itself.
pub fn looks_like_zlib(data: &[u8]) -> bool {
    match data {
        [method, flags, ..] if method & 0x0F == 8 => {
            (u16::from(*method) << 8 | u16::from(*flags)) % 31 == 0
        }
        _ => false,
    }
}

/// True for a file that looks like one of these streams, in either the form
/// the engine writes or as plain bytes.
///
/// The header's second byte is often above 0x7F, where the two forms differ,
/// so the text form is checked by decoding the first two characters.
pub fn looks_like_stream(bytes: &[u8]) -> bool {
    if looks_like_zlib(bytes) {
        return true;
    }
    let head: Vec<u8> = String::from_utf8_lossy(&bytes[..bytes.len().min(4)])
        .chars()
        .take(2)
        .filter_map(|c| u8::try_from(u32::from(c)).ok())
        .collect();
    looks_like_zlib(&head)
}

/// Unpacks a zlib stream. `None` for anything that is not one.
pub fn inflate(data: &[u8]) -> Option<Vec<u8>> {
    miniz_oxide::inflate::decompress_to_vec_zlib_with_limit(data, MAX_UNPACKED).ok()
}

pub fn deflate(data: &[u8]) -> Vec<u8> {
    miniz_oxide::deflate::compress_to_vec_zlib(data, LEVEL)
}
