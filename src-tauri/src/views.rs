//! Turning the save-file model into the shapes the interface renders.

use rpgsave::marshal::{Heap, NodeKind, Value};
use rpgsave::rpg::actor::DEFAULT_SLOTS;
use rpgsave::rpg::save::{preview, ItemKind, MAX_ITEM_COUNT};
use rpgsave::rpg::{format_playtime, GameData, SaveFile};
use rpgsave_protocol::*;

use crate::state::scalar_of;

/// Instance variables that already have a dedicated editor on the actor page.
const ACTOR_HANDLED: &[&str] = &[
    "@actor_id", "@name", "@nickname", "@class_id", "@level", "@exp", "@exp_list",
    "@hp", "@mp", "@tp", "@param_plus", "@equips", "@skills", "@states",
    "@state_turns", "@state_steps", "@weapon_id", "@armor1_id", "@armor2_id",
    "@armor3_id", "@armor4_id", "@maxhp_plus", "@maxmp_plus", "@atk_plus",
    "@def_plus", "@spi_plus", "@agi_plus",
];

pub fn summary(save: &SaveFile, data: Option<&GameData>) -> Summary {
    let party_ids = save.party_member_ids();
    let party = party_ids
        .iter()
        .map(|id| party_member(save, data, *id))
        .collect();

    Summary {
        path: save.path.to_string_lossy().into_owned(),
        file_name: save
            .path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
        engine: save.engine.id().to_owned(),
        engine_label: save.engine.label().to_owned(),
        dirty: save.dirty,
        file_size: save.size_bytes,
        document_count: save.documents.len(),
        notes: save.notes.clone(),

        game_title: data.and_then(|d| d.game_title.clone()),
        data_dir: data.map(|d| d.dir.to_string_lossy().into_owned()),
        data_loaded: data.map(|d| d.loaded.clone()).unwrap_or_default(),
        data_missing: data.map(|d| d.missing.clone()).unwrap_or_default(),

        has_playtime: save.playtime_frames().is_some(),
        playtime_seconds: save.playtime_seconds().unwrap_or(0),
        playtime_text: format_playtime(save.playtime_seconds().unwrap_or(0)),
        save_count: save.system().and_then(|s| save.heap.ivar_int(s, "@save_count")),

        gold: save.gold(),
        currency: data
            .and_then(|d| d.currency.clone())
            .unwrap_or_else(|| "G".to_owned()),
        steps: save.steps(),

        map_id: save.map_id(),
        map_name: save
            .map_id()
            .and_then(|id| data?.map_name(id).map(str::to_owned)),
        player_x: save.player_position().map(|p| p.0),
        player_y: save.player_position().map(|p| p.1),

        party,
        roster: roster(save, data),

        switch_count: save.data_len("Game_Switches").saturating_sub(1),
        variable_count: save.data_len("Game_Variables").saturating_sub(1),
        self_switch_count: save.self_switches().len(),
    }
}

fn party_member(save: &SaveFile, data: Option<&GameData>, actor_id: i64) -> PartyMember {
    let actor = save.actor(actor_id);
    let name = actor
        .and_then(|a| save.actor_string(a, "@name"))
        .or_else(|| data?.actor(actor_id).map(|a| a.name.clone()))
        .unwrap_or_else(|| format!("Actor {actor_id}"));
    let class_id = actor.and_then(|a| save.actor_class_id(a));
    PartyMember {
        actor_id,
        name,
        level: actor.and_then(|a| save.actor_level(a)).unwrap_or(0),
        class_name: class_id.and_then(|c| data?.class_name(c).map(str::to_owned)),
        hp: actor.and_then(|a| save.heap.ivar_int(a, "@hp")),
        mp: actor.and_then(|a| save.heap.ivar_int(a, "@mp")),
    }
}

