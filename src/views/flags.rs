//! Switches, variables and self switches: the game's bookkeeping.

use leptos::prelude::*;
use leptos::task::spawn_local;
use rpgsave_protocol::{Scalar, SelfSwitchRow, SwitchPage, VariablePage};

use crate::api;
use crate::state::ctx;
use crate::views::{number_field, scalar_field, text_field};

const PAGE_SIZE: usize = 100;

#[component]
pub fn SwitchesView() -> impl IntoView {
    let ctx = ctx();
    let page = RwSignal::new(SwitchPage::default());
    let query = RwSignal::new(String::new());
    let offset = RwSignal::new(0usize);

    let reload = move || {
        let (q, off) = (query.get_untracked(), offset.get_untracked());
        spawn_local(async move {
            match api::get_switch_page(&q, off, PAGE_SIZE).await {
                Ok(result) => page.set(result),
                Err(e) => ctx.fail(e),
            }
        });
    };

    Effect::new(move |_| {
        ctx.revision.track();
        query.track();
        offset.track();
        reload();
    });

    let toggle = move |id: i64, on: bool| {
        spawn_local(async move {
            match api::set_switch(id, on).await {
                Ok(_) => {
                    page.update(|p| {
                        if let Some(row) = p.rows.iter_mut().find(|r| r.id == id) {
                            row.on = on;
                        }
                    });
                    ctx.summary.update(|s| {
                        if let Some(s) = s {
                            s.dirty = true;
                        }
                    });
                }
                Err(e) => ctx.fail(e),
            }
        });
    };

    let set_all = move |on: bool| {
        let ids: Vec<i64> = page.get_untracked().rows.iter().map(|r| r.id).collect();
        spawn_local(async move {
            match api::set_switches(ids, on).await {
                Ok(applied) => {
                    if let Some(message) = applied.message {
                        ctx.done(message);
                    }
                    ctx.accept(api::summary().await);
                    reload();
                }
                Err(e) => ctx.fail(e),
            }
        });
    };

    view! {
        <section class="card full">
            <div class="toolbar">
                <h2>"Switches"</h2>
                <input
                    class="search"
                    type="search"
                    placeholder="Search by name or id"
                    prop:value=move || query.get()
                    on:input=move |ev| {
                        offset.set(0);
                        query.set(event_target_value(&ev));
                    }
                />
                <span class="grow"></span>
                <button class="btn tiny" on:click=move |_| set_all(true)>"All shown on"</button>
                <button class="btn tiny" on:click=move |_| set_all(false)>"All shown off"</button>
            </div>

            {move || {
                let p = page.get();
                if p.total == 0 {
                    return view! {
                        <p class="empty">"This save has no switch list."</p>
                    }.into_any();
                }
                if p.rows.is_empty() {
                    return view! { <p class="empty">"Nothing matches that search."</p> }.into_any();
                }
                view! {
                    <table class="grid rows">
                        <tbody>
                            {p.rows.iter().map(|row| {
                                let id = row.id;
                                let on = row.on;
                                let name = row.name.clone();
                                view! {
                                    <tr class:is-on=move || on>
                                        <td class="col-id muted">{format!("#{id}")}</td>
                                        <td>
                                            {name.unwrap_or_else(|| "(unnamed)".to_owned())}
                                        </td>
                                        <td class="col-toggle">
                                            <label class="switch">
                                                <input
                                                    type="checkbox"
                                                    prop:checked=on
                                                    on:change=move |ev| toggle(id, event_target_checked(&ev))
                                                />
                                                <span class="slider"></span>
                                            </label>
                                        </td>
                                    </tr>
                                }
                            }).collect_view()}
                        </tbody>
                    </table>
                }.into_any()
            }}

            <Pager
                offset=offset
                matched=Signal::derive(move || page.get().matched)
                total=Signal::derive(move || page.get().total)
            />
        </section>
    }
}

