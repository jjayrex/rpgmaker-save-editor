//! Exercises the command surface exactly as the interface calls it.
//!
//! The front end sends camelCase argument names, which Tauri maps onto the
//! snake_case parameters of each command. Nothing in a type system catches a
//! mismatch there, so these tests drive the real handlers through the real IPC
//! path with the real payloads.

use serde::de::DeserializeOwned;
use serde_json::{json, Value};
use tauri::ipc::{CallbackFn, InvokeBody};
use tauri::test::{mock_builder, mock_context, noop_assets, MockRuntime, INVOKE_KEY};
use tauri::webview::InvokeRequest;
use tauri::WebviewWindow;

use rpgsave_protocol::{ActorView, InventoryView, NodeView, Summary, SwitchPage, WriteResult};

fn fixture(name: &str) -> String {
    format!(
        concat!(env!("CARGO_MANIFEST_DIR"), "/../crates/rpgsave/tests/fixtures/{}"),
        name
    )
}

fn editor() -> WebviewWindow<MockRuntime> {
    let app = rpgmaker_save_editor_lib::builder(mock_builder())
        .build(mock_context(noop_assets()))
        .expect("build the app");
    tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .expect("build the window")
}

fn call<T: DeserializeOwned>(
    webview: &WebviewWindow<MockRuntime>,
    cmd: &str,
    args: Value,
) -> Result<T, String> {
    tauri::test::get_ipc_response(
        webview,
        InvokeRequest {
            cmd: cmd.to_owned(),
            callback: CallbackFn(0),
            error: CallbackFn(1),
            // The request has to look as though it came from the app's own
            // origin, or the access list treats it as remote content.
            url: "tauri://localhost".parse().expect("url"),
            body: InvokeBody::Json(args),
            headers: Default::default(),
            invoke_key: INVOKE_KEY.to_owned(),
        },
    )
    .map(|body| body.deserialize::<T>().expect("decode the reply"))
    .map_err(|error| error.to_string())
}

fn open(webview: &WebviewWindow<MockRuntime>, name: &str) -> Summary {
    call(webview, "open_save", json!({ "path": fixture(name) })).expect("open the save")
}

#[test]
fn opening_a_save_reports_what_is_in_it() {
    let webview = editor();
    let summary = open(&webview, "ace_project/Save1.rvdata2");

    assert_eq!(summary.engine, "vxace");
    assert_eq!(summary.engine_label, "VX Ace");
    assert_eq!(summary.gold, Some(12_345));
    assert_eq!(summary.game_title.as_deref(), Some("Lantern of Ys"));
    assert_eq!(summary.map_name.as_deref(), Some("Harbor Town"));
    assert_eq!(summary.party.len(), 2);
    assert_eq!(summary.party[0].name, "Reid");
    assert_eq!(summary.party[0].class_name.as_deref(), Some("Wayfarer"));
    assert!(!summary.dirty);
    assert!(summary.data_missing.is_empty());
}

#[test]
fn opening_something_that_is_not_a_save_explains_itself() {
    let webview = editor();
    let error = call::<Summary>(&webview, "open_save", json!({ "path": fixture("generate.rb") }))
        .expect_err("a Ruby script is not a save file");
    assert!(
        error.contains("Marshal"),
        "the message should say what went wrong, got: {error}"
    );
}

#[test]
fn commands_with_no_save_open_say_so() {
    let webview = editor();
    let error = call::<Summary>(&webview, "summary", json!({})).expect_err("nothing is open");
    assert!(error.contains("No save file is open"), "got: {error}");
}

