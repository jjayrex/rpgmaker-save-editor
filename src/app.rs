//! The application shell: title bar, tabs, and the file-level actions.

use leptos::prelude::*;
use leptos::task::spawn_local;
use wasm_bindgen::prelude::*;

use crate::api;
use crate::state::{ctx, format_bytes, Ctx, Level, Tab};
use crate::views;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "event"], catch)]
    async fn listen(event: &str, handler: &JsValue) -> Result<JsValue, JsValue>;
}

#[component]
pub fn App() -> impl IntoView {
    provide_context(Ctx::new());
    let ctx = ctx();
    watch_for_dropped_files(ctx);

    // A save named on the command line is already open by the time the
    // interface starts.
    spawn_local(async move {
        if let Ok(summary) = api::summary().await {
            ctx.summary.set(Some(summary));
        }
    });

    view! {
        <div class="app">
            <TitleBar/>
            <Show
                when=move || ctx.summary.with(|s| s.is_some())
                fallback=|| view! { <WelcomeScreen/> }
            >
                <TabBar/>
                <main class="content">
                    {move || match ctx.tab.get() {
                        Tab::Overview => view! { <views::overview::OverviewView/> }.into_any(),
                        Tab::Actors => view! { <views::actors::ActorsView/> }.into_any(),
                        Tab::Inventory => view! { <views::inventory::InventoryTab/> }.into_any(),
                        Tab::Switches => view! { <views::flags::SwitchesView/> }.into_any(),
                        Tab::Variables => view! { <views::flags::VariablesView/> }.into_any(),
                        Tab::SelfSwitches => view! { <views::flags::SelfSwitchesView/> }.into_any(),
                        Tab::Raw => view! { <views::raw::RawView/> }.into_any(),
                    }}
                </main>
            </Show>
            <StatusBar/>
        </div>
    }
}

/// Tauri reports files dropped on the window as an event; opening them is the
/// quickest route from "I have a save" to "I am editing it".
fn watch_for_dropped_files(ctx: Ctx) {
    let handler = Closure::<dyn FnMut(JsValue)>::new(move |event: JsValue| {
        let Some(paths) = drag_drop_paths(&event) else { return };
        let Some(path) = paths.into_iter().next() else { return };
        spawn_local(async move {
            ctx.accept_new_file(api::open_save(&path).await);
        });
    });
    spawn_local(async move {
        let _ = listen("tauri://drag-drop", handler.as_ref()).await;
        // The listener has to outlive this task.
        handler.forget();
    });
}

fn drag_drop_paths(event: &JsValue) -> Option<Vec<String>> {
    let payload = js_sys::Reflect::get(event, &JsValue::from_str("payload")).ok()?;
    let paths = js_sys::Reflect::get(&payload, &JsValue::from_str("paths")).ok()?;
    let array = js_sys::Array::from(&paths);
    let paths: Vec<String> = array.iter().filter_map(|v| v.as_string()).collect();
    (!paths.is_empty()).then_some(paths)
}

