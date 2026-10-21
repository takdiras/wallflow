//! Self-update from GitHub releases (tauri-plugin-updater + the NSIS installer).

use std::sync::Mutex;
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_updater::{Update, UpdaterExt};

const RELEASES_URL: &str = "https://github.com/takdiras/wallflow/releases/latest";
const CHECK_EVERY: Duration = Duration::from_secs(12 * 60 * 60);

/// The update found by the last check, ready to install.
#[derive(Default)]
pub struct Pending(Mutex<Option<Update>>);

#[derive(Clone, Serialize)]
pub struct UpdateInfo {
    pub version: String,
    /// False for the portable exe: it can't update itself, so the UI offers a download.
    pub installable: bool,
}

/// The NSIS installer leaves uninstall.exe next to the app. The portable exe has none,
/// and running the installer for it would put a second copy on the PC.
fn installed() -> bool {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("uninstall.exe").exists()))
        .unwrap_or(false)
}

pub fn pending(app: &AppHandle) -> Option<UpdateInfo> {
    let p = app.state::<Pending>();
    let u = p.0.lock().unwrap();
    u.as_ref().map(|u| UpdateInfo {
        version: u.version.clone(),
        installable: installed(),
    })
}

pub async fn check(app: &AppHandle) -> Result<Option<UpdateInfo>, String> {
    let update = app
        .updater()
        .map_err(|e| e.to_string())?
        .check()
        .await
        .map_err(|e| format!("Couldn't check for updates: {e}"))?;
    *app.state::<Pending>().0.lock().unwrap() = update;
    let info = pending(app);
    crate::tray::set_update(app, info.as_ref().map(|i| i.version.as_str()));
    if let Some(i) = &info {
        let _ = app.emit("update-available", i.clone());
    }
    Ok(info)
}

/// Download and run the installer. On success Windows exits this process and the
/// installer restarts the app, so this only returns on failure (or for the portable exe).
pub async fn install(app: &AppHandle) -> Result<(), String> {
    if !installed() {
        crate::system::open_url(RELEASES_URL);
        return Ok(());
    }
    let update = app
        .state::<Pending>()
        .0
        .lock()
        .unwrap()
        .clone()
        .ok_or("No update to install. Check for updates first.")?;
    let (mut done, mut shown) = (0u64, u64::MAX);
    update
        .download_and_install(
            |chunk, total| {
                done += chunk as u64;
                if let Some(total) = total.filter(|t| *t > 0) {
                    let pct = done * 100 / total;
                    if pct != shown {
                        shown = pct;
                        let _ = app.emit("update-progress", pct);
                    }
                }
            },
            || {},
        )
        .await
        .map_err(|e| format!("Update failed: {e}"))
}

/// Check shortly after launch, then twice a day. Failures (offline) are silent.
pub fn start_checker(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_secs(30)).await;
        loop {
            let _ = check(&app).await;
            tokio::time::sleep(CHECK_EVERY).await;
        }
    });
}
