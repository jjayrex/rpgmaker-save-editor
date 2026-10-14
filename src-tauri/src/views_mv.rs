//! Browsing the JSON tree of an RPG Maker MV save.
//!
//! MV saves carry whatever the game's plugins put in them, which no editor can
//! know the shape of in advance, so this walks the tree as it finds it rather
//! than from a table of known fields.

use rpgsave::mv::json::{Json, Step};
use rpgsave::mv::MvSave;
use rpgsave_protocol::*;

use crate::backend::Reply;

/// A one-line description of a value, and its editable form when it has one.
fn describe(value: &Json) -> (String, Option<Scalar>) {
    match value {
        Json::Null => ("null".to_owned(), Some(Scalar::Nil)),
        Json::Bool(b) => (b.to_string(), Some(Scalar::Bool(*b))),
        Json::Number(n) => {
            let value = n.value();
            let scalar = if value.fract() == 0.0 && value.abs() < 9e15 {
                Scalar::Int(value as i64)
            } else {
                Scalar::Float(value)
            };
            (value.to_string(), Some(scalar))
        }
        Json::Str(text) => {
            (format!("{:?}", text.as_str()), Some(Scalar::Str(text.as_str().to_owned())))
        }
        Json::Array(items) => (format!("{} items", items.len()), None),
        Json::Object(members) => {
            let fields = members
                .iter()
                .filter(|(key, _)| !key.as_str().starts_with('@'))
                .count();
            // The structure is shown as it is in the file, `JsonEx`'s own
            // members included: an array it wrapped to give it an identity is
            // an object holding `@a`, and hiding that would be a lie about
            // what a path means.
            match (value.class_name(), value.elements()) {
                (_, Some(items)) => (format!("array of {}", items.len()), None),
                (Some(class), _) => (format!("{class} · {fields} fields"), None),
                (None, _) => (format!("{fields} fields"), None),
            }
        }
    }
}

fn kind_of(value: &Json) -> &'static str {
    match value {
        Json::Array(_) => "array",
        Json::Object(_) => "object",
        _ => "value",
    }
}

/// Lists what a value holds, numbering the children that can be opened.
fn children_of(save: &MvSave, path: &[Step], value: &Json) -> Vec<ChildView> {
    let child = |step: Step, key: String, child: &Json| {
        let (preview, scalar) = describe(child);
        let node = child.is_container().then(|| {
            let mut full = path.to_vec();
            full.push(step.clone());
            save.browse_id(&full)
        });
        ChildView {
            slot: match &step {
                Step::Key(key) => Slot::Ivar(key.clone()),
                Step::Index(at) => Slot::Index(*at),
            },
            key,
            preview,
            node,
            scalar,
        }
    };

    match value {
        Json::Object(members) => members
            .iter()
            .map(|(key, member)| {
                child(Step::Key(key.as_str().to_owned()), key.as_str().to_owned(), member)
            })
            .collect(),
        Json::Array(items) => items
            .iter()
            .enumerate()
            .map(|(at, item)| child(Step::Index(at), format!("[{at}]"), item))
            .collect(),
        _ => Vec::new(),
    }
}

fn node_view(save: &MvSave, node: Option<u32>, path: &[Step], value: &Json) -> NodeView {
    let title = match path.last() {
        Some(Step::Key(key)) => key.clone(),
        Some(Step::Index(at)) => format!("[{at}]"),
        None => save
            .path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Save file".to_owned()),
    };
    let (summary, _) = describe(value);
    NodeView {
        node,
        title,
        kind: if path.is_empty() { "file".to_owned() } else { kind_of(value).to_owned() },
        summary: if path.is_empty() {
            format!("RPG Maker MV save · {summary}")
        } else {
            summary
        },
        children: children_of(save, path, value),
        binary: None,
    }
}

pub fn raw_root(save: &MvSave) -> NodeView {
    node_view(save, None, &[], &save.root)
}

pub fn raw_node(save: &MvSave, node: u32) -> Option<NodeView> {
    let path = save.browse_path(node)?;
    let value = save.root.at(&path)?;
    Some(node_view(save, Some(node), &path, value))
}

pub fn set_raw_scalar(
    save: &mut MvSave,
    node: Option<u32>,
    slot: &Slot,
    value: &Scalar,
) -> Reply<()> {
    let new = match value {
        Scalar::Nil => Json::Null,
        Scalar::Bool(b) => Json::Bool(*b),
        Scalar::Int(i) => Json::int(*i),
        Scalar::Float(f) => Json::float(*f),
        Scalar::Str(s) => Json::text(s.clone()),
        // Symbols are a Ruby idea; MV has nothing to store one as.
        Scalar::Sym(_) => return Err("JSON has no symbols.".to_owned()),
    };

    // The root is addressed as "no node", so an edit there is a member of the
    // save's own top level.
    let path = match node {
        Some(node) => save.browse_path(node).ok_or("That value is no longer in this save.")?,
        None => Vec::new(),
    };
    let target = save
        .root
        .at_mut(&path)
        .ok_or("That value is no longer in this save.")?;

    let written = match slot {
        Slot::Ivar(key) | Slot::Field(key) => target.set(key, new),
        Slot::Index(at) => match target.elements_mut() {
            Some(items) if *at < items.len() => {
                items[*at] = new;
                true
            }
            _ => false,
        },
        _ => false,
    };
    if !written {
        return Err("That value cannot be edited here.".to_owned());
    }
    save.dirty = true;
    Ok(())
}
