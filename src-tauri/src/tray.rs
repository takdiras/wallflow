//! Notification-area icon with next / previous / rotate controls.

use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, Wry};

use crate::core::{self, AppState, Entry};
use crate::update;

const TRAY_ID: &str = "wallflow";

pub struct TrayItems {
    pub rotate: CheckMenuItem<Wry>,
    pub title: MenuItem<Wry>,
    pub update: MenuItem<Wry>,
}

pub fn show_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
}

pub fn build(app: &AppHandle) -> tauri::Result<()> {
    let rotate_on = app.state::<AppState>().settings.lock().unwrap().rotate;
    let title = MenuItem::with_id(app, "title", "Wallflow", false, None::<&str>)?;
    let next = MenuItem::with_id(app, "next", "Next wallpaper", true, None::<&str>)?;
    let prev = MenuItem::with_id(app, "prev", "Previous wallpaper", true, None::<&str>)?;
    let rotate =
        CheckMenuItem::with_id(app, "rotate", "Auto-rotate", true, rotate_on, None::<&str>)?;
    let update = MenuItem::with_id(app, "update", "Check for updates", true, None::<&str>)?;
    let open = MenuItem::with_id(app, "open", "Open Wallflow", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let sep = || PredefinedMenuItem::separator(app);
    let menu = Menu::with_items(
        app,
        &[
            &title,
            &sep()?,
            &next,
            &prev,
            &rotate,
            &sep()?,
            &update,
            &open,
            &quit,
        ],
    )?;

    TrayIconBuilder::with_id(TRAY_ID)
        .icon(app.default_window_icon().cloned().expect("app icon"))
        .tooltip("Wallflow")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, ev| {
            let app = app.clone();
            match ev.id().as_ref() {
                "next" => {
                    tauri::async_runtime::spawn(async move {
                        let _ = core::next(&app).await;
                    });
                }
                "prev" => {
                    tauri::async_runtime::spawn(async move {
                        let _ = core::prev(&app).await;
                    });
                }
                "rotate" => {
                    let st = app.state::<AppState>();
                    {
                        let mut s = st.settings.lock().unwrap();
                        s.rotate = !s.rotate;
                    }
                    *st.last_change.lock().unwrap() = core::now();
                    st.save_settings();
                    sync_rotate(&app);
                    core::emit_state(&app);
                    let _ = tauri::Emitter::emit(
                        &app,
                        "settings-changed",
                        st.settings.lock().unwrap().clone(),
                    );
                }
                "update" => {
                    tauri::async_runtime::spawn(async move {
                        let found = match update::pending(&app) {
                            Some(_) => true,
                            None => matches!(update::check(&app).await, Ok(Some(_))),
                        };
                        show_main(&app);
                        if found {
                            let _ = tauri::Emitter::emit(&app, "open-view", "settings");
                        }
                    });
                }
                "open" => show_main(&app),
                "quit" => app.exit(0),
                _ => {}
            }
        })
        .on_tray_icon_event(|tray, ev| {
            // Left click = next wallpaper, double click = open window, right click = menu.
            match ev {
                TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                } => {
                    let app = tray.app_handle().clone();
                    tauri::async_runtime::spawn(async move {
                        let _ = core::next(&app).await;
                    });
                }
                TrayIconEvent::DoubleClick {
                    button: MouseButton::Left,
                    ..
                } => show_main(tray.app_handle()),
                _ => {}
            }
        })
        .build(app)?;

    app.manage(TrayItems {
        rotate,
        title,
        update,
    });
    Ok(())
}

pub fn sync_rotate(app: &AppHandle) {
    let on = app.state::<AppState>().settings.lock().unwrap().rotate;
    if let Some(items) = app.try_state::<TrayItems>() {
        let _ = items.rotate.set_checked(on);
    }
}

pub fn update(app: &AppHandle, entry: &Entry) {
    let by = format!(
        "Photo by {} on {:?}",
        entry.photo.author, entry.photo.source
    );
    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        let _ = tray.set_tooltip(Some(format!("Wallflow\n{by}")));
    }
    if let Some(items) = app.try_state::<TrayItems>() {
        let _ = items.title.set_text(by);
    }
}

pub fn set_update(app: &AppHandle, version: Option<&str>) {
    let text = match version {
        Some(v) => format!("Update to {v}…"),
        None => "Check for updates".into(),
    };
    if let Some(items) = app.try_state::<TrayItems>() {
        let _ = items.update.set_text(text);
    }
}
