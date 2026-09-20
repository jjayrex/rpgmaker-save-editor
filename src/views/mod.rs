//! The tab contents.

pub mod actors;
pub mod flags;
pub mod inventory;
pub mod overview;
pub mod raw;

use leptos::prelude::*;

use crate::state::parse_int;

/// A number input that reports its value when the user leaves the field or
/// presses Enter, rather than on every keystroke.
pub fn number_field(
    value: i64,
    class: &'static str,
    on_commit: impl Fn(i64) + 'static,
) -> impl IntoView {
    view! {
        <input
            type="number"
            class=class
            prop:value=value.to_string()
            on:change=move |ev| {
                if let Some(v) = parse_int(&event_target_value(&ev)) {
                    on_commit(v);
                }
            }
        />
    }
}

/// A text input with the same commit-on-change behaviour.
pub fn text_field(
    value: String,
    class: &'static str,
    on_commit: impl Fn(String) + 'static,
) -> impl IntoView {
    view! {
        <input
            type="text"
            class=class
            prop:value=value
            on:change=move |ev| on_commit(event_target_value(&ev))
        />
    }
}

/// A labelled row inside a card.
pub fn field_row(label: &'static str, control: impl IntoView) -> impl IntoView {
    view! {
        <label class="row">
            <span class="row-label">{label}</span>
            <span class="row-control">{control}</span>
        </label>
    }
}

/// An editor matched to the Ruby type of the value it is given.
pub fn scalar_field(
    value: rpgsave_protocol::Scalar,
    on_commit: impl Fn(rpgsave_protocol::Scalar) + Clone + 'static,
) -> impl IntoView {
    use rpgsave_protocol::Scalar;

    match value {
        Scalar::Bool(checked) => view! {
            <label class="check">
                <input
                    type="checkbox"
                    prop:checked=checked
                    on:change=move |ev| on_commit(Scalar::Bool(event_target_checked(&ev)))
                />
                <span>{if checked { "true" } else { "false" }}</span>
            </label>
        }
        .into_any(),
        Scalar::Int(n) => number_field(n, "num", move |v| on_commit(Scalar::Int(v))).into_any(),
        Scalar::Float(f) => view! {
            <input
                type="number"
                step="any"
                class="num"
                prop:value=f.to_string()
                on:change=move |ev| {
                    if let Ok(v) = event_target_value(&ev).trim().parse::<f64>() {
                        on_commit(Scalar::Float(v));
                    }
                }
            />
        }
        .into_any(),
        Scalar::Str(text) => {
            text_field(text, "text", move |v| on_commit(Scalar::Str(v))).into_any()
        }
        Scalar::Sym(name) => view! {
            <span class="input-group">
                <span class="unit">":"</span>
                {text_field(name, "text", move |v| on_commit(Scalar::Sym(v)))}
            </span>
        }
        .into_any(),
        Scalar::Nil => view! {
            <span class="nil-value">
                <span class="muted">"nil"</span>
                <button class="btn tiny" on:click=move |_| on_commit(Scalar::Int(0))>
                    "Set a number"
                </button>
            </span>
        }
        .into_any(),
    }
}
