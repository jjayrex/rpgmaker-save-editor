//! Calls into the Tauri backend.

use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::{json, Value as Json};
use wasm_bindgen::prelude::*;

use rpgsave_protocol::*;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "core"], catch)]
    async fn invoke(cmd: &str, args: JsValue) -> Result<JsValue, JsValue>;
}

/// Invokes a command and decodes its reply.
///
/// Arguments go out as a plain JavaScript object with camelCase keys, which is
/// what Tauri matches against the command's parameters.
pub async fn call<T: DeserializeOwned>(cmd: &str, args: Json) -> Result<T, String> {
    let js = to_js(args)?;
    match invoke(cmd, js).await {
        Ok(value) => serde_wasm_bindgen::from_value(value)
            .map_err(|e| format!("Unexpected reply from {cmd}: {e}")),
        Err(err) => Err(error_text(err)),
    }
}

/// Invokes a command whose reply carries nothing worth decoding.
pub async fn call_unit(cmd: &str, args: Json) -> Result<(), String> {
    let js = to_js(args)?;
    invoke(cmd, js).await.map(|_| ()).map_err(error_text)
}

fn to_js(args: Json) -> Result<JsValue, String> {
    // The default serializer emits `Map`s; Tauri needs plain objects.
    let serializer = serde_wasm_bindgen::Serializer::json_compatible();
    args.serialize(&serializer)
        .map_err(|e| format!("Could not encode arguments: {e}"))
}

fn error_text(err: JsValue) -> String {
    err.as_string()
        .or_else(|| {
            js_sys::Reflect::get(&err, &JsValue::from_str("message"))
                .ok()
                .and_then(|m| m.as_string())
        })
        .unwrap_or_else(|| "The editor backend reported an unknown error.".to_owned())
}

// ------------------------------------------------------------------- file

pub async fn pick_save_file() -> Result<Option<String>, String> {
    call("pick_save_file", json!({})).await
}

pub async fn pick_save_target(file_name: &str) -> Result<Option<String>, String> {
    call("pick_save_target", json!({ "fileName": file_name })).await
}

pub async fn pick_data_dir() -> Result<Option<String>, String> {
    call("pick_data_dir", json!({})).await
}

pub async fn open_save(path: &str) -> Result<Summary, String> {
    call("open_save", json!({ "path": path })).await
}

pub async fn close_save() -> Result<(), String> {
    call_unit("close_save", json!({})).await
}

pub async fn summary() -> Result<Summary, String> {
    call("summary", json!({})).await
}

pub async fn write_save(path: Option<String>, backup: bool) -> Result<WriteResult, String> {
    call("write_save", json!({ "path": path, "backup": backup })).await
}

pub async fn set_data_dir(path: &str) -> Result<Summary, String> {
    call("set_data_dir", json!({ "path": path })).await
}

// ------------------------------------------------------------------ party

pub async fn set_gold(value: i64) -> Result<Summary, String> {
    call("set_gold", json!({ "value": value })).await
}

pub async fn set_steps(value: i64) -> Result<Summary, String> {
    call("set_steps", json!({ "value": value })).await
}

pub async fn set_playtime(seconds: i64) -> Result<Summary, String> {
    call("set_playtime", json!({ "seconds": seconds })).await
}

pub async fn set_position(x: i64, y: i64) -> Result<Summary, String> {
    call("set_position", json!({ "x": x, "y": y })).await
}

pub async fn set_party(ids: Vec<i64>) -> Result<Summary, String> {
    call("set_party", json!({ "ids": ids })).await
}

// ----------------------------------------------------------------- actors

pub async fn get_actor(actor_id: i64) -> Result<ActorView, String> {
    call("get_actor", json!({ "actorId": actor_id })).await
}

pub async fn set_actor_text(actor_id: i64, ivar: &str, text: &str) -> Result<ActorView, String> {
    call(
        "set_actor_text",
        json!({ "actorId": actor_id, "ivar": ivar, "text": text }),
    )
    .await
}

pub async fn set_actor_int(actor_id: i64, ivar: &str, value: i64) -> Result<ActorView, String> {
    call(
        "set_actor_int",
        json!({ "actorId": actor_id, "ivar": ivar, "value": value }),
    )
    .await
}

pub async fn set_actor_scalar(
    actor_id: i64,
    ivar: &str,
    value: &Scalar,
) -> Result<ActorView, String> {
    call(
        "set_actor_scalar",
        json!({ "actorId": actor_id, "ivar": ivar, "value": value }),
    )
    .await
}

