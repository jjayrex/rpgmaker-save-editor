// RPG Maker MZ: the deflated container, and the dialect differences from MV.
//
// Everything above the container is shared with MV and tested in `mv.rs`; what
// is here is what MZ does differently.

use rpgsave::js::json::Json;
use rpgsave::js::save::{actor, Container, ItemKind};
use rpgsave::js::{zlib, JsonSave};

fn fixture() -> Vec<u8> {
    std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/mz_save.rmmzsave"))
        .expect("read")
}

fn open() -> JsonSave {
    JsonSave::from_bytes(&fixture()).expect("open the save")
}

fn reopen(save: &JsonSave) -> JsonSave {
    JsonSave::from_bytes(&save.to_bytes()).expect("reopen what we wrote")
}

/// Unpacks a file the way the engine does: read it as UTF-8, take the low byte
/// of every character, inflate.
fn unpack(bytes: &[u8]) -> String {
    let stream = zlib::from_text(bytes).expect("the engine's text form");
    assert!(zlib::looks_like_zlib(&stream), "a zlib stream");
    String::from_utf8(zlib::inflate(&stream).expect("inflates")).expect("utf-8")
}

// ------------------------------------------------------------ the container

#[test]
fn the_container_says_which_engine_wrote_it() {
    let save = open();
    assert_eq!(save.container, Container::Zlib);
    assert_eq!(save.container.engine(), "mz");
    assert_eq!(save.container.label(), "MZ");
    assert_eq!(save.container.extension(), "rmmzsave");
    assert!(JsonSave::looks_like_save(&fixture()));
    assert!(save.notes.is_empty(), "{:?}", save.notes);
}

/// The engine hands pako's output to a function that writes text, so every
/// byte above 0x7F reaches the file as two. Both directions have to agree.
#[test]
fn the_text_wrapper_round_trips_every_byte() {
    let every: Vec<u8> = (0..=255).collect();
    let text = zlib::to_text(&every);
    assert_eq!(text.len(), 128 + 128 * 2, "the high half takes two bytes each");
    assert_eq!(zlib::from_text(&text).as_deref(), Some(every.as_slice()));

    // Text holding anything that cannot have come from one byte is not it.
    assert_eq!(zlib::from_text("日本語".as_bytes()), None);
    assert_eq!(zlib::from_text(&[0xff, 0xfe]), None, "not even UTF-8");
}

#[test]
fn the_fixture_re_encodes_byte_for_byte() {
    let save = open();
    assert_eq!(save.to_bytes(), fixture(), "an unedited save must come back unchanged");
}

/// Nothing reproduces another deflate implementation's exact bytes, so what
/// matters is that what this writes is a stream the engine can read back.
#[test]
fn an_edited_save_is_a_stream_the_engine_can_read() {
    let mut save = open();
    save.set_gold(4_242);
    let bytes = save.to_bytes();
    assert_ne!(bytes, fixture(), "the edit should have changed the file");

    assert_eq!(unpack(&bytes), save.to_json_text(), "the file holds exactly the JSON we wrote");
    assert_eq!(reopen(&save).gold(), 4_242);
}

#[test]
fn a_damaged_stream_is_refused() {
    let bytes = fixture();
    let stream = zlib::from_text(&bytes).expect("unwrap");
    // Keep the header, lose the rest.
    let truncated = zlib::to_text(&stream[..stream.len() / 2]);
    let message = match JsonSave::from_bytes(&truncated) {
        Err(e) => e.to_string(),
        Ok(_) => panic!("half a stream is not a save"),
    };
    assert!(message.contains("damaged"), "{message}");
}

/// Save editors that write the stream as plain bytes produce a file the engine
/// cannot read. The editor opens it anyway, says so, and writes it back the
/// way the engine does.
#[test]
fn a_raw_stream_is_read_and_put_right_on_the_way_out() {
    let stream = zlib::from_text(&fixture()).expect("unwrap");

    let mut save = JsonSave::from_bytes(&stream).expect("open the raw form");
    assert_eq!(save.container, Container::Zlib);
    assert_eq!(save.gold(), 0);
    assert!(save.notes.iter().any(|n| n.contains("raw bytes")), "{:?}", save.notes);

    // Untouched, it is still written back exactly as it arrived.
    assert_eq!(save.to_bytes(), stream);

    save.set_gold(5);
    let written = save.to_bytes();
    assert_ne!(written, stream);
    assert_eq!(
        JsonSave::from_bytes(&written).expect("reopen").gold(),
        5,
        "and what it wrote is the engine's own form"
    );
    assert!(zlib::from_text(&written).is_some_and(|s| zlib::looks_like_zlib(&s)));
}

