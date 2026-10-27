//! A multi-volume set with a part missing (EdgeCases.dc.html): which parts
//! are there, "Locate part N…", and a watch that reopens the archive as soon
//! as the missing part appears (a download finishing).

use std::path::{Path, PathBuf};
use std::time::Duration;

use crate_core::volumes::VolumeSet;
use slint::{ModelRc, VecModel};

use super::View;
use crate::VolumePart;
use crate::i18n::{tr, trf, trn};
use crate::model;

impl View {
    /// Opening stopped at a missing volume: show the set.
    pub(super) fn missing_volume(&self, name: &str, part: usize, total: Option<usize>) {
        let located = self.state.borrow().volumes.clone();
        let set = VolumeSet::find_with(&self.path, &located);
        let dir = self.path.parent().unwrap_or(Path::new("")).to_owned();
        let mut parts: Vec<VolumePart> = set
            .as_ref()
            .map(|s| s.parts.as_slice())
            .unwrap_or_default()
            .iter()
            .map(|p| {
                let size = std::fs::metadata(p).ok().map(|m| m.len());
                VolumePart {
                    name: file_name(p).into(),
                    size: size
                        .map_or_else(|| tr("not found"), model::size_text)
                        .into(),
                    found: size.is_some(),
                }
            })
            .collect();
        // Past the last part found: add it, marked missing.
        if !parts.iter().any(|p| p.name == name) {
            parts.push(VolumePart {
                name: name.into(),
                size: tr("not found").into(),
                found: false,
            });
        }
        let base = crate_core::destination::folder_name(&self.path);
        let title = match total {
            Some(t) => trf(
                "Part {part} of {total} is missing",
                &[("part", &part.to_string()), ("total", &t.to_string())],
            ),
            None => trf("Part {part} is missing", &[("part", &part.to_string())]),
        };
        let folder = model::display_folder(&dir);
        let text = match total {
            Some(t) => trf(
                &trn(
                    "{name} is split into {n} volume. Crate looked in {folder}.",
                    "{name} is split into {n} volumes. Crate looked in {folder}.",
                    t as u64,
                ),
                &[("name", &base), ("folder", &folder)],
            ),
            None => trf(
                "{name} is split into volumes. Crate looked in {folder}.",
                &[("name", &base), ("folder", &folder)],
            ),
        };
        let hint = trf(
            "Still downloading? Crate retries automatically when {name} appears in this folder.",
            &[("name", name)],
        );
        let locate = trf("Locate part {part}…", &[("part", &part.to_string())]);
        self.g(|g| {
            g.set_status("missing".into());
            g.set_vol_title(title.into());
            g.set_status_text(text.into());
            g.set_vol_parts(ModelRc::new(VecModel::from(parts)));
            g.set_vol_hint(hint.into());
            g.set_vol_locate(locate.into());
        });
        // Watch the folder for the missing part.
        let wanted = dir.join(name);
        self.volume_watch.start(
            slint::TimerMode::Repeated,
            Duration::from_millis(1500),
            move || {
                if wanted.exists() {
                    super::now(|v| v.retry_volumes());
                }
            },
        );
    }

    fn retry_volumes(&self) {
        self.volume_watch.stop();
        self.g(|g| g.set_status("loading".into()));
        let password = self.state.borrow().password.clone();
        self.open(password);
    }

    /// "Locate part N…": the part from another folder.
    pub(super) fn locate_volume(&self) {
        let dir = self.path.parent().map(Path::to_owned);
        let title = self
            .ui
            .upgrade()
            .map(|ui| {
                use slint::ComponentHandle;
                ui.global::<crate::ArchiveUi>().get_vol_locate().to_string()
            })
            .unwrap_or_default();
        std::thread::spawn(move || {
            let mut dialog = rfd::FileDialog::new().set_title(title.trim_end_matches('…'));
            if let Some(dir) = dir {
                dialog = dialog.set_directory(dir);
            }
            if let Some(picked) = dialog.pick_file() {
                super::post(move |v| v.located_volume(picked));
            }
        });
    }

    pub(super) fn located_volume(&self, picked: PathBuf) {
        self.state.borrow_mut().volumes.push(picked);
        self.retry_volumes();
    }
}

fn file_name(p: &Path) -> String {
    p.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}
