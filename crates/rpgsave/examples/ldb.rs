//! Prints what the editor reads out of an RPG Maker 2000/2003 database.
//!
//!     cargo run -p rpgsave --example ldb -- path/to/game-folder
use rpgsave::lcf::database;

fn main() {
    let dir = std::env::args().nth(1).expect("usage: ldb <folder>");
    let Some(data) = database::load(std::path::Path::new(&dir)) else {
        eprintln!("no RPG Maker 2000/2003 database found in {dir}");
        std::process::exit(1);
    };
    println!("title     {:?}", data.game_title);
    println!("currency  {:?}", data.currency);
    println!("slots     {:?}", data.equip_types);
    println!("actors    {}", data.actors.len());
    for a in data.actors.iter().take(6) {
        println!("   #{:<3} {:<14} title {:<12} class {} levels {}..{}",
            a.id, a.name, a.nickname, a.class_id, a.initial_level, a.max_level);
    }
    println!("classes   {:?}", data.classes.iter().map(|c| (c.id, &c.name)).collect::<Vec<_>>());
    println!("items     {}", data.items.len());
    for i in data.items.iter().take(8) {
        println!("   #{:<3} {:<18} type {} price {} — {}", i.id, i.name, i.etype_id, i.price, i.description);
    }
    println!("skills    {:?}", data.skills.iter().take(6).map(|s| (s.id, &s.name)).collect::<Vec<_>>());
    println!("states    {:?}", data.states.iter().take(6).map(|s| (s.id, &s.name)).collect::<Vec<_>>());
    println!("switches  {} named", data.switch_names.iter().filter(|n| !n.is_empty()).count());
    for (id, name) in data.switch_names.iter().enumerate().filter(|(_, n)| !n.is_empty()).take(5) {
        println!("   #{id} {name}");
    }
    println!("variables {} named", data.variable_names.iter().filter(|n| !n.is_empty()).count());
    for (id, name) in data.variable_names.iter().enumerate().filter(|(_, n)| !n.is_empty()).take(5) {
        println!("   #{id} {name}");
    }
}
