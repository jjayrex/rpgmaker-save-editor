// RPG Maker MV

use rpgsave::mv::json::{self, Json};
use rpgsave::mv::save::{actor, ItemKind};
use rpgsave::mv::{database, lzstring, MvSave};

fn fixtures() -> std::path::PathBuf {
    std::path::PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures"))
}

fn fixture() -> Vec<u8> {
    std::fs::read(fixtures().join("mv_save.rpgsave")).expect("read")
}

fn open() -> MvSave {
    MvSave::from_bytes(&fixture()).expect("open the save")
}

/// Re-encodes and reopens, which is what the editor does on the way to disk.
fn reopen(save: &MvSave) -> MvSave {
    MvSave::from_bytes(&save.to_bytes()).expect("reopen what we wrote")
}

// ------------------------------------------------------------ the container

#[test]
fn lzstring_round_trips_what_it_compresses() {
    for text in [
        "",
        "a",
        "{}",
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        r#"{"gold":12345,"name":"Cahara"}"#,
        // Beyond the basic plane, where a JavaScript string is two code units
        // and the compression works one code unit at a time.
        "日本語のテキスト☃\u{1F600}",
    ] {
        let packed = lzstring::compress_to_base64(text);
        assert!(lzstring::looks_like_base64(&packed), "{packed:?} is not base64");
        assert_eq!(
            lzstring::decompress_from_base64(&packed).as_deref(),
            Some(text),
            "{text:?} did not survive"
        );
    }
}

#[test]
fn text_that_is_not_a_stream_is_refused() {
    // Base64, but not something lz-string wrote.
    assert_eq!(lzstring::decompress_from_base64("AAAA"), None);
    assert_eq!(lzstring::decompress_from_base64("not base64!"), None);
    assert!(!lzstring::looks_like_base64("{\"gold\": 1}"));
}

#[test]
fn the_fixture_re_encodes_byte_for_byte() {
    let save = open();
    assert_eq!(save.to_bytes(), fixture(), "an unedited save must come back unchanged");
}

/// The editor's own writing has to be a stream the engine can read, which
/// means it has to come back through the reader unchanged.
#[test]
fn an_edited_save_is_still_a_stream_the_engine_can_read() {
    let mut save = open();
    save.set_gold(777);
    let bytes = save.to_bytes();
    assert_ne!(bytes, fixture(), "the edit should have changed the file");

    let text = String::from_utf8(bytes.clone()).expect("base64 is text");
    let payload = lzstring::decompress_from_base64(&text).expect("unpacks");
    assert_eq!(payload, save.to_json_text(), "the file holds exactly the JSON we wrote");
    assert_eq!(MvSave::from_bytes(&bytes).expect("reopens").gold(), 777);
}

// ----------------------------------------------------------------- the JSON

#[test]
fn json_replays_what_it_did_not_change() {
    // Spellings that mean the same number, escapes that mean the same text:
    // all of it has to come back as it went in.
    let source = r#"{"a":1.0,"b":1e2,"c":-0.5,"d":"é\n\"x\"","e":[true,false,null],"f":{}}"#;
    let value = json::parse(source).expect("parse");
    assert_eq!(value.to_text(), source);

    assert_eq!(value.int_at("b"), Some(100));
    assert_eq!(value.str_at("d"), Some("é\n\"x\""));
    assert_eq!(value.get("e").and_then(Json::elements).map(<[Json]>::len), Some(3));
}

