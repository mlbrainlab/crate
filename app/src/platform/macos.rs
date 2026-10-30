use std::cell::RefCell;
use std::path::PathBuf;
use std::time::Duration;

use super::{Defaults, MimeBackend};

/// macOS keeps the native window frame; Home and the full view put its
/// traffic lights in their header ([`overlay_titlebar`]).
pub const CUSTOM_FRAME: bool = false;

/// Space between the zoom button and the header's first control.
const AFTER_BUTTONS: f64 = 12.0;

/// The window's content runs under a transparent title bar with the title
/// hidden, and an empty unified toolbar makes the bar as tall as a current
/// Mac app's, with the traffic lights centred in it (Electron's
/// "hiddenInset"). AppKit keeps them there through resizing and full
/// screen. `inset` gets the width they take from the left edge, with the
/// gap after them, and the bar's height, which Crate's header takes on so
/// its buttons line up with them; zeros for the plain title bar.
pub fn overlay_titlebar<C: slint::ComponentHandle + 'static>(
    ui: &C,
    on: bool,
    inset: impl Fn(f32, f32) + 'static,
) {
    use slint::winit_030::WinitWindowAccessor;

    // The winit window may not exist yet (a window shown before the event
    // loop runs).
    let ui = ui.clone_strong();
    let _ = slint::spawn_local(async move {
        let Ok(w) = ui.window().winit_window().await else {
            return;
        };
        let Some(ns) = ns_window(&w) else { return };
        set_overlay(&ns, on);
        let (width, height) = if on { measure(&ns) } else { (0.0, 0.0) };
        inset(width as f32, height as f32);
    });
}

fn ns_window(
    w: &slint::winit_030::winit::window::Window,
) -> Option<objc2::rc::Retained<objc2_app_kit::NSWindow>> {
    use objc2_app_kit::NSView;
    use slint::winit_030::winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};

    let RawWindowHandle::AppKit(handle) = w.window_handle().ok()?.as_raw() else {
        return None;
    };
    // SAFETY: winit's AppKit handle is its NSView, alive while `w` is.
    let view: &NSView = unsafe { handle.ns_view.cast().as_ref() };
    view.window()
}

fn set_overlay(ns: &objc2_app_kit::NSWindow, on: bool) {
    use objc2_app_kit::{
        NSTitlebarSeparatorStyle, NSToolbar, NSWindowStyleMask, NSWindowTitleVisibility,
        NSWindowToolbarStyle,
    };
    use objc2_foundation::MainThreadMarker;

    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    // The content keeps its size (the window grows or shrinks by the title
    // bar), so the full view's remembered size stays what it was.
    let size = ns.contentView().map(|v| v.frame().size);
    let mut mask = ns.styleMask();
    mask.set(NSWindowStyleMask::FullSizeContentView, on);
    ns.setStyleMask(mask);
    ns.setTitlebarAppearsTransparent(on);
    ns.setTitleVisibility(if on {
        NSWindowTitleVisibility::Hidden
    } else {
        NSWindowTitleVisibility::Visible
    });
    if on {
        ns.setToolbar(Some(&NSToolbar::new(mtm)));
        ns.setToolbarStyle(NSWindowToolbarStyle::Unified);
        // Crate's header draws its own line.
        ns.setTitlebarSeparatorStyle(NSTitlebarSeparatorStyle::None);
    } else {
        ns.setToolbar(None);
        ns.setToolbarStyle(NSWindowToolbarStyle::Automatic);
        ns.setTitlebarSeparatorStyle(NSTitlebarSeparatorStyle::Automatic);
    }
    if let Some(size) = size {
        ns.setContentSize(size);
    }
}

/// The width from the window's left edge to past the zoom button, and the
/// title bar's height, once AppKit has laid the new bar out.
fn measure(ns: &objc2_app_kit::NSWindow) -> (f64, f64) {
    use objc2_app_kit::NSWindowButton;

    if let Some(content) = ns.contentView() {
        // SAFETY: the content view's superview is the window's frame view,
        // which lives as long as the window.
        if let Some(frame_view) = unsafe { content.superview() } {
            frame_view.layoutSubtreeIfNeeded();
        }
    }
    let height = ns.frame().size.height - ns.contentLayoutRect().size.height;
    let width = ns
        .standardWindowButton(NSWindowButton::ZoomButton)
        .map(|zoom| {
            let rect = zoom.convertRect_toView(zoom.bounds(), None);
            rect.origin.x + rect.size.width + AFTER_BUTTONS
        })
        .unwrap_or(0.0);
    (width, height)
}

pub fn select_backend() -> Result<(), slint::PlatformError> {
    slint::BackendSelector::new()
        .backend_name("winit".into())
        .select()?;
    install_open_handler();
    Ok(())
}

/// What `on_open_files` was given.
type OpenFiles = Box<dyn Fn(Vec<PathBuf>)>;

