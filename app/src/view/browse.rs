//! What the lists show: the mini view's top level, the full view's folder
//! with its tree, breadcrumbs and details, search, and the selection.

use slint::{ModelRc, SharedString, VecModel};

use super::{View, sync_selected};
use crate::tree::{self, NodeId, ROOT, SortKey, Tree};
use crate::{Details, FileRow, Row, TreeRow, model};

/// Most search results the full view lists.
const SEARCH_LIMIT: usize = 5000;

impl View {
    /// Rebuilds the list on screen (mini or full) from the entries.
    pub(super) fn refresh_rows(&self) {
        if self.g(|g| g.get_full()) {
            self.refresh_full();
        } else {
            self.refresh_mini();
        }
    }

    fn refresh_mini(&self) {
        let mut s = self.state.borrow_mut();
        s.mini_rows = if s.query.trim().is_empty() {
            model::top_level(&s.entries)
        } else {
            model::search(&s.entries, &s.query)
        };
        s.row_paths = s.mini_rows.iter().map(|r| r.path.clone()).collect();
        s.selection.rows_changed();
        let rows: Vec<Row> = s
            .mini_rows
            .iter()
            .map(|r| Row {
                name: r.name.clone().into(),
                ext: r.ext.clone().into(),
                meta: r.meta.clone().into(),
                kind: r.tile as i32,
                selected: s.selection.selected.contains(&r.path),
            })
            .collect();
        self.rows_model.set_vec(rows);
        drop(s);
        self.show_selection();
    }

    /// The tree, built on first use and kept current while a listing streams.
    fn ensure_tree(&self) {
        let mut s = self.state.borrow_mut();
        if s.tree.is_none() {
            s.tree = Some(Tree::from_entries(&s.entries));
        }
    }

    fn refresh_full(&self) {
        self.ensure_tree();
        let mut guard = self.state.borrow_mut();
        let s = &mut *guard;
        let tree = s.tree.as_ref().expect("built above");
        let searching = !s.query.trim().is_empty();
        s.full_rows = if searching {
            tree.search(s.folder, &s.query, SEARCH_LIMIT)
        } else {
            tree.sorted_children(s.folder, s.sort, s.descending)
        };
        s.row_paths = s.full_rows.iter().map(|&n| tree.path(n)).collect();
        s.selection.rows_changed();
        let rows: Vec<FileRow> = s
            .full_rows
            .iter()
            .zip(&s.row_paths)
            .map(|(&n, path)| file_row(tree, n, searching, s.selection.selected.contains(path)))
            .collect();
        self.files_model.set_vec(rows);

        // Sidebar tree: the root and every expanded folder's subfolders.
        let archive_name = self.g(|g| g.get_archive_name());
        let mut tree_rows = Vec::new();
        let mut items = Vec::new();
        let mut stack = vec![(ROOT, 0)];
        while let Some((n, depth)) = stack.pop() {
            let node = tree.node(n);
            let expanded = s.expanded.contains(&n);
            let mut subs: Vec<NodeId> = tree.subfolders(n).collect();
            subs.sort_by(|&a, &b| tree::natural_cmp(&tree.node(a).name, &tree.node(b).name));
            tree_rows.push(n);
            items.push(TreeRow {
                name: if n == ROOT {
                    archive_name.clone()
                } else {
                    node.name.clone().into()
                },
                depth,
                expanded,
                has_children: !subs.is_empty(),
                count: if n == ROOT {
                    SharedString::new()
                } else {
                    model::thousands(node.items).into()
                },
                current: n == s.folder,
            });
            if expanded {
                stack.extend(subs.into_iter().rev().map(|c| (c, depth + 1)));
            }
        }
        s.tree_rows = tree_rows;

        let mut crumbs: Vec<SharedString> = tree
            .ancestry(s.folder)
            .into_iter()
            .map(|n| tree.node(n).name.clone().into())
            .collect();
        crumbs[0] = archive_name;
        let (sort, descending) = (s.sort, s.descending);
        let files_text = model::count(s.full_rows.len() as u64, "item");
        self.g(|g| {
            g.set_tree(ModelRc::new(VecModel::from(items)));
            g.set_crumbs(ModelRc::new(VecModel::from(crumbs)));
            g.set_searching(searching);
            g.set_files_text(files_text.into());
            g.set_sort_column(sort as i32);
            g.set_sort_descending(descending);
        });
        drop(guard);
        self.refresh_grid();
        self.show_selection();
    }

