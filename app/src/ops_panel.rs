//! Home's Operations panel (`design/canvas/Progress.dc.html`): reads the
//! registry in `crate::ops` every half second (also for the header's dot)
//! and sends Pause / Resume / Cancel through it.

use std::time::Duration;

use slint::{ComponentHandle, ModelRc, VecModel};

use crate::i18n::{tr, trf, trn};
use crate::ops::{self, History, Kind, Outcome, Snapshot};
use crate::{MainWindow, OpCard, OpRow, OpsUi, model};

thread_local! {
    static TIMER: slint::Timer = slint::Timer::default();
    /// What the rows' actions open, in row order.
    static OPENS: std::cell::RefCell<Vec<Option<std::path::PathBuf>>> = const { std::cell::RefCell::new(Vec::new()) };
}

pub fn connect(ui: &MainWindow) {
    let g = ui.global::<OpsUi>();
    let weak = ui.as_weak();
    g.on_open(move || {
        if let Some(ui) = weak.upgrade() {
            fill(&ui);
            ui.global::<OpsUi>().set_show(true);
        }
    });
    let w = ui.as_weak();
    g.on_pause(move |id| {
        ops::request(&id, "pause");
        later(&w);
    });
    let w = ui.as_weak();
    g.on_resume(move |id| {
        ops::request(&id, "resume");
        later(&w);
    });
    let w = ui.as_weak();
    g.on_cancel(move |id| {
        ops::request(&id, "cancel");
        later(&w);
    });
    let w = ui.as_weak();
    g.on_remove(move |id| {
        ops::request(&id, "cancel");
        if let Some(ui) = w.upgrade() {
            fill(&ui);
        }
    });
    let w = ui.as_weak();
    g.on_pause_all(move || {
        for op in ops::running().iter().filter(|o| !o.queued && !o.paused) {
            ops::request(&op.id, "pause");
        }
        later(&w);
    });
    let w = ui.as_weak();
    g.on_clear_finished(move || {
        ops::clear_history();
        if let Some(ui) = w.upgrade() {
            fill(&ui);
        }
    });
    g.on_row_action(|i| {
        let path = OPENS.with(|o| o.borrow().get(i as usize).cloned().flatten());
        if let Some(p) = path {
            let _ = crate::home::open_path(&p);
        }
    });
    let weak = ui.as_weak();
    TIMER.with(|t| {
        t.start(slint::TimerMode::Repeated, Duration::from_millis(500), move || {
            if let Some(ui) = weak.upgrade() {
                let g = ui.global::<OpsUi>();
                if g.get_show() {
                    fill(&ui);
                } else {
                    g.set_busy(ops::running().iter().any(|o| !o.queued));
                }
            }
        });
    });
}

/// A request takes effect at the job's next progress call: redraw soon.
fn later(weak: &slint::Weak<MainWindow>) {
    let weak = weak.clone();
    slint::Timer::single_shot(Duration::from_millis(350), move || {
        if let Some(ui) = weak.upgrade() {
            fill(&ui);
        }
    });
}

/// Reads the registry and the history into the panel.
pub fn fill(ui: &MainWindow) {
    let all = ops::running();
    let (queued, running): (Vec<&Snapshot>, Vec<&Snapshot>) = all.iter().partition(|o| o.queued);
    let cards: Vec<OpCard> = running.iter().map(|o| card(o)).collect();
    let mut rows: Vec<OpRow> = queued
        .iter()
        .map(|o| OpRow {
            id: o.id.clone().into(),
            text: o.title.clone().into(),
            time: tr("Waiting").into(),
            state: 0,
            action: "".into(),
        })
        .collect();
    let mut opens = vec![None; rows.len()];
    for h in ops::history().into_iter().take(50) {
        rows.push(history_row(&h));
        opens.push(h.open.clone().filter(|_| h.outcome == Outcome::Done));
    }
    OPENS.with(|o| *o.borrow_mut() = opens);
    let mut parts = Vec::new();
    if !running.is_empty() {
        parts.push(trn("{n} running", "{n} running", running.len() as u64));
    }
    if !queued.is_empty() {
        parts.push(trn("{n} queued", "{n} queued", queued.len() as u64));
    }
    let g = ui.global::<OpsUi>();
    g.set_running(ModelRc::new(VecModel::from(cards)));
    g.set_rows(ModelRc::new(VecModel::from(rows)));
    g.set_counts(parts.join(" · ").into());
    g.set_busy(!running.is_empty());
}

fn card(o: &Snapshot) -> OpCard {
    let fraction = o.fraction().clamp(0.0, 1.0);
    let eta = if o.paused || fraction < 0.02 || o.speed == 0 {
        String::new()
    } else {
        let left = o.bytes_total.saturating_sub(o.bytes_done) / o.speed.max(1);
        model::duration_text(Duration::from_secs(left))
            .map(|d| trf("About {time} left", &[("time", &d)]))
            .unwrap_or_default()
    };
    let of = |a: String, b: String| trf("{done} of {total}", &[("done", &a), ("total", &b)]);
    OpCard {
        id: o.id.clone().into(),
        title: o.title.clone().into(),
        detail: o.detail.clone().into(),
        pct: format!("{}%", (fraction * 100.0).floor() as u32).into(),
        fraction,
        eta: eta.into(),
        current: o.current.clone().into(),
        processed: of(model::size_text(o.bytes_done), model::size_text(o.bytes_total)).into(),
        files: of(model::thousands(o.files_done), model::thousands(o.files_total)).into(),
        speed: if o.speed > 0 {
            trf("{size}/s", &[("size", &model::size_text(o.speed))]).into()
        } else {
            "—".into()
        },
        paused: o.paused,
        kind: match o.kind {
            Some(Kind::Test) => 1,
            Some(Kind::Create) => 2,
            Some(Kind::Convert) => 3,
            Some(Kind::Update) => 4,
            _ => 0,
        },
    }
}

fn history_row(h: &History) -> OpRow {
    OpRow {
        id: "".into(),
        text: h.text.clone().into(),
        time: when(h.time).into(),
        state: match h.outcome {
            Outcome::Done => 1,
            Outcome::Failed => 2,
            Outcome::Cancelled => 3,
        },
        action: match (h.outcome, h.kind, &h.open) {
            (Outcome::Done, Kind::Extract, Some(_)) => tr("Open"),
            (Outcome::Done, _, Some(_)) => tr("Show in folder"),
            _ => String::new(),
        }
        .into(),
    }
}

/// "Just now", "5 min ago", "3 h ago", "Yesterday", or the date.
fn when(time: u64) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let ago = now.saturating_sub(time);
    match ago {
        0..60 => tr("Just now"),
        60..3600 => trf("{m} min ago", &[("m", &(ago / 60).to_string())]),
        3600..86_400 => trf("{h} h ago", &[("h", &(ago / 3600).to_string())]),
        86_400..172_800 => tr("Yesterday"),
        _ => model::date_text(std::time::UNIX_EPOCH + Duration::from_secs(time)),
    }
}
