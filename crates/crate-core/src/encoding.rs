//! Decoding entry names that aren't UTF-8 (old ZIPs made on Windows, mostly).

use chardetng::{EncodingDetector, Iso2022JpDetection, Utf8Detection};
use encoding_rs::Encoding;

/// A character set for entry names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NameCodec {
    Utf8,
    /// DOS code pages, which `encoding_rs` doesn't cover.
    Cp437,
    Cp720,
    Cp866,
    /// Everything in the WHATWG Encoding Standard (CP1256, Shift-JIS, GBK…).
    Web(&'static Encoding),
}

impl NameCodec {
    /// Parses a user-facing label: `utf-8`, `cp437`, `cp720`, `cp866`,
    /// `cp1256`, `cp1251`, `shift-jis`, `gbk`, or any WHATWG label.
    pub fn from_label(label: &str) -> Option<Self> {
        let l = label.trim().to_ascii_lowercase().replace('_', "-");
        match l.as_str() {
            "utf-8" | "utf8" => return Some(NameCodec::Utf8),
            "cp437" | "ibm437" | "437" => return Some(NameCodec::Cp437),
            "cp720" | "ibm720" | "720" => return Some(NameCodec::Cp720),
            "cp866" | "ibm866" | "866" => return Some(NameCodec::Cp866),
            "cp932" | "sjis" | "shift-jis" => return Some(NameCodec::Web(encoding_rs::SHIFT_JIS)),
            "cp936" => return Some(NameCodec::Web(encoding_rs::GBK)),
            _ => {}
        }
        if let Some(e) = Encoding::for_label(l.as_bytes()) {
            return Some(NameCodec::Web(e));
        }
        // "cp1256" → "windows-1256"
        let n = l.strip_prefix("cp").or_else(|| l.strip_prefix("windows"))?;
        Encoding::for_label(format!("windows-{}", n.trim_start_matches('-')).as_bytes())
            .map(NameCodec::Web)
    }

    pub fn name(&self) -> &'static str {
        match self {
            NameCodec::Utf8 => "UTF-8",
            NameCodec::Cp437 => "CP437",
            NameCodec::Cp720 => "CP720",
            NameCodec::Cp866 => "CP866",
            NameCodec::Web(e) => e.name(),
        }
    }

    /// Readable name for pickers: `Arabic (Windows-1256)`.
    pub fn label(&self) -> String {
        let language = match self {
            NameCodec::Utf8 => return "Unicode (UTF-8)".into(),
            NameCodec::Cp437 => "DOS Latin",
            NameCodec::Cp720 => "DOS Arabic",
            NameCodec::Cp866 => "DOS Cyrillic",
            NameCodec::Web(e) => match e.name() {
                "windows-1250" => "Central European",
                "windows-1251" => "Cyrillic",
                "windows-1252" => "Western",
                "windows-1253" => "Greek",
                "windows-1254" => "Turkish",
                "windows-1255" => "Hebrew",
                "windows-1256" => "Arabic",
                "windows-1257" => "Baltic",
                "windows-1258" => "Vietnamese",
                "windows-874" => "Thai",
                "Shift_JIS" | "EUC-JP" => "Japanese",
                "GBK" | "gb18030" => "Chinese, Simplified",
                "Big5" => "Chinese, Traditional",
                "EUC-KR" => "Korean",
                "KOI8-R" | "KOI8-U" | "IBM866" => "Cyrillic",
                _ => return e.name().into(),
            },
        };
        let name = self.name();
        match name.strip_prefix("windows-") {
            Some(n) => format!("{language} (Windows-{n})"),
            None => format!("{language} ({name})"),
        }
    }

    /// The encodings old archives use most, for the "Fix names" picker.
    pub fn common() -> Vec<NameCodec> {
        use encoding_rs::*;
        let mut all = vec![NameCodec::Utf8, NameCodec::Cp437];
        all.extend(
            [
                WINDOWS_1256,
                WINDOWS_1251,
                WINDOWS_1252,
                WINDOWS_1250,
                WINDOWS_1253,
                WINDOWS_1254,
                WINDOWS_1255,
                SHIFT_JIS,
                GBK,
                BIG5,
                EUC_KR,
                WINDOWS_874,
            ]
            .map(NameCodec::Web),
        );
        all.extend([NameCodec::Cp866, NameCodec::Cp720]);
        all
    }

    pub fn decode(&self, bytes: &[u8]) -> String {
        match self {
            NameCodec::Utf8 => String::from_utf8_lossy(bytes).into_owned(),
            NameCodec::Cp437 => oem_cp::decode_string_complete_table(
                bytes,
                &oem_cp::code_table::DECODING_TABLE_CP437,
            ),
            NameCodec::Cp720 => oem_cp::decode_string_complete_table(
                bytes,
                &oem_cp::code_table::DECODING_TABLE_CP720,
            ),
            NameCodec::Cp866 => oem_cp::decode_string_complete_table(
                bytes,
                &oem_cp::code_table::DECODING_TABLE_CP866,
            ),
            NameCodec::Web(e) => e.decode_without_bom_handling(bytes).0.into_owned(),
        }
    }
}

