//! The chunk encoding shared by every LCF file.
//!
//! An LCF file is a signature followed by a stream of chunks, each one a tag,
//! a length and that many bytes. A chunk's payload is either a value or
//! another stream of chunks — which of the two is a matter of schema, not of
//! encoding, so the payloads are kept as raw bytes and interpreted on demand.
//! That is also what lets an untouched file be written back byte for byte.

/// Reading past the end of a chunk stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Truncated {
    pub at: usize,
}

impl std::fmt::Display for Truncated {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "LCF data ends in the middle of a chunk, at byte {}", self.at)
    }
}

impl std::error::Error for Truncated {}

type Result<T> = std::result::Result<T, Truncated>;

/// Reads the format's variable-length integer: seven bits per byte, most
/// significant group first, with the top bit set on every byte but the last.
pub fn read_int(bytes: &[u8], pos: &mut usize) -> Result<u32> {
    let mut value: u32 = 0;
    loop {
        let byte = *bytes.get(*pos).ok_or(Truncated { at: *pos })?;
        *pos += 1;
        value = (value << 7) | u32::from(byte & 0x7f);
        if byte & 0x80 == 0 {
            return Ok(value);
        }
    }
}

pub fn write_int(value: u32, out: &mut Vec<u8>) {
    let mut groups = [0u8; 5];
    let mut count = 0;
    let mut rest = value;
    loop {
        groups[count] = (rest & 0x7f) as u8;
        count += 1;
        rest >>= 7;
        if rest == 0 {
            break;
        }
    }
    for i in (1..count).rev() {
        out.push(groups[i] | 0x80);
    }
    out.push(groups[0]);
}

pub fn int_bytes(value: u32) -> Vec<u8> {
    let mut out = Vec::new();
    write_int(value, &mut out);
    out
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Chunk {
    pub tag: u32,
    pub data: Vec<u8>,
}

/// An ordered stream of chunks.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Chunks {
    items: Vec<Chunk>,
}

impl Chunks {
    /// Reads chunks until the data runs out. The outermost stream of a file is
    /// stored this way, with no terminator.
    pub fn parse_to_end(bytes: &[u8]) -> Result<Chunks> {
        let mut pos = 0;
        let mut items = Vec::new();
        while pos < bytes.len() {
            let tag = read_int(bytes, &mut pos)?;
            if tag == 0 {
                break;
            }
            items.push(read_chunk(tag, bytes, &mut pos)?);
        }
        Ok(Chunks { items })
    }

    /// Reads one terminated stream, as nested structures are stored, and
    /// reports where it ended.
    pub fn parse_stream(bytes: &[u8], pos: &mut usize) -> Result<Chunks> {
        let mut items = Vec::new();
        loop {
            let tag = read_int(bytes, pos)?;
            if tag == 0 {
                return Ok(Chunks { items });
            }
            items.push(read_chunk(tag, bytes, pos)?);
        }
    }

    pub fn to_bytes(&self, terminated: bool) -> Vec<u8> {
        let mut out = Vec::new();
        for chunk in &self.items {
            write_int(chunk.tag, &mut out);
            write_int(chunk.data.len() as u32, &mut out);
            out.extend_from_slice(&chunk.data);
        }
        if terminated {
            out.push(0);
        }
        out
    }

