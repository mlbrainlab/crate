//! The archive as a folder tree, for the full view. Built entry by entry, so a
//! tar listing can be browsed while it streams in. Folders that have no entry
//! of their own (common in ZIPs) are created on the way.

use std::cmp::Ordering;
use std::collections::HashMap;
use std::time::SystemTime;

use crate_core::{Entry, EntryKind};

pub type NodeId = usize;
pub const ROOT: NodeId = 0;

#[derive(Debug, Default)]
pub struct Node {
    pub name: String,
    pub parent: NodeId,
    pub children: Vec<NodeId>,
    child_by_name: HashMap<String, NodeId>,
    /// The archive entry, when there is one (implicit folders have none).
    pub entry: Option<usize>,
    pub is_dir: bool,
    /// Unpacked bytes (for a folder: everything inside).
    pub size: u64,
    /// Packed bytes, when the format stores them per entry.
    pub packed: Option<u64>,
    /// Entries inside a folder, at any depth.
    pub items: u64,
    pub modified: Option<SystemTime>,
    pub encrypted: bool,
}

#[derive(Debug)]
pub struct Tree {
    nodes: Vec<Node>,
}

impl Default for Tree {
    fn default() -> Self {
        Self {
            nodes: vec![Node {
                is_dir: true,
                ..Node::default()
            }],
        }
    }
}

impl Tree {
    pub fn from_entries(entries: &[Entry]) -> Self {
        let mut tree = Tree::default();
        for e in entries {
            tree.insert(e);
        }
        tree
    }

    pub fn node(&self, id: NodeId) -> &Node {
        &self.nodes[id]
    }

    /// Adds one entry (and any folders on its path that don't exist yet).
    pub fn insert(&mut self, e: &Entry) {
        let path = e.path.trim_start_matches("./").trim_start_matches('/');
        let parts: Vec<&str> = path
            .split('/')
            .filter(|p| !p.is_empty() && *p != ".")
            .collect();
        let Some((last, dirs)) = parts.split_last() else {
            return;
        };
        let mut at = ROOT;
        for dir in dirs {
            at = self.child(at, dir, true);
        }
        let is_dir = e.kind == EntryKind::Dir;
        let id = self.child(at, last, is_dir);
        let node = &mut self.nodes[id];
        if node.entry.is_some() {
            return; // duplicate name: the first one wins in the tree
        }
        node.entry = Some(e.index);
        node.modified = e.modified;
        node.encrypted = e.encrypted;
        if !is_dir {
            node.size = e.size;
            node.packed = e.packed_size;
        }
        // Totals for every folder above.
        let (size, packed) = (if is_dir { 0 } else { e.size }, e.packed_size);
        let mut up = at;
        loop {
            let n = &mut self.nodes[up];
            n.items += 1;
            n.size += size;
            if let Some(p) = packed {
                n.packed = Some(n.packed.unwrap_or(0) + p);
            }
            if up == ROOT {
                break;
            }
            up = n.parent;
        }
    }

    fn child(&mut self, parent: NodeId, name: &str, is_dir: bool) -> NodeId {
        if let Some(&id) = self.nodes[parent].child_by_name.get(name) {
            if is_dir {
                self.nodes[id].is_dir = true;
            }
            return id;
        }
        let id = self.nodes.len();
        self.nodes.push(Node {
            name: name.to_owned(),
            parent,
            is_dir,
            ..Node::default()
        });
        let p = &mut self.nodes[parent];
        p.children.push(id);
        p.child_by_name.insert(name.to_owned(), id);
        id
    }

    /// `docs/guide.txt`; empty for the root.
    pub fn path(&self, id: NodeId) -> String {
        let mut parts = Vec::new();
        let mut at = id;
        while at != ROOT {
            parts.push(self.nodes[at].name.as_str());
            at = self.nodes[at].parent;
        }
        parts.reverse();
        parts.join("/")
    }

    /// Folders from the root down to `id` (inclusive), for breadcrumbs.
    pub fn ancestry(&self, id: NodeId) -> Vec<NodeId> {
        let mut out = vec![id];
        let mut at = id;
        while at != ROOT {
            at = self.nodes[at].parent;
            out.push(at);
        }
        out.reverse();
        out
    }

    pub fn find(&self, path: &str) -> Option<NodeId> {
        let mut at = ROOT;
        for part in path.split('/').filter(|p| !p.is_empty()) {
            at = *self.nodes[at].child_by_name.get(part)?;
        }
        Some(at)
    }

    pub fn subfolders(&self, id: NodeId) -> impl Iterator<Item = NodeId> + '_ {
        self.nodes[id]
            .children
            .iter()
            .copied()
            .filter(|&c| self.nodes[c].is_dir)
    }

    /// Children of a folder, folders first, sorted by `key`.
    pub fn sorted_children(&self, id: NodeId, key: SortKey, descending: bool) -> Vec<NodeId> {
        let mut children = self.nodes[id].children.clone();
        children.sort_by(|&a, &b| self.compare(a, b, key, descending));
        children
    }

    /// Everything below `id` whose name contains `query` (ignoring case),
    /// at most `limit`, in tree order.
    pub fn search(&self, id: NodeId, query: &str, limit: usize) -> Vec<NodeId> {
        let query = query.trim().to_lowercase();
        let mut out = Vec::new();
        let mut stack: Vec<NodeId> = self.nodes[id].children.iter().rev().copied().collect();
        while let Some(n) = stack.pop() {
            if out.len() >= limit {
                break;
            }
            if self.nodes[n].name.to_lowercase().contains(&query) {
                out.push(n);
            }
            stack.extend(self.nodes[n].children.iter().rev().copied());
        }
        out
    }

    pub fn compare(&self, a: NodeId, b: NodeId, key: SortKey, descending: bool) -> Ordering {
        let (na, nb) = (&self.nodes[a], &self.nodes[b]);
        let folders_first = nb.is_dir.cmp(&na.is_dir);
        let by_name = || natural_cmp(&na.name, &nb.name);
        let order = match key {
            SortKey::Name => by_name(),
            SortKey::Size => na.size.cmp(&nb.size).then_with(by_name),
            SortKey::Packed => na.packed.cmp(&nb.packed).then_with(by_name),
            SortKey::Ratio => ratio(na)
                .partial_cmp(&ratio(nb))
                .unwrap_or(Ordering::Equal)
                .then_with(by_name),
            SortKey::Modified => na.modified.cmp(&nb.modified).then_with(by_name),
        };
        folders_first.then(if descending { order.reverse() } else { order })
    }
}

