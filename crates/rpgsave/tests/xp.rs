//! RPG Maker XP (RGSS1) saves.
//!
//! XP differs from the later engines in ways that reach every layer: it runs at
//! 40 frames a second, keeps the party as `Game_Actor` objects rather than ids,
//! calls the secondary pool SP, counts tile positions in 128ths, and stores stat
//! curves on the actor's database entry instead of on the class.

use rpgsave::marshal::Value;
use rpgsave::rpg::{save::ItemKind, Engine, GameData, SaveFile};

fn open(name: &str) -> SaveFile {
    let path = format!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/{}"), name);
    SaveFile::open(std::path::Path::new(&path)).expect("open")
}

fn project() -> (SaveFile, GameData) {
    let save = open("xp_project/Save1.rxdata");
    let dir = save.guess_data_dir().expect("Data directory");
    let data = GameData::load(&dir, save.engine);
    (save, data)
}

#[test]
fn an_xp_save_is_recognised() {
    let save = open("xp_save.rxdata");
    assert_eq!(save.engine, Engine::Xp);
    assert_eq!(save.engine.label(), "XP");
    assert_eq!(save.documents.len(), 12);
    assert!(save.notes.is_empty(), "nothing unusual: {:?}", save.notes);

    // Game_Screen at the top level is what separates XP from VX, which keeps
    // the screen inside Game_Map and saves Game_Message instead.
    assert!(save.root("Game_Screen").is_some());
    assert!(save.root("Game_Message").is_none());
}

#[test]
fn play_time_counts_at_forty_frames_a_second() {
    let mut save = open("xp_save.rxdata");
    assert_eq!(save.engine.frame_rate(), 40);
    assert_eq!(save.playtime_frames(), Some(3 * 3600 * 40));
    assert_eq!(save.playtime_seconds(), Some(3 * 3600));

    assert!(save.set_playtime_seconds(90 * 60));
    assert_eq!(save.playtime_frames(), Some(90 * 60 * 40));

    let reloaded = SaveFile::from_bytes(&save.to_bytes(), None).expect("reload");
    assert_eq!(reloaded.playtime_seconds(), Some(90 * 60));
}

#[test]
fn the_party_is_read_through_the_actor_objects_it_holds() {
    let save = open("xp_save.rxdata");
    assert!(save.engine.party_holds_objects());
    assert_eq!(save.party_member_ids(), vec![1, 2]);

    // The entries really are objects, not ids.
    let party = save.party().expect("Game_Party");
    let members = save.heap.ivar(party, "@actors").and_then(|a| save.heap.array(a)).unwrap();
    assert_eq!(save.heap.class_name(members[0]), Some("Game_Actor"));

    // Game_Actors and Game_Party are separate Marshal documents, so the party's
    // member is a second copy of the same actor rather than the same object.
    assert_ne!(members[0], save.actor(1).unwrap());
    assert_eq!(save.actor_copies(1).len(), 2);
    assert_eq!(save.actor_copies(3).len(), 1, "an actor outside the party has one copy");
}

/// Both copies of a party member have to move together, or the menu and the
/// event system would disagree about the same actor.
#[test]
fn editing_a_party_member_updates_the_copy_the_party_holds() {
    let mut save = open("xp_save.rxdata");
    let changed = save.update_actor(1, |save, actor| {
        save.set_actor_string(actor, "@name", "Alexis")
            && save.set_actor_level(actor, 30, None) != rpgsave::rpg::LevelChange::Failed
    });
    assert!(changed);

    let reloaded = SaveFile::from_bytes(&save.to_bytes(), None).expect("reload");
    let copies = reloaded.actor_copies(1);
    assert_eq!(copies.len(), 2);
    for copy in copies {
        assert_eq!(reloaded.actor_string(copy, "@name").as_deref(), Some("Alexis"));
        assert_eq!(reloaded.actor_level(copy), Some(30));
    }
}

