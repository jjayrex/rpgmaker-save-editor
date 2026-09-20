//! Prints what the editor makes of a save file, and checks that re-encoding it
//! reproduces the original bytes.
//!
//!     cargo run -p rpgsave --example inspect -- path/to/Save1.rvdata2

use std::path::Path;

use rpgsave::marshal;
use rpgsave::rpg::{format_playtime, save::ItemKind, GameData, SaveFile};

fn main() {
    let mut failures = 0;
    for path in std::env::args().skip(1) {
        if let Err(message) = inspect(Path::new(&path)) {
            eprintln!("{path}: {message}");
            failures += 1;
        }
        println!();
    }
    if failures > 0 {
        std::process::exit(1);
    }
}

fn inspect(path: &Path) -> Result<(), String> {
    let original = std::fs::read(path).map_err(|e| e.to_string())?;
    let save = SaveFile::open(path).map_err(|e| e.to_string())?;

    println!("=== {} ===", path.display());
    println!(
        "engine        {} · {} bytes · {} documents",
        save.engine.label(),
        original.len(),
        save.documents.len()
    );

    let data = save.guess_data_dir().map(|dir| GameData::load(&dir, save.engine));
    match &data {
        Some(d) => println!("database      {} ({} files)", d.dir.display(), d.loaded.len()),
        None => println!("database      not found (ids will not have names)"),
    }

    println!(
        "play time     {}",
        format_playtime(save.playtime_seconds().unwrap_or(0))
    );
    println!("gold          {:?}", save.gold());
    println!("steps         {:?}", save.steps());
    println!("map           {:?} at {:?}", save.map_id(), save.player_position());
    println!("party         {:?}", save.party_member_ids());
    // Games that mirror their state into the save header keep two of each.
    let mirrored: Vec<String> = ["Game_Party", "Game_Switches", "Game_Variables", "Game_Actors"]
        .iter()
        .filter(|class| save.roots(class).len() > 1)
        .map(|class| format!("{class} ×{}", save.roots(class).len()))
        .collect();
    if !mirrored.is_empty() {
        println!("mirrored      {} (edits are written to every copy)", mirrored.join(", "));
    }

    println!(
        "switches      {} slots · variables {} slots · self switches {}",
        save.data_len("Game_Switches"),
        save.data_len("Game_Variables"),
        save.self_switches().len()
    );

    for kind in ItemKind::all() {
        let held = save.inventory(kind);
        println!("{:<13} {} kinds held", kind.id(), held.len());
    }

    println!("actors:");
    for (id, actor) in save.actors().into_iter().take(8) {
        println!(
            "  #{id:<3} {:<16} level {:<3} hp {:<6} class {:?} equips {:?}",
            save.actor_string(actor, "@name").unwrap_or_default(),
            save.actor_level(actor).unwrap_or(0),
            save.heap.ivar_int(actor, "@hp").unwrap_or(0),
            save.actor_class_id(actor),
            save.actor_equips(actor).iter().map(|e| e.item_id).collect::<Vec<_>>(),
        );
    }

    // The important part: what we would write back has to match what we read.
    let rewritten = marshal::dump_stream(&save.documents, &save.heap);
    if rewritten == original {
        println!("round trip    byte-identical ✓");
    } else {
        let at = rewritten
            .iter()
            .zip(&original)
            .position(|(a, b)| a != b)
            .unwrap_or(rewritten.len().min(original.len()));
        println!(
            "round trip    DIFFERS at byte {at} (ours {} bytes, original {} bytes)",
            rewritten.len(),
            original.len()
        );
        let from = at.saturating_sub(12);
        println!("  ours     {:02x?}", &rewritten[from..(at + 20).min(rewritten.len())]);
        println!("  original {:02x?}", &original[from..(at + 20).min(original.len())]);
        return Err("re-encoding is not faithful".to_owned());
    }
    Ok(())
}
