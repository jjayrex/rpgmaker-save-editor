# RPG Maker Save Editor

A cross-platform editor for RPG Maker save files, built with Tauri V2 and Leptos.

Currently supports:
- **RPG Maker VX Ace**
- **RPG Maker VX**

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

Opening a save also accepts a path on the command line
(`rpgmaker-save-editor Save1.rvdata2`) or a file dropped onto the window.

## Building

Needs the Rust toolchain, the `wasm32-unknown-unknown` target, and
[Trunk](https://github.com/trunk-rs/trunk).

```bash
rustup target add wasm32-unknown-unknown
cargo install trunk tauri-cli --locked
cargo tauri dev        # run it
cargo tauri build      # produce a bundle
```
