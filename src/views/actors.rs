//! Per-actor editing: identity, level, vitals, stats, equipment and skills.

use leptos::prelude::*;
use leptos::task::spawn_local;
use rpgsave_protocol::{ActorView as Actor, CatalogEntry, NamedId, Scalar};

use crate::api;
use crate::state::{ctx, group_digits, parse_int};
use crate::views::{field_row, number_field, scalar_field, text_field};

/// The database rows the pickers on this tab need.
#[derive(Clone, Default, PartialEq)]
struct Catalogs {
    weapons: Vec<CatalogEntry>,
    armors: Vec<CatalogEntry>,
    skills: Vec<CatalogEntry>,
    states: Vec<CatalogEntry>,
}

#[component]
pub fn ActorsView() -> impl IntoView {
    let ctx = ctx();
    let actor = RwSignal::new(None::<Actor>);
    let catalogs = RwSignal::new(Catalogs::default());

    // Whichever actor is selected, falling back to the first party member.
    let selected = Memo::new(move |_| {
        ctx.actor.get().or_else(|| {
            ctx.summary.with(|s| {
                let s = s.as_ref()?;
                s.party
                    .first()
                    .map(|m| m.actor_id)
                    .or_else(|| s.roster.iter().find(|r| r.in_save).map(|r| r.actor_id))
            })
        })
    });

    Effect::new(move |_| {
        ctx.revision.track();
        spawn_local(async move {
            let mut next = Catalogs::default();
            for (kind, slot) in [
                ("weapon", 0),
                ("armor", 1),
                ("skill", 2),
                ("state", 3),
            ] {
                if let Ok(rows) = api::get_catalog(kind, "").await {
                    match slot {
                        0 => next.weapons = rows,
                        1 => next.armors = rows,
                        2 => next.skills = rows,
                        _ => next.states = rows,
                    }
                }
            }
            catalogs.set(next);
        });
    });

    Effect::new(move |_| {
        ctx.revision.track();
        let Some(id) = selected.get() else {
            actor.set(None);
            return;
        };
        spawn_local(async move {
            match api::get_actor(id).await {
                Ok(view) => actor.set(Some(view)),
                Err(_) => actor.set(None),
            }
        });
    });

    view! {
        <div class="split">
            <aside class="sidebar">
                <h2>"Actors"</h2>
                <ul class="actor-list">
                    {move || ctx.summary.with(|summary| {
                        summary.as_ref().map(|s| {
                            s.roster.iter().map(|entry| {
                                let id = entry.actor_id;
                                let name = entry.name.clone();
                                let in_save = entry.in_save;
                                let in_party = entry.in_party;
                                view! {
                                    <li>
                                        <button
                                            class="actor-item"
                                            class:active=move || selected.get() == Some(id)
                                            class:absent=move || !in_save
                                            on:click=move |_| ctx.actor.set(Some(id))
                                        >
                                            <span class="actor-id">{format!("#{id}")}</span>
                                            <span class="actor-name">{name}</span>
                                            <Show when=move || in_party>
                                                <span class="tag">"party"</span>
                                            </Show>
                                        </button>
                                    </li>
                                }
                            }).collect_view()
                        })
                    })}
                </ul>
                <p class="hint">
                    "Greyed-out actors are defined by the game but have not been created in this \
                     save yet. Adding one to the party makes the game build it on load."
                </p>
            </aside>

            <div class="detail">
                {move || match actor.get() {
                    Some(a) => view! { <ActorDetail actor=a catalogs=catalogs.get()/> }.into_any(),
                    None => view! {
                        <p class="empty">
                            "Select an actor. Actors that the save has never instantiated cannot \
                             be edited until the game creates them."
                        </p>
                    }
                    .into_any(),
                }}
            </div>
        </div>
    }
}

