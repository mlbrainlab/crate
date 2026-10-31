//! The Crate Pro sheet and the Crate Pro group in Preferences
//! (`design/canvas/Pro.dc.html`, `app/ui/pro.slint`). Activation runs on a
//! worker thread: it may go to the signer (`license.rs`).

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use slint::ComponentHandle;

use crate::i18n::{tr, trf};
use crate::license::{self, ActivateError, License, PREFIX};
use crate::{PrefsWindow, ProUi};

/// Shown on the Buy button and in Preferences.
const PRICE: &str = "$9";

/// The Lemon Squeezy checkout, set at build time by the release workflow (a
/// repository variable). Builds without it hide the Buy button.
const BUY_URL: Option<&str> = option_env!("CRATE_BUY_URL");

fn buy_url() -> Option<&'static str> {
    BUY_URL.filter(|u| !u.is_empty())
}

pub fn start(ui: &PrefsWindow) {
    let g = ui.global::<ProUi>();
    g.set_price(PRICE.into());
    g.set_can_buy(buy_url().is_some());
    show_status(ui, license::current().as_ref());

    // Bumped by Cancel, so a late answer doesn't reopen anything.
    let attempt = Arc::new(AtomicU32::new(0));

    let weak = ui.as_weak();
    g.on_open(move |page| {
        let Some(ui) = weak.upgrade() else { return };
        let g = ui.global::<ProUi>();
        g.set_error("".into());
        if page == 1 {
            g.set_key("".into());
        }
        g.set_page(page);
        g.set_show(true);
    });
    g.on_buy(|| {
        if let Some(url) = buy_url() {
            let _ = crate::home::open_path(Path::new(url));
        }
    });
    let weak = ui.as_weak();
    let a = attempt.clone();
    g.on_cancel(move || {
        a.fetch_add(1, Ordering::SeqCst);
        if let Some(ui) = weak.upgrade() {
            let g = ui.global::<ProUi>();
            g.set_busy(false);
            g.set_show(false);
        }
    });
    let weak = ui.as_weak();
    g.on_remove(move || {
        license::remove();
        if let Some(ui) = weak.upgrade() {
            show_status(&ui, None);
        }
    });
    let weak = ui.as_weak();
    g.on_activate(move |key| {
        let Some(ui) = weak.upgrade() else { return };
        let g = ui.global::<ProUi>();
        g.set_error("".into());
        g.set_busy(true);
        let mine = attempt.fetch_add(1, Ordering::SeqCst) + 1;
        let attempt = attempt.clone();
        let weak = ui.as_weak();
        let key = key.to_string();
        std::thread::spawn(move || {
            let result = license::activate(&key);
            let _ = weak.upgrade_in_event_loop(move |ui| {
                // A key that went through still counts after Cancel.
                show_status(&ui, license::current().as_ref());
                if attempt.load(Ordering::SeqCst) != mine {
                    return;
                }
                let g = ui.global::<ProUi>();
                g.set_busy(false);
                match result {
                    Ok(_) => g.set_page(2),
                    Err(e) => g.set_error(error_text(&e, &key).into()),
                }
            });
        });
    });
}

fn show_status(ui: &PrefsWindow, license: Option<&License>) {
    let g = ui.global::<ProUi>();
    g.set_active(license.is_some());
    let since = license
        .and_then(License::issued_time)
        .map(crate::model::date_text)
        .map(|date| {
            trf(
                "Since {date} · thank you for supporting Crate",
                &[("date", &date)],
            )
        })
        .unwrap_or_else(|| tr("Thank you for supporting Crate"));
    g.set_since(since.into());
}

fn error_text(e: &ActivateError, key: &str) -> String {
    match e {
        ActivateError::Rejected if key.trim().starts_with(PREFIX) => {
            tr("This key isn’t valid. Check that you copied all of it.")
        }
        ActivateError::Rejected => {
            tr("Lemon Squeezy doesn’t know this key. Check that you copied all of it.")
        }
        ActivateError::Network(_) => {
            tr("Couldn’t reach the license server. Check your connection and try again.")
        }
        ActivateError::Unavailable => tr(
            "This copy of Crate can’t activate store keys. Paste a key that starts with CRATE-PRO.",
        ),
    }
}
