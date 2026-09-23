//! Play time lives in one place and is displayed from another: the counter is
//! an instance variable on `Game_System`, while the load screen reads the
//! `:playtime_s` string in the save header. Keeping those two in step — and
//! never rewriting the string from a counter we failed to find — is what these
//! tests cover.

use rpgsave::marshal::Value;
use rpgsave::rpg::{format_playtime, SaveFile};

fn open(name: &str) -> SaveFile {
    let path = format!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/{}"), name);
    SaveFile::open(std::path::Path::new(&path)).expect("open")
}

fn header_playtime(save: &SaveFile) -> Option<String> {
    let key = Value::Sym(save.heap.sym_id("playtime_s")?);
    let value = save.heap.hash_get(save.documents[0], key)?;
    save.heap.string(value)
}

/// RGSS3's `Game_System#on_before_save` writes `@frames_on_save`.
#[test]
fn reads_the_frame_counter_vx_ace_actually_writes() {
    let save = open("ace_project/Save1.rvdata2");
    assert_eq!(save.playtime_frames(), Some(91_235));
    assert_eq!(save.playtime_seconds(), Some(1_520));
    assert_eq!(header_playtime(&save).as_deref(), Some("00:25:20"));
}

#[test]
fn editing_play_time_updates_the_counter_and_the_header_string() {
    let mut save = open("ace_project/Save1.rvdata2");
    assert!(save.set_playtime_frames(3 * 3600 * 60 + 25 * 60 * 60 + 7 * 60));

    let bytes = save.to_bytes();
    let reloaded = SaveFile::from_bytes(&bytes, None).expect("reload");

    assert_eq!(reloaded.playtime_seconds(), Some(3 * 3600 + 25 * 60 + 7));
    assert_eq!(header_playtime(&reloaded).as_deref(), Some("03:25:07"));
    assert_eq!(format_playtime(3 * 3600 + 25 * 60 + 7), "03:25:07");
}

/// The bug this file exists for: a save whose counter is stored under a name
/// the editor does not recognise must keep the play time its load screen shows.
/// Writing a string derived from an unknown counter would display 00:00:00.
#[test]
fn an_unrecognised_counter_leaves_the_header_string_alone() {
    let mut save = open("unknown_playtime.rvdata2");
    assert_eq!(save.playtime_frames(), None, "the counter is under a custom name");
    assert_eq!(header_playtime(&save).as_deref(), Some("03:11:07"));

    // An edit to something unrelated must not disturb the play time.
    save.set_gold(4_242);
    let bytes = save.to_bytes();
    let reloaded = SaveFile::from_bytes(&bytes, None).expect("reload");

    assert_eq!(reloaded.gold(), Some(4_242));
    assert_eq!(
        header_playtime(&reloaded).as_deref(),
        Some("03:11:07"),
        "the load screen's play time must survive an unrelated edit"
    );
}

/// And with no counter to write to, the editor refuses rather than inventing a
/// field the engine will never read.
#[test]
fn play_time_cannot_be_set_when_there_is_nowhere_to_put_it() {
    let mut save = open("unknown_playtime.rvdata2");
    assert!(!save.set_playtime_frames(60 * 60));
    assert_eq!(save.playtime_frames(), None);

    let system = save.system().expect("Game_System");
    assert!(
        save.heap.ivar(system, "@frames_on_save").is_none(),
        "no counter should have been invented"
    );
}

/// Saving a file nobody edited must leave the header exactly as it was.
#[test]
fn saving_without_touching_play_time_rewrites_nothing() {
    for name in ["ace_project/Save1.rvdata2", "mirrored_header.rvdata2", "unknown_playtime.rvdata2"] {
        let before = open(name);
        let original = header_playtime(&before);

        let mut save = open(name);
        save.set_gold(1);
        let bytes = save.to_bytes();
        let after = SaveFile::from_bytes(&bytes, None).expect("reload");

        assert_eq!(header_playtime(&after), original, "{name}");
    }
}

/// On a mirrored save the counter exists in both copies and both must move.
#[test]
fn play_time_reaches_every_copy_of_game_system() {
    let mut save = open("mirrored_header.rvdata2");
    assert_eq!(save.roots("Game_System").len(), 2);
    assert!(save.set_playtime_frames(7_200 * 60));

    let bytes = save.to_bytes();
    let reloaded = SaveFile::from_bytes(&bytes, None).expect("reload");
    for system in reloaded.roots("Game_System") {
        assert_eq!(reloaded.heap.ivar_int(system, "@frames_on_save"), Some(7_200 * 60));
    }
    assert_eq!(header_playtime(&reloaded).as_deref(), Some("02:00:00"));
}