#[component]
fn ActorDetail(actor: Actor, catalogs: Catalogs) -> impl IntoView {
    let ctx = ctx();
    let update = move |result: Result<Actor, String>| match result {
        Ok(_) => {
            // The summary shows levels and names too, so refresh it as well.
            spawn_local(async move { ctx.accept(api::summary().await) });
        }
        Err(e) => ctx.fail(e),
    };

    view! {
        <div class="detail-head">
            <h2>{actor.name.clone()}</h2>
            <span class="muted">
                {format!("#{}", actor.actor_id)}
                {actor.class_name.clone().map(|c| format!(" · {c}"))}
            </span>
            <Show when={let p = actor.in_party; move || p}>
                <span class="tag">"in party"</span>
            </Show>
        </div>

        <div class="columns">
            <div class="column">
                <IdentityCard actor=actor.clone() on_change=update/>
                <ProgressCard actor=actor.clone() on_change=update/>
                <VitalsCard actor=actor.clone() on_change=update/>
            </div>
            <div class="column">
                <EquipmentCard actor=actor.clone() catalogs=catalogs.clone() on_change=update/>
                <IdListCard actor=actor.clone() catalogs=catalogs.clone() skills=true on_change=update/>
                <IdListCard actor=actor.clone() catalogs=catalogs.clone() skills=false on_change=update/>
                <ParamsCard actor=actor.clone() on_change=update/>
                <ExtraCard actor=actor.clone() on_change=update/>
            </div>
        </div>
    }
}

#[component]
fn IdentityCard(actor: Actor, on_change: impl Fn(Result<Actor, String>) + Copy + 'static) -> impl IntoView {
    let id = actor.actor_id;
    let nickname = actor.nickname.clone();
    let classes = actor.classes.clone();
    let class_id = actor.class_id;

    view! {
        <section class="card">
            <h2>"Identity"</h2>
            {field_row("Name", text_field(actor.name.clone(), "text", move |name| {
                spawn_local(async move { on_change(api::set_actor_text(id, "@name", &name).await) });
            }))}
            {nickname.map(|nick| field_row("Nickname", text_field(nick, "text", move |n| {
                spawn_local(async move { on_change(api::set_actor_text(id, "@nickname", &n).await) });
            })))}
            {class_id.map(|current| {
                if classes.is_empty() {
                    field_row("Class", number_field(current, "num", move |c| {
                        spawn_local(async move {
                            on_change(api::set_actor_int(id, "@class_id", c).await)
                        });
                    })).into_any()
                } else {
                    field_row("Class", view! {
                        <select on:change=move |ev| {
                            if let Some(c) = parse_int(&event_target_value(&ev)) {
                                spawn_local(async move {
                                    on_change(api::set_actor_int(id, "@class_id", c).await)
                                });
                            }
                        }>
                            {classes.iter().map(|c| view! {
                                <option value=c.id.to_string() selected=c.id == current>
                                    {format!("#{} {}", c.id, c.name)}
                                </option>
                            }).collect_view()}
                        </select>
                    }).into_any()
                }
            })}
            <p class="hint">
                "Changing class does not relearn skills or re-equip anything; the game does that \
                 only when the class is changed in-game."
            </p>
        </section>
    }
}

#[component]
fn ProgressCard(actor: Actor, on_change: impl Fn(Result<Actor, String>) + Copy + 'static) -> impl IntoView {
    let id = actor.actor_id;
    let Some(level) = actor.level else {
        return view! { <section class="card"><h2>"Progress"</h2>
            <p class="hint">"This actor has no level."</p></section> }.into_any();
    };
    let max_level = actor.max_level;
    let follows = actor.exp_follows_level;
    let next = actor.exp_next_level;
    let this = actor.exp_this_level;

    view! {
        <section class="card">
            <h2>"Progress"</h2>
            {field_row("Level", view! {
                <span class="input-group">
                    {number_field(level, "num tiny", move |lv| {
                        spawn_local(async move { on_change(api::set_actor_level(id, lv).await) });
                    })}
                    <span class="unit">{format!("of {max_level}")}</span>
                </span>
            })}
            {actor.exp.map(|exp| field_row("Experience", number_field(exp, "num wide", move |e| {
                spawn_local(async move { on_change(api::set_actor_exp(id, e).await) });
            })))}
            {match (follows, this, next) {
                (true, Some(this), Some(next)) => view! {
                    <p class="hint">
                        {format!(
                            "Level {level} starts at {} experience; level {} starts at {}.",
                            group_digits(this),
                            level + 1,
                            group_digits(next),
                        )}
                        " Setting the level here rewrites experience to match."
                    </p>
                }.into_any(),
                _ => view! {
                    <p class="hint">
                        "The experience curve is unknown without the game's database, so the \
                         level is changed on its own. The game may correct it after the next \
                         battle."
                    </p>
                }.into_any(),
            }}
        </section>
    }.into_any()
}

