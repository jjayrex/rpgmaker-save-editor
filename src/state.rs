//! Shared front-end state.

use leptos::prelude::*;
use rpgsave_protocol::Summary;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tab {
    Overview,
    Actors,
    Inventory,
    Switches,
    Variables,
    SelfSwitches,
    Raw,
}

impl Tab {
    pub const ALL: [Tab; 7] = [
        Tab::Overview,
        Tab::Actors,
        Tab::Inventory,
        Tab::Switches,
        Tab::Variables,
        Tab::SelfSwitches,
        Tab::Raw,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Tab::Overview => "Overview",
            Tab::Actors => "Actors",
            Tab::Inventory => "Inventory",
            Tab::Switches => "Switches",
            Tab::Variables => "Variables",
            Tab::SelfSwitches => "Self switches",
            Tab::Raw => "Raw data",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Level {
    Success,
    Error,
}

#[derive(Clone, PartialEq, Debug)]
pub struct Toast {
    pub level: Level,
    pub text: String,
}

/// Everything the whole interface shares, passed down through context.
#[derive(Clone, Copy)]
pub struct Ctx {
    pub summary: RwSignal<Option<Summary>>,
    pub tab: RwSignal<Tab>,
    pub toast: RwSignal<Option<Toast>>,
    pub busy: RwSignal<bool>,
    /// Bumped whenever the open file changes, so views reload themselves.
    pub revision: RwSignal<u32>,
    /// Which actor the Actors tab is showing.
    pub actor: RwSignal<Option<i64>>,
}

impl Default for Ctx {
    fn default() -> Self {
        Self::new()
    }
}

impl Ctx {
    pub fn new() -> Ctx {
        Ctx {
            summary: RwSignal::new(None),
            tab: RwSignal::new(Tab::Overview),
            toast: RwSignal::new(None),
            busy: RwSignal::new(false),
            revision: RwSignal::new(0),
            actor: RwSignal::new(None),
        }
    }

    pub fn done(&self, text: impl Into<String>) {
        self.toast.set(Some(Toast { level: Level::Success, text: text.into() }));
    }

    pub fn fail(&self, text: impl Into<String>) {
        self.toast.set(Some(Toast { level: Level::Error, text: text.into() }));
    }

    pub fn clear_toast(&self) {
        self.toast.set(None);
    }

    /// Stores a fresh summary, or reports why one could not be produced.
    pub fn accept(&self, result: Result<Summary, String>) {
        match result {
            Ok(summary) => self.summary.set(Some(summary)),
            Err(message) => self.fail(message),
        }
    }

    /// Same, for a command that replaced the open file.
    pub fn accept_new_file(&self, result: Result<Summary, String>) {
        let ok = result.is_ok();
        self.accept(result);
        if ok {
            self.revision.update(|r| *r += 1);
            self.actor.set(None);
            self.tab.set(Tab::Overview);
        }
    }

    /// Opens the Actors tab on a particular actor.
    pub fn show_actor(&self, actor_id: i64) {
        self.actor.set(Some(actor_id));
        self.tab.set(Tab::Actors);
    }

    pub fn close(&self) {
        self.summary.set(None);
        self.actor.set(None);
        self.revision.update(|r| *r += 1);
        self.tab.set(Tab::Overview);
    }
}

/// Reads the shared context. Every view runs inside `App`, which provides it.
pub fn ctx() -> Ctx {
    expect_context::<Ctx>()
}

/// Parses what the user typed into a number input, ignoring empty text.
pub fn parse_int(text: &str) -> Option<i64> {
    let cleaned: String = text.chars().filter(|c| !c.is_whitespace() && *c != ',').collect();
    if cleaned.is_empty() {
        return None;
    }
    cleaned.parse().ok()
}

/// `12345` becomes `12 345`, which is easier to read at a glance.
pub fn group_digits(value: i64) -> String {
    let negative = value < 0;
    let digits = value.abs().to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push('\u{202f}');
        }
        out.push(c);
    }
    if negative {
        format!("-{out}")
    } else {
        out
    }
}

pub fn format_bytes(bytes: usize) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else {
        format!("{:.1} KiB", bytes as f64 / 1024.0)
    }
}

/// `3661` becomes `1:01:01`.
pub fn format_duration(seconds: i64) -> String {
    let seconds = seconds.max(0);
    format!("{}:{:02}:{:02}", seconds / 3600, seconds / 60 % 60, seconds % 60)
}