    pub fn tags(&self) -> impl Iterator<Item = u32> + '_ {
        self.items.iter().map(|c| c.tag)
    }

    pub fn iter(&self) -> impl Iterator<Item = &Chunk> {
        self.items.iter()
    }

    pub fn get(&self, tag: u32) -> Option<&[u8]> {
        self.items.iter().find(|c| c.tag == tag).map(|c| c.data.as_slice())
    }

    /// Replaces a chunk, or inserts it in tag order when it is not there yet.
    /// A field left at its default is simply absent from the file, so adding
    /// chunks is routine rather than exceptional.
    pub fn set(&mut self, tag: u32, data: Vec<u8>) {
        match self.items.iter_mut().find(|c| c.tag == tag) {
            Some(chunk) => chunk.data = data,
            None => {
                let at = self.items.iter().position(|c| c.tag > tag).unwrap_or(self.items.len());
                self.items.insert(at, Chunk { tag, data });
            }
        }
    }

    pub fn remove(&mut self, tag: u32) {
        self.items.retain(|c| c.tag != tag);
    }

    // ---- typed views ------------------------------------------------------

    /// A scalar integer field. Absent means the field is at its default.
    pub fn int(&self, tag: u32) -> Option<u32> {
        let data = self.get(tag)?;
        let mut pos = 0;
        read_int(data, &mut pos).ok()
    }

    pub fn set_int(&mut self, tag: u32, value: u32) {
        self.set(tag, int_bytes(value));
    }

    pub fn bool(&self, tag: u32) -> Option<bool> {
        self.int(tag).map(|v| v != 0)
    }

    pub fn set_bool(&mut self, tag: u32, value: bool) {
        self.set_int(tag, u32::from(value));
    }

    pub fn string(&self, tag: u32) -> Option<&[u8]> {
        self.get(tag)
    }

    pub fn double(&self, tag: u32) -> Option<f64> {
        let data = self.get(tag)?;
        Some(f64::from_le_bytes(data.get(..8)?.try_into().ok()?))
    }

    /// Vectors are stored as packed little-endian values, not as the
    /// variable-length integers used for scalars.
    pub fn vec_i16(&self, tag: u32) -> Vec<i16> {
        self.get(tag)
            .map(|d| d.chunks_exact(2).map(|c| i16::from_le_bytes([c[0], c[1]])).collect())
            .unwrap_or_default()
    }

    pub fn set_vec_i16(&mut self, tag: u32, values: &[i16]) {
        self.set(tag, values.iter().flat_map(|v| v.to_le_bytes()).collect());
    }

    pub fn vec_i32(&self, tag: u32) -> Vec<i32> {
        self.get(tag)
            .map(|d| {
                d.chunks_exact(4)
                    .map(|c| i32::from_le_bytes([c[0], c[1], c[2], c[3]]))
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn set_vec_i32(&mut self, tag: u32, values: &[i32]) {
        self.set(tag, values.iter().flat_map(|v| v.to_le_bytes()).collect());
    }

    pub fn vec_u8(&self, tag: u32) -> Vec<u8> {
        self.get(tag).map(<[u8]>::to_vec).unwrap_or_default()
    }

    pub fn set_vec_u8(&mut self, tag: u32, values: &[u8]) {
        self.set(tag, values.to_vec());
    }
}

fn read_chunk(tag: u32, bytes: &[u8], pos: &mut usize) -> Result<Chunk> {
    let size = read_int(bytes, pos)? as usize;
    let end = pos.checked_add(size).ok_or(Truncated { at: *pos })?;
    let data = bytes.get(*pos..end).ok_or(Truncated { at: *pos })?.to_vec();
    *pos = end;
    Ok(Chunk { tag, data })
}

/// An array of sub-structures: a count, then each entry's id followed by its
/// own terminated chunk stream.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Array {
    pub entries: Vec<(u32, Chunks)>,
}

impl Array {
    pub fn parse(bytes: &[u8]) -> Result<Array> {
        let mut pos = 0;
        let count = read_int(bytes, &mut pos)?;
        let mut entries = Vec::with_capacity(count as usize);
        for _ in 0..count {
            let id = read_int(bytes, &mut pos)?;
            entries.push((id, Chunks::parse_stream(bytes, &mut pos)?));
        }
        Ok(Array { entries })
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::new();
        write_int(self.entries.len() as u32, &mut out);
        for (id, chunks) in &self.entries {
            write_int(*id, &mut out);
            out.extend_from_slice(&chunks.to_bytes(true));
        }
        out
    }

    pub fn get(&self, id: u32) -> Option<&Chunks> {
        self.entries.iter().find(|(i, _)| *i == id).map(|(_, c)| c)
    }

    pub fn get_mut(&mut self, id: u32) -> Option<&mut Chunks> {
        self.entries.iter_mut().find(|(i, _)| *i == id).map(|(_, c)| c)
    }
}