/// Everyone the editor can show: actors already in the save, plus any the
/// database defines that the game has not created yet.
fn roster(save: &SaveFile, data: Option<&GameData>) -> Vec<RosterEntry> {
    let party = save.party_member_ids();
    let mut rows: Vec<RosterEntry> = save
        .actors()
        .into_iter()
        .map(|(id, actor)| RosterEntry {
            actor_id: id,
            name: save
                .actor_string(actor, "@name")
                .unwrap_or_else(|| format!("Actor {id}")),
            in_save: true,
            in_party: party.contains(&id),
        })
        .collect();

    if let Some(data) = data {
        for info in &data.actors {
            if rows.iter().any(|r| r.actor_id == info.id) {
                continue;
            }
            rows.push(RosterEntry {
                actor_id: info.id,
                name: info.name.clone(),
                in_save: false,
                in_party: party.contains(&info.id),
            });
        }
    }
    rows.sort_by_key(|r| r.actor_id);
    rows
}

pub fn actor_view(save: &SaveFile, data: Option<&GameData>, actor_id: i64) -> Option<ActorView> {
    let actor = save.actor(actor_id)?;
    let heap = &save.heap;
    let class_id = save.actor_class_id(actor);
    let level = save.actor_level(actor);

    let params = save
        .actor_param_plus(actor)
        .into_iter()
        .map(|(label, ivar, index, plus)| ParamView {
            index,
            label: label.to_owned(),
            plus,
            base: save.actor_param_base(actor, index as u32, data),
            ivar,
        })
        .collect::<Vec<_>>();

    let equips = save
        .actor_equips(actor)
        .into_iter()
        .map(|slot| EquipSlot {
            label: data
                .and_then(|d| d.equip_type_name(slot.slot))
                .or_else(|| DEFAULT_SLOTS.get(slot.slot).copied())
                .unwrap_or("Equipment")
                .to_owned(),
            kind: slot.kind.id().to_owned(),
            item_name: data
                .and_then(|d| d.entry_name(slot.kind, slot.item_id))
                .map(str::to_owned),
            slot: slot.slot,
            item_id: slot.item_id,
        })
        .collect();

    let skills = save
        .actor_id_list(actor, "@skills")
        .into_iter()
        .map(|id| NamedId {
            id,
            name: data
                .and_then(|d| d.skill_name(id))
                .map(str::to_owned)
                .unwrap_or_else(|| format!("Skill {id}")),
        })
        .collect();

    let states = save
        .actor_id_list(actor, "@states")
        .into_iter()
        .map(|id| NamedId {
            id,
            name: data
                .and_then(|d| d.state_name(id))
                .map(str::to_owned)
                .unwrap_or_else(|| format!("State {id}")),
        })
        .collect();

    let extra = heap
        .ivar_names(actor)
        .into_iter()
        .filter(|name| !ACTOR_HANDLED.contains(&name.as_str()))
        .filter_map(|name| {
            let value = heap.ivar(actor, &name)?;
            Some(FieldView {
                label: humanize(&name),
                value: scalar_of(heap, value)?,
                ivar: name,
            })
        })
        .collect();

    let bonus = |index: usize| params.get(index).map(|p| p.plus).unwrap_or(0);
    Some(ActorView {
        actor_id,
        name: save.actor_string(actor, "@name").unwrap_or_default(),
        nickname: save.actor_string(actor, "@nickname"),
        class_id,
        class_name: class_id.and_then(|c| data?.class_name(c).map(str::to_owned)),
        classes: data
            .map(|d| {
                d.classes
                    .iter()
                    .map(|c| NamedId { id: c.id, name: c.name.clone() })
                    .collect()
            })
            .unwrap_or_default(),
        in_party: save.party_member_ids().contains(&actor_id),

        level,
        max_level: save.actor_max_level(actor, data),
        exp: save.actor_exp(actor),
        exp_this_level: level.and_then(|l| save.exp_for_level(actor, l, data)),
        exp_next_level: level.and_then(|l| save.exp_for_level(actor, l + 1, data)),
        exp_follows_level: level
            .and_then(|l| save.exp_for_level(actor, l, data))
            .is_some(),

        hp: heap.ivar_int(actor, "@hp"),
        mp: heap.ivar_int(actor, "@mp"),
        tp: heap.ivar_number(actor, "@tp"),
        max_hp: save.actor_param_base(actor, 0, data).map(|b| b + bonus(0)),
        max_mp: save.actor_param_base(actor, 1, data).map(|b| b + bonus(1)),

        params,
        equips,
        skills,
        states,
        extra,
    })
}

