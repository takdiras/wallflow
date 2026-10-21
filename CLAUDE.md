# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

Wallflow is a Windows-only wallpaper changer built with Tauri 2 (Rust backend, plain HTML/CSS/JS front end with no build step). It pulls photos from Wallhaven and Bing (no key), plus Unsplash and Pexels when the user adds API keys in Settings.

## Commands

All Cargo commands run from `src-tauri/`.

```bash
cargo run                                          # dev build + launch
cargo fmt --check                                  # CI: formatting
cargo clippy --all-targets --locked -- -D warnings # CI: lint (warnings fail CI)
cargo build --locked                               # CI: build
cargo build --release                              # -> target/release/wallflow.exe
cargo tauri build                                  # NSIS installer (needs `cargo install tauri-cli --version "^2"`)
```

There are no tests in the repo. CI (`.github/workflows/ci.yml`, windows-latest) runs exactly fmt → clippy → build, so run those three before considering a change done.

The `ui/` folder is served directly (`frontendDist: "../ui"`); edit and relaunch, nothing to compile. `scripts/make-icons.ps1` regenerates `src-tauri/icons/`.

## Releasing

Pushing a `v*` tag triggers `.github/workflows/release.yml`, which **fails unless the tag equals the version in both `src-tauri/tauri.conf.json` and `src-tauri/Cargo.toml`** — bump both together. It publishes the NSIS installer, a portable exe, SHA256SUMS and `latest.json` to a GitHub release.

In-app updates: `bundle.createUpdaterArtifacts` makes the bundler sign the installer, so `cargo tauri build` / `npx @tauri-apps/cli@2 build` **fail without `TAURI_SIGNING_PRIVATE_KEY`** (CI reads it from the repo secret; the maintainer's copy is `~/.tauri/wallflow.key`, no password). Plain `cargo build`/`cargo run` don't need it. The matching public key is `plugins.updater.pubkey` in `tauri.conf.json`; changing the keypair strands existing installs. The updater fetches `releases/latest/download/latest.json`, which the workflow generates from the `.sig`.

## Architecture

- **`main.rs`** — Tauri commands (the whole JS↔Rust API: settings, state, browse, wallpaper navigation, `open_url`, and the three update commands), app setup, single-instance plugin, and close-to-tray. `save_settings` normalizes input (clamps interval, lowercases/dedups categories) and toggles autostart only when that flag changed. A new command must be added to `generate_handler!` and called via `invoke` in `ui/app.js`.
- **`core.rs`** — `AppState` (managed by Tauri): `settings`/`history` behind `std::sync::Mutex`, a shared `reqwest::Client`, and a `busy: tokio::sync::Mutex<()>`. Every wallpaper-changing action goes through `run()`, which `try_lock`s `busy` (concurrent requests are rejected, not queued), emits `busy`/`error` events, then emits `state-changed` with a `Snapshot`. History behaves like browser history: picking a new photo after going back drops the forward entries; capped at 40, evicted files are deleted. `next()` steps forward in history if possible, otherwise fetches a random photo via `providers::random`. The scheduler is a 20 s polling loop comparing `now()` against `last_change + interval`; on failure it resets `last_change` to back off a full interval.
- **`providers.rs`** — one parser per source turning JSON into the common `Photo` struct (`Source` enum: unsplash/pexels/wallhaven/bing). `browse` powers the Discover grid; `random` picks for rotation while avoiding history keys. `track_download` pings Unsplash's download endpoint (required by their API terms).
- **`system.rs`** — Windows integration: sets the wallpaper via COM `IDesktopWallpaper` on a dedicated thread (falls back to `SystemParametersInfo`), writes fit style to the registry, forces `BackgroundType=0` to leave Spotlight/slideshow, and manages autostart via the HKCU `Run` key with the `--autostart` arg (which starts hidden and optionally changes the wallpaper on login).
- **`tray.rs`** — tray icon/menu; must be kept in sync with state (`tray::update`, `tray::sync_rotate`) when settings or the current wallpaper change.
- **`update.rs`** — self-update via `tauri-plugin-updater`: checks 30 s after launch and every 12 h, keeps the found `Update` in the `Pending` state, and emits `update-available` / `update-progress`. Installing runs the NSIS installer in passive mode and the process exits. The portable exe is detected by the missing `uninstall.exe` next to it and only gets a link to the release page.
- **`accent.rs`** — derives the UI accent colour from the downloaded image.
- **`ui/app.js`** — uses `window.__TAURI__` (`withGlobalTauri: true`), keeps local `settings`/`snap` copies and re-renders on the `state-changed`, `settings-changed`, `busy`, `error` events. Errors from commands are returned as `Result<_, String>` and shown as toasts.

## Gotchas

- Settings/history JSON live in `%APPDATA%\dev.wallflow.app`; downloaded images live in `Pictures\Wallflow` because the shell refuses wallpapers stored under AppData. `AppState::load` migrates old entries from the AppData `wallpapers` folder.
- Window permissions are allowlisted in `src-tauri/capabilities/default.json`; new window/core APIs used from JS need adding there.
- The main window is frameless (`decorations: false`) and starts hidden; `tray::show_main` reveals it.
