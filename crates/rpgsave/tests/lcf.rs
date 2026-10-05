// RPG Maker 2000 / 2003 saves

use rpgsave::lcf::save::{actor, LcfSave};

fn fixture() -> Vec<u8> {
    std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/Save01.lsd")).expect("read")
}

fn open() -> LcfSave {
    LcfSave::from_bytes(&fixture()).expect("parse")
}

#[test]
fn an_untouched_save_is_written_back_byte_for_byte() {
    let original = fixture();
    let save = LcfSave::from_bytes(&original).expect("parse");
    assert_eq!(save.to_bytes(), original);
    assert!(!save.dirty);
}

#[test]
fn something_that_is_not_a_save_is_refused() {
    let Err(error) = LcfSave::from_bytes(b"\x0bLcfMapUnit!!") else {
        panic!("a map file is not a save file");
    };
    assert!(error.to_string().contains("LcfSaveData"), "got: {error}");
    assert!(!LcfSave::looks_like_save(b"\x04\x08{\x00"), "that is Ruby Marshal data");
    assert!(LcfSave::looks_like_save(&fixture()));
}

#[test]
fn reads_what_the_save_actually_holds() {
    let save = open();
    assert_eq!(save.party_member_ids(), vec![1, 2]);
    assert_eq!(save.items(), vec![(1, 10)]);
    assert_eq!(save.steps(), 5);
    assert_eq!(save.gold(), 0, "the field is absent, meaning the default");
    assert_eq!(save.playtime_frames(), 3_493);
    assert_eq!(save.map_id(), 1);
    assert_eq!(save.player_position(), (31, 18));
    assert_eq!(save.actor_ids(), vec![1, 2, 3, 4, 5]);

    assert_eq!(save.actor_int(1, actor::LEVEL), Some(1));
    assert_eq!(save.actor_int(1, actor::CURRENT_HP), Some(300));
    assert_eq!(save.actor_int(1, actor::CURRENT_SP), Some(100));
    assert_eq!(save.actor_equipment(1), vec![17, 25, 44, 0, 0]);
    assert_eq!(save.actor_equipment(3), vec![18, 26, 34, 37, 40]);

    // Names are left as a marker when the game has not renamed the actor; the
    // real name lives in the database.
    assert_eq!(save.actor_name(1), None);
    assert_eq!(save.title_hero_name().as_deref(), Some("Franz"));

    assert_eq!(save.variable(1), 7);
    assert_eq!(save.variable_count(), 1);
    assert_eq!(save.switch_count(), 0, "this save has no switches set");
}

#[test]
fn edits_survive_a_round_trip() {
    let mut save = open();
    save.set_gold(4_242);
    save.set_steps(99);
    save.set_item_count(1, 50);
    save.set_item_count(7, 3); // an item the party did not have
    save.set_player_position(12, 9);
    save.set_playtime_seconds(3_600);
    save.set_party_member_ids(&[2, 1, 3]);
    assert!(save.dirty);

    let reloaded = LcfSave::from_bytes(&save.to_bytes()).expect("reparse");
    assert_eq!(reloaded.gold(), 4_242);
    assert_eq!(reloaded.steps(), 99);
    assert_eq!(reloaded.items(), vec![(1, 50), (7, 3)]);
    assert_eq!(reloaded.player_position(), (12, 9));
    assert_eq!(reloaded.playtime_frames(), 3_600 * 60);
    assert_eq!(reloaded.party_member_ids(), vec![2, 1, 3]);
}

/// A field at its default is absent from the file, so setting it has to add
/// the chunk — gold is not in this save at all until it is changed.
#[test]
fn a_field_that_was_absent_can_be_set() {
    let mut save = open();
    assert_eq!(save.gold(), 0);
    save.set_gold(500);
    let reloaded = LcfSave::from_bytes(&save.to_bytes()).expect("reparse");
    assert_eq!(reloaded.gold(), 500);
    // And everything else is still intact.
    assert_eq!(reloaded.items(), vec![(1, 10)]);
    assert_eq!(reloaded.actor_equipment(1), vec![17, 25, 44, 0, 0]);
}

#[test]
fn switches_and_variables_grow_as_needed() {
    let mut save = open();
    assert!(save.set_switch(5, true));
    assert!(save.set_variable(3, -250));

    let reloaded = LcfSave::from_bytes(&save.to_bytes()).expect("reparse");
    assert!(reloaded.switch(5));
    assert!(!reloaded.switch(4), "the slots in between default to off");
    assert_eq!(reloaded.switch_count(), 5);
    assert_eq!(reloaded.variable(3), -250);
    assert_eq!(reloaded.variable(1), 7, "the value that was already there");
    assert_eq!(reloaded.variable_count(), 3);
}

#[test]
fn actors_can_be_edited() {
    let mut save = open();
    assert!(save.set_actor_int(1, actor::LEVEL, 25));
    assert!(save.set_actor_int(1, actor::CURRENT_HP, 999));
    assert!(save.set_actor_int(1, actor::EXP, 12_345));
    assert!(save.set_actor_equipment(1, 0, 21));
    assert!(save.set_actor_skills(1, &[1, 2, 9]));
    assert!(save.set_actor_name(1, "Franziska"));
    assert!(!save.set_actor_int(99, actor::LEVEL, 2), "no such actor");

    let reloaded = LcfSave::from_bytes(&save.to_bytes()).expect("reparse");
    assert_eq!(reloaded.actor_int(1, actor::LEVEL), Some(25));
    assert_eq!(reloaded.actor_int(1, actor::CURRENT_HP), Some(999));
    assert_eq!(reloaded.actor_int(1, actor::EXP), Some(12_345));
    assert_eq!(reloaded.actor_equipment(1), vec![21, 25, 44, 0, 0]);
    assert_eq!(reloaded.actor_skills(1), vec![1, 2, 9]);
    assert_eq!(reloaded.actor_name(1).as_deref(), Some("Franziska"));
    // The other actors are untouched.
    assert_eq!(reloaded.actor_int(2, actor::CURRENT_HP), Some(200));
}

/// The file-select screen reads a copy of the leader's details, as the RGSS
/// engines do, so it has to follow the party.
#[test]
fn the_save_menu_header_follows_the_leader() {
    let mut save = open();
    save.set_actor_name(2, "Mirai");
    save.set_actor_int(2, actor::LEVEL, 7);
    save.set_actor_int(2, actor::CURRENT_HP, 123);
    save.set_party_member_ids(&[2, 1]);
    save.refresh_title();

    let reloaded = LcfSave::from_bytes(&save.to_bytes()).expect("reparse");
    assert_eq!(reloaded.title_hero_name().as_deref(), Some("Mirai"));
}
