use std::cell::RefCell;
use std::path::PathBuf;
use std::time::Duration;

use super::{Defaults, MimeBackend};

/// macOS keeps the native title bar (M8).
pub const CUSTOM_FRAME: bool = false;

pub fn select_backend() -> Result<(), slint::PlatformError> {
    slint::BackendSelector::new()
        .backend_name("winit".into())
        .select()?;
    install_open_handler();
    Ok(())
}

thread_local! {
    /// Files Finder sent before anyone asked for them (at launch).
    static OPENED: RefCell<Vec<PathBuf>> = const { RefCell::new(Vec::new()) };
    static ON_OPEN: RefCell<Option<Box<dyn Fn(Vec<PathBuf>)>>> = const { RefCell::new(None) };
}

/// See `platform::take_opened_files`.
pub fn take_opened_files() -> Vec<PathBuf> {
    OPENED.with(|o| std::mem::take(&mut *o.borrow_mut()))
}

/// See `platform::on_open_files`.
pub fn on_open_files(open: impl Fn(Vec<PathBuf>) + 'static) {
    ON_OPEN.with(|o| *o.borrow_mut() = Some(Box::new(open)));
}

fn opened(paths: Vec<PathBuf>) {
    if paths.is_empty() {
        return;
    }
    let handled = ON_OPEN.with(|o| match &*o.borrow() {
        Some(open) => {
            open(paths.clone());
            true
        }
        None => false,
    });
    if !handled {
        OPENED.with(|o| o.borrow_mut().extend(paths));
    }
}

/// Finder doesn't pass files as arguments: it launches the app (or finds it
/// running) and sends an "open documents" Apple Event, which AppKit hands to
/// the app delegate's `application:openURLs:`. winit's delegate has no such
/// method and winit no such event, so add the method to its class, as winit
/// itself does for `sendEvent:`. Runs after the backend made the event loop
/// (and set the delegate), before the loop runs and AppKit sends the event.
fn install_open_handler() {
    use objc2::runtime::{AnyClass, AnyObject, Imp, Sel};
    use objc2::{ffi, sel};
    use objc2_app_kit::NSApplication;
    use objc2_foundation::{MainThreadMarker, NSArray, NSURL};

    unsafe extern "C-unwind" fn open_urls(
        _this: &AnyObject,
        _cmd: Sel,
        _app: &AnyObject,
        urls: &NSArray<NSURL>,
    ) {
        opened(
            urls.to_vec()
                .iter()
                .filter_map(|u| u.to_file_path())
                .collect(),
        );
    }

    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let app = NSApplication::sharedApplication(mtm);
    let Some(delegate) = app.delegate() else {
        return;
    };
    let object: &AnyObject = (*delegate).as_ref();
    let class: *const AnyClass = object.class();
    // SAFETY: the function matches the selector's types (`v@:@@`: no
    // result; self, _cmd, the application, the array of URLs), and
    // class_addMethod leaves the class alone if it has the method already.
    unsafe {
        let imp = std::mem::transmute::<
            unsafe extern "C-unwind" fn(&AnyObject, Sel, &AnyObject, &NSArray<NSURL>),
            Imp,
        >(open_urls);
        ffi::class_addMethod(
            class.cast_mut(),
            sel!(application:openURLs:),
            imp,
            c"v@:@@".as_ptr(),
        );
    }
    // AppKit checks what the delegate answers to when it's set: set it again.
    app.setDelegate(Some(&delegate));
}

/// Time spent before `main`; only measured on Linux for now.
pub fn process_age() -> Option<Duration> {
    None
}

/// Menlo ships with every macOS since 10.6.
pub fn monospace_family() -> Option<String> {
    Some("Menlo".into())
}

/// Default apps come with M8.
pub const SYSTEM_MIME: MimeBackend = MimeBackend {
    support: || Defaults::Unsupported,
    query: |_| None,
    set: |_, _| Err(std::io::Error::other("not supported yet")),
};

/// Dock progress comes with M8.
pub fn taskbar_progress(_window: &slint::Window, _fraction: Option<f32>) {}

/// The user's Downloads or Desktop folder from the system, when it says
/// (`settings::user_dir` reads the XDG user dirs instead).
pub fn known_folder(_key: &str) -> Option<std::path::PathBuf> {
    None
}
