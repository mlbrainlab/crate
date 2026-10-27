//! About & support (`design/canvas/About.dc.html`): the About sheet on Home,
//! the project links, and the one-time support nudge after enough
//! successful operations. Every window is its own process, so the count of
//! operations lives in `state.ini` (`support.*` keys).

use std::path::Path;

use slint::ComponentHandle;

use crate::i18n::{trf, trn};
use crate::settings::Settings;
use crate::{AboutUi, MainWindow};

const SOURCE: &str = "https://github.com/mlbrainlab/crate";

/// Donation links. Empty until the accounts exist (see `.github/FUNDING.yml`,
/// which should list the same ones): an empty link hides its button, and
/// with none the Support box and the nudge stay hidden.
pub const SPONSOR: &str = "";
pub const PATREON: &str = "";
pub const PAYPAL: &str = "";

/// The nudge comes after this many successful operations, and again this
/// many after "Maybe later".
const NUDGE_EVERY: u64 = 50;

/// One successful extract, test, create, convert or update. Best effort.
pub fn count_success() {
    let mut s = Settings::load();
    let ops = s.get::<u64>("support.ops").unwrap_or(0);
    s.set("support.ops", ops + 1);
    s.save();
}

/// The number of operations to mention, if the nudge is due.
fn nudge_due(s: &Settings) -> Option<u64> {
    if s.get::<u8>("support.never") == Some(1) {
        return None;
    }
    let ops = s.get::<u64>("support.ops").unwrap_or(0);
    let next = s.get::<u64>("support.next").unwrap_or(NUDGE_EVERY);
    (ops >= next).then_some(ops)
}

fn link(which: &str) -> String {
    match which {
        "sponsor" => SPONSOR.to_owned(),
        "patreon" => PATREON.to_owned(),
        "paypal" => PAYPAL.to_owned(),
        "issues" => format!("{SOURCE}/issues/new"),
        "releases" => format!("{SOURCE}/releases"),
        _ => SOURCE.to_owned(),
    }
}

fn platform() -> &'static str {
    if cfg!(target_os = "macos") {
        "macOS"
    } else if cfg!(windows) {
        "Windows"
    } else {
        "Linux"
    }
}

/// Wires the About sheet and the nudge of Home; shows the nudge if it's due.
pub fn connect(ui: &MainWindow) {
    let version = env!("CARGO_PKG_VERSION");
    let g = ui.global::<AboutUi>();
    g.set_sponsor(SPONSOR.into());
    g.set_patreon(PATREON.into());
    g.set_paypal(PAYPAL.into());

    let weak = ui.as_weak();
    g.on_open(move || {
        let Some(ui) = weak.upgrade() else { return };
        let g = ui.global::<AboutUi>();
        g.set_version(
            trf(
                "Version {version} · {platform}",
                &[("version", version), ("platform", platform())],
            )
            .into(),
        );
        g.set_whats_new(trf("What’s new in {version}", &[("version", version)]).into());
        g.set_show(true);
    });
    g.on_open_link(|which| {
        let url = link(&which);
        if !url.is_empty() {
            let _ = crate::home::open_path(Path::new(&url));
        }
    });

    let weak = ui.as_weak();
    let close_nudge = move |key: &str, value: u64| {
        let mut s = Settings::load();
        s.set(key, value);
        s.save();
        if let Some(ui) = weak.upgrade() {
            ui.global::<AboutUi>().set_nudge(false);
        }
    };
    let close = close_nudge.clone();
    let weak = ui.as_weak();
    g.on_nudge_support(move || {
        close("support.never", 1);
        if let Some(ui) = weak.upgrade() {
            ui.global::<AboutUi>().invoke_open();
        }
    });
    let close = close_nudge.clone();
    g.on_nudge_later(move || {
        let ops = Settings::load().get::<u64>("support.ops").unwrap_or(0);
        close("support.next", ops + NUDGE_EVERY);
    });
    g.on_nudge_never(move || close_nudge("support.never", 1));

    if let Some(ops) = nudge_due(&Settings::load()) {
        g.set_nudge_title(
            trn(
                "Crate has handled {n} archive for you",
                "Crate has handled {n} archives for you",
                ops,
            )
            .into(),
        );
        g.set_nudge(true);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nudge_timing() {
        let mut s = Settings::default();
        assert_eq!(nudge_due(&s), None);
        s.set("support.ops", 49);
        assert_eq!(nudge_due(&s), None);
        s.set("support.ops", 50);
        assert_eq!(nudge_due(&s), Some(50));
        s.set("support.next", 100);
        assert_eq!(nudge_due(&s), None, "maybe later");
        s.set("support.ops", 120);
        assert_eq!(nudge_due(&s), Some(120));
        s.set("support.never", 1);
        assert_eq!(nudge_due(&s), None);
        assert!(link("issues").ends_with("/issues/new"));
    }
}
