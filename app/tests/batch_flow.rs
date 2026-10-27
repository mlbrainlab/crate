//! Batch extract (Batch.dc.html): several archives from Home, each into its
//! own folder or all into one; one needs a password, one is damaged.
//! Screenshots 100-*.png.

mod common;
#[path = "../../crates/crate-core/tests/support/mod.rs"]
mod support;

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use common::Harness;
use crate_app::{BatchState, BatchUi};
use slint::{ComponentHandle, Model};

static OPENED: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());

fn record(path: &Path) -> std::io::Result<()> {
    OPENED.lock().unwrap().push(path.to_owned());
    Ok(())
}

fn states(home: &crate_app::MainWindow) -> Vec<BatchState> {
    home.global::<BatchUi>()
        .get_items()
        .iter()
        .map(|i| i.state)
        .collect()
}

fn settled(home: &crate_app::MainWindow) -> bool {
    !states(home)
        .iter()
        .any(|s| matches!(s, BatchState::Waiting | BatchState::Running))
}

#[test]
fn batch_flow() {
    let h = Harness::new();
    crate_app::home::set_opener(record);
    let dir = tempfile::tempdir().unwrap();
    let path = |name: &str| dir.path().join(name);
    support::sample_zip(&path("a.zip"));
    support::sample_zip(&path("b.zip"));
    support::encrypted_zip(&path("secret.zip"));
    support::truncated_tar_gz(&path("cut.tar.gz"));

    let home = h.home(vec![]);
    home.global::<crate_app::FirstRunUi>().invoke_skip();
    let b = || home.global::<BatchUi>();
    let archives = ["a.zip", "b.zip", "secret.zip", "cut.tar.gz"].map(path).to_vec();
    crate_app::batch::start(&home, archives, false);

    // Ready: nothing written until Extract.
    assert!(b().get_show() && !b().get_started());
    assert_eq!(b().get_title(), "Extract 4 archives");
    h.shot("100-batch-ready");
    b().invoke_start();
    h.until("settled", || settled(&home));
    use BatchState::*;
    assert_eq!(states(&home), [Done, Done, NeedsPassword, Failed]);
    // a.zip holds one folder, extracted as it is; b.zip's would clash, so it
    // gets a folder of its own.
    assert!(path("project/README.md").is_file());
    assert!(path("b/project/README.md").is_file());
    let status = |i: usize| b().get_items().row_data(i).unwrap().status.to_string();
    assert_eq!(status(0), "Done · single folder, no wrapper added");
    assert!(b().get_any_needs() && !b().get_finished());
    assert_eq!(b().get_counts(), "2 done · 1 needs you · 1 failed");
    h.shot("101-batch-problems");

    // The password: wrong, then right.
    b().invoke_row_action(2);
    assert_eq!(b().get_pw_row(), 2);
    h.shot("102-batch-password");
    b().set_pw_text("nope".into());
    b().invoke_submit_password();
    h.until("tried", || settled(&home));
    assert_eq!(states(&home)[2], NeedsPassword);
    assert_eq!(status(2), "Wrong password");
    b().invoke_row_action(2);
    b().set_pw_text("secret".into());
    b().invoke_submit_password();
    h.until("unlocked", || states(&home)[2] == Done);
    assert!(path("secret/secret.txt").is_file());

    // The damaged one: skip it, and the batch is over.
    b().invoke_skip_problems();
    assert!(b().get_finished());
    assert_eq!(b().get_title(), "Extracted 3 of 4 archives");
    h.shot("103-batch-done");
    b().invoke_row_action(0);
    assert_eq!(OPENED.lock().unwrap().last().unwrap(), &path("project"));
    b().invoke_close();
    assert!(!b().get_show());

    // All into one folder: "Extracted", next to the first archive.
    let two = tempfile::tempdir().unwrap();
    let p2 = |name: &str| two.path().join(name);
    support::sample_zip(&p2("x.zip"));
    support::zipcrypto_zip(&p2("y.zip"));
    crate_app::batch::start(&home, vec![p2("x.zip"), p2("y.zip")], false);
    b().invoke_set_mode(1);
    b().invoke_start();
    h.until("settled", || settled(&home));
    assert_eq!(states(&home), [Done, NeedsPassword]);
    b().invoke_row_action(1);
    b().set_pw_text("secret".into());
    b().invoke_submit_password();
    h.until("done", || b().get_finished());
    assert!(p2("Extracted/project/README.md").is_file());
    assert!(p2("Extracted/secret.txt").is_file());
    b().invoke_open_folder();
    assert_eq!(OPENED.lock().unwrap().last().unwrap(), &p2("Extracted"));
    h.shot("104-batch-one-folder");

    // Arabic: mirrored columns.
    let mut st = crate_app::settings::Settings::load();
    st.set("pref.language", "ar");
    st.save();
    let home = h.home(vec![]);
    crate_app::batch::start(&home, vec![p2("x.zip"), p2("y.zip")], false);
    assert_eq!(home.global::<BatchUi>().get_title(), "استخراج أرشيفين");
    h.shot("105-ar-batch");
}
