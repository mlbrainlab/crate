use std::cell::RefCell;
use std::path::PathBuf;
use std::time::Duration;

use super::{Defaults, MimeBackend};

/// macOS keeps the native window frame; Home and the full view put its
/// traffic lights in their header ([`overlay_titlebar`]).
pub const CUSTOM_FRAME: bool = false;

/// Height of Crate's header bar (`Header`, `FullBody`): the traffic lights
/// are centred in it.
const HEADER_HEIGHT: f64 = 56.0;
/// Space between the zoom button and the header's first control.
const AFTER_BUTTONS: f64 = 12.0;

/// The window's content runs under a transparent title bar with the title
/// hidden, and the traffic lights move down into the header's centre, at
/// the same distance from the left edge. AppKit lays the buttons out again
/// on every resize and on leaving full screen, so they're placed again on
/// each `Resized` ([`window_event`]).
pub fn overlay_titlebar<C: slint::ComponentHandle + 'static>(
    ui: &C,
    on: bool,
    inset: impl Fn(f32) + 'static,
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
        inset(if on { place_buttons(&ns) as f32 } else { 0.0 });
    });
}

/// A window's winit events, from its event filter: puts the traffic lights
/// back after AppKit moved them.
pub fn window_event(window: &slint::Window, event: &slint::winit_030::winit::event::WindowEvent) {
    use slint::winit_030::WinitWindowAccessor;

    if let slint::winit_030::winit::event::WindowEvent::Resized(_) = event {
        window.with_winit_window(|w| {
            if let Some(ns) = ns_window(w).filter(|ns| is_overlay(ns)) {
                place_buttons(&ns);
            }
        });
    }
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

/// In the overlay title bar, and not in full screen (where AppKit shows the
/// title bar on its own when the pointer reaches the top).
fn is_overlay(ns: &objc2_app_kit::NSWindow) -> bool {
    use objc2_app_kit::NSWindowStyleMask;
    let mask = ns.styleMask();
    mask.contains(NSWindowStyleMask::FullSizeContentView)
        && !mask.contains(NSWindowStyleMask::FullScreen)
}

fn set_overlay(ns: &objc2_app_kit::NSWindow, on: bool) {
    use objc2_app_kit::{NSWindowStyleMask, NSWindowTitleVisibility};

    // The content keeps its size (the window grows or shrinks by the title
    // bar), so the full view's remembered size stays what it was.
    let size = ns.contentView().map(|v| v.frame().size);
    let mut mask = ns.styleMask();
    mask.set(NSWindowStyleMask::FullSizeContentView, on);
    ns.setStyleMask(mask);
    if let Some(size) = size {
        ns.setContentSize(size);
    }
    ns.setTitlebarAppearsTransparent(on);
    ns.setTitleVisibility(if on {
        NSWindowTitleVisibility::Hidden
    } else {
        NSWindowTitleVisibility::Visible
    });
}

/// Centres the traffic lights in the header and returns the width they take
/// from the left edge, with the gap after them. Their container grows to
/// the header's height, or AppKit would clip them.
fn place_buttons(ns: &objc2_app_kit::NSWindow) -> f64 {
    use objc2_app_kit::NSWindowButton;
    use objc2_foundation::NSPoint;

    let buttons = [
        NSWindowButton::CloseButton,
        NSWindowButton::MiniaturizeButton,
        NSWindowButton::ZoomButton,
    ]
    .map(|b| ns.standardWindowButton(b));
    let [Some(close), Some(mini), Some(zoom)] = buttons else {
        return 0.0;
    };
    let size = close.frame().size;
    let step = mini.frame().origin.x - close.frame().origin.x;
    // SAFETY: the buttons are in the window's view hierarchy (the title
    // bar view, inside its container), which outlives this call.
    let container = unsafe { close.superview().and_then(|v| v.superview()) };
    if let Some(container) = container {
        let mut frame = container.frame();
        frame.size.height = HEADER_HEIGHT;
        frame.origin.y = ns.frame().size.height - HEADER_HEIGHT;
        container.setFrame(frame);
    }
    let margin = (HEADER_HEIGHT - size.height) / 2.0;
    for (i, button) in [&close, &mini, &zoom].into_iter().enumerate() {
        button.setFrameOrigin(NSPoint::new(margin + i as f64 * step, margin));
    }
    margin + 2.0 * step + size.width + AFTER_BUTTONS
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