#[component]
pub fn VariablesView() -> impl IntoView {
    let ctx = ctx();
    let page = RwSignal::new(VariablePage::default());
    let query = RwSignal::new(String::new());
    let offset = RwSignal::new(0usize);

    let reload = move || {
        let (q, off) = (query.get_untracked(), offset.get_untracked());
        spawn_local(async move {
            match api::get_variable_page(&q, off, PAGE_SIZE).await {
                Ok(result) => page.set(result),
                Err(e) => ctx.fail(e),
            }
        });
    };

    Effect::new(move |_| {
        ctx.revision.track();
        query.track();
        offset.track();
        reload();
    });

    let commit = move |id: i64, value: Scalar| {
        spawn_local(async move {
            match api::set_variable(id, &value).await {
                Ok(_) => {
                    page.update(|p| {
                        if let Some(row) = p.rows.iter_mut().find(|r| r.id == id) {
                            row.value = value.clone();
                            row.complex = None;
                        }
                    });
                    ctx.summary.update(|s| {
                        if let Some(s) = s {
                            s.dirty = true;
                        }
                    });
                }
                Err(e) => ctx.fail(e),
            }
        });
    };

    view! {
        <section class="card full">
            <div class="toolbar">
                <h2>"Variables"</h2>
                <input
                    class="search"
                    type="search"
                    placeholder="Search by name or id"
                    prop:value=move || query.get()
                    on:input=move |ev| {
                        offset.set(0);
                        query.set(event_target_value(&ev));
                    }
                />
            </div>

            {move || {
                let p = page.get();
                if p.total == 0 {
                    return view! {
                        <p class="empty">"This save has no variable list."</p>
                    }.into_any();
                }
                if p.rows.is_empty() {
                    return view! { <p class="empty">"Nothing matches that search."</p> }.into_any();
                }
                view! {
                    <table class="grid rows">
                        <tbody>
                            {p.rows.iter().map(|row| {
                                let id = row.id;
                                let name = row.name.clone();
                                let complex = row.complex.clone();
                                let value = row.value.clone();
                                let type_name = value.type_name();
                                view! {
                                    <tr>
                                        <td class="col-id muted">{format!("#{id}")}</td>
                                        <td>{name.unwrap_or_else(|| "(unnamed)".to_owned())}</td>
                                        <td class="col-value">
                                            {match complex {
                                                Some(description) => view! {
                                                    <span class="muted" title="Editing this kind of value is only possible on the Raw data tab">
                                                        {description}
                                                    </span>
                                                }.into_any(),
                                                None => scalar_field(value.clone(), move |v| commit(id, v)).into_any(),
                                            }}
                                        </td>
                                        <td class="col-type">
                                            <select on:change=move |ev| {
                                                let next = match event_target_value(&ev).as_str() {
                                                    "bool" => Scalar::Bool(false),
                                                    "string" => Scalar::Str(String::new()),
                                                    "float" => Scalar::Float(0.0),
                                                    "nil" => Scalar::Nil,
                                                    _ => Scalar::Int(0),
                                                };
                                                commit(id, next);
                                            }>
                                                {["int", "float", "string", "bool", "nil"].map(|t| view! {
                                                    <option value=t selected=t == type_name>{t}</option>
                                                }).collect_view()}
                                            </select>
                                        </td>
                                    </tr>
                                }
                            }).collect_view()}
                        </tbody>
                    </table>
                }.into_any()
            }}

            <Pager
                offset=offset
                matched=Signal::derive(move || page.get().matched)
                total=Signal::derive(move || page.get().total)
            />
        </section>
    }
}

#[component]
fn Pager(offset: RwSignal<usize>, matched: Signal<usize>, total: Signal<usize>) -> impl IntoView {
    view! {
        <div class="pager">
            <button
                class="btn tiny"
                disabled=move || offset.get() == 0
                on:click=move |_| offset.update(|o| *o = o.saturating_sub(PAGE_SIZE))
            >
                "Previous"
            </button>
            <span class="muted small">
                {move || {
                    let shown_from = offset.get() + 1;
                    let shown_to = (offset.get() + PAGE_SIZE).min(matched.get());
                    format!(
                        "{shown_from}–{shown_to} of {} matching · {} in this save",
                        matched.get(),
                        total.get(),
                    )
                }}
            </span>
            <button
                class="btn tiny"
                disabled=move || offset.get() + PAGE_SIZE >= matched.get()
                on:click=move |_| offset.update(|o| *o += PAGE_SIZE)
            >
                "Next"
            </button>
        </div>
    }
}

