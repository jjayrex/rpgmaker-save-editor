//! The codec is checked against save data produced by a real Ruby, using the
//! same class shapes RGSS dumps. Re-serialising an untouched file must give
//! back the exact bytes it came from.

use rpgsave::marshal::{self, Heap, NodeKind, Value};

fn fixture(name: &str) -> Vec<u8> {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/");
    std::fs::read(format!("{path}{name}")).unwrap_or_else(|e| panic!("{name}: {e}"))
}

fn reload(name: &str) -> (Heap, Vec<Value>, Vec<u8>) {
    let data = fixture(name);
    let mut heap = Heap::new();
    let values = marshal::load_stream(&data, &mut heap).expect("load");
    let out = marshal::dump_stream(&values, &heap);
    (heap, values, out)
}

fn assert_byte_identical(name: &str) -> (Heap, Vec<Value>) {
    let data = fixture(name);
    let (heap, values, out) = reload(name);
    if out != data {
        let at = out
            .iter()
            .zip(&data)
            .position(|(a, b)| a != b)
            .unwrap_or(out.len().min(data.len()));
        panic!(
            "{name}: re-serialised bytes differ at offset {at} \
             (ours {} bytes, ruby {} bytes)\n ours: {:02x?}\n ruby: {:02x?}",
            out.len(),
            data.len(),
            &out[at.saturating_sub(8)..(at + 16).min(out.len())],
            &data[at.saturating_sub(8)..(at + 16).min(data.len())],
        );
    }
    (heap, values)
}

/// A modern Ruby interns equal floats as flonums and writes a back-reference
/// for the second `0.0`, where RGSS3's Ruby 1.9.2 would write the float twice.
/// Either way the links in the file are preserved exactly as they were found,
/// so the bytes come back identical.
#[test]
fn ace_save_round_trips_byte_for_byte() {
    let (heap, values) = assert_byte_identical("ace_save.rvdata2");
    assert_eq!(values.len(), 2, "VX Ace saves hold a header and a contents hash");

    let header = values[0];
    let playtime = heap
        .hash_get(header, heap.sym_id("playtime_s").map(Value::Sym).unwrap())
        .and_then(|v| heap.string(v));
    assert_eq!(playtime.as_deref(), Some("00:25:20"));

    let contents = values[1];
    let party = heap
        .hash_get(contents, Value::Sym(heap.sym_id("party").unwrap()))
        .unwrap();
    assert_eq!(heap.class_name(party), Some("Game_Party"));
    assert_eq!(heap.ivar_int(party, "@gold"), Some(12_345));
    assert_eq!(heap.ivar_int(party, "@steps"), Some(4_207));
}

#[test]
fn xp_save_round_trips_byte_for_byte() {
    let (heap, values) = assert_byte_identical("xp_save.rxdata");
    assert_eq!(values.len(), 12, "XP saves are twelve documents in a row");
    assert_eq!(values[1], Value::Int(3 * 3600 * 40), "frame count");
    assert_eq!(heap.class_name(values[8]), Some("Game_Party"));
    assert_eq!(heap.ivar_int(values[8], "@gold"), Some(12_345));
}

#[test]
fn vx_save_round_trips_byte_for_byte() {
    let (heap, values) = assert_byte_identical("vx_save.rvdata");
    assert_eq!(values.len(), 14, "VX saves are fourteen documents in a row");
    assert_eq!(values[1], Value::Int(91_235), "frame count");
    assert_eq!(heap.class_name(values[10]), Some("Game_Party"));
    assert_eq!(heap.ivar_int(values[10], "@gold"), Some(12_345));
}

#[test]
fn edge_cases_round_trip_byte_for_byte() {
    let (heap, values) = assert_byte_identical("edge_cases.bin");
    let root = values[0];

    // Object links: the three entries are one and the same Ruby String.
    let links = heap.hash_get(root, heap.new_str_key("links")).unwrap();
    let items = heap.array(links).unwrap();
    assert_eq!(items[0], items[1]);
    assert_eq!(items[1], items[2]);

    // A hash that contains itself resolves through the object table.
    let cycle = heap.hash_get(root, heap.new_str_key("cycle")).unwrap();
    assert_eq!(cycle, root);

    // RGSS `Table` arrives as a `_dump` payload we can inspect.
    let table = heap.hash_get(root, heap.new_str_key("table")).unwrap();
    assert_eq!(heap.class_name(table), Some("Table"));
    let NodeKind::UserDef { data, .. } = heap.kind(table.as_ref().unwrap()) else {
        panic!("Table should decode as a user-defined dump");
    };
    assert_eq!(u32::from_le_bytes(data[4..8].try_into().unwrap()), 4, "xsize");
}

