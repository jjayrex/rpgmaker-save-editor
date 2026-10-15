//! Prints what the editor reads out of an RPG Maker MV or MZ save, and checks
//! that it re-encodes to the bytes it came from.
//!
//!     cargo run -p rpgsave --example jsonsave -- path/to/file1.rpgsave
use rpgsave::js::save::{actor, ItemKind};
use rpgsave::js::{database, JsonSave};

fn main() {
    let path = std::env::args().nth(1).expect("usage: mv <file.rpgsave>");
    let path = std::path::Path::new(&path);
    let save = match JsonSave::open(path) {
        Ok(save) => save,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    };
    let data = path
        .parent()
        .and_then(database::find_data_dir)
        .and_then(|dir| database::load(&dir));

    println!("file      {} bytes -> {} characters of JSON", save.size_bytes, save.to_json_text().len());
    println!("re-encode {}", if save.to_bytes() == std::fs::read(path).unwrap() { "byte-identical" } else { "DIFFERS" });
    println!("database  {:?}", data.as_ref().map(|d| (&d.game_title, d.loaded.len(), d.missing.len())));
    println!("playtime  {} frames", save.playtime_frames());
    println!("saves     {:?}", save.save_count());
    println!("gold      {}", save.gold());
    println!("steps     {}", save.steps());
    let (x, y) = save.player_position();
    println!("position  map {} at ({x}, {y})", save.map_id());
    println!("party     {:?}", save.party_member_ids());
    println!("switches  {} · variables {}", save.switch_count(), save.variable_count());
    println!("self sw.  {} set", save.self_switches().len());
    for kind in [ItemKind::Item, ItemKind::Weapon, ItemKind::Armor] {
        let held = save.items(kind);
        println!("{kind:?}  {} kinds held {:?}", held.len(), &held[..held.len().min(6)]);
    }
    println!("actors    {:?}", save.actor_ids());
    for id in save.actor_ids().into_iter().take(4) {
        println!(
            "   #{id} {:?} {:?} class {:?} level {:?} exp {:?} hp {:?}/mp {:?} skills {} states {:?}",
            save.actor_text(id, actor::NAME),
            save.actor_text(id, actor::NICKNAME),
            save.actor_int(id, actor::CLASS_ID),
            save.actor_int(id, actor::LEVEL),
            save.actor_exp(id),
            save.actor_int(id, actor::HP),
            save.actor_int(id, actor::MP),
            save.actor_skills(id).len(),
            save.actor_states(id),
        );
        println!("        equipment {:?}", save.actor_equipment(id));
        println!("        params    {:?}", (0..8).map(|i| save.actor_param_plus(id, i)).collect::<Vec<_>>());
    }
}