/// Columns the list can be sorted by.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SortKey {
    #[default]
    Name,
    Size,
    Packed,
    Ratio,
    Modified,
}

impl SortKey {
    pub fn from_index(i: i32) -> Self {
        match i {
            1 => SortKey::Size,
            2 => SortKey::Packed,
            3 => SortKey::Ratio,
            4 => SortKey::Modified,
            _ => SortKey::Name,
        }
    }
}

/// Space saved, 0..1 (`None` when the format doesn't store packed sizes).
pub fn ratio(n: &Node) -> Option<f32> {
    let packed = n.packed?;
    (n.size > 0).then(|| (1.0 - packed as f64 / n.size as f64).clamp(0.0, 1.0) as f32)
}

/// Names compared like a file manager: case-insensitive, with digit runs
/// compared as numbers (`file2` before `file10`).
pub fn natural_cmp(a: &str, b: &str) -> Ordering {
    let (mut a, mut b) = (a.chars().peekable(), b.chars().peekable());
    loop {
        match (a.peek().copied(), b.peek().copied()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(x), Some(y)) if x.is_ascii_digit() && y.is_ascii_digit() => {
                let take = |it: &mut std::iter::Peekable<std::str::Chars>| {
                    let mut s = String::new();
                    while let Some(c) = it.peek().filter(|c| c.is_ascii_digit()) {
                        s.push(*c);
                        it.next();
                    }
                    s
                };
                let (da, db) = (take(&mut a), take(&mut b));
                let (ta, tb) = (da.trim_start_matches('0'), db.trim_start_matches('0'));
                let o = ta.len().cmp(&tb.len()).then_with(|| ta.cmp(tb));
                if o != Ordering::Equal {
                    return o;
                }
            }
            (Some(x), Some(y)) => {
                let o = x.to_lowercase().cmp(y.to_lowercase());
                if o != Ordering::Equal {
                    return o;
                }
                a.next();
                b.next();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(index: usize, path: &str, size: u64) -> Entry {
        Entry {
            index,
            path: path.into(),
            kind: if path.ends_with('/') {
                EntryKind::Dir
            } else {
                EntryKind::File
            },
            size,
            packed_size: Some(size / 2),
            modified: None,
            mode: None,
            crc32: None,
            encrypted: false,
            method: None,
        }
    }

    fn tree() -> Tree {
        Tree::from_entries(&[
            entry(0, "photos/", 0),
            entry(1, "photos/raw/img10.jpg", 100),
            entry(2, "photos/raw/img2.jpg", 300),
            entry(3, "./photos/cover.png", 50),
            entry(4, "README.md", 10),
            entry(5, "src/main.rs", 20),
        ])
    }

    #[test]
    fn builds_folders_and_totals() {
        let t = tree();
        let photos = t.find("photos").unwrap();
        let raw = t.find("photos/raw").unwrap();
        assert_eq!(t.node(photos).entry, Some(0));
        assert_eq!(t.node(raw).entry, None, "implicit folder");
        assert!(t.node(raw).is_dir);
        assert_eq!((t.node(photos).items, t.node(photos).size), (3, 450));
        assert_eq!(t.node(ROOT).items, 6);
        assert_eq!(t.path(raw), "photos/raw");
        assert_eq!(t.ancestry(raw), [ROOT, photos, raw]);
        assert_eq!(ratio(t.node(raw)), Some(0.5));
    }

    #[test]
    fn sorting() {
        let t = tree();
        let names = |ids: Vec<NodeId>| -> Vec<String> {
            ids.into_iter().map(|i| t.node(i).name.clone()).collect()
        };
        assert_eq!(
            names(t.sorted_children(ROOT, SortKey::Name, false)),
            ["photos", "src", "README.md"],
            "folders first"
        );
        let raw = t.find("photos/raw").unwrap();
        assert_eq!(
            names(t.sorted_children(raw, SortKey::Name, false)),
            ["img2.jpg", "img10.jpg"],
            "natural order"
        );
        assert_eq!(
            names(t.sorted_children(raw, SortKey::Size, true)),
            ["img2.jpg", "img10.jpg"]
        );
        assert_eq!(natural_cmp("File", "file"), Ordering::Equal);
    }

    #[test]
    fn search_below_a_folder() {
        let t = tree();
        let hits = t.search(ROOT, "IMG", 10);
        assert_eq!(hits.len(), 2);
        assert_eq!(t.path(hits[0]), "photos/raw/img10.jpg");
        assert_eq!(t.search(t.find("src").unwrap(), "img", 10).len(), 0);
        assert_eq!(t.search(ROOT, "", 3).len(), 3, "limit");
    }
}