/// The multi-word argument names are the ones a casing mismatch would break.
#[test]
fn actor_commands_accept_the_front_ends_argument_names() {
    let webview = editor();
    open(&webview, "ace_project/Save1.rvdata2");

    let actor: ActorView = call(&webview, "get_actor", json!({ "actorId": 1 })).expect("get_actor");
    assert_eq!(actor.name, "Reid");
    assert_eq!(actor.level, Some(12));
    assert_eq!(actor.equips[0].item_name.as_deref(), Some("Bronze Sword"));

    let actor: ActorView = call(
        &webview,
        "set_actor_text",
        json!({ "actorId": 1, "ivar": "@name", "text": "Reidalt" }),
    )
    .expect("set_actor_text");
    assert_eq!(actor.name, "Reidalt");

    let actor: ActorView = call(&webview, "set_actor_level", json!({ "actorId": 1, "level": 20 }))
        .expect("set_actor_level");
    assert_eq!(actor.level, Some(20));
    assert_eq!(actor.exp, Some(40_899), "experience follows the class curve");

    let actor: ActorView = call(
        &webview,
        "set_actor_equip",
        json!({ "actorId": 1, "slot": 2, "kind": "armor", "itemId": 9 }),
    )
    .expect("set_actor_equip");
    assert_eq!(actor.equips[2].item_name.as_deref(), Some("Gale Charm"));

    let actor: ActorView = call(&webview, "full_heal", json!({ "actorId": 1 })).expect("full_heal");
    assert_eq!(actor.hp, actor.max_hp);
}

#[test]
fn party_inventory_and_switch_commands_round_trip() {
    let webview = editor();
    open(&webview, "ace_project/Save1.rvdata2");

    let summary: Summary = call(&webview, "set_gold", json!({ "value": 777 })).expect("set_gold");
    assert_eq!(summary.gold, Some(777));
    assert!(summary.dirty, "editing marks the file as unsaved");

    let summary: Summary = call(&webview, "set_position", json!({ "x": 11, "y": 4 })).expect("move");
    assert_eq!((summary.player_x, summary.player_y), (Some(11), Some(4)));

    let inventory: InventoryView = call(
        &webview,
        "set_item_count",
        json!({ "kind": "item", "id": 3, "count": 12 }),
    )
    .expect("set_item_count");
    let antidote = inventory.rows.iter().find(|r| r.id == 3).expect("the new row");
    assert_eq!(antidote.count, 12);
    assert_eq!(antidote.name.as_deref(), Some("Antidote"));

    call::<Value>(&webview, "set_switch", json!({ "id": 3, "on": true })).expect("set_switch");
    let page: SwitchPage =
        call(&webview, "get_switch_page", json!({ "query": "secret", "offset": 0, "limit": 20 }))
            .expect("get_switch_page");
    assert_eq!(page.rows.len(), 1, "the search should narrow to one switch");
    assert_eq!(page.rows[0].name.as_deref(), Some("Secret door open"));
    assert!(page.rows[0].on);

    let rows: Value = call(
        &webview,
        "set_self_switch",
        json!({ "mapId": 12, "eventId": 3, "letter": "C", "on": true }),
    )
    .expect("set_self_switch");
    let rows = rows.as_array().expect("an array of self switches");
    assert!(rows.iter().any(|r| r["map_id"] == 12 && r["letter"] == "C" && r["on"] == true));
}

#[test]
fn the_raw_editor_can_walk_and_change_the_object_graph() {
    let webview = editor();
    open(&webview, "ace_project/Save1.rvdata2");

    let root: NodeView = call(&webview, "raw_root", json!({})).expect("raw_root");
    assert_eq!(root.children.len(), 2, "a VX Ace save is two documents");

    let contents = root.children[1].node.expect("the contents hash is a node");
    let node: NodeView = call(&webview, "raw_node", json!({ "node": contents })).expect("raw_node");
    let party_child = node
        .children
        .iter()
        .find(|c| c.key == ":party")
        .expect("the party entry");

    let party: NodeView =
        call(&webview, "raw_node", json!({ "node": party_child.node.unwrap() })).expect("party");
    assert_eq!(party.title, "Game_Party");

    // Change gold through the raw editor and check the overview agrees.
    let updated: NodeView = call(
        &webview,
        "set_raw_scalar",
        json!({
            "node": party.node,
            "slot": { "ivar": "@gold" },
            "value": { "type": "int", "value": 4_242 },
        }),
    )
    .expect("set_raw_scalar");
    let gold = updated.children.iter().find(|c| c.key == "@gold").expect("@gold");
    assert_eq!(gold.preview, "4242");

    let summary: Summary = call(&webview, "summary", json!({})).expect("summary");
    assert_eq!(summary.gold, Some(4_242));
}