pub fn inventory(save: &SaveFile, data: Option<&GameData>, kind: ItemKind) -> InventoryView {
    let rows = save
        .inventory(kind)
        .into_iter()
        .map(|(id, count)| {
            let entry = data.and_then(|d| d.catalog(kind).iter().find(|e| e.id == id));
            ItemRow {
                id,
                count,
                name: entry.map(|e| e.name.clone()),
                icon_index: entry.map(|e| e.icon_index).unwrap_or(0),
                description: entry.map(|e| e.description.clone()).unwrap_or_default(),
                price: entry.map(|e| e.price).unwrap_or(0),
            }
        })
        .collect();
    InventoryView {
        kind: kind.id().to_owned(),
        rows,
        max_count: MAX_ITEM_COUNT,
    }
}

/// The database rows the pickers offer, filtered by a search box.
///
/// `kind` covers the three inventory containers plus skills and states, which
/// are picked the same way but are not carried as items.
pub fn catalog(
    save: &SaveFile,
    data: Option<&GameData>,
    kind: &str,
    query: &str,
) -> Vec<CatalogEntry> {
    let Some(data) = data else { return Vec::new() };
    let item_kind = ItemKind::from_id(kind);
    let entries: &[rpgsave::rpg::gamedata::Entry] = match (item_kind, kind) {
        (Some(k), _) => data.catalog(k),
        (None, "skill") => &data.skills,
        (None, "state") => &data.states,
        _ => return Vec::new(),
    };
    let held = item_kind.map(|k| save.inventory(k)).unwrap_or_default();
    let query = query.trim().to_lowercase();
    entries
        .iter()
        .filter(|e| {
            query.is_empty()
                || e.name.to_lowercase().contains(&query)
                || e.id.to_string() == query
        })
        .map(|e| CatalogEntry {
            id: e.id,
            name: e.name.clone(),
            icon_index: e.icon_index,
            description: e.description.clone(),
            price: e.price,
            etype_id: e.etype_id,
            held: held
                .iter()
                .find(|(id, _)| *id == e.id)
                .map(|(_, c)| *c)
                .unwrap_or(0),
        })
        .collect()
}

pub fn switch_page(
    save: &SaveFile,
    data: Option<&GameData>,
    query: &str,
    offset: usize,
    limit: usize,
) -> SwitchPage {
    let total = save.data_len("Game_Switches").saturating_sub(1);
    let query = query.trim().to_lowercase();
    let matches: Vec<i64> = (1..=total as i64)
        .filter(|id| matches_query(&query, *id, data.and_then(|d| d.switch_name(*id))))
        .collect();
    SwitchPage {
        total,
        matched: matches.len(),
        rows: matches
            .into_iter()
            .skip(offset)
            .take(limit)
            .map(|id| SwitchRow {
                id,
                name: data.and_then(|d| d.switch_name(id)).map(str::to_owned),
                on: save.switch(id),
            })
            .collect(),
    }
}

pub fn variable_page(
    save: &SaveFile,
    data: Option<&GameData>,
    query: &str,
    offset: usize,
    limit: usize,
) -> VariablePage {
    let total = save.data_len("Game_Variables").saturating_sub(1);
    let query = query.trim().to_lowercase();
    let matches: Vec<i64> = (1..=total as i64)
        .filter(|id| matches_query(&query, *id, data.and_then(|d| d.variable_name(*id))))
        .collect();
    let matched = matches.len();
    let rows = matches
        .into_iter()
        .skip(offset)
        .take(limit)
        .map(|id| {
            let value = save.variable(id).unwrap_or(Value::Nil);
            let scalar = scalar_of(&save.heap, value);
            VariableRow {
                id,
                name: data.and_then(|d| d.variable_name(id)).map(str::to_owned),
                // Anything that is not a plain value still gets a description,
                // so the row shows what is there instead of pretending it is nil.
                complex: match scalar {
                    Some(_) => None,
                    None => Some(preview(&save.heap, value)),
                },
                value: scalar.unwrap_or(Scalar::Nil),
            }
        })
        .collect();
    VariablePage { total, matched, rows }
}

