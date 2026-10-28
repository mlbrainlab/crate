//! Crate desktop app (Slint UI): the windows and their behavior. Archive work
//! lives in `crate-core`; `main.rs` only parses the command line.

pub mod about;
pub mod batch;
pub mod dirs;
pub mod first_run;
pub mod home;
pub mod i18n;
pub mod instance;
pub mod model;
pub mod ops;
pub mod ops_panel;
pub mod platform;
pub mod prefs;
pub mod preview;
pub mod settings;
pub mod shortcuts;
pub mod tree;
pub mod view;

slint::include_modules!();

use std::time::Duration;

use slint::ComponentHandle;
use slint::winit_030::WinitWindowAccessor;

/// Hands an interactive move to the window manager. The compositor keeps the
/// button release that ends the move, so tell Slint the button is up; otherwise
/// the header keeps the pointer grab and every later click lands on it.
/// The release is sent from a zero-delay timer because Slint ignores events
/// dispatched while it is still handling the pointer move that started the drag.
pub fn begin_window_drag<C: ComponentHandle + 'static>(ui: &C) {
    ui.window().with_winit_window(|w| {
        let _ = w.drag_window();
    });
    let weak = ui.as_weak();
    slint::Timer::single_shot(Duration::ZERO, move || {
        if let Some(ui) = weak.upgrade() {
            ui.window()
                .dispatch_event(slint::platform::WindowEvent::PointerReleased {
                    position: slint::LogicalPosition::new(-1.0, -1.0),
                    button: slint::platform::PointerEventButton::Left,
                });
        }
    });
}