    /// Selected flags, counts, cursor and details after a selection change.
    pub(super) fn show_selection(&self) {
        let s = self.state.borrow();
        let full = self.g(|g| g.get_full());
        if full {
            sync_selected(
                &self.files_model,
                &s.row_paths,
                &s.selection.selected,
                |r| &mut r.selected,
            );
        } else {
            sync_selected(&self.rows_model, &s.row_paths, &s.selection.selected, |r| {
                &mut r.selected
            });
        }
        let size: u64 = match &s.tree {
            Some(tree) => s
                .selection
                .selected
                .iter()
                .filter_map(|p| tree.find(p))
                .map(|n| tree.node(n).size)
                .sum(),
            None => 0,
        };
        let details = full
            .then(|| s.selection.cursor.and_then(|c| s.full_rows.get(c)).copied())
            .flatten()
            .and_then(|n| s.tree.as_ref().map(|t| details(t, &s.entries, n)));
        let cursor = s.selection.cursor.map_or(-1, |c| c as i32);
        let visible = s
            .row_paths
            .iter()
            .filter(|p| s.selection.selected.contains(*p))
            .count() as i32;
        let count = s.selection.selected.len() as i32;
        drop(s);
        self.g(|g| {
            g.set_selected_count(count);
            g.set_visible_selected(visible);
            g.set_selected_size(model::size_text(size).into());
            g.set_cursor(cursor);
            g.set_has_details(details.is_some());
            if let Some(d) = details {
                g.set_details(d);
            }
        });
        if full {
            self.sync_grid_selection();
        }
    }

    /// Changes the selection, unless an extraction is running.
    pub(super) fn select(&self, change: impl FnOnce(&mut model::Picker, &[String])) {
        {
            let mut s = self.state.borrow_mut();
            if s.cancel.is_some() {
                return;
            }
            let super::State {
                selection,
                row_paths,
                ..
            } = &mut *s;
            change(selection, row_paths);
        }
        self.show_selection();
    }

    pub(super) fn search(&self, query: &str) {
        self.state.borrow_mut().query = query.to_owned();
        self.refresh_rows();
    }

    /// Double-click or Enter: a folder in the full view opens in place;
    /// anything else is opened with its app.
    pub(super) fn activate_row(&self, index: i32) {
        let Ok(index) = usize::try_from(index) else {
            return;
        };
        let full = self.g(|g| g.get_full());
        let folder = {
            let s = self.state.borrow();
            match (&s.tree, full) {
                (Some(tree), true) => s
                    .full_rows
                    .get(index)
                    .copied()
                    .filter(|&n| tree.node(n).is_dir),
                _ => None,
            }
        };
        match folder {
            Some(n) => self.go_to(n),
            None => self.open_row(index),
        }
    }

    /// Shows a folder in the full view (and clears any search).
    pub(super) fn go_to(&self, folder: NodeId) {
        {
            let mut s = self.state.borrow_mut();
            s.folder = folder;
            s.query.clear();
            if let Some(tree) = &s.tree {
                let up = tree.ancestry(folder);
                s.expanded.extend(up);
            }
        }
        self.g(|g| g.set_query(SharedString::new()));
        self.refresh_full();
    }

    pub(super) fn go_up(&self) {
        let parent = {
            let s = self.state.borrow();
            let Some(tree) = &s.tree else { return };
            (s.folder != ROOT).then(|| tree.node(s.folder).parent)
        };
        if let Some(p) = parent {
            self.go_to(p);
        }
    }

    pub(super) fn open_tree_row(&self, i: usize) {
        let node = self.state.borrow().tree_rows.get(i).copied();
        if let Some(n) = node {
            self.go_to(n);
        }
    }

