//! Actor editing, and the database lookups that give ids their names.

use rpgsave::rpg::{
    actor::LevelChange, save::ItemKind, Engine, GameData, SaveFile,
};

fn project(name: &str) -> (SaveFile, GameData) {
    let path = format!(
        concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/{}"),
        name
    );
    let save = SaveFile::open(std::path::Path::new(&path)).expect("open");
    let dir = save.guess_data_dir().expect("Data directory next to the save");
    let data = GameData::load(&dir, save.engine);
    (save, data)
}

#[test]
fn finds_and_reads_the_project_database() {
    for (name, engine) in [
        ("ace_project/Save1.rvdata2", Engine::VxAce),
        ("vx_project/Save1.rvdata", Engine::Vx),
    ] {
        let (save, data) = project(name);
        assert_eq!(save.engine, engine);
        assert!(data.missing.is_empty(), "{name}: missing {:?}", data.missing);

        assert_eq!(data.game_title.as_deref(), Some("Lantern of Ys"));
        assert_eq!(data.currency.as_deref(), Some("G"));
        assert_eq!(data.switch_name(1), Some("Met the innkeeper"));
        assert_eq!(data.switch_name(4), None, "unnamed switches stay unnamed");
        assert_eq!(data.variable_name(11), Some("Score"));
        assert_eq!(data.entry_name(ItemKind::Item, 7), Some("Harbour Key"));
        assert_eq!(data.entry_name(ItemKind::Weapon, 2), Some("Storm Rod"));
        assert_eq!(data.entry_name(ItemKind::Armor, 9), Some("Gale Charm"));
        assert_eq!(data.skill_name(4), Some("Spark"));
        assert_eq!(data.state_name(4), Some("Poison"));
        assert_eq!(data.class_name(2), Some("Stormcaller"));
        assert_eq!(data.map_name(12), Some("Sunken Vault"));
        assert_eq!(data.equip_type_name(1), Some("Shield"));
    }
}

#[test]
fn reads_actor_details_on_both_engines() {
    for name in ["ace_project/Save1.rvdata2", "vx_project/Save1.rvdata"] {
        let (save, data) = project(name);
        let actor = save.actor(1).expect("actor 1");

        assert_eq!(save.actor_string(actor, "@name").as_deref(), Some("Reid"));
        assert_eq!(save.actor_level(actor), Some(12));
        assert_eq!(save.actor_class_id(actor), Some(1));
        assert!(save.actor_exp(actor).is_some(), "{name}: experience");

        let equips = save.actor_equips(actor);
        assert_eq!(equips.len(), 5, "{name}: five slots");
        assert_eq!(equips[0].kind, ItemKind::Weapon);
        assert_eq!(equips[0].item_id, 1);
        assert_eq!(equips[1].kind, ItemKind::Armor);
        assert_eq!(equips[4].item_id, 2);

        assert_eq!(save.actor_id_list(actor, "@skills"), vec![1, 2, 4]);

        let params = save.actor_param_plus(actor);
        assert_eq!(params[2].0, "Attack");
        assert_eq!(params[2].3, 5, "{name}: the fixture gives Reid +5 attack");

        // Base stats come from the class table in the database.
        assert_eq!(save.actor_param_base(actor, 0, Some(&data)), Some(400 + 42 * 12));
    }
}

#[test]
fn level_changes_carry_experience_with_them() {
    // VX keeps the whole curve on the actor, so no database is needed.
    let (mut save, _) = project("vx_project/Save1.rvdata");
    let actor = save.actor(1).unwrap();
    assert_eq!(save.set_actor_level(actor, 20, None), LevelChange::LevelAndExp);
    assert_eq!(save.actor_level(actor), Some(20));
    assert_eq!(save.actor_exp(actor), Some(20 * 20 * 10), "from @exp_list");

    // VX Ace computes it from the class's four curve parameters.
    let (mut save, data) = project("ace_project/Save1.rvdata2");
    let actor = save.actor(1).unwrap();
    assert_eq!(save.set_actor_level(actor, 20, Some(&data)), LevelChange::LevelAndExp);
    let expected = data.class(1).unwrap().exp_for_level(20).unwrap();
    assert_eq!(save.actor_exp(actor), Some(expected));
    assert!(expected > 0);

    // Without the database, the level still moves and the caller is told.
    let actor = save.actor(2).unwrap();
    assert_eq!(save.set_actor_level(actor, 30, None), LevelChange::LevelOnly);
    assert_eq!(save.actor_level(actor), Some(30));
}

