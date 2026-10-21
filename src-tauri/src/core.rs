//! App state: settings, wallpaper history, and the change/rotate logic.

use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use reqwest::Client;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};

use crate::providers::{self, Keys, Photo, Source};
use crate::{accent, system};

const HISTORY_LIMIT: usize = 40;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub unsplash_key: String,
    pub pexels_key: String,
    pub use_unsplash: bool,
    pub use_pexels: bool,
    pub use_wallhaven: bool,
    pub use_bing: bool,
    pub categories: Vec<String>,
    pub rotate: bool,
    pub interval_minutes: u32,
    pub change_on_startup: bool,
    pub launch_at_startup: bool,
    /// "system" | "light" | "dark"
    pub theme: String,
    /// "fill" | "fit" | "stretch" | "center" | "span" | "tile"
    pub fit: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            unsplash_key: String::new(),
            pexels_key: String::new(),
            use_unsplash: true,
            use_pexels: true,
            use_wallhaven: true,
            use_bing: true,
            categories: vec!["nature".into(), "landscape".into(), "mountains".into()],
            rotate: false,
            interval_minutes: 60,
            change_on_startup: true,
            launch_at_startup: false,
            theme: "system".into(),
            fit: "fill".into(),
        }
    }
}

impl Settings {
    pub fn keys(&self) -> Keys {
        let k = |s: &str| Some(s.trim().to_string()).filter(|s| !s.is_empty());
        Keys {
            unsplash: k(&self.unsplash_key),
            pexels: k(&self.pexels_key),
        }
    }
    pub fn enabled_sources(&self) -> Vec<Source> {
        let mut v = Vec::new();
        if self.use_unsplash {
            v.push(Source::Unsplash);
        }
        if self.use_pexels {
            v.push(Source::Pexels);
        }
        if self.use_wallhaven {
            v.push(Source::Wallhaven);
        }
        if self.use_bing {
            v.push(Source::Bing);
        }
        v
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entry {
    pub photo: Photo,
    pub path: PathBuf,
    pub accent: String,
    pub set_at: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct History {
    pub entries: Vec<Entry>,
    pub index: usize,
}

/// What the UI needs to render the "now playing" view.
#[derive(Debug, Clone, Serialize)]
pub struct Snapshot {
    pub current: Option<Entry>,
    pub index: usize,
    pub total: usize,
    pub has_prev: bool,
    pub history: Vec<Entry>,
    pub next_change_at: Option<u64>,
    pub busy: bool,
}

pub struct AppState {
    pub settings: Mutex<Settings>,
    pub history: Mutex<History>,
    pub client: Client,
    pub busy: tokio::sync::Mutex<()>,
    pub last_change: Mutex<u64>,
    pub dir: PathBuf,
    pub images: PathBuf,
}

pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn read_json<T: for<'de> Deserialize<'de> + Default>(path: &PathBuf) -> T {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn write_json<T: Serialize>(path: &PathBuf, v: &T) {
    if let Ok(s) = serde_json::to_string_pretty(v) {
        let tmp = path.with_extension("tmp");
        if std::fs::write(&tmp, s).is_ok() {
            let _ = std::fs::rename(tmp, path);
        }
    }
}

impl AppState {
    pub fn load(app: &AppHandle) -> Self {
        let dir = app
            .path()
            .app_data_dir()
            .unwrap_or_else(|_| std::env::temp_dir().join("Wallflow"));
        let _ = std::fs::create_dir_all(&dir);
        // Images live in Pictures: the shell refuses wallpapers stored under the
        // app's AppData folder (IDesktopWallpaper returns "file not found").
        let images = app
            .path()
            .picture_dir()
            .map(|p| p.join("Wallflow"))
            .unwrap_or_else(|_| dir.join("wallpapers"));
        let _ = std::fs::create_dir_all(&images);
        let settings: Settings = read_json(&dir.join("settings.json"));
        let mut history: History = read_json(&dir.join("history.json"));
        for e in &mut history.entries {
            if !e.path.starts_with(&images) {
                if let Some(name) = e.path.file_name() {
                    let dest = images.join(name);
                    if std::fs::rename(&e.path, &dest).is_ok() || dest.exists() {
                        e.path = dest;
                    }
                }
            }
        }
        let _ = std::fs::remove_dir(dir.join("wallpapers")); // only succeeds once empty
        history.entries.retain(|e| e.path.exists());
        sweep_images(&images, &history.entries);
        history.index = history.index.min(history.entries.len().saturating_sub(1));
        let last = history
            .entries
            .get(history.index)
            .map(|e| e.set_at)
            .unwrap_or(0);
        Self {
            settings: Mutex::new(settings),
            history: Mutex::new(history),
            client: Client::builder()
                .user_agent("Wallflow/0.1")
                .timeout(Duration::from_secs(60))
                .build()
                .expect("http client"),
            busy: tokio::sync::Mutex::new(()),
            last_change: Mutex::new(last),
            dir,
            images,
        }
    }

    pub fn save_settings(&self) {
        write_json(
            &self.dir.join("settings.json"),
            &*self.settings.lock().unwrap(),
        );
    }

    fn save_history(&self) {
        write_json(
            &self.dir.join("history.json"),
            &*self.history.lock().unwrap(),
        );
    }

    pub fn snapshot(&self) -> Snapshot {
        let h = self.history.lock().unwrap();
        let s = self.settings.lock().unwrap();
        let next_change_at = s
            .rotate
            .then(|| *self.last_change.lock().unwrap() + s.interval_minutes.max(1) as u64 * 60);
        Snapshot {
            current: h.entries.get(h.index).cloned(),
            index: h.index,
            total: h.entries.len(),
            has_prev: h.index > 0,
            history: h.entries.iter().rev().cloned().collect(),
            next_change_at,
            busy: self.busy.try_lock().is_err(),
        }
    }
}

fn emit(app: &AppHandle) {
    let st = app.state::<AppState>();
    let _ = app.emit("state-changed", st.snapshot());
}

fn apply(app: &AppHandle, entry: &Entry) -> Result<(), String> {
    let fit = app.state::<AppState>().settings.lock().unwrap().fit.clone();
    system::set_wallpaper(&entry.path, &fit)?;
    *app.state::<AppState>().last_change.lock().unwrap() = now();
    crate::tray::update(app, entry);
    Ok(())
}

/// Download a photo, set it as wallpaper and push it onto the history.
async fn download_and_set(app: &AppHandle, photo: Photo) -> Result<(), String> {
    let st = app.state::<AppState>();
    let (keys, span) = {
        let s = st.settings.lock().unwrap();
        (s.keys(), s.fit == "span")
    };
    let url = photo.download_url(system::screen_width(span));
    let bytes = st
        .client
        .get(&url)
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .map_err(|e| format!("Download failed: {e}"))?
        .bytes()
        .await
        .map_err(|e| format!("Download failed: {e}"))?;

    let path = st
        .images
        .join(format!("{}.{}", photo.key(), photo.extension()));
    std::fs::write(&path, &bytes).map_err(|e| format!("Could not save image: {e}"))?;
    providers::track_download(&st.client, &keys, &photo).await;

    let p = path.clone();
    let color = tauri::async_runtime::spawn_blocking(move || accent::from_file(&p))
        .await
        .ok()
        .flatten()
        .unwrap_or_else(|| photo.color.clone());

    let entry = Entry {
        photo,
        path,
        accent: color,
        set_at: now(),
    };
    apply(app, &entry)?;

    let removed = {
        let mut h = st.history.lock().unwrap();
        // Choosing a new wallpaper after going back drops the "forward" entries,
        // like a browser. Keep files that are still referenced elsewhere.
        let cut = (h.index + 1).min(h.entries.len());
        let mut removed: Vec<Entry> = h.entries.split_off(cut);
        h.entries.retain(|e| e.photo.key() != entry.photo.key());
        h.entries.push(entry);
        while h.entries.len() > HISTORY_LIMIT {
            removed.push(h.entries.remove(0));
        }
        h.index = h.entries.len() - 1;
        removed.retain(|r| !h.entries.iter().any(|e| e.path == r.path));
        removed
    };
    for r in removed {
        let _ = std::fs::remove_file(r.path);
    }
    st.save_history();
    Ok(())
}

/// Delete images no history entry points to (failed applies, lost history,
/// failed deletes). Only our own jpg/png files, in case the user put others here.
fn sweep_images(images: &std::path::Path, entries: &[Entry]) {
    let Ok(files) = std::fs::read_dir(images) else {
        return;
    };
    for path in files.flatten().map(|f| f.path()) {
        let ours = path
            .extension()
            .is_some_and(|x| x.eq_ignore_ascii_case("jpg") || x.eq_ignore_ascii_case("png"));
        let used = entries
            .iter()
            .any(|e| e.path.file_name() == path.file_name());
        if ours && !used && path.is_file() {
            let _ = std::fs::remove_file(&path);
        }
    }
}

async fn run<F, Fut>(app: &AppHandle, f: F) -> Result<Snapshot, String>
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = Result<(), String>>,
{
    let st = app.state::<AppState>();
    let Ok(_guard) = st.busy.try_lock() else {
        return Err("Already changing the wallpaper…".into());
    };
    let _ = app.emit("busy", true);
    let r = f().await;
    // Still under the busy lock, so no download is in flight.
    let entries = st.history.lock().unwrap().entries.clone();
    sweep_images(&st.images, &entries);
    drop(_guard);
    let _ = app.emit("busy", false);
    if let Err(e) = &r {
        let _ = app.emit("error", e.clone());
    }
    emit(app);
    r.map(|_| st.snapshot())
}

pub async fn set_photo(app: &AppHandle, photo: Photo) -> Result<Snapshot, String> {
    run(app, || download_and_set(app, photo)).await
}

/// Move forward in history, or fetch a fresh random photo from the categories.
pub async fn next(app: &AppHandle) -> Result<Snapshot, String> {
    run(app, || async move {
        let st = app.state::<AppState>();
        let forward = {
            let mut h = st.history.lock().unwrap();
            if h.index + 1 < h.entries.len() {
                h.index += 1;
                h.entries.get(h.index).cloned()
            } else {
                None
            }
        };
        if let Some(e) = forward {
            apply(app, &e)?;
            st.save_history();
            return Ok(());
        }
        let (keys, sources, cats) = {
            let s = st.settings.lock().unwrap();
            (s.keys(), s.enabled_sources(), s.categories.clone())
        };
        let avoid: Vec<String> = st
            .history
            .lock()
            .unwrap()
            .entries
            .iter()
            .map(|e| e.photo.key())
            .collect();
        let photo = providers::random(&st.client, &keys, &sources, &cats, &avoid).await?;
        download_and_set(app, photo).await
    })
    .await
}

pub async fn prev(app: &AppHandle) -> Result<Snapshot, String> {
    run(app, || async move { step_to(app, None) }).await
}

pub async fn goto(app: &AppHandle, index: usize) -> Result<Snapshot, String> {
    run(app, || async move { step_to(app, Some(index)) }).await
}

fn step_to(app: &AppHandle, index: Option<usize>) -> Result<(), String> {
    let st = app.state::<AppState>();
    let e = {
        let mut h = st.history.lock().unwrap();
        let target = match index {
            Some(i) if i < h.entries.len() => i,
            Some(_) => return Err("That wallpaper is no longer in history".into()),
            None if h.index > 0 => h.index - 1,
            None => return Err("This is the oldest wallpaper in history".into()),
        };
        h.index = target;
        h.entries[target].clone()
    };
    apply(app, &e)?;
    st.save_history();
    Ok(())
}

/// Background loop for timed rotation.
pub fn start_scheduler(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(20)).await;
            let due = {
                let st = app.state::<AppState>();
                let s = st.settings.lock().unwrap();
                let last = *st.last_change.lock().unwrap();
                s.rotate && now() >= last + s.interval_minutes.max(1) as u64 * 60
            };
            if due && next(&app).await.is_err() {
                // Avoid hammering the API after a failure: wait a full interval.
                *app.state::<AppState>().last_change.lock().unwrap() = now();
            }
        }
    });
}

pub fn emit_state(app: &AppHandle) {
    emit(app)
}