// -------------------------------------------------------------- the dialect

/// MV wraps every array in an object so that it can carry an identity; MZ
/// dropped that, and writes plain arrays.
#[test]
fn arrays_are_plain_json() {
    let save = open();
    let text = save.to_json_text();
    assert!(!text.contains(r#""@a""#), "MZ has no array wrappers");
    assert!(!text.contains(r#""@c""#), "nor object identities");
    assert!(!text.contains(r#""@r""#), "nor references back to them");
    // The class names it does keep.
    assert!(text.contains(r#""@":"Game_Party""#));

    let switches = save.root.get("switches").and_then(|s| s.get("_data")).expect("switches");
    assert!(matches!(switches, Json::Array(_)), "read straight as an array");
}

/// Whatever a game's plugins added has to come back untouched; this save has
/// two plugin suites in it.
#[test]
fn plugin_sections_survive_an_edit() {
    let mut save = open();
    let before = save.root.get("cgmz").map(Json::to_text).expect("a plugin section");
    save.set_gold(1);

    let save = reopen(&save);
    assert_eq!(save.root.get("cgmz").map(Json::to_text).as_deref(), Some(before.as_str()));
    assert!(save.root.get("eli").is_some(), "and the other one");
}

// ------------------------------------------------------------- the contents

#[test]
fn reads_the_overview() {
    let save = open();

    assert_eq!(save.gold(), 0);
    assert_eq!(save.steps(), 28);
    assert_eq!(save.playtime_frames(), 17_096);
    assert_eq!(save.save_count(), Some(1));
    assert_eq!(save.map_id(), 32);
    assert_eq!(save.player_position(), (26, 6));
    assert_eq!(save.party_member_ids(), vec![11, 12]);
    assert_eq!(save.switch_count(), 150);
    assert_eq!(save.variable_count(), 103);
    assert_eq!(save.self_switches().len(), 31);
    assert_eq!(save.items(ItemKind::Item), vec![(11, 4), (13, 3)]);
}

#[test]
fn reads_actors() {
    let save = open();

    assert_eq!(save.actor_ids(), vec![11, 12]);
    assert_eq!(save.actor_text(11, actor::NAME), Some("Ziedrick"));
    assert_eq!(save.actor_int(11, actor::LEVEL), Some(6));
    assert_eq!(save.actor_exp(11), Some(1_250));
    assert_eq!(save.actor_int(11, actor::HP), Some(45));
    assert_eq!(save.actor_int(11, actor::MP), Some(5));
    assert_eq!(save.actor_skills(11).len(), 2);
    // This game gives its actors more than the engine's five slots, so the
    // count comes from the save rather than from a fixed list.
    assert_eq!(save.actor_equipment(11).len(), 7);
    assert_eq!(save.actor_equipment(11)[0], ("weapon".to_owned(), 19));
}

#[test]
fn edits_survive_a_round_trip() {
    let mut save = open();

    save.set_gold(123_456);
    save.set_steps(42);
    save.set_playtime_seconds(60);
    save.set_player_position(5, 6);
    save.set_party_member_ids(&[12]);
    save.set_item_count(ItemKind::Item, 11, 99);
    save.set_switch(7, true);
    save.set_variable(9, Json::int(55));
    save.set_self_switch(32, 5, "C", true);
    save.set_actor_field(11, actor::NAME, Json::text("Ziedrick ☃"));
    save.set_actor_exp(11, 9_001);

    let save = reopen(&save);
    assert_eq!(save.gold(), 123_456);
    assert_eq!(save.steps(), 42);
    assert_eq!(save.playtime_frames(), 3_600);
    assert_eq!(save.player_position(), (5, 6));
    assert_eq!(save.party_member_ids(), vec![12]);
    assert_eq!(save.item_count(ItemKind::Item, 11), 99);
    assert!(save.switch(7));
    assert_eq!(save.variable(9).and_then(Json::as_i64), Some(55));
    assert!(save.self_switches().contains(&(32, 5, "C".to_owned(), true)));
    assert_eq!(save.actor_text(11, actor::NAME), Some("Ziedrick ☃"));
    assert_eq!(save.actor_exp(11), Some(9_001));
}
