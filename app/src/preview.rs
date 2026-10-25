//! Quick preview (Preview.dc.html): turns the first bytes of an entry into
//! something to look at: an image, text with line numbers, or a hex dump.
//! Runs on a worker; nothing here touches the UI.

use slint::{Rgba8Pixel, SharedPixelBuffer};

use crate::model;

/// How much of an entry the preview reads.
pub const IMAGE_LIMIT: u64 = 64 << 20;
pub const TEXT_LIMIT: u64 = 512 << 10;
pub const HEX_LIMIT: u64 = 64 << 10;
/// Images are scaled down to fit this, to keep memory in check.
const MAX_SIDE: u32 = 2048;
const MAX_LINES: usize = 20_000;

pub enum Content {
    Image {
        pixels: SharedPixelBuffer<Rgba8Pixel>,
        info: String,
    },
    /// SVG, which Slint renders itself.
    Svg(Vec<u8>),
    Text {
        lines: Vec<String>,
        language: &'static str,
        info: String,
    },
    Hex {
        lines: Vec<String>,
        info: String,
    },
}

/// How many bytes to read for `name`.
pub fn limit_for(name: &str) -> u64 {
    if is_image(name) || ext(name) == "svg" {
        IMAGE_LIMIT
    } else {
        TEXT_LIMIT
    }
}

/// Builds the preview for `name` from its first bytes (`size` is the whole
/// entry's size).
pub fn prepare(name: &str, data: &[u8], size: u64) -> Content {
    if ext(name) == "svg" {
        return Content::Svg(data.to_vec());
    }
    if is_image(name)
        && let Some(content) = image(data, size)
    {
        return content;
    }
    match text(data) {
        Some(text) => {
            let mut lines: Vec<String> = text.lines().take(MAX_LINES).map(expand_tabs).collect();
            if lines.is_empty() {
                lines.push(String::new());
            }
            let shown = data.len() as u64;
            let info = if shown < size {
                format!(
                    "First {} lines · {} of {}",
                    model::thousands(lines.len() as u64),
                    model::size_text(shown),
                    model::size_text(size)
                )
            } else {
                format!(
                    "{} · {}",
                    model::count(lines.len() as u64, "line"),
                    model::size_text(size)
                )
            };
            Content::Text {
                lines,
                language: language(name),
                info,
            }
        }
        None => {
            let shown = &data[..data.len().min(HEX_LIMIT as usize)];
            Content::Hex {
                lines: hex_lines(shown),
                info: if (shown.len() as u64) < size {
                    format!(
                        "Showing the first {} of {}",
                        model::size_text(shown.len() as u64),
                        model::size_text(size)
                    )
                } else {
                    model::size_text(size)
                },
            }
        }
    }
}

fn ext(name: &str) -> String {
    name.rsplit_once('.')
        .map(|(_, e)| e.to_ascii_lowercase())
        .unwrap_or_default()
}

fn is_image(name: &str) -> bool {
    matches!(
        ext(name).as_str(),
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "ico"
    )
}

fn image(data: &[u8], size: u64) -> Option<Content> {
    let img = image::load_from_memory(data).ok()?;
    let (w, h) = (img.width(), img.height());
    let img = if w > MAX_SIDE || h > MAX_SIDE {
        img.resize(MAX_SIDE, MAX_SIDE, image::imageops::FilterType::Triangle)
    } else {
        img
    };
    let rgba = img.to_rgba8();
    let pixels = SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(
        rgba.as_raw(),
        rgba.width(),
        rgba.height(),
    );
    Some(Content::Image {
        pixels,
        info: format!("{w} × {h} · {}", model::size_text(size)),
    })
}

/// The bytes as text, if they look like text: no NUL bytes, and valid UTF-8
/// (a character cut off at the end of what was read is fine).
pub fn text(data: &[u8]) -> Option<String> {
    if data[..data.len().min(8192)].contains(&0) {
        return None;
    }
    match std::str::from_utf8(data) {
        Ok(s) => Some(s.to_owned()),
        Err(e) if e.error_len().is_none() => {
            Some(String::from_utf8_lossy(&data[..e.valid_up_to()]).into_owned())
        }
        Err(_) => None,
    }
}