pub async fn set_actor_level(actor_id: i64, level: i64) -> Result<ActorView, String> {
    call("set_actor_level", json!({ "actorId": actor_id, "level": level })).await
}

pub async fn set_actor_exp(actor_id: i64, exp: i64) -> Result<ActorView, String> {
    call("set_actor_exp", json!({ "actorId": actor_id, "exp": exp })).await
}

pub async fn set_actor_param(
    actor_id: i64,
    index: usize,
    value: i64,
) -> Result<ActorView, String> {
    call(
        "set_actor_param",
        json!({ "actorId": actor_id, "index": index, "value": value }),
    )
    .await
}

pub async fn set_actor_equip(
    actor_id: i64,
    slot: usize,
    kind: &str,
    item_id: i64,
) -> Result<ActorView, String> {
    call(
        "set_actor_equip",
        json!({ "actorId": actor_id, "slot": slot, "kind": kind, "itemId": item_id }),
    )
    .await
}

pub async fn set_actor_skills(actor_id: i64, ids: Vec<i64>) -> Result<ActorView, String> {
    call("set_actor_skills", json!({ "actorId": actor_id, "ids": ids })).await
}

pub async fn set_actor_states(actor_id: i64, ids: Vec<i64>) -> Result<ActorView, String> {
    call("set_actor_states", json!({ "actorId": actor_id, "ids": ids })).await
}

pub async fn full_heal(actor_id: i64) -> Result<ActorView, String> {
    call("full_heal", json!({ "actorId": actor_id })).await
}

// -------------------------------------------------------------- inventory

pub async fn get_inventory(kind: &str) -> Result<InventoryView, String> {
    call("get_inventory", json!({ "kind": kind })).await
}

pub async fn set_item_count(kind: &str, id: i64, count: i64) -> Result<InventoryView, String> {
    call("set_item_count", json!({ "kind": kind, "id": id, "count": count })).await
}

pub async fn get_catalog(kind: &str, query: &str) -> Result<Vec<CatalogEntry>, String> {
    call("get_catalog", json!({ "kind": kind, "query": query })).await
}

// --------------------------------------------------- switches / variables

pub async fn get_switch_page(
    query: &str,
    offset: usize,
    limit: usize,
) -> Result<SwitchPage, String> {
    call(
        "get_switch_page",
        json!({ "query": query, "offset": offset, "limit": limit }),
    )
    .await
}

pub async fn set_switch(id: i64, on: bool) -> Result<Applied, String> {
    call("set_switch", json!({ "id": id, "on": on })).await
}

pub async fn set_switches(ids: Vec<i64>, on: bool) -> Result<Applied, String> {
    call("set_switches", json!({ "ids": ids, "on": on })).await
}

pub async fn get_variable_page(
    query: &str,
    offset: usize,
    limit: usize,
) -> Result<VariablePage, String> {
    call(
        "get_variable_page",
        json!({ "query": query, "offset": offset, "limit": limit }),
    )
    .await
}

pub async fn set_variable(id: i64, value: &Scalar) -> Result<Applied, String> {
    call("set_variable", json!({ "id": id, "value": value })).await
}

pub async fn get_self_switches() -> Result<Vec<SelfSwitchRow>, String> {
    call("get_self_switches", json!({})).await
}

pub async fn set_self_switch(
    map_id: i64,
    event_id: i64,
    letter: &str,
    on: bool,
) -> Result<Vec<SelfSwitchRow>, String> {
    call(
        "set_self_switch",
        json!({ "mapId": map_id, "eventId": event_id, "letter": letter, "on": on }),
    )
    .await
}

pub async fn remove_self_switch(
    map_id: i64,
    event_id: i64,
    letter: &str,
) -> Result<Vec<SelfSwitchRow>, String> {
    call(
        "remove_self_switch",
        json!({ "mapId": map_id, "eventId": event_id, "letter": letter }),
    )
    .await
}

// ------------------------------------------------------------ raw editor

pub async fn raw_root() -> Result<NodeView, String> {
    call("raw_root", json!({})).await
}

pub async fn raw_node(node: u32) -> Result<NodeView, String> {
    call("raw_node", json!({ "node": node })).await
}

pub async fn set_raw_scalar(
    node: Option<u32>,
    slot: &Slot,
    value: &Scalar,
) -> Result<NodeView, String> {
    call(
        "set_raw_scalar",
        json!({ "node": node, "slot": slot, "value": value }),
    )
    .await
}