#[test]
fn writing_produces_a_file_the_editor_can_reopen() {
    let webview = editor();
    open(&webview, "ace_project/Save1.rvdata2");
    call::<Summary>(&webview, "set_gold", json!({ "value": 4_096 })).expect("set_gold");

    let target = std::env::temp_dir().join("rpgsave-command-test.rvdata2");
    let _ = std::fs::remove_file(&target);
    let result: WriteResult = call(
        &webview,
        "write_save",
        json!({ "path": target.to_string_lossy(), "backup": false }),
    )
    .expect("write_save");
    assert!(std::path::Path::new(&result.path).is_file());

    let summary: Summary = call(&webview, "summary", json!({})).expect("summary");
    assert!(!summary.dirty, "writing clears the unsaved marker");

    let reopened: Summary =
        call(&webview, "open_save", json!({ "path": target.to_string_lossy() })).expect("reopen");
    assert_eq!(reopened.gold, Some(4_096));
    let _ = std::fs::remove_file(&target);
}

#[test]
fn a_vx_save_goes_through_the_same_commands() {
    let webview = editor();
    let summary = open(&webview, "vx_project/Save1.rvdata");
    assert_eq!(summary.engine, "vx");
    assert_eq!(summary.gold, Some(12_345));

    let actor: ActorView = call(&webview, "get_actor", json!({ "actorId": 1 })).expect("get_actor");
    assert_eq!(actor.name, "Reid");
    assert!(actor.nickname.is_none(), "VX actors have no nickname field");

    // VX carries its own experience table, so no database is needed.
    let actor: ActorView = call(&webview, "set_actor_level", json!({ "actorId": 1, "level": 20 }))
        .expect("set_actor_level");
    assert_eq!(actor.exp, Some(4_000));
}

#[test]
fn an_xp_save_goes_through_the_same_commands() {
    let webview = editor();
    let summary = open(&webview, "xp_project/Save1.rxdata");
    assert_eq!(summary.engine, "xp");
    assert_eq!(summary.engine_label, "XP");
    assert_eq!(summary.frame_rate, 40, "XP counts play time at 40 fps");
    assert_eq!(summary.gold, Some(12_345));
    assert_eq!(summary.party[0].name, "Aluxes");
    assert_eq!(summary.map_name.as_deref(), Some("Harbor Town"));

    let actor: ActorView = call(&webview, "get_actor", json!({ "actorId": 1 })).expect("get_actor");
    assert_eq!(actor.mp_label, "SP");
    assert_eq!(actor.mp_ivar, "@sp");
    assert_eq!(actor.equips[1].label, "Shield", "slot names come from XP's words");
    assert_eq!(actor.params[2].label, "Strength");

    // The interface sends whichever field name the view reported.
    let actor: ActorView = call(
        &webview,
        "set_actor_int",
        json!({ "actorId": 1, "ivar": actor.mp_ivar, "value": 42 }),
    )
    .expect("set_actor_int");
    assert_eq!(actor.mp, Some(42));

    // Adding an actor the save has never created is refused, with a reason.
    let error = call::<Summary>(&webview, "set_party", json!({ "ids": [1, 99] }))
        .expect_err("actor 99 does not exist in this save");
    assert!(error.contains("has not been created"), "got: {error}");

    let summary: Summary = call(&webview, "set_party", json!({ "ids": [2, 1] })).expect("set_party");
    assert_eq!(summary.party.iter().map(|m| m.actor_id).collect::<Vec<_>>(), vec![2, 1]);
}