fn expand_tabs(line: &str) -> String {
    line.trim_end_matches('\r').replace('\t', "    ")
}

/// `00000010  52 47 42 20 58 59 5A 20  RGB XYZ ` (8 bytes per line, like
/// the mockup).
pub fn hex_lines(data: &[u8]) -> Vec<String> {
    data.chunks(8)
        .enumerate()
        .map(|(i, chunk)| {
            let hex: Vec<String> = chunk.iter().map(|b| format!("{b:02X}")).collect();
            let ascii: String = chunk
                .iter()
                .map(|&b| {
                    if (0x20..0x7f).contains(&b) {
                        b as char
                    } else {
                        '.'
                    }
                })
                .collect();
            format!("{:08x}  {:<24} {ascii}", i * 8, hex.join(" "))
        })
        .collect()
}

pub fn language(name: &str) -> &'static str {
    match ext(name).as_str() {
        "rs" => "Rust",
        "py" => "Python",
        "js" | "mjs" => "JavaScript",
        "ts" | "tsx" => "TypeScript",
        "c" | "h" => "C",
        "cpp" | "cc" | "hpp" => "C++",
        "go" => "Go",
        "java" => "Java",
        "kt" => "Kotlin",
        "sh" | "bash" => "Shell",
        "md" => "Markdown",
        "json" => "JSON",
        "toml" => "TOML",
        "yaml" | "yml" => "YAML",
        "xml" => "XML",
        "html" | "htm" => "HTML",
        "css" => "CSS",
        "sql" => "SQL",
        "csv" => "CSV",
        _ => "Text",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_or_binary() {
        assert_eq!(text(b"hello\n").as_deref(), Some("hello\n"));
        assert_eq!(text(b"a\0b"), None);
        assert_eq!(text(&[0xff, 0xfe, 0x41]), None);
        // A multi-byte character cut off by the read limit.
        let mut cut = "مرحبا".as_bytes().to_vec();
        cut.pop();
        assert_eq!(text(&cut).as_deref(), Some("مرح\u{0628}"));
    }

    #[test]
    fn hex_dump() {
        let lines = hex_lines(b"\x00\x00\x02\x24appl\x04");
        assert_eq!(lines[0], "00000000  00 00 02 24 61 70 70 6C  ...$appl");
        assert_eq!(lines[1], "00000008  04                       .");
    }

    #[test]
    fn prepares_each_kind() {
        match prepare("src/main.rs", b"fn main() {\n\tprintln!();\n}\n", 26) {
            Content::Text {
                lines,
                language,
                info,
            } => {
                assert_eq!(lines, ["fn main() {", "    println!();", "}"]);
                assert_eq!(language, "Rust");
                assert_eq!(info, "3 lines · 26 bytes");
            }
            _ => panic!("text expected"),
        }
        assert!(matches!(
            prepare("a.bin", &[0, 1, 2], 3),
            Content::Hex { .. }
        ));
        // A PNG made on the fly.
        let mut png = Vec::new();
        image::RgbaImage::new(3, 2)
            .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .unwrap();
        match prepare("x.png", &png, png.len() as u64) {
            Content::Image { pixels, info } => {
                assert_eq!((pixels.width(), pixels.height()), (3, 2));
                assert!(info.starts_with("3 × 2 · "));
            }
            _ => panic!("image expected"),
        }
        // Not really a PNG: falls back to hex.
        assert!(matches!(
            prepare("fake.png", &[0, 9, 9], 3),
            Content::Hex { .. }
        ));
        assert_eq!(limit_for("a.jpg"), IMAGE_LIMIT);
        assert_eq!(limit_for("a.txt"), TEXT_LIMIT);
    }
}
