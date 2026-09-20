//! A browser for everything else in the file.
//!
//! Games modify their own save data through scripts, so no fixed set of
//! editors can cover every save. This tab walks the decoded Ruby object graph
//! itself and lets any simple value be changed.

use leptos::prelude::*;
use leptos::task::spawn_local;
use rpgsave_protocol::{NodeView, Scalar, Slot};

use crate::api;
use crate::state::ctx;
use crate::views::scalar_field;

/// One step of the path the user has drilled into.
#[derive(Clone, PartialEq)]
struct Crumb {
    label: String,
    node: Option<u32>,
}

#[component]
pub fn RawView() -> impl IntoView {
    let ctx = ctx();
    let node = RwSignal::new(None::<NodeView>);
    let trail = RwSignal::new(Vec::<Crumb>::new());

    let load = move |target: Option<u32>| {
        spawn_local(async move {
            let result = match target {
                Some(id) => api::raw_node(id).await,
                None => api::raw_root().await,
            };
            match result {
                Ok(view) => node.set(Some(view)),
                Err(e) => ctx.fail(e),
            }
        });
    };

    Effect::new(move |_| {
        ctx.revision.track();
        trail.set(Vec::new());
        load(None);
    });

    let open = move |label: String, target: Option<u32>| {
        trail.update(|t| t.push(Crumb { label, node: target }));
        load(target);
    };

    let go_to = move |depth: usize| {
        let target = if depth == 0 {
            None
        } else {
            trail.with_untracked(|t| t.get(depth - 1).and_then(|c| c.node))
        };
        trail.update(|t| t.truncate(depth.saturating_sub(1)));
        load(target);
    };

    let commit = move |slot: Slot, value: Scalar| {
        let parent = node.get_untracked().and_then(|n| n.node);
        spawn_local(async move {
            match api::set_raw_scalar(parent, &slot, &value).await {
                Ok(view) => {
                    node.set(Some(view));
                    ctx.accept(api::summary().await);
                }
                Err(e) => ctx.fail(e),
            }
        });
    };

    view! {
        <section class="card full">
            <div class="toolbar">
                <h2>"Raw data"</h2>
                <nav class="crumbs">
                    <button class="crumb" on:click=move |_| go_to(0)>"File"</button>
                    {move || trail.get().iter().enumerate().map(|(i, crumb)| {
                        let depth = i + 1;
                        view! {
                            <span class="crumb-sep">"›"</span>
                            <button class="crumb" on:click=move |_| go_to(depth)>
                                {crumb.label.clone()}
                            </button>
                        }
                    }).collect_view()}
                </nav>
            </div>

            {move || match node.get() {
                None => view! { <p class="empty">"Loading…"</p> }.into_any(),
                Some(view_data) => {
                    let title = view_data.title.clone();
                    let summary = view_data.summary.clone();
                    let binary = view_data.binary.clone();
                    let children = view_data.children.clone();
                    view! {
                        <div class="node-head">
                            <span class="strong">{title}</span>
                            <span class="muted">{summary}</span>
                        </div>

                        {binary.map(|b| view! {
                            <div class="binary">
                                <p class="muted">
                                    {format!("{} · {} bytes of packed data", b.description, b.bytes)}
                                </p>
                                <Show when={let has = !b.preview.is_empty(); move || has}>
                                    <p class="mono small">
                                        {b.preview.iter().map(|v| v.to_string())
                                            .collect::<Vec<_>>().join(" ")}
                                        " …"
                                    </p>
                                </Show>
                                <p class="hint">
                                    "Packed values such as map tiles and screen tones are shown \
                                     here but not edited; changing them safely means changing the \
                                     map, not the save."
                                </p>
                            </div>
                        })}

                        <table class="grid rows">
                            <tbody>
                                {children.iter().map(|child| {
                                    let key = child.key.clone();
                                    let label = child.key.clone();
                                    let preview = child.preview.clone();
                                    let target = child.node;
                                    let slot = child.slot.clone();
                                    let scalar = child.scalar.clone();
                                    view! {
                                        <tr>
                                            <td class="col-key mono">{key}</td>
                                            <td class="col-value">
                                                {match scalar {
                                                    Some(value) => {
                                                        let slot = slot.clone();
                                                        scalar_field(value, move |v| commit(slot.clone(), v)).into_any()
                                                    }
                                                    None => view! {
                                                        <span class="muted">{preview.clone()}</span>
                                                    }.into_any(),
                                                }}
                                            </td>
                                            <td class="col-open">
                                                {target.map(|id| view! {
                                                    <button class="btn tiny"
                                                        on:click=move |_| open(label.clone(), Some(id))>
                                                        "Open"
                                                    </button>
                                                })}
                                            </td>
                                        </tr>
                                    }
                                }).collect_view()}
                            </tbody>
                        </table>
                    }.into_any()
                }
            }}
        </section>
    }
}