#[test]
fn an_edited_number_is_written_afresh() {
    let mut value = json::parse(r#"{"gold":1e2,"steps":7}"#).expect("parse");
    value.set("gold", Json::int(250));
    assert_eq!(value.to_text(), r#"{"gold":250,"steps":7}"#);
}

#[test]
fn json_that_is_not_json_is_refused() {
    for broken in ["", "{", "{\"a\"}", "[1,]", "tru", "{} junk"] {
        assert!(json::parse(broken).is_err(), "{broken:?} should not parse");
    }
}

// ------------------------------------------------------------- the overview

#[test]
fn reads_the_overview() {
    let save = open();

    assert_eq!(save.gold(), 0);
    assert_eq!(save.steps(), 32_355);
    assert_eq!(save.playtime_frames(), 1_392_284);
    assert_eq!(save.save_count(), Some(65));
    assert_eq!(save.map_id(), 119);
    assert_eq!(save.player_position(), (38, 28));
    assert_eq!(save.party_member_ids(), vec![1, 7, 6, 3]);
    assert_eq!(save.switch_count(), 3565);
    assert_eq!(save.variable_count(), 402);
}

#[test]
fn reads_what_the_party_is_carrying() {
    let save = open();

    assert_eq!(save.item_count(ItemKind::Item, 5), 10);
    assert_eq!(save.item_count(ItemKind::Weapon, 7), 2);
    assert_eq!(save.item_count(ItemKind::Armor, 8), 3);
    assert_eq!(save.item_count(ItemKind::Item, 9_999), 0, "nothing held is nothing");
    assert_eq!(save.items(ItemKind::Weapon).len(), 8);
}

#[test]
fn reads_actors() {
    let save = open();

    assert_eq!(save.actor_ids().len(), 23);
    assert_eq!(save.actor_text(1, actor::NAME), Some("Cahara"));
    assert_eq!(save.actor_text(1, actor::NICKNAME), Some("Rhabdophobia"));
    assert_eq!(save.actor_int(1, actor::CLASS_ID), Some(1));
    assert_eq!(save.actor_int(1, actor::LEVEL), Some(2));
    assert_eq!(save.actor_exp(1), Some(146), "experience is kept per class");
    assert_eq!(save.actor_int(1, actor::HP), Some(100));
    assert_eq!(save.actor_int(1, actor::MP), Some(93));
    assert_eq!(save.actor_states(1), vec![69]);
    assert_eq!(save.actor_skills(1).len(), 14);
    assert_eq!(
        save.actor_equipment(1),
        vec![
            ("weapon".to_owned(), 42),
            ("armor".to_owned(), 16),
            ("armor".to_owned(), 34),
            ("armor".to_owned(), 33),
            ("armor".to_owned(), 40),
        ]
    );
    assert_eq!(save.actor(99), None, "an actor the game never created");
}

#[test]
fn reads_switches_variables_and_self_switches() {
    let save = open();

    assert!(save.switch(18), "a switch the game set");
    assert!(!save.switch(1), "a switch it never touched");
    assert_eq!(save.variable(4).and_then(Json::as_i64), Some(3));
    assert_eq!(save.variable(5), None, "an untouched variable holds nothing");

    let self_switches = save.self_switches();
    assert_eq!(self_switches.len(), 354);
    assert!(self_switches.contains(&(10, 1, "A".to_owned(), true)));
}

// ---------------------------------------------------------------- the edits

#[test]
fn edits_survive_a_round_trip() {
    let mut save = open();

    save.set_gold(123_456);
    save.set_steps(42);
    save.set_playtime_seconds(3_600);
    save.set_player_position(5, 6);
    save.set_party_member_ids(&[3, 1]);
    save.set_item_count(ItemKind::Item, 5, 99);
    save.set_item_count(ItemKind::Weapon, 42, 1);
    save.set_switch(2, true);
    save.set_variable(5, Json::int(12));
    save.set_self_switch(119, 3, "B", true);

    let save = reopen(&save);
    assert_eq!(save.gold(), 123_456);
    assert_eq!(save.steps(), 42);
    assert_eq!(save.playtime_frames(), 216_000, "seconds are stored as frames");
    assert_eq!(save.player_position(), (5, 6));
    assert_eq!(save.party_member_ids(), vec![3, 1]);
    assert_eq!(save.item_count(ItemKind::Item, 5), 99);
    assert_eq!(save.item_count(ItemKind::Weapon, 42), 1);
    assert!(save.switch(2));
    assert_eq!(save.variable(5).and_then(Json::as_i64), Some(12));
    assert!(save.self_switches().contains(&(119, 3, "B".to_owned(), true)));
}

#[test]
fn actor_edits_survive_a_round_trip() {
    let mut save = open();

    save.set_actor_field(1, actor::NAME, Json::text("Le Marchand"));
    save.set_actor_field(1, actor::LEVEL, Json::int(30));
    save.set_actor_exp(1, 9_001);
    save.set_actor_field(1, actor::HP, Json::int(250));
    save.set_actor_param_plus(1, 2, 15);
    save.set_actor_skills(1, &[1, 2, 3]);
    save.set_actor_states(1, &[]);
    save.set_actor_equipment(1, 0, "weapon", 7);
    save.set_actor_equipment(1, 1, "armor", 0);

    let save = reopen(&save);
    assert_eq!(save.actor_text(1, actor::NAME), Some("Le Marchand"));
    assert_eq!(save.actor_int(1, actor::LEVEL), Some(30));
    assert_eq!(save.actor_exp(1), Some(9_001));
    assert_eq!(save.actor_int(1, actor::HP), Some(250));
    assert_eq!(save.actor_param_plus(1, 2), Some(15));
    assert_eq!(save.actor_skills(1), vec![1, 2, 3]);
    assert!(save.actor_states(1).is_empty());
    assert_eq!(
        save.actor_equipment(1)[..2],
        [("weapon".to_owned(), 7), ("armor".to_owned(), 0)]
    );
    // Nothing should still be counting down a state the actor has lost.
    let turns = save.actor(1).and_then(|a| a.get(actor::STATE_TURNS)).expect("state turns");
    assert_eq!(turns.int_at("69"), None);
}

/// The engine deletes an entry rather than keeping a count of zero, and a save
/// that says `"5":0` is not one the game would have written.
#[test]
fn clearing_an_item_removes_its_entry() {
    let mut save = open();
    save.set_item_count(ItemKind::Item, 5, 0);

    let text = save.to_json_text();
    assert!(!text.contains(r#""5":0"#), "the entry should be gone, not zeroed");
    assert_eq!(reopen(&save).item_count(ItemKind::Item, 5), 0);
}

#[test]
fn counts_and_gold_are_held_to_what_the_engine_allows() {
    let mut save = open();
    save.set_gold(i64::MAX);
    save.set_item_count(ItemKind::Item, 5, 1_000);
    save.set_steps(-7);

    assert_eq!(save.gold(), 99_999_999);
    assert_eq!(save.item_count(ItemKind::Item, 5), 99);
    assert_eq!(save.steps(), 0);
}

/// Moving the player has to move the view with them, or the game starts with
/// the party off screen.
#[test]
fn moving_the_player_moves_the_camera_too() {
    let save = open();
    let before = save
        .root
        .get("map")
        .and_then(|m| m.get("_displayX"))
        .and_then(Json::as_f64)
        .expect("a display position");
    let (x, _) = save.player_position();

    let mut save = open();
    save.set_player_position(x + 10, 0);
    let after = save
        .root
        .get("map")
        .and_then(|m| m.get("_displayX"))
        .and_then(Json::as_f64)
        .expect("a display position");
    assert_eq!(after, before + 10.0);
}

#[test]
fn an_edit_marks_the_save_dirty() {
    let mut save = open();
    assert!(!save.dirty);
    save.set_gold(1);
    assert!(save.dirty);
}

// --------------------------------------------------------------- the refusal

#[test]
fn something_that_is_not_an_mv_save_is_refused() {
    // A Ruby Marshal stream, which is what the older engines write.
    let marshal = std::fs::read(fixtures().join("ace_save.rvdata2")).expect("read");
    assert!(!MvSave::looks_like_save(&marshal));
    assert!(MvSave::from_bytes(&marshal).is_err());

    // Base64 that is not a stream.
    assert!(MvSave::from_bytes(b"AAAA").is_err());

    // MV's successor stores the same JSON zlib-compressed, which comes out as
    // base64 of a zlib header; saying which engine it is beats "not base64".
    let mz = match MvSave::from_bytes(b"eJyrVgrOL8pMUbJSMtQDAB3+BM4=") {
        Err(e) => e.to_string(),
        Ok(_) => panic!("a zlib stream is not an MV save"),
    };
    assert!(mz.contains("MZ"), "{mz}");

    // MV's own bookkeeping files are the same format with no game state in
    // them, and saying so is more use than failing to find a party.
    let global = lzstring::compress_to_base64(r#"[{"globalId":"x","title":"Game"}]"#);
    let message = match MvSave::from_bytes(global.as_bytes()) {
        Err(e) => e.to_string(),
        Ok(_) => panic!("a file with no party in it is not a save slot"),
    };
    assert!(message.contains("not a save slot"), "{message}");
}

// -------------------------------------------------------------- the database

/// MV ships its database as JSON, so this builds the smallest project that
/// exercises every table the editor reads.
fn write_project(dir: &std::path::Path) {
    let data = dir.join("www").join("data");
    std::fs::create_dir_all(&data).expect("create the project");
    std::fs::create_dir_all(dir.join("www").join("save")).expect("create the save folder");
    for (name, contents) in [
        (
            "System.json",
            r#"{"gameTitle":"Hunger","currencyUnit":"coins",
                "switches":["","Met the merchant",""],
                "variables":["","","Debt"],
                "equipTypes":["","Weapon","Shield","Head","Body","Trinket"]}"#,
        ),
        (
            "Actors.json",
            r#"[null,{"id":1,"name":"Cahara","nickname":"Mercenary","classId":1,
                "initialLevel":1,"maxLevel":50}]"#,
        ),
        (
            "Classes.json",
            r#"[null,{"id":1,"name":"Mercenary","expParams":[30,20,30,30],
                "params":[[10,100,200],[5,50,60],[1,2,3],[1,2,3],[1,2,3],[1,2,3],[1,2,3],[1,2,3]]}]"#,
        ),
        ("Items.json", r#"[null,{"id":1,"name":"Bandages","iconIndex":176,"description":"Stops bleeding.","price":30}]"#),
        ("Weapons.json", r#"[null,{"id":1,"name":"Dagger","iconIndex":96,"price":120,"etypeId":1}]"#),
        ("Armors.json", r#"[null,{"id":1,"name":"Leather","iconIndex":128,"price":90,"etypeId":4}]"#),
        ("Skills.json", r#"[null,{"id":1,"name":"Slash"}]"#),
        ("States.json", r#"[null,{"id":1,"name":"Bleeding"}]"#),
        ("MapInfos.json", r#"[null,{"id":1,"name":"Dungeon"},{"id":2,"name":"Crypt"}]"#),
    ] {
        std::fs::write(data.join(name), contents).expect("write a table");
    }
}

#[test]
fn the_database_turns_ids_into_names() {
    let dir = std::env::temp_dir().join(format!("rpgsave-mv-data-{}", std::process::id()));
    write_project(&dir);

    let data_dir = database::find_data_dir(&dir.join("www").join("save")).expect("find data");
    assert_eq!(data_dir, dir.join("www").join("data"), "found from the save folder beside it");

    let data = database::load(&data_dir).expect("load");
    assert_eq!(data.game_title.as_deref(), Some("Hunger"));
    assert_eq!(data.currency.as_deref(), Some("coins"));
    assert_eq!(data.actor(1).map(|a| a.name.as_str()), Some("Cahara"));
    assert_eq!(data.actor(1).map(|a| a.max_level), Some(50));
    assert_eq!(data.class_name(1), Some("Mercenary"));
    assert_eq!(data.entry_name(ItemKind::Item, 1), Some("Bandages"));
    assert_eq!(data.entry_name(ItemKind::Weapon, 1), Some("Dagger"));
    assert_eq!(data.entry_name(ItemKind::Armor, 1), Some("Leather"));
    assert_eq!(data.skill_name(1), Some("Slash"));
    assert_eq!(data.state_name(1), Some("Bleeding"));
    assert_eq!(data.map_name(2), Some("Crypt"));
    assert_eq!(data.switch_name(1), Some("Met the merchant"));
    assert_eq!(data.switch_name(2), None, "unnamed switches stay unnamed");
    assert_eq!(data.variable_name(2), Some("Debt"));
    // Slot 0 of the engine's list is its own "none", so the slots the
    // interface shows start at the weapon.
    assert_eq!(data.equip_type_name(0), Some("Weapon"));
    assert_eq!(data.equip_type_name(4), Some("Trinket"));
    assert!(data.missing.is_empty(), "{:?}", data.missing);

    // The curves MV keeps on the class, which the editor reads to show what a
    // level is worth and to keep experience in step with it.
    let class = data.class(1).expect("the class");
    assert_eq!(class.param(0, 2), Some(200), "max HP at level 2");
    assert_eq!(class.param(1, 1), Some(50));
    assert_eq!(class.exp_for_level(1), Some(0));
    assert!(class.exp_for_level(10).unwrap_or(0) > 0);

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn a_folder_with_no_database_in_it_loads_nothing() {
    let dir = std::env::temp_dir().join(format!("rpgsave-mv-empty-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("create");
    assert!(database::load(&dir).is_none());
    assert!(database::find_data_dir(&dir).is_none());
    std::fs::remove_dir_all(&dir).ok();
}