    pub(super) fn toggle_tree_row(&self, i: usize) {
        {
            let mut s = self.state.borrow_mut();
            let Some(&n) = s.tree_rows.get(i) else { return };
            if !s.expanded.remove(&n) {
                s.expanded.insert(n);
            }
        }
        self.refresh_full();
    }

    pub(super) fn open_crumb(&self, i: usize) {
        let node = {
            let s = self.state.borrow();
            s.tree
                .as_ref()
                .and_then(|t| t.ancestry(s.folder).get(i).copied())
        };
        if let Some(n) = node {
            self.go_to(n);
        }
    }

    /// A column header: sort by it, or flip the order if it already is.
    pub(super) fn sort_by(&self, column: i32) {
        {
            let mut s = self.state.borrow_mut();
            let key = SortKey::from_index(column);
            if s.sort == key {
                s.descending = !s.descending;
            } else {
                s.sort = key;
                // Big things first is what people look for.
                s.descending = key != SortKey::Name;
            }
        }
        self.refresh_full();
    }

    fn current_node(&self) -> Option<(NodeId, String)> {
        let s = self.state.borrow();
        let c = s.selection.cursor?;
        Some((*s.full_rows.get(c)?, s.row_paths.get(c)?.clone()))
    }

    pub(super) fn copy_path(&self) {
        if let Some((_, path)) = self.current_node() {
            self.copy_to_clipboard(&path, "Copied the path");
        }
    }

    pub(super) fn copy_checksum(&self) {
        let crc = self.current_node().and_then(|(n, _)| {
            let s = self.state.borrow();
            let entry = s.tree.as_ref()?.node(n).entry?;
            s.entries.get(entry)?.crc32
        });
        if let Some(crc) = crc {
            self.copy_to_clipboard(&format!("{crc:08x}"), "Copied the CRC32 checksum");
        }
    }
}

fn file_row(tree: &Tree, n: NodeId, searching: bool, selected: bool) -> FileRow {
    let node = tree.node(n);
    let (ext, tile) = if node.is_dir {
        (String::new(), model::Tile::Folder)
    } else {
        model::file_kind(&node.name)
    };
    let folder = if searching {
        let parent = tree.path(node.parent);
        if parent.is_empty() {
            "/".to_owned()
        } else {
            parent
        }
    } else {
        String::new()
    };
    FileRow {
        name: node.name.clone().into(),
        folder: folder.into(),
        ext: ext.into(),
        kind: tile as i32,
        is_dir: node.is_dir,
        encrypted: node.encrypted,
        size: model::size_text(node.size).into(),
        packed: node
            .packed
            .map(model::size_text)
            .unwrap_or_else(|| "—".into())
            .into(),
        ratio: tree::ratio(node).unwrap_or(-1.0),
        modified: node
            .modified
            .map(model::date_text)
            .unwrap_or_else(|| "—".into())
            .into(),
        selected,
    }
}

fn details(tree: &Tree, entries: &[crate_core::Entry], n: NodeId) -> Details {
    let node = tree.node(n);
    let entry = node.entry.and_then(|i| entries.get(i));
    let (ext, tile) = if node.is_dir {
        (String::new(), model::Tile::Folder)
    } else {
        model::file_kind(&node.name)
    };
    Details {
        name: node.name.clone().into(),
        path: tree.path(n).into(),
        ext: ext.into(),
        kind: tile as i32,
        is_dir: node.is_dir,
        size: model::size_text(node.size).into(),
        packed: node
            .packed
            .map(model::size_text)
            .unwrap_or_else(|| "—".into())
            .into(),
        method: if node.is_dir {
            model::thousands(node.items).into()
        } else {
            entry
                .and_then(|e| e.method.clone())
                .unwrap_or_else(|| "—".into())
                .into()
        },
        modified: node
            .modified
            .map(model::date_text)
            .unwrap_or_else(|| "—".into())
            .into(),
        crc: entry
            .and_then(|e| e.crc32)
            .map(|c| format!("{c:08x}"))
            .unwrap_or_else(|| "—".into())
            .into(),
        encrypted: node.encrypted,
    }
}
