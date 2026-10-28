use std::time::Duration;

use super::{Defaults, MimeBackend};

/// The native title bar for now (the custom frame is its own M7 task).
pub const CUSTOM_FRAME: bool = false;

pub fn select_backend() -> Result<(), slint::PlatformError> {
    slint::BackendSelector::new()
        .backend_name("winit".into())
        .select()
}

/// Time spent before `main`; only measured on Linux.
pub fn process_age() -> Option<Duration> {
    None
}

/// Consolas ships with every Windows since Vista.
pub fn monospace_family() -> Option<String> {
    Some("Consolas".into())
}

/// Default apps: Windows doesn't let apps choose themselves, so "making
/// Crate the default" opens Settings → Default apps on Crate's page (the
/// installer registers it as "Crate", see packaging/windows/crate.iss).
pub const SYSTEM_MIME: MimeBackend = MimeBackend {
    support: || {
        if installed() {
            Defaults::SettingsPage
        } else {
            Defaults::NotInstalled
        }
    },
    query: |mime| {
        let ext = extension(mime)?;
        let key = format!(
            r"Software\Microsoft\Windows\CurrentVersion\Explorer\FileExts\{ext}\UserChoice"
        );
        let prog_id = reg_string(HKEY_CURRENT_USER, &key, "ProgId")?;
        (prog_id == PROG_ID).then(|| format!("{}.desktop", super::APP_ID))
    },
    set: |_, _| open::that_detached("ms-settings:defaultapps?registeredAppUser=Crate"),
};

use ::windows::Win32::System::Registry::{HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};

/// The installer's ProgID for archives.
const PROG_ID: &str = "Crate.Archive";

/// Installed for this user or for everyone.
fn installed() -> bool {
    [HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE]
        .into_iter()
        .any(|root| reg_string(root, r"Software\RegisteredApplications", "Crate").is_some())
}

/// The extension Explorer keys the choice by, for a type's main MIME type.
fn extension(mime: &str) -> Option<&'static str> {
    Some(match mime {
        "application/zip" => ".zip",
        "application/x-7z-compressed" => ".7z",
        "application/vnd.rar" => ".rar",
        "application/x-compressed-tar" => ".tgz",
        _ => return None,
    })
}

/// A string value from the registry.
fn reg_string(root: HKEY, key: &str, value: &str) -> Option<String> {
    use ::windows::Win32::Foundation::ERROR_SUCCESS;
    use ::windows::Win32::System::Registry::{RRF_RT_REG_SZ, RegGetValueW};
    use ::windows::core::HSTRING;

    let (key, value) = (HSTRING::from(key), HSTRING::from(value));
    let mut buf = [0u16; 512];
    let mut size = std::mem::size_of_val(&buf) as u32;
    // SAFETY: the buffer and its size in bytes match.
    let result = unsafe {
        RegGetValueW(
            root,
            &key,
            &value,
            RRF_RT_REG_SZ,
            None,
            Some(buf.as_mut_ptr().cast()),
            Some(&mut size),
        )
    };
    if result != ERROR_SUCCESS {
        return None;
    }
    // The size includes the terminating NUL.
    let len = (size as usize / 2).saturating_sub(1);
    Some(String::from_utf16_lossy(&buf[..len]))
}

/// Progress on the window's taskbar button: a fraction from 0 to 1, or
/// `None` to clear it. Failures are ignored (no taskbar, Explorer
/// restarting).
pub fn taskbar_progress(window: &slint::Window, fraction: Option<f32>) {
    use ::windows::Win32::Foundation::HWND;
    use ::windows::Win32::UI::Shell::{TBPF_NOPROGRESS, TBPF_NORMAL};
    use slint::winit_030::WinitWindowAccessor;
    use slint::winit_030::winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};

    window.with_winit_window(|w| {
        let Ok(handle) = w.window_handle() else {
            return;
        };
        let RawWindowHandle::Win32(h) = handle.as_raw() else {
            return;
        };
        let hwnd = HWND(h.hwnd.get() as *mut _);
        let Some(taskbar) = taskbar() else {
            return;
        };
        // SAFETY: COM calls on the UI thread with a live window handle.
        unsafe {
            match fraction {
                Some(f) => {
                    let done = (f.clamp(0.0, 1.0) * 1000.0) as u64;
                    let _ = taskbar.SetProgressState(hwnd, TBPF_NORMAL);
                    let _ = taskbar.SetProgressValue(hwnd, done, 1000);
                }
                None => {
                    let _ = taskbar.SetProgressState(hwnd, TBPF_NOPROGRESS);
                }
            }
        }
    });
}

thread_local! {
    static TASKBAR: std::cell::OnceCell<Option<::windows::Win32::UI::Shell::ITaskbarList3>> =
        const { std::cell::OnceCell::new() };
}

/// The taskbar's COM object, made once per (UI) thread.
fn taskbar() -> Option<::windows::Win32::UI::Shell::ITaskbarList3> {
    use ::windows::Win32::System::Com::{
        CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx,
    };
    use ::windows::Win32::UI::Shell::{ITaskbarList3, TaskbarList};

    TASKBAR.with(|t| {
        t.get_or_init(|| {
            // SAFETY: plain COM setup; winit has usually initialized COM on
            // this thread already, which makes this a no-op.
            unsafe {
                let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
                let list: ITaskbarList3 =
                    CoCreateInstance(&TaskbarList, None, CLSCTX_INPROC_SERVER).ok()?;
                list.HrInit().ok()?;
                Some(list)
            }
        })
        .clone()
    })
}

/// The user's Downloads (`DOWNLOAD`) or Desktop (`DESKTOP`) folder from the
/// shell, which knows when it was moved (to OneDrive, another drive).
pub fn known_folder(key: &str) -> Option<std::path::PathBuf> {
    use ::windows::Win32::System::Com::CoTaskMemFree;
    use ::windows::Win32::UI::Shell::{
        FOLDERID_Desktop, FOLDERID_Downloads, KF_FLAG_DEFAULT, SHGetKnownFolderPath,
    };

    let id = match key {
        "DOWNLOAD" => &FOLDERID_Downloads,
        "DESKTOP" => &FOLDERID_Desktop,
        _ => return None,
    };
    // SAFETY: the shell allocates the string; it's copied, then freed.
    unsafe {
        let path = SHGetKnownFolderPath(id, KF_FLAG_DEFAULT, None).ok()?;
        let text = path.to_string().ok();
        CoTaskMemFree(Some(path.0 as *const _));
        text.map(std::path::PathBuf::from)
    }
}