#[component]
fn TitleBar() -> impl IntoView {
    let ctx = ctx();

    let open = move |_| {
        spawn_local(async move {
            ctx.busy.set(true);
            match api::pick_save_file().await {
                Ok(Some(path)) => ctx.accept_new_file(api::open_save(&path).await),
                Ok(None) => {}
                Err(e) => ctx.fail(e),
            }
            ctx.busy.set(false);
        });
    };

    let save = move |_| {
        spawn_local(async move {
            ctx.busy.set(true);
            match api::write_save(None, true).await {
                Ok(result) => {
                    let backup = match result.backup {
                        Some(path) => format!(" · backup: {}", file_name(&path)),
                        None => String::new(),
                    };
                    ctx.done(format!("Saved {}{backup}", file_name(&result.path)));
                    ctx.accept(api::summary().await);
                }
                Err(e) => ctx.fail(e),
            }
            ctx.busy.set(false);
        });
    };

    let save_as = move |_| {
        spawn_local(async move {
            let name = ctx
                .summary
                .with(|s| s.as_ref().map(|s| s.file_name.clone()))
                .unwrap_or_else(|| "Save1.rvdata2".to_owned());
            ctx.busy.set(true);
            match api::pick_save_target(&name).await {
                Ok(Some(path)) => match api::write_save(Some(path), false).await {
                    Ok(result) => {
                        ctx.done(format!("Saved a copy as {}", file_name(&result.path)));
                        ctx.accept(api::summary().await);
                    }
                    Err(e) => ctx.fail(e),
                },
                Ok(None) => {}
                Err(e) => ctx.fail(e),
            }
            ctx.busy.set(false);
        });
    };

    let close = move |_| {
        spawn_local(async move {
            match api::close_save().await {
                Ok(()) => ctx.close(),
                Err(e) => ctx.fail(e),
            }
        });
    };

    view! {
        <header class="titlebar">
            <div class="brand">
                <span class="brand-mark">"RPG Maker"</span>
                <span class="brand-name">"Save Editor"</span>
            </div>

            <div class="file-info">
                {move || ctx.summary.with(|summary| match summary {
                    None => view! { <span class="muted">"No file open"</span> }.into_any(),
                    Some(s) => {
                        let dirty = s.dirty;
                        view! {
                            <span class="file-name" title=s.path.clone()>{s.file_name.clone()}</span>
                            <span class="badge">{s.engine_label.clone()}</span>
                            {s.game_title.clone().map(|t| view! { <span class="muted">{t}</span> })}
                            <Show when=move || dirty>
                                <span class="dot" title="Unsaved changes"></span>
                            </Show>
                        }.into_any()
                    }
                })}
            </div>

            <div class="actions">
                <button class="btn" on:click=open>"Open"</button>
                <Show when=move || ctx.summary.with(|s| s.is_some())>
                    <button class="btn primary" on:click=save>"Save"</button>
                    <button class="btn" on:click=save_as>"Save as"</button>
                    <button class="btn ghost" on:click=close>"Close"</button>
                </Show>
            </div>
        </header>
    }
}

#[component]
fn TabBar() -> impl IntoView {
    let ctx = ctx();
    view! {
        <nav class="tabs">
            {Tab::ALL
                .into_iter()
                .map(|tab| {
                    view! {
                        <button
                            class="tab"
                            class:active=move || ctx.tab.get() == tab
                            on:click=move |_| ctx.tab.set(tab)
                        >
                            {tab.label()}
                        </button>
                    }
                })
                .collect_view()}
        </nav>
    }
}

#[component]
fn StatusBar() -> impl IntoView {
    let ctx = ctx();
    view! {
        <footer class="statusbar">
            <div class="status-left">
                {move || ctx.summary.with(|s| s.as_ref().map(|s| {
                    view! {
                        <span class="path" title=s.path.clone()>{s.path.clone()}</span>
                        <span class="muted">
                            {format!("{} · {} documents", format_bytes(s.file_size), s.document_count)}
                        </span>
                    }
                }))}
            </div>
            <div class="status-right">
                <Show when=move || ctx.busy.get()>
                    <span class="muted">"Working…"</span>
                </Show>
                {move || ctx.toast.get().map(|toast| {
                    let class = match toast.level {
                        Level::Error => "toast error",
                        Level::Success => "toast success",
                    };
                    view! {
                        <span class=class on:click=move |_| ctx.clear_toast()>
                            {toast.text}
                        </span>
                    }
                })}
            </div>
        </footer>
    }
}

#[component]
fn WelcomeScreen() -> impl IntoView {
    let ctx = ctx();
    let open = move |_| {
        spawn_local(async move {
            match api::pick_save_file().await {
                Ok(Some(path)) => ctx.accept_new_file(api::open_save(&path).await),
                Ok(None) => {}
                Err(e) => ctx.fail(e),
            }
        });
    };

    view! {
        <main class="welcome">
            <div class="welcome-card">
                <h1>"RPG Maker Save Editor"</h1>
                <p>
                    "Open a save file from RPG Maker VX Ace, VX or XP."
                </p>
                <button class="btn primary large" on:click=open>"Open a save file…"</button>
                <p class="muted small">
                    "You can also drop a save file onto this window."
                </p>
                <ul class="muted small notes">
                    <li>"Every write keeps a timestamped backup of the previous file."</li>
                    <li>
                        "If the game's "<code>"Data"</code>" folder is readable, items, skills, \
                         switches and maps are shown by name instead of by id."
                    </li>
                </ul>
            </div>
        </main>
    }
}

pub fn file_name(path: &str) -> String {
    path.rsplit(['/', '\\']).next().unwrap_or(path).to_owned()
}
