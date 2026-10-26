//! Compiles the Slint UI with its translations bundled (`translations/`), and
//! turns the same `.po` files into a table for text built in Rust
//! (`src/i18n.rs`), so both sides share one catalog per language.

use std::fmt::Write as _;
use std::path::Path;

/// Bundled languages besides English (the source language).
const LANGUAGES: [&str; 1] = ["ar"];

fn main() {
    let config = slint_build::CompilerConfiguration::new()
        .with_bundled_translations("translations")
        .with_default_translation_context(slint_build::DefaultTranslationContext::None);
    slint_build::compile_with_config("ui/app.slint", config).expect("Slint build failed");

    let mut out = String::new();
    for lang in LANGUAGES {
        let path = format!("translations/{lang}/LC_MESSAGES/crate-app.po");
        println!("cargo:rerun-if-changed={path}");
        let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
        let mut entries = parse_po(&text);
        entries.sort_by(|a, b| a.0.cmp(&b.0));
        entries.dedup_by(|a, b| a.0 == b.0);
        writeln!(
            out,
            "pub static {}: &[(&str, &[&str])] = &[",
            lang.to_uppercase()
        )
        .unwrap();
        for (id, forms) in entries {
            write!(out, "    ({id:?}, &[").unwrap();
            for f in forms {
                write!(out, "{f:?}, ").unwrap();
            }
            writeln!(out, "]),").unwrap();
        }
        writeln!(out, "];").unwrap();
    }
    let dest = Path::new(&std::env::var("OUT_DIR").unwrap()).join("catalogs.rs");
    std::fs::write(dest, out).unwrap();
}

/// `(msgid, translations)` for every translated, non-fuzzy entry. Plural
/// entries are keyed by their singular msgid and list every form.
fn parse_po(text: &str) -> Vec<(String, Vec<String>)> {
    #[derive(Default)]
    struct Entry {
        fuzzy: bool,
        id: String,
        forms: Vec<String>,
    }
    let mut entries = Vec::new();
    let mut cur = Entry::default();
    // Which string a continuation line ("…") adds to.
    enum Target {
        None,
        Id,
        Plural,
        Form(usize),
    }
    let mut target = Target::None;
    let finish = |e: Entry, entries: &mut Vec<(String, Vec<String>)>| {
        if !e.fuzzy
            && !e.id.is_empty()
            && e.forms.iter().all(|f| !f.is_empty())
            && !e.forms.is_empty()
        {
            entries.push((e.id, e.forms));
        }
    };
    for line in text.lines().map(str::trim) {
        if line.is_empty() {
            finish(std::mem::take(&mut cur), &mut entries);
            target = Target::None;
        } else if let Some(flags) = line.strip_prefix("#,") {
            cur.fuzzy |= flags.contains("fuzzy");
        } else if line.starts_with('#') {
            // A comment; obsolete entries ("#~") are skipped with them.
        } else if let Some(rest) = line.strip_prefix("msgid_plural ") {
            let _ = unquote(rest);
            target = Target::Plural;
        } else if let Some(rest) = line.strip_prefix("msgid ") {
            cur.id = unquote(rest);
            target = Target::Id;
        } else if let Some(rest) = line.strip_prefix("msgstr[") {
            let (n, s) = rest.split_once("] ").expect("msgstr[n] \"…\"");
            let n: usize = n.parse().expect("plural index");
            if cur.forms.len() <= n {
                cur.forms.resize(n + 1, String::new());
            }
            cur.forms[n] = unquote(s);
            target = Target::Form(n);
        } else if let Some(rest) = line.strip_prefix("msgstr ") {
            cur.forms = vec![unquote(rest)];
            target = Target::Form(0);
        } else if line.starts_with('"') {
            let more = unquote(line);
            match target {
                Target::Id => cur.id.push_str(&more),
                Target::Form(n) => cur.forms[n].push_str(&more),
                Target::Plural | Target::None => {}
            }
        }
    }
    finish(cur, &mut entries);
    entries
}

/// `"a\"b\n"` → `a"b` + newline.
fn unquote(s: &str) -> String {
    let s = s.trim();
    let inner = s
        .strip_prefix('"')
        .and_then(|s| s.strip_suffix('"'))
        .unwrap_or(s);
    let mut out = String::new();
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some(other) => out.push(other),
                None => {}
            }
        } else {
            out.push(c);
        }
    }
    out
}
