# RPG Maker Save Editor

[![Build](https://github.com/jjayrex/rpgmaker-save-editor/actions/workflows/build.yml/badge.svg)](https://github.com/jjayrex/rpgmaker-save-editor/actions/workflows/build.yml)

A cross-platform editor for RPG Maker save files, built with Tauri V2 and Leptos.

Currently supports:
- **RPG Maker VX Ace** (`.rvdata2`)
- **RPG Maker VX** (`.rvdata`)
- **RPG Maker XP** (`.rxdata`)

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

The engines differ in more than their file extension, and the editor follows
each one: XP counts play time at 40 frames a second rather than 60, calls the
secondary pool SP, and stores the party as the actor objects themselves — which,
because every object is written as its own Marshal document, means a party
member exists twice in the file. Edits are applied to every copy.

Opening a save also accepts a path on the command line
(`rpgmaker-save-editor Save1.rvdata2`) or a file dropped onto the window.

## Building

Needs the Rust toolchain, the `wasm32-unknown-unknown` target, and
[Trunk](https://github.com/trunk-rs/trunk).

```bash
rustup target add wasm32-unknown-unknown
cargo install trunk tauri-cli --locked
cargo tauri dev        # run it
cargo tauri build      # produce installers
```

On Arch-based distributions the AppImage step fails with `failed to run
linuxdeploy`: linuxdeploy carries its own `strip`, which cannot read the
`.relr.dyn` sections in current system libraries. Either skip stripping with
`NO_STRIP=true cargo tauri build`, or build just what you need with
`cargo tauri build --bundles deb`.

## Automated builds

`.github/workflows/build.yml` runs on pushes to `main`, on pull requests, and on
demand from the Actions tab:

- **Tests and lints** — `cargo clippy -D warnings` and the test suite. Pull
  requests stop here.
- **Installers** — Windows (`.exe`, `.msi`) and Linux (`.deb`, `.AppImage`),
  attached to the run as artifacts. Linux builds on Ubuntu 22.04, so the result
  runs on that release and anything newer.
- 
