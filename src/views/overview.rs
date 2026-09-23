//! The first tab: the file itself, the party's shared state, and who is in it.

use leptos::prelude::*;
use leptos::task::spawn_local;
use rpgsave_protocol::Summary;

use crate::api;
use crate::state::{ctx, format_bytes, format_duration, group_digits, parse_int};
use crate::views::{field_row, number_field};

#[component]
pub fn OverviewView() -> impl IntoView {
    let ctx = ctx();
    view! {
        {move || ctx.summary.get().map(|s| view! {
            <div class="columns">
                <div class="column">
                    <PartyCard summary=s.clone()/>
                    <LocationCard summary=s.clone()/>
                </div>
                <div class="column">
                    <MembersCard summary=s.clone()/>
                    <FileCard summary=s.clone()/>
                </div>
            </div>
        })}
    }
}

#[component]
fn PartyCard(summary: Summary) -> impl IntoView {
    let ctx = ctx();
    let currency = summary.currency.clone();
    let gold = summary.gold;
    let steps = summary.steps;
    let playtime = summary.playtime_seconds;
    let has_playtime = summary.has_playtime;

    let set_gold = move |value: i64| {
        spawn_local(async move { ctx.accept(api::set_gold(value).await) });
    };
    let set_steps = move |value: i64| {
        spawn_local(async move { ctx.accept(api::set_steps(value).await) });
    };

    view! {
        <section class="card">
            <h2>"Party"</h2>
            {match gold {
                Some(gold) => view! {
                    <>
                        {field_row("Gold", view! {
                            <span class="input-group">
                                {number_field(gold, "num wide", set_gold)}
                                <span class="unit">{currency.clone()}</span>
                            </span>
                        })}
                        <p class="hint">
                            "Both engines cap gold at "{group_digits(99_999_999)}"."
                        </p>
                    </>
                }.into_any(),
                None => view! {
                    <p class="hint">"This save has no gold field."</p>
                }.into_any(),
            }}

            {steps.map(|steps| field_row("Steps", number_field(steps, "num", set_steps)))}

            <Show
                when=move || has_playtime
                fallback=|| view! {
                    <p class="hint">
                        "Play time is not stored anywhere this editor recognises in this save, so \
                         it is left alone."
                    </p>
                }
            >
                <PlaytimeRow seconds=playtime/>
            </Show>
        </section>
    }
}

#[component]
fn PlaytimeRow(seconds: i64) -> impl IntoView {
    let ctx = ctx();
    let hours = seconds / 3600;
    let minutes = seconds / 60 % 60;
    let secs = seconds % 60;

    // Each part commits the whole play time, so the others have to be read
    // back from the value this row was rendered with.
    let commit = move |h: i64, m: i64, s: i64| {
        let total = h.max(0) * 3600 + m.clamp(0, 59) * 60 + s.clamp(0, 59);
        spawn_local(async move { ctx.accept(api::set_playtime(total).await) });
    };

    view! {
        {field_row("Play time", view! {
            <span class="input-group">
                {number_field(hours, "num tiny", move |h| commit(h, minutes, secs))}
                <span class="unit">"h"</span>
                {number_field(minutes, "num tiny", move |m| commit(hours, m, secs))}
                <span class="unit">"m"</span>
                {number_field(secs, "num tiny", move |s| commit(hours, minutes, s))}
                <span class="unit">"s"</span>
            </span>
        })}
        <p class="hint">{format!("Stored as {} frames at 60 fps.", seconds * 60)}</p>
    }
}

#[component]
fn LocationCard(summary: Summary) -> impl IntoView {
    let ctx = ctx();
    let Some(map_id) = summary.map_id else {
        return view! { <section class="card"><h2>"Location"</h2>
            <p class="hint">"This save has no map data."</p></section> }.into_any();
    };
    let x = summary.player_x.unwrap_or(0);
    let y = summary.player_y.unwrap_or(0);
    let has_player = summary.player_x.is_some();
    let map_name = summary.map_name.clone();

    let move_to = move |nx: i64, ny: i64| {
        spawn_local(async move { ctx.accept(api::set_position(nx, ny).await) });
    };

    view! {
        <section class="card">
            <h2>"Location"</h2>
            <div class="row">
                <span class="row-label">"Map"</span>
                <span class="row-control">
                    <span class="pill">{format!("#{map_id}")}</span>
                    {map_name.clone().map(|n| view! { <span class="strong">{n}</span> })}
                </span>
            </div>
            <Show when=move || has_player>
                {field_row("Position", view! {
                    <span class="input-group">
                        <span class="unit">"x"</span>
                        {number_field(x, "num tiny", move |nx| move_to(nx, y))}
                        <span class="unit">"y"</span>
                        {number_field(y, "num tiny", move |ny| move_to(x, ny))}
                    </span>
                })}
                <p class="hint">
                    "The map itself is not changed here — moving the player far outside the \
                     current map will strand them."
                </p>
            </Show>
        </section>
    }.into_any()
}

