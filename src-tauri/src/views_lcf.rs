//! Browsing the chunk tree of an RPG Maker 2000/2003 save.
//!
//! These saves have no database to name things, so being able to look at the
//! structure directly matters more here than on the RGSS engines.

use rpgsave::lcf::chunk::{read_int, Array, Chunks};
use rpgsave::lcf::LcfSave;
use rpgsave_protocol::*;

use crate::backend::Reply;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Shape {
    Struct,
    Array,
}

/// The top level of a save, from liblcf's definitions.
const TOP: [(u32, &str, Shape); 15] = [
    (0x64, "title", Shape::Struct),
    (0x65, "system", Shape::Struct),
    (0x66, "screen", Shape::Struct),
    (0x67, "pictures", Shape::Array),
    (0x68, "party_location", Shape::Struct),
    (0x69, "boat_location", Shape::Struct),
    (0x6A, "ship_location", Shape::Struct),
    (0x6B, "airship_location", Shape::Struct),
    (0x6C, "actors", Shape::Array),
    (0x6D, "inventory", Shape::Struct),
    (0x6E, "targets", Shape::Array),
    (0x6F, "map_info", Shape::Struct),
    (0x70, "panorama", Shape::Struct),
    (0x71, "foreground_event_execstate", Shape::Struct),
    (0x72, "common_events", Shape::Array),
];

/// Field names for the structures this editor understands. Anything else is
/// shown by its chunk number alone.
fn field_name(top_tag: u32, tag: u32) -> Option<&'static str> {
    let table: &[(u32, &str)] = match top_tag {
        0x64 => &[
            (0x01, "timestamp"), (0x0B, "hero_name"), (0x0C, "hero_level"), (0x0D, "hero_hp"),
            (0x15, "face1_name"), (0x16, "face1_id"), (0x17, "face2_name"), (0x18, "face2_id"),
        ],
        0x65 => &[
            (0x0B, "frame_count"), (0x1F, "switch_count"), (0x20, "switches"),
            (0x21, "variable_count"), (0x22, "variables"), (0x83, "save_count"),
        ],
        0x68..=0x6B => &[
            (0x01, "active"), (0x0B, "map_id"), (0x0C, "position_x"), (0x0D, "position_y"),
            (0x15, "direction"), (0x16, "facing"), (0x49, "sprite_name"),
        ],
        0x6C => &[
            (0x01, "name"), (0x02, "title"), (0x0B, "sprite_name"), (0x1F, "level"),
            (0x20, "exp"), (0x21, "hp_mod"), (0x22, "sp_mod"), (0x29, "attack_mod"),
            (0x2A, "defense_mod"), (0x2B, "spirit_mod"), (0x2C, "agility_mod"),
            (0x33, "skill_count"), (0x34, "skills"), (0x3D, "equipped"),
            (0x47, "current_hp"), (0x48, "current_sp"), (0x5A, "class_id"),
        ],
        0x6D => &[
            (0x01, "party_count"), (0x02, "party"), (0x0B, "item_count"), (0x0C, "item_ids"),
            (0x0D, "item_counts"), (0x0E, "item_usage"), (0x15, "gold"), (0x2A, "steps"),
        ],
        _ => &[],
    };
    table.iter().find(|(t, _)| *t == tag).map(|(_, name)| *name)
}

/// Node numbers are derived from the position in the file, so they mean the
/// same thing on the next request without anything being remembered.
fn struct_id(index: usize) -> u32 {
    (index as u32 + 1) * 10_000
}

fn element_id(index: usize, element: usize) -> u32 {
    struct_id(index) + element as u32 + 1
}

fn decode(node: u32) -> (usize, Option<usize>) {
    let index = (node / 10_000) as usize - 1;
    let element = (node % 10_000) as usize;
    (index, element.checked_sub(1))
}

/// What a chunk's bytes appear to hold.
fn describe(data: &[u8]) -> (String, Option<Scalar>) {
    if data.is_empty() {
        return ("(empty)".to_owned(), None);
    }
    let mut pos = 0;
    if let Ok(value) = read_int(data, &mut pos)
        && pos == data.len()
    {
        return (value.to_string(), Some(Scalar::Int(i64::from(value))));
    }
    if data.iter().all(|b| (0x20..0x7f).contains(b)) {
        let text = String::from_utf8_lossy(data).into_owned();
        return (format!("{text:?}"), Some(Scalar::Str(text)));
    }
    let preview: Vec<String> = data.iter().take(8).map(|b| format!("{b:02x}")).collect();
    (format!("{} bytes · {}…", data.len(), preview.join(" ")), None)
}