#[component]
fn VitalsCard(actor: Actor, on_change: impl Fn(Result<Actor, String>) + Copy + 'static) -> impl IntoView {
    let id = actor.actor_id;
    let max_hp = actor.max_hp;
    let max_mp = actor.max_mp;

    view! {
        <section class="card">
            <h2>"Vitals"</h2>
            {actor.hp.map(|hp| field_row("HP", view! {
                <span class="input-group">
                    {number_field(hp, "num", move |v| {
                        spawn_local(async move { on_change(api::set_actor_int(id, "@hp", v).await) });
                    })}
                    {max_hp.map(|m| view! { <span class="unit">{format!("/ {m}")}</span> })}
                </span>
            }))}
            {actor.mp.map(|mp| field_row("MP", view! {
                <span class="input-group">
                    {number_field(mp, "num", move |v| {
                        spawn_local(async move { on_change(api::set_actor_int(id, "@mp", v).await) });
                    })}
                    {max_mp.map(|m| view! { <span class="unit">{format!("/ {m}")}</span> })}
                </span>
            }))}
            {actor.tp.map(|tp| field_row("TP", number_field(tp as i64, "num tiny", move |v| {
                spawn_local(async move {
                    on_change(api::set_actor_scalar(id, "@tp", &Scalar::Float(v as f64)).await)
                });
            })))}
            <div class="button-row">
                <button class="btn" on:click=move |_| {
                    spawn_local(async move { on_change(api::full_heal(id).await) });
                }>"Full heal"</button>
            </div>
            <Show when=move || max_hp.is_none()>
                <p class="hint">
                    "Maximum HP and MP are computed by the game from the class and equipment, so \
                     they are not stored in the save. With the database loaded the editor can \
                     show them."
                </p>
            </Show>
        </section>
    }
}

#[component]
fn ParamsCard(actor: Actor, on_change: impl Fn(Result<Actor, String>) + Copy + 'static) -> impl IntoView {
    let id = actor.actor_id;
    let params = actor.params.clone();
    if params.is_empty() {
        return ().into_any();
    }
    view! {
        <section class="card">
            <h2>"Stat bonuses"</h2>
            <table class="grid">
                <thead>
                    <tr><th>"Parameter"</th><th>"Base"</th><th>"Bonus"</th><th>"Total"</th></tr>
                </thead>
                <tbody>
                    {params.iter().map(|p| {
                        let index = p.index;
                        let base = p.base;
                        let plus = p.plus;
                        view! {
                            <tr>
                                <td>{p.label.clone()}</td>
                                <td class="muted">{base.map(|b| b.to_string()).unwrap_or_else(|| "—".to_owned())}</td>
                                <td>
                                    {number_field(plus, "num tiny", move |v| {
                                        spawn_local(async move {
                                            on_change(api::set_actor_param(id, index, v).await)
                                        });
                                    })}
                                </td>
                                <td class="strong">
                                    {base.map(|b| (b + plus).to_string()).unwrap_or_else(|| "—".to_owned())}
                                </td>
                            </tr>
                        }
                    }).collect_view()}
                </tbody>
            </table>
            <p class="hint">
                "These are the permanent bonuses stored on the actor, the ones stat-boosting \
                 items grant. Equipment is added on top by the game."
            </p>
        </section>
    }.into_any()
}

#[component]
fn EquipmentCard(
    actor: Actor,
    catalogs: Catalogs,
    on_change: impl Fn(Result<Actor, String>) + Copy + 'static,
) -> impl IntoView {
    let id = actor.actor_id;
    let equips = actor.equips.clone();
    if equips.is_empty() {
        return ().into_any();
    }

    view! {
        <section class="card">
            <h2>"Equipment"</h2>
            {equips.iter().map(|slot| {
                let index = slot.slot;
                let kind = slot.kind.clone();
                let kind_for_change = kind.clone();
                let current = slot.item_id;
                let options = if kind == "weapon" { catalogs.weapons.clone() } else { catalogs.armors.clone() };
                let label = slot.label.clone();
                let item_name = slot.item_name.clone();

                let control = if options.is_empty() {
                    view! {
                        <span class="input-group">
                            {number_field(current, "num", move |item_id| {
                                let kind = kind_for_change.clone();
                                spawn_local(async move {
                                    on_change(api::set_actor_equip(id, index, &kind, item_id).await)
                                });
                            })}
                            {item_name.clone().map(|n| view! { <span class="unit">{n}</span> })}
                        </span>
                    }.into_any()
                } else {
                    view! {
                        <select on:change=move |ev| {
                            let kind = kind_for_change.clone();
                            if let Some(item_id) = parse_int(&event_target_value(&ev)) {
                                spawn_local(async move {
                                    on_change(api::set_actor_equip(id, index, &kind, item_id).await)
                                });
                            }
                        }>
                            <option value="0" selected=current == 0>"— empty —"</option>
                            {options.iter().map(|e| view! {
                                <option value=e.id.to_string() selected=e.id == current>
                                    {format!("#{} {}", e.id, e.name)}
                                </option>
                            }).collect_view()}
                        </select>
                    }.into_any()
                };

                view! {
                    <label class="row">
                        <span class="row-label">{label}</span>
                        <span class="row-control">{control}</span>
                    </label>
                }
            }).collect_view()}
            <p class="hint">
                "Equipment here is written straight into the slot; the game will not check that \
                 the actor is allowed to use it."
            </p>
        </section>
    }.into_any()
}

