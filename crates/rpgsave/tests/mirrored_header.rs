//! Some games mirror the whole game state into the save header so the load
//! screen can show gold, play time and party without loading the save proper.
//! Reads have to come from the copy the game will load, and writes have to
//! reach both — otherwise the editor changes what the load screen shows while
//! the game itself carries on with the old values.

use rpgsave::marshal::Value;
use rpgsave::rpg::{save::ItemKind, SaveFile};

fn open(name: &str) -> SaveFile {
    let path = format!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/{}"), name);
    SaveFile::open(std::path::Path::new(&path)).expect("open")
}

/// `Game_Party` appears twice: once in the header, once in the contents.
fn party_golds(save: &SaveFile) -> Vec<i64> {
    save.roots("Game_Party")
        .into_iter()
        .map(|party| save.heap.ivar_int(party, "@gold").unwrap_or(-1))
        .collect()
}

#[test]
fn the_fixture_really_does_hold_two_copies() {
    let save = open("mirrored_header.rvdata2");
    assert_eq!(save.roots("Game_Party").len(), 2);
    assert_eq!(save.roots("Game_Switches").len(), 2);
    assert_eq!(save.roots("Game_Actors").len(), 2);
    assert!(
        save.notes.iter().any(|n| n.contains("mirrors its whole state")),
        "the user should be told this file is unusual: {:?}",
        save.notes
    );
}

/// The header is not the contents, even when it looks like it.
#[test]
fn reads_come_from_the_document_the_game_loads() {
    let save = open("mirrored_header.rvdata2");
    let contents = save.documents[1];
    let party_key = Value::Sym(save.heap.sym_id("party").expect("the :party key"));
    let contents_party = save.heap.hash_get(contents, party_key).expect("party");

    assert_eq!(
        save.party(),
        Some(contents_party),
        "the party read back should be the one in the contents document"
    );
}

#[test]
fn editing_gold_changes_every_copy() {
    let mut save = open("mirrored_header.rvdata2");
    assert_eq!(party_golds(&save), vec![12_345, 12_345]);

    assert!(save.set_gold(5_000));
    assert_eq!(party_golds(&save), vec![5_000, 5_000]);

    let bytes = save.to_bytes();
    let reloaded = SaveFile::from_bytes(&bytes, None).expect("reload");
    assert_eq!(party_golds(&reloaded), vec![5_000, 5_000]);
    assert_eq!(reloaded.gold(), Some(5_000));
}

#[test]
fn every_other_edit_reaches_both_copies_too() {
    let mut save = open("mirrored_header.rvdata2");
    save.set_steps(99);
    save.set_switch(3, true);
    save.set_variable(2, Value::Int(-7));
    save.set_item_count(ItemKind::Item, 1, 42);
    save.set_party_member_ids(&[2, 1]);
    save.set_player_position(20, 3);
    save.set_self_switch(3, 4, "A", false);

    let bytes = save.to_bytes();
    let reloaded = SaveFile::from_bytes(&bytes, None).expect("reload");

    // Check the header copy directly, not through the accessors, which would
    // read the contents copy and hide a half-applied edit.
    let header = reloaded.documents[0];
    let key = |name: &str| Value::Sym(reloaded.heap.sym_id(name).expect("key"));
    let heap = &reloaded.heap;

    let party = heap.hash_get(header, key("party")).expect("party");
    assert_eq!(heap.ivar_int(party, "@steps"), Some(99));
    let members = heap.ivar(party, "@actors").and_then(|a| heap.array(a)).expect("members");
    assert_eq!(members, [Value::Int(2), Value::Int(1)]);
    let items = heap.ivar(party, "@items").expect("items");
    assert_eq!(heap.hash_get(items, Value::Int(1)), Some(Value::Int(42)));

    let switches = heap.hash_get(header, key("switches")).expect("switches");
    let data = heap.ivar(switches, "@data").and_then(|d| heap.array(d)).expect("data");
    assert_eq!(data[3], Value::Bool(true));

    let variables = heap.hash_get(header, key("variables")).expect("variables");
    let data = heap.ivar(variables, "@data").and_then(|d| heap.array(d)).expect("data");
    assert_eq!(data[2], Value::Int(-7));

    let player = heap.hash_get(header, key("player")).expect("player");
    assert_eq!(heap.ivar_int(player, "@x"), Some(20));

    let self_switches = heap.hash_get(header, key("self_switches")).expect("self switches");
    let data = heap.ivar(self_switches, "@data").expect("data");
    let letter = heap.new_str_like("A");
    let switch_key = heap.array_like(&[Value::Int(3), Value::Int(4), letter]);
    assert_eq!(heap.hash_get(data, switch_key), Some(Value::Bool(false)));

    // And the contents copy, which is what the game loads.
    assert_eq!(reloaded.steps(), Some(99));
    assert!(reloaded.switch(3));
    assert_eq!(reloaded.player_position(), Some((20, 3)));
}

#[test]
fn actor_edits_reach_both_copies() {
    let mut save = open("mirrored_header.rvdata2");
    assert_eq!(save.actor_copies(1).len(), 2);

    let changed = save.update_actor(1, |save, actor| {
        save.set_actor_string(actor, "@name", "Reidalt")
            && save.set_actor_id_list(actor, "@skills", &[1, 2, 9])
    });
    assert!(changed);

    let bytes = save.to_bytes();
    let reloaded = SaveFile::from_bytes(&bytes, None).expect("reload");
    for actor in reloaded.actor_copies(1) {
        assert_eq!(reloaded.actor_string(actor, "@name").as_deref(), Some("Reidalt"));
        assert_eq!(reloaded.actor_id_list(actor, "@skills"), vec![1, 2, 9]);
    }
}

/// An ordinary save has one copy of everything; nothing here should change it.
#[test]
fn a_plain_save_is_unaffected() {
    let mut save = open("ace_project/Save1.rvdata2");
    assert_eq!(save.roots("Game_Party").len(), 1);
    assert!(!save.notes.iter().any(|n| n.contains("mirrors")));

    save.set_gold(1);
    assert_eq!(save.gold(), Some(1));
}

/// Test-only conveniences for building lookup keys out of a shared heap.
trait KeyBuilders {
    fn new_str_like(&self, text: &str) -> Value;
    fn array_like(&self, items: &[Value]) -> Value;
}

impl KeyBuilders for rpgsave::marshal::Heap {
    fn new_str_like(&self, text: &str) -> Value {
        for id in 0..self.len() as u32 {
            if let rpgsave::marshal::NodeKind::Str(bytes) = self.kind(id)
                && bytes == text.as_bytes()
            {
                return Value::Ref(id);
            }
        }
        panic!("no string node {text:?}");
    }

    fn array_like(&self, items: &[Value]) -> Value {
        for id in 0..self.len() as u32 {
            if let rpgsave::marshal::NodeKind::Array(values) = self.kind(id)
                && values.len() == items.len()
                && values.iter().zip(items).all(|(a, b)| self.value_eq(*a, *b))
            {
                return Value::Ref(id);
            }
        }
        panic!("no array node matching {items:?}");
    }
}
