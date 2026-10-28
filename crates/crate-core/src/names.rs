//! Names an archive may hold that the destination's file system won't take
//! (`FileIssues.dc.html`): Windows' reserved device names, characters it
//! forbids, names ending in a dot or space, very long paths, and, on Windows
//! and macOS, two names that differ only by case.
//!
//! The rules are a value ([`NameRules`]), not a `cfg`, so the Windows rules
//! run in tests on every system.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Which rules the destination follows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NameRules {
    /// Windows' naming rules (reserved names, forbidden characters, no
    /// trailing dot or space, the 260-character path limit).
    pub windows: bool,
    /// `README.md` and `readme.md` are the same file.
    pub case_insensitive: bool,
}

impl NameRules {
    /// The rules of the system Crate runs on.
    pub fn native() -> Self {
        Self {
            windows: cfg!(windows),
            case_insensitive: cfg!(any(windows, target_os = "macos")),
        }
    }

    pub fn windows() -> Self {
        Self {
            windows: true,
            case_insensitive: true,
        }
    }
}

/// Why a name had to change.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NameIssue {
    /// `CON`, `PRN`, `AUX`, `NUL`, `COM1`–`COM9`, `LPT1`–`LPT9`, with any
    /// extension.
    Reserved,
    /// Characters Windows forbids, in the order they appear (each once).
    Characters(Vec<char>),
    /// Windows drops a trailing dot or space, so the name wouldn't round-trip.
    TrailingDotOrSpace,
}

impl std::fmt::Display for NameIssue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NameIssue::Reserved => f.write_str("reserved device name"),
            NameIssue::TrailingDotOrSpace => f.write_str("ends with a dot or space"),
            NameIssue::Characters(chars) => {
                f.write_str("contains")?;
                for c in chars {
                    write!(f, " {}", c.escape_debug())?;
                }
                Ok(())
            }
        }
    }
}

/// The longest path Windows handles without long-path mode.
pub const WINDOWS_MAX_PATH: usize = 260;

const FORBIDDEN: &[char] = &['<', '>', ':', '"', '/', '\\', '|', '?', '*'];

fn reserved(stem: &str) -> bool {
    let upper = stem.trim_end_matches(' ').to_ascii_uppercase();
    match upper.as_str() {
        "CON" | "PRN" | "AUX" | "NUL" => true,
        _ => {
            let tail = upper
                .strip_prefix("COM")
                .or_else(|| upper.strip_prefix("LPT"));
            tail.is_some_and(|tail| {
                matches!(
                    tail,
                    "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "¹" | "²" | "³"
                )
            })
        }
    }
}

/// The name Windows would accept for one path component, and why it
/// changed; `None` when it's fine. `CON.txt` → `CON_.txt`, `a:b?` → `a_b_`,
/// `notes.` → `notes`.
pub fn fix_component(name: &str) -> Option<(String, NameIssue)> {
    let mut bad: Vec<char> = Vec::new();
    let mut fixed: String = name
        .chars()
        .map(|c| {
            if FORBIDDEN.contains(&c) || (c as u32) < 32 {
                if !bad.contains(&c) {
                    bad.push(c);
                }
                '_'
            } else {
                c
            }
        })
        .collect();
    let mut issue = (!bad.is_empty()).then_some(NameIssue::Characters(bad));

    let trimmed = fixed.trim_end_matches(['.', ' ']);
    if trimmed.len() != fixed.len() {
        fixed = if trimmed.is_empty() {
            "_".to_owned()
        } else {
            trimmed.to_owned()
        };
        issue.get_or_insert(NameIssue::TrailingDotOrSpace);
    }

    let (stem, ext) = match fixed.find('.') {
        Some(i) => fixed.split_at(i),
        None => (fixed.as_str(), ""),
    };
    if reserved(stem) {
        fixed = format!("{}_{ext}", stem.trim_end_matches(' '));
        issue = Some(NameIssue::Reserved);
    }
    issue.map(|i| (fixed, i))
}

/// Fixes every component of `rel` (a path from
/// [`safe_relative_path`](crate::safety::safe_relative_path)). `edited` holds
/// names the user typed, keyed by the original path of the component
/// (`docs/CON.txt`); they win over the automatic fix. Returns the new path
/// and the issues found, with the original path of each changed component.
pub fn fix_path(
    rel: &Path,
    rules: NameRules,
    edited: &HashMap<String, String>,
) -> (PathBuf, Vec<(String, NameIssue)>) {
    let mut original = String::new();
    let mut out = PathBuf::new();
    let mut issues = Vec::new();
    for part in rel.components() {
        let name = part.as_os_str().to_string_lossy();
        if !original.is_empty() {
            original.push('/');
        }
        original.push_str(&name);
        let fixed = if !rules.windows {
            None
        } else {
            fix_component(&name)
        };
        match (
            edited.get(&original).and_then(|n| usable_edit(n, rules)),
            fixed,
        ) {
            (Some(new), Some((_, issue))) => {
                issues.push((original.clone(), issue));
                out.push(new);
            }
            (Some(new), None) => out.push(new),
            (None, Some((new, issue))) => {
                issues.push((original.clone(), issue));
                out.push(new);
            }
            (None, None) => out.push(&*name),
        }
    }
    (out, issues)
}