#[test]
fn party_membership_can_be_changed_within_the_saves_own_actors() {
    let mut save = open("xp_save.rxdata");
    assert!(save.set_party_member_ids(&[3, 1]));
    assert_eq!(save.party_member_ids(), vec![3, 1]);

    let reloaded = SaveFile::from_bytes(&save.to_bytes(), None).expect("reload");
    assert_eq!(reloaded.party_member_ids(), vec![3, 1]);
    let party = reloaded.party().unwrap();
    let members = reloaded.heap.ivar(party, "@actors").and_then(|a| reloaded.heap.array(a)).unwrap();
    assert_eq!(
        reloaded.heap.class_name(members[0]),
        Some("Game_Actor"),
        "the party still holds actor objects"
    );
    assert_eq!(reloaded.heap.ivar_int(members[0], "@actor_id"), Some(3));
}

/// An actor the game has never created has no object to put in the party.
#[test]
fn an_actor_the_save_does_not_hold_cannot_join() {
    let mut save = open("xp_save.rxdata");
    assert!(save.party_can_include(3));
    assert!(!save.party_can_include(99));
    assert!(!save.set_party_member_ids(&[1, 99]));
    assert_eq!(save.party_member_ids(), vec![1, 2], "the party is left alone");
}

#[test]
fn the_secondary_pool_is_sp() {
    let mut save = open("xp_save.rxdata");
    assert_eq!(save.engine.mp_ivar(), "@sp");
    assert_eq!(save.engine.mp_label(), "SP");

    let actor = save.actor(1).expect("actor 1");
    assert_eq!(save.heap.ivar_int(actor, "@sp"), Some(80 + 12 * 5));
    assert!(save.heap.ivar(actor, "@mp").is_none());

    save.full_heal_actor(actor, None);
    assert!(save.heap.ivar_int(actor, "@sp").unwrap() > 0, "the SP pool was refilled");
    assert!(save.heap.ivar(actor, "@mp").is_none(), "no MP field was invented");
}

#[test]
fn stat_bonuses_use_xps_six_parameters() {
    let (save, data) = project();
    let actor = save.actor(1).expect("actor 1");

    let params = save.actor_param_plus(actor);
    let labels: Vec<&str> = params.iter().map(|p| p.0).collect();
    assert_eq!(
        labels,
        ["Max HP", "Max SP", "Strength", "Dexterity", "Agility", "Intelligence"]
    );
    assert_eq!(params[2].3, 4, "the fixture gives Aluxes +4 strength");

    // The curve comes from the actor's own table, not from the class.
    assert_eq!(save.actor_param_base(actor, 0, Some(&data)), Some(500 + 35 * 12));
    assert_eq!(save.actor_param_base(actor, 1, Some(&data)), Some(80 + 8 * 12));

    let mut save = save;
    assert!(save.set_actor_param_plus(actor, 2, 25));
    assert_eq!(save.actor_param_plus(actor)[2].3, 25);
}

#[test]
fn the_player_moves_in_hundred_and_twenty_eighths_of_a_tile() {
    let mut save = open("xp_save.rxdata");
    assert_eq!(save.engine.tile_subdivisions(), Some(128));
    assert_eq!(save.player_position(), Some((8, 6)));

    assert!(save.set_player_position(11, 4));
    let player = save.player().unwrap();
    assert_eq!(save.heap.ivar_int(player, "@real_x"), Some(11 * 128));
    assert_eq!(save.heap.ivar_int(player, "@real_y"), Some(4 * 128));
}

#[test]
fn the_database_resolves_xp_names() {
    let (_, data) = project();
    assert!(data.missing.is_empty(), "missing {:?}", data.missing);

    assert_eq!(data.entry_name(ItemKind::Item, 7), Some("Harbour Key"));
    assert_eq!(data.entry_name(ItemKind::Weapon, 2), Some("Storm Rod"));
    assert_eq!(data.entry_name(ItemKind::Armor, 9), Some("Gale Charm"));
    assert_eq!(data.skill_name(7), Some("Spark"));
    assert_eq!(data.state_name(4), Some("Poison"));
    assert_eq!(data.class_name(3), Some("Warlock"));
    assert_eq!(data.map_name(12), Some("Sunken Vault"));
    assert_eq!(data.switch_name(5), Some("Boat unlocked"));
    assert_eq!(data.variable_name(11), Some("Score"));

    // XP keeps these under `words` rather than `terms`.
    assert_eq!(data.currency.as_deref(), Some("G"));
    assert_eq!(data.equip_type_name(0), Some("Weapon"));
    assert_eq!(data.equip_type_name(3), Some("Body Armor"));

    // The title lives in Game.ini, as XP has no field for it.
    assert_eq!(data.game_title.as_deref(), Some("Lantern of Ys"));
}

