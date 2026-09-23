//! Reading and editing the two save layouts, checked against fixtures whose
//! contents are known exactly.

use rpgsave::marshal::Value;
use rpgsave::rpg::{save::ItemKind, Engine, SaveFile};

fn open(name: &str) -> SaveFile {
    let path = format!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/{}"), name);
    SaveFile::open(std::path::Path::new(&path)).expect("open")
}

#[test]
fn reads_a_vx_ace_save() {
    let save = open("ace_save.rvdata2");
    assert_eq!(save.engine, Engine::VxAce);
    assert_eq!(save.gold(), Some(12_345));
    assert_eq!(save.steps(), Some(4_207));
    assert_eq!(save.party_member_ids(), vec![1, 2]);
    assert_eq!(save.map_id(), Some(3));
    assert_eq!(save.player_position(), Some((8, 6)));
    assert_eq!(save.playtime_frames(), Some(91_235));
    assert_eq!(save.playtime_seconds(), Some(1_520));

    assert_eq!(save.inventory(ItemKind::Item), vec![(1, 9), (2, 3), (7, 1)]);
    assert_eq!(save.inventory(ItemKind::Weapon), vec![(1, 1), (2, 1)]);

    assert!(save.switch(1) && save.switch(13));
    assert!(!save.switch(3));
    assert_eq!(save.variable(11).and_then(Value::as_int), Some(999_999));

    let actors = save.actors();
    assert_eq!(actors.len(), 3);
    let (id, reid) = actors[0];
    assert_eq!(id, 1);
    assert_eq!(save.heap.ivar(reid, "@name").and_then(|v| save.heap.string(v)).as_deref(), Some("Reid"));
    assert_eq!(save.heap.ivar_int(reid, "@level"), Some(12));

    let self_switches = save.self_switches();
    assert_eq!(self_switches[0], (3, 4, "A".to_owned(), true));
}

#[test]
fn reads_a_vx_save() {
    let save = open("vx_save.rvdata");
    assert_eq!(save.engine, Engine::Vx);
    assert_eq!(save.gold(), Some(12_345));
    assert_eq!(save.party_member_ids(), vec![1, 2]);
    // VX keeps play time in its own document rather than on Game_System.
    assert_eq!(save.playtime_frames(), Some(91_235));
    assert_eq!(save.map_id(), Some(3));
    assert!(save.switch(5));
    assert_eq!(save.actors().len(), 3);
}

#[test]
fn edits_survive_a_round_trip() {
    for name in ["ace_save.rvdata2", "vx_save.rvdata"] {
        let mut save = open(name);
        assert!(save.set_gold(777));
        assert!(save.set_steps(42));
        assert!(save.set_switch(3, true));
        assert!(save.set_switch(1, false));
        assert!(save.set_variable(2, Value::Int(-500)));
        assert!(save.set_item_count(ItemKind::Item, 1, 99));
        assert!(save.set_item_count(ItemKind::Item, 2, 0)); // removes the entry
        assert!(save.set_item_count(ItemKind::Armor, 9, 4)); // brand-new entry
        assert!(save.set_party_member_ids(&[2, 3, 1]));
        assert!(save.set_player_position(11, 5));
        assert!(save.set_playtime_frames(3 * 3600 * 60));
        assert!(save.dirty);

        let bytes = save.to_bytes();
        let reloaded = SaveFile::from_bytes(&bytes, None).expect("reload");

        assert_eq!(reloaded.gold(), Some(777), "{name}");
        assert_eq!(reloaded.steps(), Some(42));
        assert!(reloaded.switch(3) && !reloaded.switch(1));
        assert_eq!(reloaded.variable(2).and_then(Value::as_int), Some(-500));
        assert_eq!(
            reloaded.inventory(ItemKind::Item),
            vec![(1, 99), (7, 1)],
            "count of 2 should be gone"
        );
        assert_eq!(reloaded.inventory(ItemKind::Armor), vec![(1, 2), (2, 1), (9, 4)]);
        assert_eq!(reloaded.party_member_ids(), vec![2, 3, 1]);
        assert_eq!(reloaded.player_position(), Some((11, 5)));
        assert_eq!(reloaded.playtime_seconds(), Some(3 * 3600));
    }
}

#[test]
fn gold_and_item_counts_stay_within_engine_limits() {
    let mut save = open("ace_save.rvdata2");
    save.set_gold(i64::MAX);
    assert_eq!(save.gold(), Some(99_999_999));
    save.set_gold(-5);
    assert_eq!(save.gold(), Some(0));
    save.set_item_count(ItemKind::Item, 1, 1_000);
    assert_eq!(save.inventory(ItemKind::Item)[0], (1, 99));
}

#[test]
fn the_save_header_follows_the_edited_party() {
    let mut save = open("ace_save.rvdata2");
    save.set_party_member_ids(&[3]);
    save.set_playtime_frames(60 * 60);
    let bytes = save.to_bytes();

    let reloaded = SaveFile::from_bytes(&bytes, None).expect("reload");
    let header = reloaded.documents[0];
    let playtime = reloaded
        .heap
        .sym_id("playtime_s")
        .and_then(|s| reloaded.heap.hash_get(header, Value::Sym(s)))
        .and_then(|v| reloaded.heap.string(v));
    assert_eq!(playtime.as_deref(), Some("00:01:00"));

    let characters = reloaded
        .heap
        .sym_id("characters")
        .and_then(|s| reloaded.heap.hash_get(header, Value::Sym(s)))
        .expect("characters");
    let rows = reloaded.heap.array(characters).expect("array");
    assert_eq!(rows.len(), 1, "one party member means one face on the load screen");
}

