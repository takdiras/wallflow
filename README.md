# Wallflow

A small Windows wallpaper app written in Rust (Tauri 2). It pulls photos from **Wallhaven** and **Bing** (no key needed), plus **Unsplash** and **Pexels** if you add API keys.

- **Discover**: browse recommended photos (Wallhaven top list, Bing daily images, Unsplash *Wallpapers*, Pexels *Curated*) or search, then click one to set it.
- **Rotation**: pick categories and Wallflow sets a random photo from them on a timer (5 min to 1 week).
- **Manual**: Next and Previous buttons in the app or the tray icon. Next moves forward through history, or fetches a new random photo when you're already at the newest one.
- **On startup**: optionally start with Windows (silently, in the tray) and set a fresh wallpaper at each sign-in.
- **UI**: minimal glassmorphism over a blurred copy of the active wallpaper. The accent colour comes from the wallpaper, and there are light, dark and system themes.
- **Tray**: left-click for the next wallpaper, double-click to open, right-click for Next / Previous / Auto-rotate / Quit. Closing the window hides it to the tray.

## API keys (optional, free)

- Unsplash: create an app at <https://unsplash.com/oauth/applications> and copy its **Access Key** (demo apps get 50 requests/hour).
- Pexels: <https://www.pexels.com/api/> (200 requests/hour).

Wallhaven and Bing work without any key. To add Unsplash or Pexels, paste a key under **Settings → API keys**.

## Build

Requirements: Rust (stable, MSVC), the Visual Studio C++ build tools, and WebView2 (included with Windows 11).

```bash
cd src-tauri
cargo run
```

Release exe: `cargo build --release` → `src-tauri/target/release/wallflow.exe`.

To build an installer, run `cargo install tauri-cli --version "^2"` once, then `cargo tauri build` in `src-tauri`.

## Layout

| Path | Purpose |
| --- | --- |
| `src-tauri/src/providers.rs` | Wallhaven/Bing/Unsplash/Pexels clients, browse, random pick |
| `src-tauri/src/core.rs` | settings, history, next/prev, rotation scheduler |
| `src-tauri/src/system.rs` | sets the wallpaper via `IDesktopWallpaper`, fit style, autostart (HKCU Run) |
| `src-tauri/src/accent.rs` | picks the accent colour from the image |
| `src-tauri/src/tray.rs` | tray icon and menu |
| `ui/` | HTML/CSS/JS front end (no build step) |

Settings and history are stored in `%APPDATA%\dev.wallflow.app`. Downloaded wallpapers (the last 40) go in `Pictures\Wallflow`, because Windows refuses wallpaper files stored under AppData. Applying a wallpaper switches Windows out of Spotlight/slideshow mode.