#[component]
fn MembersCard(summary: Summary) -> impl IntoView {
    let ctx = ctx();
    let party = summary.party.clone();
    let roster = summary.roster.clone();
    let ids: Vec<i64> = party.iter().map(|m| m.actor_id).collect();

    let apply = move |ids: Vec<i64>| {
        spawn_local(async move { ctx.accept(api::set_party(ids).await) });
    };

    // Reading the party out of the shared summary when a button is pressed —
    // rather than capturing it — keeps these closures `Copy`, which is what
    // `Show` needs to be able to render its children more than once.
    let current = move || {
        ctx.summary.with(|s| {
            s.as_ref()
                .map(|s| s.party.iter().map(|m| m.actor_id).collect::<Vec<i64>>())
                .unwrap_or_default()
        })
    };

    let move_by = move |index: usize, delta: isize| {
        let mut next = current();
        let target = index as isize + delta;
        if target < 0 || target as usize >= next.len() {
            return;
        }
        next.swap(index, target as usize);
        apply(next);
    };
    let remove = move |index: usize| {
        let mut next = current();
        if index < next.len() {
            next.remove(index);
            apply(next);
        }
    };
    let add = move |actor_id: i64| {
        let mut next = current();
        if !next.contains(&actor_id) {
            next.push(actor_id);
            apply(next);
        }
    };

    let available: Vec<_> = roster
        .iter()
        .filter(|r| !ids.contains(&r.actor_id))
        .cloned()
        .collect();

    view! {
        <section class="card">
            <h2>"Party members"</h2>
            <ol class="member-list">
                {party
                    .iter()
                    .enumerate()
                    .map(|(index, member)| {
                        let id = member.actor_id;
                        view! {
                            <li class="member">
                                <button
                                    class="link"
                                    on:click=move |_| ctx.show_actor(id)
                                >
                                    <span class="member-name">{member.name.clone()}</span>
                                </button>
                                <span class="muted small">
                                    {format!("#{} · level {}", member.actor_id, member.level)}
                                    {member.class_name.clone().map(|c| format!(" · {c}"))}
                                </span>
                                <span class="member-actions">
                                    <button class="icon" title="Move up"
                                        on:click=move |_| move_by(index, -1)>"↑"</button>
                                    <button class="icon" title="Move down"
                                        on:click=move |_| move_by(index, 1)>"↓"</button>
                                    <button class="icon danger" title="Remove from party"
                                        on:click=move |_| remove(index)>"✕"</button>
                                </span>
                            </li>
                        }
                    })
                    .collect_view()}
            </ol>
            <Show when=move || party.is_empty()>
                <p class="hint">"The party is empty. The game will not start well like this."</p>
            </Show>

            <Show when={let has_more = !available.is_empty(); move || has_more}>
                <div class="add-row">
                    <span class="row-label">"Add"</span>
                    <select on:change=move |ev| {
                        if let Some(id) = parse_int(&event_target_value(&ev)) {
                            add(id);
                        }
                    }>
                        <option value="">"Choose an actor…"</option>
                        {available
                            .iter()
                            .map(|r| {
                                let label = if r.in_save {
                                    format!("#{} {}", r.actor_id, r.name)
                                } else {
                                    format!("#{} {} (not yet in this save)", r.actor_id, r.name)
                                };
                                view! { <option value=r.actor_id.to_string()>{label}</option> }
                            })
                            .collect_view()}
                    </select>
                </div>
            </Show>
        </section>
    }
}

#[component]
fn FileCard(summary: Summary) -> impl IntoView {
    let ctx = ctx();
    let s = summary.clone();

    let choose_data = move |_| {
        spawn_local(async move {
            match api::pick_data_dir().await {
                Ok(Some(dir)) => match api::set_data_dir(&dir).await {
                    Ok(summary) => {
                        ctx.summary.set(Some(summary));
                        ctx.done("Database loaded.");
                        ctx.revision.update(|r| *r += 1);
                    }
                    Err(e) => ctx.fail(e),
                },
                Ok(None) => {}
                Err(e) => ctx.fail(e),
            }
        });
    };

    view! {
        <section class="card">
            <h2>"File"</h2>
            <dl class="facts">
                <dt>"Engine"</dt><dd>{s.engine_label.clone()}</dd>
                <dt>"Size"</dt><dd>{format_bytes(s.file_size)}</dd>
                <dt>"Documents"</dt><dd>{s.document_count.to_string()}</dd>
                {s.save_count.map(|c| view! { <dt>"Times saved"</dt><dd>{c.to_string()}</dd> })}
                <dt>"Play time"</dt><dd>{format_duration(s.playtime_seconds)}</dd>
                <dt>"Switches"</dt><dd>{s.switch_count.to_string()}</dd>
                <dt>"Variables"</dt><dd>{s.variable_count.to_string()}</dd>
                <dt>"Self switches"</dt><dd>{s.self_switch_count.to_string()}</dd>
            </dl>

            {s.notes.iter().map(|note| view! {
                <p class="banner warn">{note.clone()}</p>
            }).collect_view()}

            <h3>"Game database"</h3>
            {match s.data_dir.clone() {
                Some(dir) => view! {
                    <>
                        <p class="path small">{dir}</p>
                        <p class="muted small">
                            {format!("Loaded: {}", s.data_loaded.join(", "))}
                        </p>
                        {(!s.data_missing.is_empty()).then(|| view! {
                            <p class="muted small">
                                {format!("Not found: {}", s.data_missing.join(", "))}
                            </p>
                        })}
                    </>
                }.into_any(),
                None => view! {
                    <>
                        <p class="banner">
                            "No database was found next to this save, so items, skills and \
                             switches are shown by id only. If the game's files are not packed \
                             into an archive, point the editor at its "<code>"Data"</code>" folder."
                        </p>
                    </>
                }.into_any(),
            }}
            <button class="btn" on:click=choose_data>"Choose Data folder…"</button>
        </section>
    }
}