#[test]
fn experience_matches_the_engine_formula() {
    let (_, data) = project("ace_project/Save1.rvdata2");
    let class = data.class(1).unwrap();
    // Reference values produced by running VX Ace's own
    // `RPG::Class#exp_for_level` in Ruby with this class's [30, 20, 30, 30].
    for (level, expected) in [
        (1, 0),
        (2, 50),
        (3, 162),
        (10, 5_296),
        (20, 40_899),
        (50, 529_492),
        (99, 2_547_133),
    ] {
        assert_eq!(class.exp_for_level(level), Some(expected), "level {level}");
    }
}

#[test]
fn equipment_and_skills_can_be_changed() {
    for name in ["ace_project/Save1.rvdata2", "vx_project/Save1.rvdata"] {
        let (mut save, _) = project(name);
        let actor = save.actor(1).unwrap();

        assert!(save.set_actor_equip(actor, 0, ItemKind::Weapon, 2));
        assert!(save.set_actor_equip(actor, 2, ItemKind::Armor, 9));
        assert!(save.set_actor_equip(actor, 4, ItemKind::Armor, 0));
        assert!(save.set_actor_id_list(actor, "@skills", &[1, 2, 4, 5]));
        assert!(save.set_actor_string(actor, "@name", "Reidalt"));
        assert!(save.set_actor_param_plus(actor, 2, 25));

        let bytes = save.to_bytes();
        let reloaded = SaveFile::from_bytes(&bytes, None).expect("reload");
        let actor = reloaded.actor(1).unwrap();

        let equips = reloaded.actor_equips(actor);
        assert_eq!(equips[0].item_id, 2, "{name}");
        assert_eq!(equips[2].item_id, 9);
        assert_eq!(equips[2].kind, ItemKind::Armor);
        assert_eq!(equips[4].item_id, 0, "emptied slot");
        assert_eq!(reloaded.actor_id_list(actor, "@skills"), vec![1, 2, 4, 5]);
        assert_eq!(reloaded.actor_string(actor, "@name").as_deref(), Some("Reidalt"));
        assert_eq!(reloaded.actor_param_plus(actor)[2].3, 25);
    }
}

#[test]
fn renaming_an_actor_keeps_the_ruby_string_encoding() {
    let (mut save, _) = project("ace_project/Save1.rvdata2");
    let actor = save.actor(1).unwrap();
    save.set_actor_string(actor, "@name", "Ríadán");

    let bytes = save.to_bytes();
    // RGSS3 strings carry `:E => true`; the rewritten name must keep it or the
    // engine treats the bytes as ASCII-8BIT and mangles them.
    let name_value = save.heap.ivar(actor, "@name").unwrap();
    let ivars = save.heap.ivar_names(name_value);
    assert_eq!(ivars, vec!["E".to_owned()]);

    let reloaded = SaveFile::from_bytes(&bytes, None).unwrap();
    let actor = reloaded.actor(1).unwrap();
    assert_eq!(reloaded.actor_string(actor, "@name").as_deref(), Some("Ríadán"));
}

#[test]
fn full_heal_restores_stats_and_clears_states() {
    let (mut save, data) = project("ace_project/Save1.rvdata2");
    let actor = save.actor(1).unwrap();
    save.set_actor_states(actor, &[4]);
    assert_eq!(save.actor_id_list(actor, "@states"), vec![4]);

    save.full_heal_actor(actor, Some(&data));
    assert_eq!(save.heap.ivar_int(actor, "@hp"), Some(400 + 42 * 12));
    assert!(save.actor_id_list(actor, "@states").is_empty());
}
