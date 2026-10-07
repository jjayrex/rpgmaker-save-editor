# RPG Maker Save Editor

[![Build](https://github.com/jjayrex/rpgmaker-save-editor/actions/workflows/build.yml/badge.svg)](https://github.com/jjayrex/rpgmaker-save-editor/actions/workflows/build.yml)

A cross-platform editor for RPG Maker save files, built with Tauri V2 and Leptos.

Currently supports:
- **RPG Maker VX Ace** (`.rvdata2`)
- **RPG Maker VX** (`.rvdata`)
- **RPG Maker XP** (`.rxdata`)
- **RPG Maker 2000 and 2003** (`.lsd`)

## What it can edit

| Tab | Covers |
| --- | --- |
| Overview | Gold, steps, play time, map position, party line-up, file facts |
| Actors | Name, nickname, class, level and experience, HP/MP/TP, stat bonuses, equipment, skills, states, plus every other stored field |
| Inventory | Items, weapons and armours, with a searchable picker from the game's database |
| Switches | Paged and searchable, with bulk on/off |
| Variables | Paged and searchable; values can change Ruby type |
| Self switches | The per-event flags that mark chests as opened and conversations as done |
| Raw data | The decoded Ruby object graph, for anything a game's scripts added |

If the game's `Data` directory is readable, ids are resolved to names — items,
skills, states, classes, maps, switches and variables.

RPG Maker 2000 and 2003 predate RGSS and share a different format entirely —
LCF, a tree of binary chunks rather than a Ruby object graph. Both engines write
the same save format, so one reader serves both; the chunk numbers follow
liblcf, EasyRPG's reference implementation. Their database is `RPG_RT.ldb` in
the game folder, which the editor reads for actor, class, item, skill, state,
switch and variable names, and for the game's own equipment slot and currency
terms. Map names come from `RPG_RT.lmt` beside it, whose map list is written
straight after the signature rather than as a chunk. The format has no
self switches, nicknames or TP, and one table holds everything carried — the
Items, Weapons and Armours tabs are filled by what the database says each entry
is.

The engines differ in more than their file extension, and the editor follows
each one: XP counts play time at 40 frames a second rather than 60, calls the
secondary pool SP, and stores the party as the actor objects themselves — which,
because every object is written as its own Marshal document, means a party
member exists twice in the file. Edits are applied to every copy.

Opening a save also accepts a path on the command line
(`rpgmaker-save-editor Save1.rvdata2`) or a file dropped onto the window.

On Linux with the proprietary NVIDIA driver, WebKitGTK's DMA-BUF renderer cannot
share buffers with it and the window comes up blank. The editor detects that
driver at startup and turns the renderer off; setting
`WEBKIT_DISABLE_DMABUF_RENDERER` yourself overrides the decision either way.

## Building

Needs the Rust toolchain, the `wasm32-unknown-unknown` target, and
[Trunk](https://github.com/trunk-rs/trunk).

```bash
rustup target add wasm32-unknown-unknown
cargo install trunk tauri-cli --locked
cargo tauri dev        # run it
cargo tauri build      # produce installers
```

## Automated builds

- **Tests and lints** — `cargo clippy -D warnings` and the test suite. Pull
  requests stop here.
- **Installers** — Windows (`.exe`, `.msi`) and Linux (`.deb`), attached to the
  run as artifacts. Linux builds on Ubuntu 22.04, so the result runs on that
  release and anything newer.
- **Portable builds** — the bare executable for each platform, for running
  without installing anything. Everything the app needs is compiled in except
  the system webview: WebView2 on Windows (part of Windows 10 and 11), and
  WebKitGTK on Linux (`webkit2gtk-4.1`, which your distribution provides).
