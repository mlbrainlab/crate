//! The Operations panel (Progress.dc.html): running jobs with Pause /
//! Resume / Cancel, the queue, the history, Clear finished; a real
//! extraction ends up in the history. Screenshots 12x-*.png.

mod common;
#[path = "../../crates/crate-core/tests/support/mod.rs"]
mod support;

use std::sync::Arc;
use std::time::Duration;

use common::{Harness, g};
use crate_app::OpsUi;
use crate_app::ops::{Kind, Op, Outcome};
use crate_core::{CancelToken, Progress};
use slint::{ComponentHandle, Model};

fn record(path: &std::path::Path) -> std::io::Result<()> {
    common::OPENED.lock().unwrap().push(path.to_owned());
    Ok(())
}

/// A job "in another window": a thread reporting progress until cancelled.
fn fake_job(op: Arc<Op>, cancel: CancelToken) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let mut done = 3_800_000_000u64;
        while !cancel.is_cancelled() {
            done += 1_000_000;
            op.progress(&Progress {
                bytes_done: done,
                bytes_total: 6_200_000_000,
                files_done: 11_204,
                files_total: 18_204,
                current: "Documents/Projects/brand/exports/hero@3x.png".into(),
            });
            std::thread::sleep(Duration::from_millis(20));
        }
    })
}

#[test]
fn operations_flow() {
    let h = Harness::new();
    let dir = tempfile::tempdir().unwrap();
    let path = |name: &str| dir.path().join(name);

    // A finished job and one that failed, for the history.
    Op::start(
        Kind::Create,
        "Compressing a.zip".into(),
        String::new(),
        CancelToken::new(),
    )
    .finish(
        Outcome::Done,
        "Created project-assets.zip · 812 MB",
        Some(dir.path()),
    );
    Op::start(
        Kind::Convert,
        "Converting old.rar".into(),
        String::new(),
        CancelToken::new(),
    )
    .finish(
        Outcome::Failed,
        "Couldn’t convert old.rar: part 3 of this archive is missing (old.part3.rar)",
        None,
    );
    // Running and queued.
    let cancel = CancelToken::new();
    let running = Op::start(
        Kind::Create,
        "Compressing home-backup-oct.tar.zst".into(),
        "→ /mnt/backup/".into(),
        cancel.clone(),
    );
    let job = fake_job(running.clone(), cancel.clone());
    let queued = Op::queued(
        Kind::Extract,
        "Extract sounds.tar.gz".into(),
        String::new(),
        CancelToken::new(),
    );

    let home = h.home(vec![]);
    home.global::<crate_app::FirstRunUi>().invoke_skip();
    let o = || home.global::<OpsUi>();
    o().invoke_open();
    // (The job writes its progress at most every 250 ms.)
    h.until("filled", || {
        o().get_running()
            .row_data(0)
            .is_some_and(|c| c.files == "11,204 of 18,204")
    });
    assert!(o().get_show() && o().get_busy());
    assert_eq!(o().get_counts(), "1 running · 1 queued");
    let rows: Vec<String> = o().get_rows().iter().map(|r| r.text.to_string()).collect();
    assert_eq!(rows[0], "Extract sounds.tar.gz", "queued first: {rows:?}");
    assert!(
        rows[1].starts_with("Couldn’t convert"),
        "newest history next: {rows:?}"
    );
    let card = o().get_running().row_data(0).unwrap();
    assert_eq!(card.files, "11,204 of 18,204");
    h.shot("120-operations");

    // Pause, then Resume, from the panel.
    o().invoke_pause(card.id.clone());
    h.until("paused", || {
        o().get_running().row_data(0).is_some_and(|c| c.paused)
    });
    h.shot("121-operations-paused");
    o().invoke_resume(card.id.clone());
    h.until("resumed", || {
        o().get_running().row_data(0).is_some_and(|c| !c.paused)
    });
    // Remove the queued one; Cancel the running one.
    o().invoke_remove(queued.id().into());
    assert!(queued.removed());
    o().invoke_cancel(card.id.clone());
    job.join().unwrap();
    assert!(cancel.is_cancelled());
    running.finish(
        Outcome::Cancelled,
        "Cancelled: home-backup-oct.tar.zst",
        None,
    );
    drop(queued);
    h.until("idle", || {
        o().get_running().row_count() == 0 && !o().get_busy()
    });

    // A history row opens what it made.
    crate_app::home::set_opener(record);
    let created = o()
        .get_rows()
        .iter()
        .position(|r| r.text.starts_with("Created"))
        .unwrap();
    assert_eq!(
        o().get_rows().row_data(created).unwrap().action,
        "Show in folder"
    );
    o().invoke_row_action(created as i32);
    assert_eq!(common::OPENED.lock().unwrap().last().unwrap(), dir.path());

    // Clear finished.
    o().invoke_clear_finished();
    assert_eq!(o().get_rows().row_count(), 0);

    // A real extraction in an archive window lands in the history.
    support::sample_zip(&path("project-assets.zip"));
    let ui = h.open(&path("project-assets.zip"));
    h.until("listing", || {
        g(&ui).get_status() == "ready" && !g(&ui).get_streaming()
    });
    g(&ui).invoke_extract();
    h.until("done", || g(&ui).get_show_done());
    let history = crate_app::ops::history();
    assert!(
        history[0]
            .text
            .starts_with("Extracted project-assets.zip → "),
        "{history:?}"
    );
    assert!(crate_app::ops::running().is_empty());
}
