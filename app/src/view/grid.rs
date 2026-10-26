//! The full view's grid (Grid.dc.html): tiles in rows as wide as the window
//! allows, thumbnails made only for tiles that come into view and cached
//! for the session. Folders that are mostly photos and videos switch to
//! the grid by themselves.

use crate::i18n::{trf, trn};
use std::rc::Rc;

use slint::{Image, Model, ModelRc, Rgba8Pixel, SharedPixelBuffer, VecModel};

use crate_core::CancelToken;

use super::{View, post};
use crate::tree::NodeId;
use crate::{Tile, TileRow, model, preview};

const TILE_MIN: f32 = 168.0;
const GAP: f32 = 10.0;
const THUMB_SIDE: u32 = 256;
/// Thumbnails per background read; tar has to scan the stream each time.
const THUMB_BATCH: usize = 32;

impl View {
    /// True when the folder on screen should be a grid, and whether that is
    /// the automatic choice (so the banner can say why).
    pub(super) fn grid_choice(&self) -> (bool, bool) {
        let s = self.state.borrow();
        if !s.query.trim().is_empty() {
            return (s.grid_user, false);
        }
        let auto = s.prefs.grid_for_media
            && !s.list_here.contains(&s.folder)
            && self.is_media_folder(&s.full_rows);
        (s.grid_user || auto, auto && !s.grid_user)
    }

    /// At least 4 files, and 4 in 5 of them pictures or videos.
    fn is_media_folder(&self, rows: &[NodeId]) -> bool {
        let s = self.state.borrow();
        let Some(tree) = &s.tree else { return false };
        let files: Vec<_> = rows
            .iter()
            .map(|&n| tree.node(n))
            .filter(|n| !n.is_dir)
            .collect();
        let media = files
            .iter()
            .filter(|n| {
                matches!(
                    model::file_kind(&n.name).1,
                    model::Tile::Image | model::Tile::Media
                )
            })
            .count();
        files.len() >= 4 && media * 5 >= files.len() * 4
    }

    /// Called at the end of a full-view refresh.
    pub(super) fn refresh_grid(&self) {
        let (grid, auto) = self.grid_choice();
        let banner = if auto && !self.state.borrow().banner_ok {
            let s = self.state.borrow();
            let tree = s.tree.as_ref().expect("full view has a tree");
            let files = s
                .full_rows
                .iter()
                .filter(|&&n| !tree.node(n).is_dir)
                .count();
            let media = s
                .full_rows
                .iter()
                .filter(|&&n| {
                    let k = model::file_kind(&tree.node(n).name).1;
                    matches!(k, model::Tile::Image | model::Tile::Media)
                })
                .count();
            trf(
                &trn(
                    "Showing as grid because {media} of {n} item here are photos or videos.",
                    "Showing as grid because {media} of {n} items here are photos or videos.",
                    files as u64,
                ),
                &[("media", &media.to_string())],
            )
        } else {
            String::new()
        };
        self.g(|g| {
            g.set_grid(grid);
            g.set_grid_banner(banner.into());
        });
        if grid {
            self.rebuild_grid();
        } else {
            self.state.borrow_mut().tile_models.clear();
            self.g(|g| g.set_grid_rows(ModelRc::default()));
        }
    }

    fn rebuild_grid(&self) {
        let (rows, cols) = self.grid_rows();
        // State is free again here: new tiles may ask for thumbnails at once.
        self.g(|g| {
            g.set_grid_columns(cols);
            g.set_grid_rows(ModelRc::new(VecModel::from(rows)));
        });
    }

    fn grid_rows(&self) -> (Vec<TileRow>, i32) {
        let mut guard = self.state.borrow_mut();
        let s = &mut *guard;
        let tree = s.tree.as_ref().expect("full view has a tree");
        let cols = s.grid_columns.max(1);
        let mut rows = Vec::new();
        s.tile_models.clear();
        s.tile_at.clear();
        for (r, chunk) in s.full_rows.chunks(cols).enumerate() {
            let tiles: Vec<Tile> = chunk
                .iter()
                .enumerate()
                .map(|(c, &n)| {
                    let index = r * cols + c;
                    let node = tree.node(n);
                    let (ext, kind) = if node.is_dir {
                        (String::new(), model::Tile::Folder)
                    } else {
                        model::file_kind(&node.name)
                    };
                    let thumb = node.entry.and_then(|e| s.thumbs.get(&e)).cloned();
                    if let Some(e) = node.entry {
                        s.tile_at.insert(e, (r, c));
                    }
                    Tile {
                        index: index as i32,
                        name: node.name.clone().into(),
                        meta: if node.is_dir {
                            model::count(node.items, "item")
                        } else {
                            model::size_text(node.size)
                        }
                        .into(),
                        ext: ext.into(),
                        kind: kind as i32,
                        selected: s
                            .row_paths
                            .get(index)
                            .is_some_and(|p| s.selection.selected.contains(p)),
                        has_thumb: thumb.is_some(),
                        thumb: thumb.unwrap_or_default(),
                    }
                })
                .collect();
            let model = Rc::new(VecModel::from(tiles));
            s.tile_models.push(model.clone());
            rows.push(TileRow {
                tiles: model.into(),
            });
        }
        (rows, cols as i32)
    }