#[test]
fn levels_experience_and_equipment_edit_as_on_the_other_engines() {
    let mut save = open("xp_save.rxdata");
    let actor = save.actor(1).expect("actor 1");

    // XP carries the whole curve on the actor, so no database is needed.
    assert_eq!(
        save.set_actor_level(actor, 20, None),
        rpgsave::rpg::LevelChange::LevelAndExp
    );
    assert_eq!(save.actor_exp(actor), Some(20 * 20 * 12));

    assert!(save.set_actor_equip(actor, 0, ItemKind::Weapon, 2));
    assert!(save.set_actor_equip(actor, 2, ItemKind::Armor, 9));
    assert!(save.set_actor_id_list(actor, "@skills", &[1, 7]));
    assert!(save.set_actor_string(actor, "@name", "Alexis"));
    assert!(save.set_gold(500));
    assert!(save.set_item_count(ItemKind::Item, 3, 20));
    assert!(save.set_switch(7, true));
    assert!(save.set_variable(2, Value::Int(-40)));

    let reloaded = SaveFile::from_bytes(&save.to_bytes(), None).expect("reload");
    let actor = reloaded.actor(1).expect("actor 1");
    assert_eq!(reloaded.actor_level(actor), Some(20));
    let equips = reloaded.actor_equips(actor);
    assert_eq!(equips[0].item_id, 2);
    assert_eq!(equips[2].item_id, 9);
    assert_eq!(reloaded.actor_id_list(actor, "@skills"), vec![1, 7]);
    assert_eq!(reloaded.actor_string(actor, "@name").as_deref(), Some("Alexis"));
    assert_eq!(reloaded.gold(), Some(500));
    assert_eq!(reloaded.inventory(ItemKind::Item), vec![(1, 9), (2, 3), (3, 20), (7, 1)]);
    assert!(reloaded.switch(7));
    assert_eq!(reloaded.variable(2).and_then(Value::as_int), Some(-40));
}

/// XP strings are Ruby 1.8 bytes, with no encoding instance variable.
#[test]
fn strings_are_written_the_way_rgss1_wrote_them() {
    let mut save = open("xp_save.rxdata");
    assert!(!save.engine.strings_carry_encoding());

    let actor = save.actor(1).unwrap();
    save.set_actor_string(actor, "@name", "Alexis");
    let name = save.heap.ivar(actor, "@name").unwrap();
    assert!(save.heap.ivar_names(name).is_empty(), "no :E tag on an XP string");
}

/// The header document is the character list, keyed on hue rather than index.
#[test]
fn the_character_list_follows_the_party() {
    let mut save = open("xp_save.rxdata");
    save.set_party_member_ids(&[3]);

    let reloaded = SaveFile::from_bytes(&save.to_bytes(), None).expect("reload");
    let characters = reloaded.documents[0];
    let rows = reloaded.heap.array(characters).expect("the character list");
    assert_eq!(rows.len(), 1);
    let first = reloaded.heap.array(rows[0]).expect("name and hue");
    assert_eq!(reloaded.heap.string(first[0]).as_deref(), Some("001-Fighter01"));
    assert_eq!(first[1], Value::Int(0), "the hue, as XP stores it");
}

#[test]
fn saving_an_untouched_xp_save_changes_nothing() {
    for name in ["xp_save.rxdata", "xp_project/Save1.rxdata"] {
        let path = format!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/{}"), name);
        let original = std::fs::read(&path).expect("read");
        let mut save = SaveFile::open(std::path::Path::new(&path)).expect("open");
        assert_eq!(save.to_bytes(), original, "{name}");
    }
}
