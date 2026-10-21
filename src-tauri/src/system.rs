//! Windows integration: setting the desktop wallpaper, screen size, and autostart.

use std::os::windows::ffi::OsStrExt;
use std::path::Path;

use windows_sys::Win32::UI::WindowsAndMessaging::{
    GetSystemMetrics, SystemParametersInfoW, SM_CXSCREEN, SM_CXVIRTUALSCREEN, SPIF_SENDCHANGE,
    SPIF_UPDATEINIFILE, SPI_SETDESKWALLPAPER,
};
use winreg::enums::{HKEY_CURRENT_USER, KEY_SET_VALUE};
use winreg::RegKey;

const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const APP_NAME: &str = "Wallflow";
pub const AUTOSTART_ARG: &str = "--autostart";

pub fn screen_width(span: bool) -> u32 {
    let w = unsafe {
        GetSystemMetrics(if span {
            SM_CXVIRTUALSCREEN
        } else {
            SM_CXSCREEN
        })
    };
    if w > 0 {
        w as u32
    } else {
        2560
    }
}

/// (WallpaperStyle, TileWallpaper) registry values for each fit mode.
fn style_values(fit: &str) -> (&'static str, &'static str) {
    match fit {
        "fit" => ("6", "0"),
        "stretch" => ("2", "0"),
        "center" => ("0", "0"),
        "tile" => ("0", "1"),
        "span" => ("22", "0"),
        _ => ("10", "0"), // fill
    }
}

pub fn set_wallpaper(path: &Path, fit: &str) -> Result<(), String> {
    let (style, tile) = style_values(fit);
    if let Ok((desk, _)) = RegKey::predef(HKEY_CURRENT_USER).create_subkey(r"Control Panel\Desktop")
    {
        let _ = desk.set_value("WallpaperStyle", &style);
        let _ = desk.set_value("TileWallpaper", &tile);
    }
    // IDesktopWallpaper is what the Settings app uses: unlike the legacy SPI call it
    // also switches the background mode to "Picture" when Spotlight/slideshow is active.
    let (p, f) = (path.to_path_buf(), fit.to_string());
    match std::thread::spawn(move || set_via_com(&p, &f)).join() {
        Ok(Ok(())) => {}
        Ok(Err(e)) => {
            return Err(format!(
                "Windows couldn't apply the wallpaper: {}",
                e.message()
            ))
        }
        Err(_) => set_via_spi(path)?,
    }
    // Leave Spotlight / slideshow mode so it doesn't replace our image later.
    if let Ok((wp, _)) = RegKey::predef(HKEY_CURRENT_USER)
        .create_subkey(r"Software\Microsoft\Windows\CurrentVersion\Explorer\Wallpapers")
    {
        let _ = wp.set_value("BackgroundType", &0u32);
    }
    Ok(())
}

fn set_via_com(path: &Path, fit: &str) -> windows::core::Result<()> {
    use windows::core::{HSTRING, PCWSTR};
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_ALL, COINIT_APARTMENTTHREADED,
    };
    use windows::Win32::UI::Shell::{
        DesktopWallpaper, IDesktopWallpaper, DWPOS_CENTER, DWPOS_FILL, DWPOS_FIT, DWPOS_SPAN,
        DWPOS_STRETCH, DWPOS_TILE,
    };

    let pos = match fit {
        "fit" => DWPOS_FIT,
        "stretch" => DWPOS_STRETCH,
        "center" => DWPOS_CENTER,
        "tile" => DWPOS_TILE,
        "span" => DWPOS_SPAN,
        _ => DWPOS_FILL,
    };
    unsafe {
        let init = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let result = (|| {
            let dw: IDesktopWallpaper = CoCreateInstance(&DesktopWallpaper, None, CLSCTX_ALL)?;
            dw.SetPosition(pos)?;
            let file = HSTRING::from(path.as_os_str());
            // Set each monitor explicitly; a null monitor id is unreliable while Spotlight is active.
            let n = dw.GetMonitorDevicePathCount()?;
            if n == 0 {
                return dw.SetWallpaper(PCWSTR::null(), &file);
            }
            for i in 0..n {
                let id = dw.GetMonitorDevicePathAt(i)?;
                dw.SetWallpaper(PCWSTR(id.0), &file)?;
            }
            Ok(())
        })();
        if init.is_ok() {
            CoUninitialize();
        }
        result
    }
}

fn set_via_spi(path: &Path) -> Result<(), String> {
    let mut wide: Vec<u16> = path.as_os_str().encode_wide().collect();
    wide.push(0);
    let ok = unsafe {
        SystemParametersInfoW(
            SPI_SETDESKWALLPAPER,
            0,
            wide.as_mut_ptr() as *mut _,
            SPIF_UPDATEINIFILE | SPIF_SENDCHANGE,
        )
    };
    if ok == 0 {
        Err(format!(
            "Windows refused to set the wallpaper: {}",
            std::io::Error::last_os_error()
        ))
    } else {
        Ok(())
    }
}

pub fn set_autostart(enabled: bool) -> Result<(), String> {
    let run = RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey_with_flags(RUN_KEY, KEY_SET_VALUE)
        .map_err(|e| e.to_string())?;
    if enabled {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        run.set_value(APP_NAME, &format!("\"{}\" {AUTOSTART_ARG}", exe.display()))
            .map_err(|e| e.to_string())
    } else {
        match run.delete_value(APP_NAME) {
            Ok(_) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.to_string()),
        }
    }
}

pub fn open_url(url: &str) {
    if url.starts_with("https://") {
        let _ = std::process::Command::new("rundll32")
            .args(["url.dll,FileProtocolHandler", url])
            .spawn();
    }
}