fn matches_query(query: &str, id: i64, name: Option<&str>) -> bool {
    if query.is_empty() {
        return true;
    }
    if id.to_string().contains(query) {
        return true;
    }
    name.is_some_and(|n| n.to_lowercase().contains(query))
}

pub fn self_switches(save: &SaveFile, data: Option<&GameData>) -> Vec<SelfSwitchRow> {
    save.self_switches()
        .into_iter()
        .map(|(map_id, event_id, letter, on)| SelfSwitchRow {
            map_name: data.and_then(|d| d.map_name(map_id)).map(str::to_owned),
            map_id,
            event_id,
            letter,
            on,
        })
        .collect()
}

// -------------------------------------------------------------- raw editor

/// The top of the tree: one entry per Marshal document in the file.
pub fn raw_root(save: &SaveFile) -> NodeView {
    let children = save
        .documents
        .iter()
        .enumerate()
        .map(|(i, value)| ChildView {
            slot: Slot::Document(i),
            key: format!("Document {i}"),
            preview: preview(&save.heap, *value),
            node: value.as_ref(),
            scalar: scalar_of(&save.heap, *value),
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
        summary: format!("{} Marshal documents", save.documents.len()),
        children,
        binary: None,
    }
}

pub fn node_view(heap: &Heap, id: u32) -> Option<NodeView> {
    let node = heap.get(id)?;
    let mut children = Vec::new();
    let mut binary = None;

    let child = |heap: &Heap, slot: Slot, key: String, value: Value| ChildView {
        slot,
        key,
        preview: preview(heap, value),
        node: value.as_ref(),
        scalar: scalar_of(heap, value),
    };

    match &node.kind {
        NodeKind::Array(items) => {
            for (i, value) in items.iter().enumerate() {
                children.push(child(heap, Slot::Index(i), format!("[{i}]"), *value));
            }
        }
        NodeKind::Hash { entries, default } => {
            for (i, (key, value)) in entries.iter().enumerate() {
                let label = preview(heap, *key);
                children.push(child(heap, Slot::Entry(i), label, *value));
                if key.as_ref().is_some() {
                    // Composite keys are worth exposing so they can be inspected.
                    children.push(child(
                        heap,
                        Slot::EntryKey(i),
                        format!("  key of {i}"),
                        *key,
                    ));
                }
            }
            if let Some(d) = default {
                children.push(child(heap, Slot::Default, "(default)".to_owned(), *d));
            }
        }
        NodeKind::Struct { fields, .. } => {
            for (name, value) in fields {
                let name = heap.sym(*name).to_owned();
                children.push(child(heap, Slot::Field(name.clone()), name, *value));
            }
        }
        NodeKind::UserMarshal { value, .. } => {
            children.push(child(heap, Slot::Inner, "(marshal_dump)".to_owned(), *value));
        }
        NodeKind::UserClass { inner, .. } | NodeKind::Extended { inner, .. } => {
            children.push(child(heap, Slot::Inner, "(wrapped)".to_owned(), *inner));
        }
        NodeKind::UserDef { class, data } => {
            let class = heap.sym(*class).to_owned();
            let table = rpgsave::rpg::Table::parse(data);
            binary = Some(BinaryView {
                description: rpgsave::rpg::rgss::describe(&class, data),
                class,
                bytes: data.len(),
                preview: table
                    .map(|t| t.data.iter().take(64).map(|v| *v as i64).collect())
                    .unwrap_or_default(),
            });
        }
        _ => {}
    }

    for (name, value) in &node.ivars {
        let name = heap.sym(*name).to_owned();
        children.push(child(heap, Slot::Ivar(name.clone()), name, *value));
    }

    Some(NodeView {
        node: Some(id),
        title: heap
            .class_name(Value::Ref(id))
            .map(str::to_owned)
            .unwrap_or_else(|| node.kind.tag().to_owned()),
        kind: node.kind.tag().to_owned(),
        summary: preview(heap, Value::Ref(id)),
        children,
        binary,
    })
}

/// "@character_name" becomes "Character name".
pub fn humanize(ivar: &str) -> String {
    let trimmed = ivar.trim_start_matches('@').replace('_', " ");
    let mut chars = trimmed.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => trimmed,
    }
}
