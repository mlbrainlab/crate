//! The UI language. Slint text uses `@tr()` with the bundled translations;
//! text built in Rust uses [`tr`], [`trn`] and [`trf`] with the same `.po`
//! files (compiled into a table by `build.rs`). Strings are found by
//! `scripts/update-translations.sh`, which looks for these function names.

use std::sync::atomic::{AtomicU8, Ordering};

include!(concat!(env!("OUT_DIR"), "/catalogs.rs"));

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Language {
    English = 0,
    Arabic = 1,
}

impl Language {
    /// The bundled translation's folder name ("" for the source language).
    pub fn code(self) -> &'static str {
        match self {
            Language::English => "en",
            Language::Arabic => "ar",
        }
    }

    pub fn rtl(self) -> bool {
        self == Language::Arabic
    }

    /// `ar`, `ar_SA.UTF-8`, `ar-EG` → Arabic; anything else → English.
    pub fn from_locale(locale: &str) -> Self {
        let base = locale
            .split(['_', '-', '.', '@'])
            .next()
            .unwrap_or_default()
            .to_ascii_lowercase();
        match base.as_str() {
            "ar" => Language::Arabic,
            _ => Language::English,
        }
    }

    /// The system's language (what "System" in Preferences means).
    pub fn system() -> Self {
        sys_locale::get_locale().map_or(Language::English, |l| Self::from_locale(&l))
    }

    /// The language preference: "" or "system", "en", "ar".
    pub fn from_pref(pref: &str) -> Self {
        match pref {
            "en" => Language::English,
            "ar" => Language::Arabic,
            _ => Self::system(),
        }
    }
}

static CURRENT: AtomicU8 = AtomicU8::new(Language::English as u8);

pub fn current() -> Language {
    match CURRENT.load(Ordering::Relaxed) {
        1 => Language::Arabic,
        _ => Language::English,
    }
}

/// Switches both sides: Rust text from now on, and every Slint window of
/// this process. Call after the first window exists (Slint keeps its
/// translations in a context created with it).
pub fn set(lang: Language) {
    CURRENT.store(lang as u8, Ordering::Relaxed);
    let _ = slint::select_bundled_translation(lang.code());
}

fn catalog() -> Option<&'static [(&'static str, &'static [&'static str])]> {
    match current() {
        Language::English => None,
        Language::Arabic => Some(AR),
    }
}

fn lookup(id: &str) -> Option<&'static [&'static str]> {
    let cat = catalog()?;
    cat.binary_search_by(|(k, _)| (*k).cmp(id))
        .ok()
        .map(|i| cat[i].1)
}

/// Translates a fixed string.
pub fn tr(msgid: &str) -> String {
    lookup(msgid)
        .and_then(|forms| forms.first())
        .map_or_else(|| msgid.to_owned(), |s| (*s).to_owned())
}

/// Translates and fills `{name}` placeholders: `trf("Opened {name}", &[("name", n)])`.
pub fn trf(msgid: &str, args: &[(&str, &str)]) -> String {
    fill(tr(msgid), args)
}

/// Translates a count: `trn("{n} item", "{n} items", n)`, with `{n}` filled in.
/// Further placeholders can follow with [`fill`].
pub fn trn(singular: &str, plural: &str, n: u64) -> String {
    let text = match lookup(singular) {
        Some(forms) => forms
            .get(plural_index(current(), n))
            .or(forms.last())
            .copied()
            .unwrap_or(plural),
        None if n == 1 => singular,
        None => plural,
    };
    text.replace("{n}", &n.to_string())
}

/// Fills `{name}` placeholders in already translated text.
pub fn fill(mut text: String, args: &[(&str, &str)]) -> String {
    for (k, v) in args {
        text = text.replace(&format!("{{{k}}}"), v);
    }
    text
}

/// The plural form for `n`, as the language's `Plural-Forms` header says.
fn plural_index(lang: Language, n: u64) -> usize {
    match lang {
        Language::English => usize::from(n != 1),
        // nplurals=6; plural=n==0 ? 0 : n==1 ? 1 : n==2 ? 2 :
        //   n%100>=3 && n%100<=10 ? 3 : n%100>=11 ? 4 : 5;
        Language::Arabic => match (n, n % 100) {
            (0, _) => 0,
            (1, _) => 1,
            (2, _) => 2,
            (_, 3..=10) => 3,
            (_, 11..) => 4,
            _ => 5,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locales_and_plurals() {
        assert_eq!(Language::from_locale("ar_SA.UTF-8"), Language::Arabic);
        assert_eq!(Language::from_locale("ar-EG"), Language::Arabic);
        assert_eq!(Language::from_locale("en_US.UTF-8"), Language::English);
        assert_eq!(Language::from_locale("C"), Language::English);
        let forms: Vec<usize> = [0, 1, 2, 3, 10, 11, 99, 100, 102, 103]
            .into_iter()
            .map(|n| plural_index(Language::Arabic, n))
            .collect();
        assert_eq!(forms, [0, 1, 2, 3, 3, 4, 4, 5, 5, 3]);
        assert_eq!(fill("a {x} b".into(), &[("x", "1")]), "a 1 b");
    }
}