/// Skills and states are both plain id lists with a picker.
#[component]
fn IdListCard(
    actor: Actor,
    catalogs: Catalogs,
    skills: bool,
    on_change: impl Fn(Result<Actor, String>) + Copy + 'static,
) -> impl IntoView {
    let id = actor.actor_id;
    let entries: Vec<NamedId> = if skills { actor.skills.clone() } else { actor.states.clone() };
    let options = if skills { catalogs.skills.clone() } else { catalogs.states.clone() };
    let title = if skills { "Skills" } else { "States" };
    let ids: Vec<i64> = entries.iter().map(|e| e.id).collect();

    let apply = move |next: Vec<i64>| {
        spawn_local(async move {
            let result = if skills {
                api::set_actor_skills(id, next).await
            } else {
                api::set_actor_states(id, next).await
            };
            on_change(result);
        });
    };

    let remove = {
        let ids = ids.clone();
        move |remove_id: i64| {
            let next: Vec<i64> = ids.iter().copied().filter(|i| *i != remove_id).collect();
            apply(next);
        }
    };
    let add = {
        let ids = ids.clone();
        move |add_id: i64| {
            if ids.contains(&add_id) {
                return;
            }
            let mut next = ids.clone();
            next.push(add_id);
            apply(next);
        }
    };

    view! {
        <section class="card">
            <h2>{title}</h2>
            <ul class="chips">
                {entries.iter().map(|entry| {
                    let entry_id = entry.id;
                    let remove = remove.clone();
                    view! {
                        <li class="chip">
                            <span>{entry.name.clone()}</span>
                            <span class="muted small">{format!("#{entry_id}")}</span>
                            <button class="icon danger" title="Remove"
                                on:click=move |_| remove(entry_id)>"✕"</button>
                        </li>
                    }
                }).collect_view()}
            </ul>
            <Show when={let empty = entries.is_empty(); move || empty}>
                <p class="muted small">"None."</p>
            </Show>
            <div class="add-row">
                {if options.is_empty() {
                    view! {
                        <span class="input-group">
                            {number_field(0, "num tiny", move |v| if v > 0 { add(v) })}
                            <span class="unit">"id to add"</span>
                        </span>
                    }.into_any()
                } else {
                    view! {
                        <select on:change=move |ev| {
                            if let Some(v) = parse_int(&event_target_value(&ev))
                                && v > 0
                            {
                                add(v);
                            }
                        }>
                            <option value="0">"Add…"</option>
                            {options.iter().map(|e| view! {
                                <option value=e.id.to_string()>{format!("#{} {}", e.id, e.name)}</option>
                            }).collect_view()}
                        </select>
                    }.into_any()
                }}
            </div>
        </section>
    }
}

#[component]
fn ExtraCard(actor: Actor, on_change: impl Fn(Result<Actor, String>) + Copy + 'static) -> impl IntoView {
    let id = actor.actor_id;
    let extra = actor.extra.clone();
    if extra.is_empty() {
        return ().into_any();
    }
    view! {
        <details class="card">
            <summary><h2>"Other fields"</h2></summary>
            <p class="hint">
                "Everything else this actor stores, including anything the game's scripts added."
            </p>
            {extra.iter().map(|field| {
                let ivar = field.ivar.clone();
                let control = scalar_field(field.value.clone(), move |value| {
                    let ivar = ivar.clone();
                    spawn_local(async move {
                        on_change(api::set_actor_scalar(id, &ivar, &value).await)
                    });
                });
                view! {
                    <label class="row">
                        <span class="row-label" title=field.ivar.clone()>{field.label.clone()}</span>
                        <span class="row-control">{control}</span>
                    </label>
                }
            }).collect_view()}
        </details>
    }.into_any()
}