/// A name the user typed, made safe: one plain component, fixed by the same
/// rules; `None` (keep the automatic fix) when nothing usable is left.
fn usable_edit(name: &str, rules: NameRules) -> Option<String> {
    let name = name.trim();
    if name.is_empty() || name == "." || name == ".." || name.contains(['/', '\\', '\0']) {
        return None;
    }
    let name = match rules.windows.then(|| fix_component(name)).flatten() {
        Some((fixed, _)) => fixed,
        None => name.to_owned(),
    };
    let single = Path::new(&name).components().count() == 1
        && matches!(
            Path::new(&name).components().next(),
            Some(std::path::Component::Normal(_))
        );
    single.then_some(name)
}

/// The length Windows counts for `dest` joined with `rel`, in UTF-16 units.
pub fn windows_path_len(dest: &Path, rel: &Path) -> usize {
    dest.join(rel)
        .as_os_str()
        .to_string_lossy()
        .encode_utf16()
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fix(name: &str) -> Option<String> {
        fix_component(name).map(|(n, _)| n)
    }

    #[test]
    fn mockup_examples() {
        assert_eq!(
            fix_component("CON.txt"),
            Some(("CON_.txt".into(), NameIssue::Reserved))
        );
        assert_eq!(
            fix_component("report: v2?.pdf"),
            Some((
                "report_ v2_.pdf".into(),
                NameIssue::Characters(vec![':', '?'])
            ))
        );
        assert_eq!(
            fix_component("notes."),
            Some(("notes".into(), NameIssue::TrailingDotOrSpace))
        );
    }

    #[test]
    fn reserved_names() {
        for name in [
            "con",
            "Prn.tar.gz",
            "aux",
            "NUL.",
            "com1.log",
            "LPT9",
            "COM¹",
        ] {
            assert!(fix(name).is_some(), "{name}");
        }
        assert_eq!(fix("nul.").as_deref(), Some("nul_"));
        assert_eq!(fix("lpt1.txt").as_deref(), Some("lpt1_.txt"));
        for name in [
            "CONSOLE",
            "com10",
            "com0",
            "LPT",
            "icon.png",
            "a.con",
            "auxiliary",
        ] {
            assert_eq!(fix(name), None, "{name}");
        }
    }

    #[test]
    fn other_names() {
        assert_eq!(fix("ok name.txt"), None);
        assert_eq!(fix("تقرير.txt"), None);
        assert_eq!(fix("tab\there").as_deref(), Some("tab_here"));
        assert_eq!(fix("a<b>c|d*e\"f").as_deref(), Some("a_b_c_d_e_f"));
        assert_eq!(fix("trailing ").as_deref(), Some("trailing"));
        assert_eq!(fix("...").as_deref(), Some("_"));
        assert_eq!(fix(".bashrc"), None);
    }

    #[test]
    fn whole_paths() {
        let none = HashMap::new();
        let (p, issues) = fix_path(Path::new("aux/x:y/ok.txt"), NameRules::windows(), &none);
        assert_eq!(p, Path::new("aux_/x_y/ok.txt"));
        assert_eq!(issues.len(), 2);
        assert_eq!(issues[0].0, "aux");
        assert_eq!(issues[1].0, "aux/x:y");

        // Without the Windows rules nothing changes (on Windows `x:y` can't
        // be a path component at all: `x:` reads as a drive).
        #[cfg(not(windows))]
        {
            let (p, issues) = fix_path(Path::new("aux/x:y"), NameRules::default(), &none);
            assert_eq!(p, Path::new("aux/x:y"));
            assert!(issues.is_empty());
        }

        let edited = HashMap::from([("aux".to_owned(), "auxiliary".to_owned())]);
        let (p, issues) = fix_path(Path::new("aux/a.txt"), NameRules::windows(), &edited);
        assert_eq!(p, Path::new("auxiliary/a.txt"));
        assert_eq!(issues.len(), 1, "still reported as a change");

        // Typed names can't climb out or hide a separator.
        for typed in ["..", "../up", "a/b", "c:\\x", "", "  "] {
            let edited = HashMap::from([("aux".to_owned(), typed.to_owned())]);
            let (p, _) = fix_path(Path::new("aux/a.txt"), NameRules::windows(), &edited);
            assert!(
                p == Path::new("aux_/a.txt") || p == Path::new("c__x/a.txt"),
                "{typed:?} gave {p:?}"
            );
        }
    }
}