/// A helper so the tests can look values up by string key without repeating
/// the interning dance.
trait StrKey {
    fn new_str_key(&self, s: &str) -> Value;
}

impl StrKey for Heap {
    fn new_str_key(&self, s: &str) -> Value {
        // Keys are compared structurally, so a throwaway heap node is fine —
        // but we cannot mutate a shared heap here, so scan instead.
        for id in 0..self.len() as u32 {
            if let NodeKind::Str(b) = self.kind(id)
                && b == s.as_bytes()
            {
                return Value::Ref(id);
            }
        }
        panic!("no string node {s:?} in heap");
    }
}

/// The authoritative check: hand our re-serialised bytes to a real Ruby and
/// confirm they load back into the same object graph. Skipped when `ruby` is
/// not installed — the byte-identity tests above still cover the codec.
#[test]
fn ruby_loads_what_we_write() {
    let fixtures = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures");
    if std::process::Command::new("ruby").arg("-v").output().is_err() {
        eprintln!("skipping: ruby is not installed");
        return;
    }
    for name in ["ace_save.rvdata2", "vx_save.rvdata", "xp_save.rxdata", "edge_cases.bin"] {
        let (_, _, out) = reload(name);
        let rewritten = std::env::temp_dir().join(format!("rpgsave-rewritten-{name}"));
        std::fs::write(&rewritten, &out).expect("write");

        let result = std::process::Command::new("ruby")
            .arg("verify.rb")
            .arg(format!("{fixtures}/{name}"))
            .arg(&rewritten)
            .current_dir(fixtures)
            .output()
            .expect("run ruby");
        let stdout = String::from_utf8_lossy(&result.stdout);
        assert!(
            stdout.starts_with("MATCH"),
            "{name}: ruby refused our output\nstdout: {stdout}\nstderr: {}",
            String::from_utf8_lossy(&result.stderr)
        );
        let _ = std::fs::remove_file(&rewritten);
    }
}

/// Ruby picks between `100` and `1e2` by where the decimal point falls; the
/// encoder has to make the same choice or untouched saves stop round-tripping.
/// The table is generated by `fixtures/generate.rb` from a real Ruby.
#[test]
fn floats_encode_exactly_as_ruby_does() {
    let table = String::from_utf8(fixture("floats.txt")).expect("utf-8");
    let mut checked = 0;

    for line in table.lines() {
        let (bits, expected) = line.split_once('\t').expect("two columns");
        let value = f64::from_bits(u64::from_str_radix(bits, 16).expect("hex bits"));

        let mut heap = Heap::new();
        let float = heap.new_float(value);
        let encoded = marshal::dump(float, &heap);
        let payload = String::from_utf8_lossy(&encoded[4..]).into_owned();
        assert_eq!(payload, expected, "encoding {value:?} ({bits})");

        // And it has to read back as the very same double.
        let mut heap = Heap::new();
        let decoded = marshal::load(&encoded, &mut heap).expect("load");
        assert_eq!(
            heap.number(decoded).map(f64::to_bits),
            Some(value.to_bits()),
            "round-tripping {value:?}"
        );
        checked += 1;
    }
    assert!(checked >= 40, "the reference table should not be nearly empty");
}

/// Every Ruby has formatted floats differently — 1.8 used C's `%g`, so RGSS1
/// writes `100` where RGSS3 writes `1e2`. Rather than encode each engine's
/// rules, a float that was read and not changed is written back verbatim.
#[test]
fn floats_are_written_back_exactly_as_they_were_read() {
    // `100` is what Ruby 1.8 wrote; a modern Ruby would spell it `1e2`.
    let legacy = b"\x04\x08[\x07f\x08100f\x0c0.00001";
    let mut heap = Heap::new();
    let values = marshal::load(legacy, &mut heap).expect("load");

    let items = heap.array(values).expect("array");
    assert_eq!(heap.number(items[0]), Some(100.0));
    assert_eq!(heap.number(items[1]), Some(0.00001));
    assert_eq!(marshal::dump(values, &heap), legacy, "spelling preserved");

    // A float the editor creates has no original spelling to keep, so it gets
    // the modern one.
    let fresh = heap.new_float(100.0);
    let encoded = marshal::dump(fresh, &heap);
    assert_eq!(&encoded[2..], b"f\x081e2");

    // And a value that no longer matches its text is re-formatted.
    let mut heap = Heap::new();
    let reparsed = marshal::load(&encoded, &mut heap).expect("load");
    assert_eq!(heap.number(reparsed), Some(100.0));
}