    /// Selected flags on the tiles after a selection change.
    pub(super) fn sync_grid_selection(&self) {
        let s = self.state.borrow();
        for model in &s.tile_models {
            for i in 0..model.row_count() {
                let Some(mut tile) = model.row_data(i) else {
                    continue;
                };
                let want = s
                    .row_paths
                    .get(tile.index as usize)
                    .is_some_and(|p| s.selection.selected.contains(p));
                if tile.selected != want {
                    tile.selected = want;
                    model.set_row_data(i, tile);
                }
            }
        }
    }

    pub(super) fn set_grid(&self, on: bool) {
        {
            let mut s = self.state.borrow_mut();
            s.grid_user = on;
            let folder = s.folder;
            if on {
                s.list_here.remove(&folder);
            } else {
                s.list_here.insert(folder);
            }
        }
        self.refresh_grid();
    }

    pub(super) fn grid_banner_list(&self) {
        let folder = self.state.borrow().folder;
        self.state.borrow_mut().list_here.insert(folder);
        self.refresh_grid();
    }

    pub(super) fn grid_banner_ok(&self) {
        self.state.borrow_mut().banner_ok = true;
        self.g(|g| g.set_grid_banner("".into()));
    }

    /// The grid's width changed: as many columns as fit.
    pub(super) fn grid_width(&self, width: f32) {
        let cols = (((width + GAP) / (TILE_MIN + GAP)).floor() as usize).max(1);
        if cols != self.state.borrow().grid_columns {
            self.state.borrow_mut().grid_columns = cols;
            if self.g(|g| g.get_grid()) {
                self.rebuild_grid();
            }
        }
    }

    /// A picture tile came into view: queue its thumbnail.
    pub(super) fn need_thumb(&self, index: i32) {
        {
            let mut s = self.state.borrow_mut();
            let Some(&node) = s.full_rows.get(index as usize) else {
                return;
            };
            let Some(entry) = s.tree.as_ref().and_then(|t| t.node(node).entry) else {
                return;
            };
            if s.thumbs.contains_key(&entry)
                || s.thumb_failed.contains(&entry)
                || s.thumb_queue.contains(&entry)
            {
                return;
            }
            // Encrypted pictures stay tiles: no password prompt for a thumbnail.
            if s.entries.get(entry).is_some_and(|e| e.encrypted)
                && !s.archive.as_ref().is_some_and(|a| a.has_password())
            {
                return;
            }
            s.thumb_queue.push(entry);
        }
        self.pump_thumbs();
    }

    /// Starts a background read for the newest queued thumbnails.
    fn pump_thumbs(&self) {
        let (batch, copy) = {
            let mut s = self.state.borrow_mut();
            if s.thumb_running || s.thumb_queue.is_empty() {
                return;
            }
            // Newest first: those are the tiles on screen now.
            let take = s.thumb_queue.len().min(THUMB_BATCH);
            let at = s.thumb_queue.len() - take;
            let batch: Vec<usize> = s.thumb_queue.split_off(at);
            let copy = match s.archive.as_ref().map(|a| a.try_clone()) {
                Some(Ok(a)) => a,
                _ => {
                    // The archive is busy (extracting): try again later.
                    s.thumb_queue.extend(batch);
                    return;
                }
            };
            s.thumb_running = true;
            (batch, copy)
        };
        std::thread::spawn(move || {
            let mut archive = copy;
            let _ = archive.read_entries(
                &batch,
                preview::IMAGE_LIMIT,
                &mut |entry, data| {
                    let thumb = preview::thumbnail(&data, THUMB_SIDE);
                    post(move |v| v.thumb_ready(entry, thumb));
                },
                &CancelToken::new(),
            );
            post(|v| {
                v.state.borrow_mut().thumb_running = false;
                v.pump_thumbs();
            });
        });
    }

    fn thumb_ready(&self, entry: usize, thumb: Option<SharedPixelBuffer<Rgba8Pixel>>) {
        let Some(pixels) = thumb else {
            self.state.borrow_mut().thumb_failed.insert(entry);
            return;
        };
        let image = Image::from_rgba8(pixels);
        let s = &mut *self.state.borrow_mut();
        s.thumbs.insert(entry, image.clone());
        if let Some(&(r, c)) = s.tile_at.get(&entry)
            && let Some(model) = s.tile_models.get(r)
            && let Some(mut tile) = model.row_data(c)
        {
            tile.thumb = image;
            tile.has_thumb = true;
            model.set_row_data(c, tile);
        }
    }
}
