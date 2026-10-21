#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod accent;
mod core;
mod providers;
mod system;
mod tray;

use tauri::{AppHandle, Emitter, Manager, State, WindowEvent};

use crate::core::{AppState, Settings, Snapshot};
use crate::providers::{Photo, Source};

#[tauri::command]
fn get_settings(state: State<AppState>) -> Settings {
    state.settings.lock().unwrap().clone()
}

#[tauri::command]
fn save_settings(app: AppHandle, state: State<AppState>, settings: Settings) -> Result<Settings, String> {
    let mut settings = settings;
    settings.interval_minutes = settings.interval_minutes.clamp(1, 7 * 24 * 60);
    settings.categories = settings
        .categories
        .iter()
        .map(|c| c.trim().to_lowercase())
        .filter(|c| !c.is_empty())
        .collect();
    settings.categories.dedup();

    let autostart_changed = {
        let old = state.settings.lock().unwrap();
        let changed = old.launch_at_startup != settings.launch_at_startup;
        if old.rotate != settings.rotate || old.interval_minutes != settings.interval_minutes {
            *state.last_change.lock().unwrap() = core::now();
        }
        changed
    };
    if autostart_changed {
        system::set_autostart(settings.launch_at_startup)?;
    }
    *state.settings.lock().unwrap() = settings.clone();
    state.save_settings();
    tray::sync_rotate(&app);
    core::emit_state(&app);
    Ok(settings)
}

#[tauri::command]
fn get_state(state: State<AppState>) -> Snapshot {
    state.snapshot()
}

#[tauri::command]
async fn browse(state: State<'_, AppState>, source: Option<Source>, query: String, page: u32) -> Result<Vec<Photo>, String> {
    let (keys, sources) = {
        let s = state.settings.lock().unwrap();
        (s.keys(), s.enabled_sources())
    };
    providers::browse(&state.client, &keys, &sources, source, &query, page.max(1)).await
}

#[tauri::command]
async fn set_photo(app: AppHandle, photo: Photo) -> Result<Snapshot, String> {
    core::set_photo(&app, photo).await
}

#[tauri::command]
async fn next_wallpaper(app: AppHandle) -> Result<Snapshot, String> {
    core::next(&app).await
}

#[tauri::command]
async fn prev_wallpaper(app: AppHandle) -> Result<Snapshot, String> {
    core::prev(&app).await
}

#[tauri::command]
async fn goto_history(app: AppHandle, index: usize) -> Result<Snapshot, String> {
    core::goto(&app, index).await
}

#[tauri::command]
fn open_url(url: String) {
    system::open_url(&url);
}

fn main() {
    let autostarted = std::env::args().any(|a| a == system::AUTOSTART_ARG);

    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| tray::show_main(app)))
        .invoke_handler(tauri::generate_handler![
            get_settings,
            save_settings,
            get_state,
            browse,
            set_photo,
            next_wallpaper,
            prev_wallpaper,
            goto_history,
            open_url
        ])
        .setup(move |app| {
            let handle = app.handle().clone();
            app.manage(AppState::load(&handle));
            tray::build(&handle)?;
            core::start_scheduler(handle.clone());

            if !autostarted {
                tray::show_main(&handle);
            }
            let state = handle.state::<AppState>();
            let current = {
                let h = state.history.lock().unwrap();
                h.entries.get(h.index).cloned()
            };
            if let Some(cur) = current {
                tray::update(&handle, &cur);
            }
            // "Every time Windows starts": a fresh wallpaper on login.
            if autostarted && state.settings.lock().unwrap().change_on_startup {
                let h = handle.clone();
                tauri::async_runtime::spawn(async move {
                    // Give the network a moment to come up after login.
                    for _ in 0..6 {
                        if core::next(&h).await.is_ok() {
                            break;
                        }
                        tokio::time::sleep(std::time::Duration::from_secs(10)).await;
                    }
                });
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            // Closing the window keeps Wallflow running in the tray.
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
                let _ = window.emit("hidden", ());
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running Wallflow");
}