#[component]
pub fn SelfSwitchesView() -> impl IntoView {
    let ctx = ctx();
    let rows = RwSignal::new(Vec::<SelfSwitchRow>::new());
    let query = RwSignal::new(String::new());

    let new_map = RwSignal::new(0i64);
    let new_event = RwSignal::new(0i64);
    let new_letter = RwSignal::new("A".to_string());

    let reload = move || {
        spawn_local(async move {
            match api::get_self_switches().await {
                Ok(result) => rows.set(result),
                Err(e) => ctx.fail(e),
            }
        });
    };

    Effect::new(move |_| {
        ctx.revision.track();
        reload();
    });

    let accept = move |result: Result<Vec<SelfSwitchRow>, String>| match result {
        Ok(result) => {
            rows.set(result);
            spawn_local(async move { ctx.accept(api::summary().await) });
        }
        Err(e) => ctx.fail(e),
    };

    let add = move |_| {
        let (map, event, letter) = (new_map.get(), new_event.get(), new_letter.get());
        if map <= 0 || event <= 0 {
            ctx.fail("A self switch needs a map id and an event id.");
            return;
        }
        spawn_local(async move { accept(api::set_self_switch(map, event, &letter, true).await) });
    };

    view! {
        <section class="card full">
            <div class="toolbar">
                <h2>"Self switches"</h2>
                <input
                    class="search"
                    type="search"
                    placeholder="Search by map or event"
                    prop:value=move || query.get()
                    on:input=move |ev| query.set(event_target_value(&ev))
                />
            </div>
            <p class="hint">
                "Self switches are the per-event flags an event sets on itself — usually what \
                 marks a chest as opened or a conversation as finished. Only events that have \
                 set one appear here."
            </p>

            {move || {
                let filter = query.get().to_lowercase();
                let visible: Vec<SelfSwitchRow> = rows
                    .get()
                    .into_iter()
                    .filter(|r| {
                        filter.is_empty()
                            || r.map_id.to_string().contains(&filter)
                            || r.event_id.to_string().contains(&filter)
                            || r.map_name.as_deref().is_some_and(|n| n.to_lowercase().contains(&filter))
                    })
                    .collect();

                if visible.is_empty() {
                    return view! {
                        <p class="empty">"No self switches have been set in this save."</p>
                    }.into_any();
                }
                view! {
                    <table class="grid rows">
                        <thead>
                            <tr>
                                <th>"Map"</th><th>"Event"</th><th>"Letter"</th>
                                <th>"Value"</th><th></th>
                            </tr>
                        </thead>
                        <tbody>
                            {visible.iter().map(|row| {
                                let (map, event, letter) = (row.map_id, row.event_id, row.letter.clone());
                                let letter_for_remove = letter.clone();
                                let on = row.on;
                                let map_name = row.map_name.clone();
                                view! {
                                    <tr>
                                        <td>
                                            <span class="muted">{format!("#{map}")}</span>
                                            {map_name.map(|n| view! { <span class="strong block">{n}</span> })}
                                        </td>
                                        <td class="muted">{format!("#{event}")}</td>
                                        <td><span class="pill">{letter.clone()}</span></td>
                                        <td class="col-toggle">
                                            <label class="switch">
                                                <input
                                                    type="checkbox"
                                                    prop:checked=on
                                                    on:change=move |ev| {
                                                        let letter = letter.clone();
                                                        let value = event_target_checked(&ev);
                                                        spawn_local(async move {
                                                            accept(api::set_self_switch(map, event, &letter, value).await)
                                                        });
                                                    }
                                                />
                                                <span class="slider"></span>
                                            </label>
                                        </td>
                                        <td>
                                            <button class="icon danger" title="Delete this entry"
                                                on:click=move |_| {
                                                    let letter = letter_for_remove.clone();
                                                    spawn_local(async move {
                                                        accept(api::remove_self_switch(map, event, &letter).await)
                                                    });
                                                }
                                            >"✕"</button>
                                        </td>
                                    </tr>
                                }
                            }).collect_view()}
                        </tbody>
                    </table>
                }.into_any()
            }}

            <div class="add-row">
                <span class="row-label">"Add"</span>
                <span class="input-group">
                    <span class="unit">"map"</span>
                    {number_field(0, "num tiny", move |v| new_map.set(v))}
                    <span class="unit">"event"</span>
                    {number_field(0, "num tiny", move |v| new_event.set(v))}
                    <span class="unit">"letter"</span>
                    {text_field("A".to_owned(), "text tiny", move |v| new_letter.set(v))}
                    <button class="btn tiny" on:click=add>"Add as on"</button>
                </span>
            </div>
        </section>
    }
}