fn children_of(top_tag: u32, chunks: &Chunks) -> Vec<ChildView> {
    chunks
        .iter()
        .map(|chunk| {
            let (preview, scalar) = describe(&chunk.data);
            let key = match field_name(top_tag, chunk.tag) {
                Some(name) => format!("{name} (0x{:02x})", chunk.tag),
                None => format!("0x{:02x}", chunk.tag),
            };
            ChildView { slot: Slot::Ivar(chunk.tag.to_string()), key, preview, node: None, scalar }
        })
        .collect()
}

pub fn raw_root(save: &LcfSave) -> NodeView {
    let children = TOP
        .iter()
        .enumerate()
        .filter_map(|(index, (tag, name, shape))| {
            let data = save.chunk(*tag)?;
            let preview = match shape {
                Shape::Array => {
                    let count = Array::parse(data).map(|a| a.entries.len()).unwrap_or(0);
                    format!("{count} entries · {} bytes", data.len())
                }
                Shape::Struct => format!("{} bytes", data.len()),
            };
            Some(ChildView {
                slot: Slot::Ivar(tag.to_string()),
                key: format!("{name} (0x{tag:02x})"),
                preview,
                node: Some(struct_id(index)),
                scalar: None,
            })
        })
        .collect();

    NodeView {
        node: None,
        title: save
            .path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Save file".to_owned()),
        kind: "file".to_owned(),
        summary: "RPG Maker 2000/2003 save".to_owned(),
        children,
        binary: None,
    }
}

pub fn raw_node(save: &LcfSave, node: u32) -> Option<NodeView> {
    let (index, element) = decode(node);
    let (tag, name, shape) = TOP.get(index).copied()?;
    let data = save.chunk(tag)?;

    match (shape, element) {
        // An array, listing its entries.
        (Shape::Array, None) => {
            let array = Array::parse(data).ok()?;
            let children = array
                .entries
                .iter()
                .enumerate()
                .map(|(at, (id, chunks))| ChildView {
                    slot: Slot::Index(at),
                    key: format!("#{id}"),
                    preview: format!("{} fields", chunks.iter().count()),
                    node: Some(element_id(index, at)),
                    scalar: None,
                })
                .collect();
            Some(NodeView {
                node: Some(node),
                title: name.to_owned(),
                kind: "array".to_owned(),
                summary: format!("{} entries", array.entries.len()),
                children,
                binary: None,
            })
        }
        // One entry of an array.
        (Shape::Array, Some(at)) => {
            let array = Array::parse(data).ok()?;
            let (id, chunks) = array.entries.get(at)?;
            Some(NodeView {
                node: Some(node),
                title: format!("{name} #{id}"),
                kind: "object".to_owned(),
                summary: format!("{} fields", chunks.iter().count()),
                children: children_of(tag, chunks),
                binary: None,
            })
        }
        // A plain structure.
        (Shape::Struct, _) => {
            let mut pos = 0;
            let chunks = Chunks::parse_stream(data, &mut pos).ok()?;
            Some(NodeView {
                node: Some(node),
                title: name.to_owned(),
                kind: "object".to_owned(),
                summary: format!("{} fields", chunks.iter().count()),
                children: children_of(tag, &chunks),
                binary: None,
            })
        }
    }
}

pub fn set_raw_scalar(
    save: &mut LcfSave,
    node: Option<u32>,
    slot: &Slot,
    value: &Scalar,
) -> Reply<()> {
    let node = node.ok_or("Choose a structure to edit a field of.")?;
    let Slot::Ivar(tag) = slot else {
        return Err("Only fields can be edited here.".to_owned());
    };
    let tag: u32 = tag.parse().map_err(|_| "That field cannot be edited.".to_owned())?;

    let (index, element) = decode(node);
    let (top_tag, _, _) = *TOP.get(index).ok_or("No such structure in this save.")?;

    // Work out what to write before touching the save, so a value this format
    // cannot hold is refused rather than half-applied.
    let payload: Box<dyn FnOnce(&mut Chunks)> = match value {
        Scalar::Int(v) => {
            let v = (*v).clamp(0, i64::from(u32::MAX)) as u32;
            Box::new(move |chunks: &mut Chunks| chunks.set_int(tag, v))
        }
        Scalar::Bool(v) => {
            let v = *v;
            Box::new(move |chunks: &mut Chunks| chunks.set_bool(tag, v))
        }
        Scalar::Str(text) => {
            let bytes = text.as_bytes().to_vec();
            Box::new(move |chunks: &mut Chunks| chunks.set(tag, bytes))
        }
        Scalar::Float(_) | Scalar::Sym(_) | Scalar::Nil => {
            return Err("This format stores whole numbers and text only.".to_owned());
        }
    };

    match element {
        Some(at) => {
            if !save.update_array_entry(top_tag, at, payload) {
                return Err("That entry is no longer in this save.".to_owned());
            }
        }
        None => save.update_struct(top_tag, payload),
    }
    Ok(())
}
