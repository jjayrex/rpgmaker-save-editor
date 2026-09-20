//! The RGSS built-in classes that marshal themselves through `_dump`.

/// `Table` — the 1/2/3-dimensional Int16 grid used for map data and stat curves.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Table {
    pub dim: u32,
    pub xsize: u32,
    pub ysize: u32,
    pub zsize: u32,
    pub data: Vec<i16>,
}

impl Table {
    /// Decodes a `Table#_dump` payload: five little-endian `u32`s followed by
    /// `xsize * ysize * zsize` little-endian `i16`s.
    pub fn parse(bytes: &[u8]) -> Option<Table> {
        if bytes.len() < 20 {
            return None;
        }
        let word = |i: usize| u32::from_le_bytes(bytes[i * 4..i * 4 + 4].try_into().unwrap());
        let (dim, xsize, ysize, zsize, count) = (word(0), word(1), word(2), word(3), word(4));
        let count = count as usize;
        if bytes.len() < 20 + count * 2 {
            return None;
        }
        let data = bytes[20..20 + count * 2]
            .chunks_exact(2)
            .map(|c| i16::from_le_bytes([c[0], c[1]]))
            .collect();
        Some(Table { dim, xsize, ysize, zsize, data })
    }

    pub fn get(&self, x: u32, y: u32, z: u32) -> Option<i16> {
        if x >= self.xsize || y >= self.ysize || z >= self.zsize {
            return None;
        }
        let index = (z * self.ysize * self.xsize + y * self.xsize + x) as usize;
        self.data.get(index).copied()
    }

    pub fn summary(&self) -> String {
        match self.dim {
            1 => format!("Table({})", self.xsize),
            2 => format!("Table({}×{})", self.xsize, self.ysize),
            _ => format!("Table({}×{}×{})", self.xsize, self.ysize, self.zsize),
        }
    }
}

/// `Color` and `Tone` both dump as four little-endian doubles.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Quad(pub f64, pub f64, pub f64, pub f64);

impl Quad {
    pub fn parse(bytes: &[u8]) -> Option<Quad> {
        if bytes.len() < 32 {
            return None;
        }
        let f = |i: usize| f64::from_le_bytes(bytes[i * 8..i * 8 + 8].try_into().unwrap());
        Some(Quad(f(0), f(1), f(2), f(3)))
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(32);
        for v in [self.0, self.1, self.2, self.3] {
            out.extend_from_slice(&v.to_le_bytes());
        }
        out
    }
}

/// `Rect` dumps as four little-endian `i32`s.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rect(pub i32, pub i32, pub i32, pub i32);

impl Rect {
    pub fn parse(bytes: &[u8]) -> Option<Rect> {
        if bytes.len() < 16 {
            return None;
        }
        let i = |n: usize| i32::from_le_bytes(bytes[n * 4..n * 4 + 4].try_into().unwrap());
        Some(Rect(i(0), i(1), i(2), i(3)))
    }
}

/// Human-readable one-liner for a `_dump` payload of a known RGSS class.
pub fn describe(class: &str, data: &[u8]) -> String {
    match class {
        "Table" => Table::parse(data)
            .map(|t| t.summary())
            .unwrap_or_else(|| format!("Table (unreadable, {} bytes)", data.len())),
        "Color" | "Tone" => Quad::parse(data)
            .map(|q| format!("{class}({}, {}, {}, {})", q.0, q.1, q.2, q.3))
            .unwrap_or_else(|| format!("{class} (unreadable)")),
        "Rect" => Rect::parse(data)
            .map(|r| format!("Rect({}, {}, {}, {})", r.0, r.1, r.2, r.3))
            .unwrap_or_else(|| "Rect (unreadable)".to_owned()),
        _ => format!("{class} ({} bytes)", data.len()),
    }
}
