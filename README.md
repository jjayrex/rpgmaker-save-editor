# RPG Maker Save Editor

[![Build](https://github.com/jjayrex/rpgmaker-save-editor/actions/workflows/build.yml/badge.svg)](https://github.com/jjayrex/rpgmaker-save-editor/actions/workflows/build.yml)

A cross-platform editor for RPG Maker save files, built with Tauri V2 and Leptos.

Currently supports:
- **RPG Maker VX Ace** (`.rvdata2`)
- **RPG Maker VX** (`.rvdata`)
- **RPG Maker XP** (`.rxdata`)
- **RPG Maker 2000 and 2003** (`.lsd`)
- **RPG Maker MV** (`.rpgsave`)

## What it can edit

| Tab | Covers |
| --- | --- |
| Overview | Gold, steps, play time, map position, party line-up, file facts |
| Actors | Name, nickname, class, level and experience, HP/MP/TP, stat bonuses, equipment, skills, states, plus every other stored field |
| Inventory | Items, weapons and armours, with a searchable picker from the game's database |
| Switches | Paged and searchable, with bulk on/off |
| Variables | Paged and searchable; values can change Ruby type |
| Self switches | The per-event flags that mark chests as opened and conversations as done |
| Raw data | The decoded save, object by object, for anything a game's scripts or plugins added |

If the game's database is readable, ids are resolved to names — items, skills,
states, classes, maps, switches and variables. Each engine keeps it somewhere
different, and the editor looks for all of them: `Data/*.rvdata2` and its
siblings for the RGSS engines, `RPG_RT.ldb` for 2000 and 2003, and `data/*.json`
for MV.

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

RPG Maker MV left Ruby behind for JavaScript, so a `.rpgsave` is the game's
objects written as JSON by the engine's own `JsonEx`, compressed with lz-string
and stored as base64 text. Both of those are implemented here rather than
depended on: the compression is a port of lz-string, and the JSON parser keeps
every member in the engine's order and replays the original text of every value
it was not asked to change, so a save the editor has not edited is written back
byte for byte. `JsonEx`'s own bookkeeping — the `@` class name an object is
rebuilt from, the `@c` identity and the `@r` back-reference — is passed through
untouched. MV holds experience per class rather than per actor, and marks each
equipped thing with whether it is a weapon or a piece of armour, so the editor
follows the save rather than guessing from the slot.

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