/// How an archive's non-UTF-8 names were decoded. `guessed` is true when Crate
/// picked the encoding itself, so the UI can offer "Fix names"
/// (`MiniViewRTL.dc.html`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NameEncoding {
    pub codec: NameCodec,
    pub guessed: bool,
}

/// Decodes raw names. Valid UTF-8 names stay as they are unless an encoding is
/// forced; the rest are decoded with `forced` or a guess. Returns `None` for
/// the encoding when every name was UTF-8 and nothing was forced.
pub fn decode_names(
    raw: &[&[u8]],
    forced: Option<NameCodec>,
) -> (Vec<String>, Option<NameEncoding>) {
    if let Some(codec) = forced {
        let names = raw.iter().map(|b| codec.decode(b)).collect();
        return (
            names,
            Some(NameEncoding {
                codec,
                guessed: false,
            }),
        );
    }
    let legacy: Vec<&[u8]> = raw
        .iter()
        .copied()
        .filter(|b| std::str::from_utf8(b).is_err())
        .collect();
    if legacy.is_empty() {
        let names = raw
            .iter()
            .map(|b| String::from_utf8_lossy(b).into_owned())
            .collect();
        return (names, None);
    }
    let codec = guess(&legacy);
    let names = raw
        .iter()
        .map(|b| match std::str::from_utf8(b) {
            Ok(s) => s.to_owned(),
            Err(_) => codec.decode(b),
        })
        .collect();
    (
        names,
        Some(NameEncoding {
            codec,
            guessed: true,
        }),
    )
}

fn guess(names: &[&[u8]]) -> NameCodec {
    let mut detector = EncodingDetector::new(Iso2022JpDetection::Deny);
    for name in names {
        detector.feed(name, false);
        detector.feed(b"\n", false);
    }
    detector.feed(b"", true);
    let guess = detector.guess(None, Utf8Detection::Deny);
    // windows-1252 is chardetng's "no idea" answer; the ZIP spec's default for
    // names without the UTF-8 flag is CP437.
    if guess == encoding_rs::WINDOWS_1252 {
        NameCodec::Cp437
    } else {
        NameCodec::Web(guess)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels() {
        assert_eq!(
            NameCodec::from_label("cp1256"),
            Some(NameCodec::Web(encoding_rs::WINDOWS_1256))
        );
        assert_eq!(
            NameCodec::from_label("CP1251"),
            Some(NameCodec::Web(encoding_rs::WINDOWS_1251))
        );
        assert_eq!(
            NameCodec::from_label("shift_jis"),
            Some(NameCodec::Web(encoding_rs::SHIFT_JIS))
        );
        assert_eq!(
            NameCodec::from_label("gbk"),
            Some(NameCodec::Web(encoding_rs::GBK))
        );
        assert_eq!(NameCodec::from_label("cp437"), Some(NameCodec::Cp437));
        assert_eq!(NameCodec::from_label("utf8"), Some(NameCodec::Utf8));
        assert_eq!(NameCodec::from_label("klingon"), None);
    }

    #[test]
    fn readable_labels() {
        assert_eq!(
            NameCodec::from_label("cp1256").unwrap().label(),
            "Arabic (Windows-1256)"
        );
        assert_eq!(NameCodec::Cp437.label(), "DOS Latin (CP437)");
        let common = NameCodec::common();
        assert!(common.contains(&NameCodec::Web(encoding_rs::SHIFT_JIS)));
        assert!(common.iter().all(|c| c.label().contains('(')));
    }

    #[test]
    fn utf8_names_need_no_encoding() {
        let raw: Vec<&[u8]> = vec![b"a.txt", "تقرير.txt".as_bytes()];
        let (names, enc) = decode_names(&raw, None);
        assert_eq!(names, ["a.txt", "تقرير.txt"]);
        assert_eq!(enc, None);
    }

    #[test]
    fn arabic_cp1256_is_guessed() {
        let (bytes, _, _) = encoding_rs::WINDOWS_1256.encode("التقرير السنوي للشركة.txt");
        let (bytes2, _, _) = encoding_rs::WINDOWS_1256.encode("الصور/رحلة الصيف.jpg");
        let raw: Vec<&[u8]> = vec![&bytes, &bytes2, b"readme.txt"];
        let (names, enc) = decode_names(&raw, None);
        assert_eq!(
            enc,
            Some(NameEncoding {
                codec: NameCodec::Web(encoding_rs::WINDOWS_1256),
                guessed: true
            })
        );
        assert_eq!(names[0], "التقرير السنوي للشركة.txt");
        assert_eq!(names[2], "readme.txt");
    }

    #[test]
    fn forced_encoding_wins() {
        let raw: Vec<&[u8]> = vec![b"\x82\xa0.txt"];
        let (names, enc) = decode_names(&raw, Some(NameCodec::Cp437));
        assert_eq!(names, ["éá.txt"]);
        assert_eq!(
            enc,
            Some(NameEncoding {
                codec: NameCodec::Cp437,
                guessed: false
            })
        );
    }
}