thread_local! {
    /// Files Finder sent before anyone asked for them (at launch).
    static OPENED: RefCell<Vec<PathBuf>> = const { RefCell::new(Vec::new()) };
    static ON_OPEN: RefCell<Option<OpenFiles>> = const { RefCell::new(None) };
    /// The progress bar on the Dock icon while there is one.
    static DOCK_BAR: RefCell<Option<objc2::rc::Retained<objc2_app_kit::NSProgressIndicator>>> =
        const { RefCell::new(None) };
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

/// Default apps through NSWorkspace (macOS 12+), keyed by the file types
/// (UTIs) the bundle's Info.plist declares. Setting one makes macOS ask the
/// user to confirm; the answer comes later, so the result isn't waited for.
pub const SYSTEM_MIME: MimeBackend = MimeBackend {
    support: || {
        if in_bundle() {
            Defaults::Confirm
        } else {
            Defaults::NotInstalled
        }
    },
    query: |mime| {
        use objc2_app_kit::NSWorkspace;
        use objc2_foundation::NSBundle;

        let uti = content_type(mime)?;
        let app = NSWorkspace::sharedWorkspace().URLForApplicationToOpenContentType(&uti)?;
        let id = NSBundle::bundleWithURL(&app)?.bundleIdentifier()?;
        (id.to_string() == super::APP_ID).then(|| format!("{}.desktop", super::APP_ID))
    },
    set: |_, mimes| {
        use objc2_app_kit::NSWorkspace;
        use objc2_foundation::NSBundle;

        if !in_bundle() {
            return Err(std::io::Error::other("Crate isn’t running from Crate.app"));
        }
        let app = NSBundle::mainBundle().bundleURL();
        let workspace = NSWorkspace::sharedWorkspace();
        let mut done = Vec::new();
        for uti in mimes.iter().filter_map(|m| content_type(m)) {
            let id = uti.identifier().to_string();
            if done.contains(&id) {
                continue;
            }
            workspace
                .setDefaultApplicationAtURL_toOpenContentType_completionHandler(&app, &uti, None);
            done.push(id);
        }
        Ok(())
    },
};

/// Running from Crate.app (not `cargo run`): only the bundle has the
/// identifier, and only the bundle can be a default app.
fn in_bundle() -> bool {
    objc2_foundation::NSBundle::mainBundle()
        .bundleIdentifier()
        .is_some_and(|id| id.to_string() == super::APP_ID)
}

/// The file type (UTI) for a MIME type, as declared in
/// packaging/macos/Info.plist; several MIME types share one.
fn content_type(mime: &str) -> Option<objc2::rc::Retained<objc2_uniform_type_identifiers::UTType>> {
    let id = match mime {
        "application/zip" | "application/x-zip-compressed" => "com.pkware.zip-archive",
        "application/x-7z-compressed" => "org.7-zip.7-zip-archive",
        "application/vnd.rar" | "application/x-rar" | "application/x-rar-compressed" => {
            "com.rarlab.rar-archive"
        }
        "application/x-compressed-tar" => "org.gnu.gnu-zip-tar-archive",
        "application/x-tar" | "application/x-gtar" => "public.tar-archive",
        "application/x-xz-compressed-tar" => "org.tukaani.tar-xz-archive",
        "application/x-zstd-compressed-tar" => "io.github.mlbrainlab.crate.tar-zst-archive",
        "application/x-bzip2-compressed-tar" | "application/x-bzip-compressed-tar" => {
            "io.github.mlbrainlab.crate.tar-bz2-archive"
        }
        _ => return None,
    };
    objc2_uniform_type_identifiers::UTType::typeWithIdentifier(
        &objc2_foundation::NSString::from_str(id),
    )
}

/// Progress on the Dock icon: a fraction from 0 to 1, or `None` to clear
/// it. Each Crate window is its own process, so the icon is this window's.
/// The tile shows the app icon with a progress bar along its bottom.
pub fn taskbar_progress(_window: &slint::Window, fraction: Option<f32>) {
    use objc2::MainThreadOnly;
    use objc2_app_kit::{
        NSApplication, NSImageView, NSProgressIndicator, NSProgressIndicatorStyle,
    };
    use objc2_foundation::{MainThreadMarker, NSPoint, NSRect, NSSize};

    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let tile = NSApplication::sharedApplication(mtm).dockTile();
    DOCK_BAR.with(|bar| {
        let mut bar = bar.borrow_mut();
        match fraction {
            Some(f) => {
                let indicator = bar.get_or_insert_with(|| {
                    let app = NSApplication::sharedApplication(mtm);
                    let size = tile.size();
                    let view = match app.applicationIconImage() {
                        Some(icon) => NSImageView::imageViewWithImage(&icon, mtm),
                        None => NSImageView::initWithFrame(
                            NSImageView::alloc(mtm),
                            NSRect::new(NSPoint::new(0.0, 0.0), size),
                        ),
                    };
                    view.setFrame(NSRect::new(NSPoint::new(0.0, 0.0), size));
                    // A bar across the bottom, inside the icon's margin.
                    let inset = size.width * 0.1;
                    let indicator = NSProgressIndicator::initWithFrame(
                        NSProgressIndicator::alloc(mtm),
                        NSRect::new(
                            NSPoint::new(inset, size.height * 0.06),
                            NSSize::new(size.width - 2.0 * inset, size.height * 0.12),
                        ),
                    );
                    indicator.setStyle(NSProgressIndicatorStyle::Bar);
                    indicator.setIndeterminate(false);
                    indicator.setMinValue(0.0);
                    indicator.setMaxValue(1.0);
                    view.addSubview(&indicator);
                    tile.setContentView(Some(&view));
                    indicator
                });
                indicator.setDoubleValue(f.clamp(0.0, 1.0).into());
            }
            None => {
                if bar.take().is_none() {
                    return;
                }
                tile.setContentView(None);
            }
        }
        tile.display();
    });
}

/// The user's Downloads or Desktop folder from the system, when it says
/// (`settings::user_dir` reads the XDG user dirs instead).
pub fn known_folder(_key: &str) -> Option<std::path::PathBuf> {
    None
}
