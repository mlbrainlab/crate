//! Before extracting: names the destination won't take, then names that
//! differ only by case (`FileIssues.dc.html`). Both are found up front with
//! `Archive::check_names`, so the user decides before anything is written.

use slint::{ModelRc, VecModel};

use crate_core::names::NameIssue;
use crate_core::{CaseChoice, CaseClash, NameCheck};

use super::View;
use super::extract::Request;
use crate::NameFix;
use crate::i18n::{tr, trf, trn};
use crate::model;

/// An extraction waiting for the name questions.
pub(super) struct Pending {
    request: Request,
    check: NameCheck,
    /// The case clash being asked about.
    clash: usize,
}

/// `contains : and ?`
fn why(issue: &NameIssue) -> String {
    match issue {
        NameIssue::Reserved => tr("reserved device name"),
        NameIssue::TrailingDotOrSpace => tr("ends with a dot or space"),
        NameIssue::Characters(chars) => {
            let shown: Vec<String> = chars
                .iter()
                .map(|&c| {
                    if c.is_control() {
                        format!("U+{:04X}", c as u32)
                    } else {
                        c.to_string()
                    }
                })
                .collect();
            match shown.as_slice() {
                [a] => trf("contains {a}", &[("a", a)]),
                [a, b] => trf("contains {a} and {b}", &[("a", a), ("b", b)]),
                _ => trf("contains {list}", &[("list", &shown.join(" "))]),
            }
        }
    }
}

/// `…/dddd/deep.txt` for a long path: the last two parts.
fn tail(path: &str) -> String {
    let parts: Vec<&str> = path.rsplitn(3, '/').collect();
    if parts.len() < 3 {
        path.to_owned()
    } else {
        format!("…/{}/{}", parts[1], parts[0])
    }
}

fn rows(check: &NameCheck, request: &Request) -> Vec<NameFix> {
    let mut rows: Vec<NameFix> = check
        .renamed
        .iter()
        .map(|r| NameFix {
            old: r.path.rsplit('/').next().unwrap_or(&r.path).into(),
            new: request
                .options
                .edited_names
                .get(&r.path)
                .unwrap_or(&r.new_name)
                .into(),
            why: why(&r.issue).into(),
            long: false,
        })
        .collect();
    rows.extend(check.long_paths.iter().map(|(path, len)| {
        NameFix {
            old: trf(
                "{path} ({n} characters)",
                &[("path", &tail(path)), ("n", &len.to_string())],
            )
            .into(),
            new: tr("uses long-path mode").into(),
            why: tr("longer than 260 characters").into(),
            long: true,
        }
    }));
    rows
}

impl View {
    /// True when `request` has to wait for the user's answers first; it is
    /// run once they're in.
    pub(super) fn ask_about_names(&self, request: &mut Request) -> bool {
        if request.names_checked {
            return false;
        }
        request.options.names = crate::platform::name_rules();
        let check = {
            let s = self.state.borrow();
            let Some(archive) = &s.archive else {
                return false;
            };
            archive.check_names(&request.selection, &request.dest.dest, &request.options)
        };
        if check.renamed.is_empty() && check.case_clashes.is_empty() {
            request.names_checked = true;
            return false;
        }
        let pending = Pending {
            request: request.clone(),
            check,
            clash: 0,
        };
        if pending.check.renamed.is_empty() {
            self.next_clash(pending);
        } else {
            self.show_name_fixes(&pending);
            self.state.borrow_mut().name_fixes = Some(pending);
        }
        true
    }

    fn show_name_fixes(&self, p: &Pending) {
        let n = p.check.renamed.len() as u64;
        let rows = rows(&p.check, &p.request);
        self.g(|g| {
            g.set_nf_title(
                trn(
                    "{n} name needs to change to extract here",
                    "{n} names need to change to extract here",
                    n,
                )
                .into(),
            );
            g.set_nf_count(n as i32);
            g.set_nf_rows(ModelRc::new(VecModel::from(rows)));
            g.set_nf_show(true);
        });
    }

    pub(super) fn name_fix_answer(&self, answer: i32) {
        let Some(mut p) = self.state.borrow_mut().name_fixes.take() else {
            return;
        };
        match answer {
            // Edit names… / Done editing
            1 => {
                self.g(|g| g.set_nf_editing(!g.get_nf_editing()));
                self.state.borrow_mut().name_fixes = Some(p);
                return;
            }
            0 => p.request.options.skip_bad_names = true,
            2 => {}
            _ => {
                self.g(|g| {
                    g.set_nf_show(false);
                    g.set_nf_editing(false);
                });
                return;
            }
        }
        self.g(|g| {
            g.set_nf_show(false);
            g.set_nf_editing(false);
        });
        // The answers can change which names clash: look again.
        if let Some(archive) = &self.state.borrow().archive {
            p.check = archive.check_names(
                &p.request.selection,
                &p.request.dest.dest,
                &p.request.options,
            );
        }
        p.clash = 0;
        self.next_clash(p);
    }

    pub(super) fn name_fix_edited(&self, row: usize, text: &str) {
        if let Some(p) = self.state.borrow_mut().name_fixes.as_mut()
            && let Some(r) = p.check.renamed.get(row)
        {
            p.request
                .options
                .edited_names
                .insert(r.path.clone(), text.to_owned());
        }
    }

    /// Asks about the next case clash, or starts the extraction.
    fn next_clash(&self, mut p: Pending) {
        match p.check.case_clashes.get(p.clash) {
            Some(c) => {
                self.show_clash(c);
                self.state.borrow_mut().name_fixes = Some(p);
            }
            None => {
                p.request.names_checked = true;
                self.run(p.request);
            }
        }
    }

    fn show_clash(&self, c: &CaseClash) {
        let name = |path: &str| path.rsplit('/').next().unwrap_or(path).to_owned();
        let (first, second) = (name(&c.other), name(&c.path));
        let mut meta = vec![first.clone(), model::size_text(c.other_size)];
        meta.extend(c.other_modified.map(model::date_text));
        let body = trf(
            "This archive was made on a system where {a} and {b} are different files. This disk treats them as the same name.",
            &[("a", &first), ("b", &second)],
        );
        self.g(|g| {
            g.set_cc_body(body.into());
            g.set_cc_first(meta.join(" · ").into());
            g.set_cc_second(second.into());
            g.set_cc_both(c.both_name.clone().into());
            g.set_cc_show(true);
        });
    }

    pub(super) fn case_clash_answer(&self, answer: i32, for_all: bool) {
        self.g(|g| g.set_cc_show(false));
        let Some(mut p) = self.state.borrow_mut().name_fixes.take() else {
            return;
        };
        let choice = match answer {
            0 => CaseChoice::KeepNewer,
            1 => CaseChoice::Skip,
            2 => CaseChoice::KeepBoth,
            _ => return,
        };
        if for_all {
            p.request.options.case_clash = choice;
            p.clash = p.check.case_clashes.len();
        } else {
            let path = p.check.case_clashes[p.clash].path.clone();
            p.request.options.case_choices.insert(path, choice);
            p.clash += 1;
        }
        self.next_clash(p);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reasons_and_tails() {
        assert_eq!(
            why(&NameIssue::Characters(vec![':', '?'])),
            "contains : and ?"
        );
        assert_eq!(why(&NameIssue::Characters(vec!['\t'])), "contains U+0009");
        assert_eq!(tail("a/b/c/d.txt"), "…/c/d.txt");
        assert_eq!(tail("c/d.txt"), "c/d.txt");
    }
}
