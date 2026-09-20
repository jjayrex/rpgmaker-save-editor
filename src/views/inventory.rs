//! What the party is carrying.

use leptos::prelude::*;
use leptos::task::spawn_local;
use rpgsave_protocol::{CatalogEntry, InventoryView};

use crate::api;
use crate::state::ctx;
use crate::views::number_field;

const KINDS: [(&str, &str); 3] = [("item", "Items"), ("weapon", "Weapons"), ("armor", "Armors")];

#[component]
pub fn InventoryTab() -> impl IntoView {
    let ctx = ctx();
    let kind = RwSignal::new("item".to_string());
    let inventory = RwSignal::new(InventoryView::default());
    let catalog = RwSignal::new(Vec::<CatalogEntry>::new());
    let query = RwSignal::new(String::new());

    let reload = move || {
        let current = kind.get();
        let search = query.get();
        spawn_local(async move {
            match api::get_inventory(&current).await {
                Ok(view) => inventory.set(view),
                Err(e) => ctx.fail(e),
            }
            match api::get_catalog(&current, &search).await {
                Ok(rows) => catalog.set(rows),
                Err(e) => ctx.fail(e),
            }
        });
    };

    Effect::new(move |_| {
        ctx.revision.track();
        kind.track();
        query.track();
        reload();
    });

    let set_count = move |id: i64, count: i64| {
        let current = kind.get();
        spawn_local(async move {
            match api::set_item_count(&current, id, count).await {
                Ok(view) => {
                    inventory.set(view);
                    // Held counts are shown in the picker too.
                    if let Ok(rows) = api::get_catalog(&current, &query.get_untracked()).await {
                        catalog.set(rows);
                    }
                    ctx.accept(api::summary().await);
                }
                Err(e) => ctx.fail(e),
            }
        });
    };

    view! {
        <div class="split wide-left">
            <div class="detail">
                <div class="subtabs">
                    {KINDS.map(|(id, label)| view! {
                        <button
                            class="subtab"
                            class:active=move || kind.get() == id
                            on:click=move |_| kind.set(id.to_string())
                        >
                            {label}
                        </button>
                    }).collect_view()}
                </div>

                <section class="card">
                    <h2>"Carried"</h2>
                    {move || {
                        let view_data = inventory.get();
                        if view_data.rows.is_empty() {
                            return view! {
                                <p class="empty">"Nothing of this kind is being carried."</p>
                            }.into_any();
                        }
                        let max = view_data.max_count;
                        view! {
                            <table class="grid">
                                <thead>
                                    <tr>
                                        <th class="col-id">"Id"</th>
                                        <th>"Name"</th>
                                        <th class="col-count">"Count"</th>
                                        <th></th>
                                    </tr>
                                </thead>
                                <tbody>
                                    {view_data.rows.iter().map(|row| {
                                        let id = row.id;
                                        let count = row.count;
                                        let name = row.name.clone();
                                        let description = row.description.clone();
                                        view! {
                                            <tr>
                                                <td class="muted">{format!("#{id}")}</td>
                                                <td>
                                                    <span class="strong">
                                                        {name.unwrap_or_else(|| format!("Unknown ({id})"))}
                                                    </span>
                                                    <Show when={
                                                        let has = !description.is_empty();
                                                        move || has
                                                    }>
                                                        <span class="muted small block">
                                                            {description.clone()}
                                                        </span>
                                                    </Show>
                                                </td>
                                                <td>
                                                    {number_field(count, "num tiny", move |v| {
                                                        set_count(id, v.clamp(0, max))
                                                    })}
                                                </td>
                                                <td>
                                                    <button class="icon danger" title="Remove"
                                                        on:click=move |_| set_count(id, 0)>"✕"</button>
                                                </td>
                                            </tr>
                                        }
                                    }).collect_view()}
                                </tbody>
                            </table>
                        }.into_any()
                    }}
                    <p class="hint">
                        {move || format!(
                            "The engine stops at {} of any one entry; setting a count to zero \
                             removes it from the bag entirely.",
                            inventory.get().max_count,
                        )}
                    </p>
                </section>
            </div>

            <aside class="sidebar right">
                <h2>"Add from the database"</h2>
                <input
                    class="search"
                    type="search"
                    placeholder="Search by name or id"
                    prop:value=move || query.get()
                    on:input=move |ev| query.set(event_target_value(&ev))
                />
                {move || {
                    let rows = catalog.get();
                    if rows.is_empty() {
                        return view! {
                            <p class="hint">
                                "No database is loaded, so there is nothing to pick from. You can \
                                 still change the count of anything already carried, and the \
                                 Overview tab can point the editor at the game's Data folder."
                            </p>
                        }.into_any();
                    }
                    view! {
                        <ul class="catalog">
                            {rows.iter().map(|entry| {
                                let id = entry.id;
                                let held = entry.held;
                                view! {
                                    <li class="catalog-row">
                                        <span class="catalog-name">
                                            <span class="strong">{entry.name.clone()}</span>
                                            <span class="muted small">{format!("#{id}")}</span>
                                        </span>
                                        <span class="catalog-actions">
                                            <Show when=move || held != 0>
                                                <span class="tag">{format!("×{held}")}</span>
                                            </Show>
                                            <button class="btn tiny"
                                                on:click=move |_| set_count(id, held + 1)>"+1"</button>
                                            <button class="btn tiny"
                                                on:click=move |_| set_count(id, 99)>"99"</button>
                                        </span>
                                    </li>
                                }
                            }).collect_view()}
                        </ul>
                    }.into_any()
                }}
            </aside>
        </div>
    }
}