#[test]
fn self_switches_can_be_set_and_cleared() {
    let mut save = open("ace_save.rvdata2");
    save.set_self_switch(3, 4, "A", false);
    save.set_self_switch(12, 1, "C", true);
    let bytes = save.to_bytes();
    let reloaded = SaveFile::from_bytes(&bytes, None).expect("reload");

    let switches = reloaded.self_switches();
    assert!(switches.contains(&(3, 4, "A".to_owned(), false)));
    assert!(switches.contains(&(12, 1, "C".to_owned(), true)));

    let mut save = SaveFile::from_bytes(&bytes, None).expect("reload");
    assert!(save.remove_self_switch(3, 9, "B"));
    assert!(!save.self_switches().iter().any(|s| s.1 == 9));
}

/// Writing goes through a temporary file and leaves a backup behind.
#[test]
fn writing_makes_a_backup_and_replaces_the_file() {
    let dir = std::env::temp_dir().join(format!("rpgsave-write-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let target = dir.join("Save1.rvdata2");
    std::fs::copy(
        concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/ace_save.rvdata2"),
        &target,
    )
    .unwrap();

    let mut save = SaveFile::open(&target).expect("open");
    save.set_gold(1);
    let backup = save.write(&target, true).expect("write").expect("backup path");

    assert!(backup.exists(), "a backup of the original should remain");
    assert!(!save.dirty);
    assert_eq!(SaveFile::open(&target).unwrap().gold(), Some(1));
    assert_eq!(SaveFile::open(&backup).unwrap().gold(), Some(12_345));
    assert!(
        std::fs::read_dir(&dir).unwrap().flatten().all(|e| !e.file_name().to_string_lossy().contains(".tmp")),
        "the temporary file should not survive"
    );
    std::fs::remove_dir_all(&dir).ok();
}

/// Ruby must be able to load a save we edited, not just our own reader.
#[test]
fn ruby_loads_an_edited_save() {
    let fixtures = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures");
    if std::process::Command::new("ruby").arg("-v").output().is_err() {
        eprintln!("skipping: ruby is not installed");
        return;
    }
    for name in ["ace_save.rvdata2", "vx_save.rvdata"] {
        let mut save = open(name);
        save.set_gold(54_321);
        save.set_switch(7, true);
        save.set_item_count(ItemKind::Item, 3, 12);
        let out = std::env::temp_dir().join(format!("rpgsave-edited-{name}"));
        std::fs::write(&out, save.to_bytes()).unwrap();

        let result = std::process::Command::new("ruby")
            .arg("-e")
            .arg(
                r#"
                require_relative "rgss_classes"
                docs = []
                File.open(ARGV[0], "rb") { |f| docs << Marshal.load(f) until f.eof? }
                party = docs.flat_map { |d| d.is_a?(Hash) ? d.values : [d] }
                            .find { |o| o.is_a?(Game_Party) }
                gold = party.instance_variable_get(:@gold)
                items = party.instance_variable_get(:@items)
                switches = docs.flat_map { |d| d.is_a?(Hash) ? d.values : [d] }
                               .find { |o| o.is_a?(Game_Switches) }
                               .instance_variable_get(:@data)
                puts "gold=#{gold} item3=#{items[3]} switch7=#{switches[7]}"
                "#,
            )
            .arg(&out)
            .current_dir(fixtures)
            .output()
            .expect("run ruby");
        let stdout = String::from_utf8_lossy(&result.stdout);
        assert_eq!(
            stdout.trim(),
            "gold=54321 item3=12 switch7=true",
            "{name}: ruby read back {stdout}{}",
            String::from_utf8_lossy(&result.stderr)
        );
        std::fs::remove_file(&out).ok();
    }
}

#[test]
fn an_xp_save_is_turned_away_with_a_clear_reason() {
    let path = std::env::temp_dir().join("rpgsave-not-supported.rxdata");
    std::fs::copy(
        concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/ace_save.rvdata2"),
        &path,
    )
    .unwrap();

    let Err(error) = SaveFile::open(&path) else {
        panic!("XP saves are not supported yet and should be refused");
    };
    assert!(
        error.to_string().contains("RPG Maker XP"),
        "the message should name the engine, got: {error}"
    );
    let _ = std::fs::remove_file(&path);
}

/// Opening a file and saving it without touching anything must reproduce it
/// byte for byte. This covers the whole write path, including the header
/// refresh, which has no business rewriting fields nobody edited.
#[test]
fn saving_an_untouched_file_changes_nothing() {
    for name in [
        "ace_save.rvdata2",
        "vx_save.rvdata",
        "ace_project/Save1.rvdata2",
        "vx_project/Save1.rvdata",
        "mirrored_header.rvdata2",
        "unknown_playtime.rvdata2",
    ] {
        let path = format!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/{}"), name);
        let original = std::fs::read(&path).expect("read");
        let mut save = SaveFile::open(std::path::Path::new(&path)).expect("open");

        assert_eq!(save.to_bytes(), original, "{name} changed when saved untouched");
        assert!(!save.dirty, "{name}: nothing was edited");
    }
}
