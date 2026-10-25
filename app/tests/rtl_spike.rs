//! M5 spike: how Slint 1.18 handles right-to-left text (bidi, Arabic shaping,
//! font fallback from "Crate Inter") and whether layouts mirror on their own.
//! Writes `rtl-*.png` to `target/ui-snapshots/`; the finding is in the M5 notes.
//! Throwaway: it asserts nothing about the look, it only takes pictures.

#[path = "../../crates/crate-core/tests/support/mod.rs"]
mod support;

mod common;

use std::io::Write;

use common::{Harness, g, rows};
use slint::ComponentHandle;

slint::slint! {
    import "../assets/fonts/inter/Inter-Medium.ttf";
    import "../assets/fonts/inter/Inter-SemiBold.ttf";

    component Case inherits Rectangle {
        in property <string> label;
        height: 34px;
        HorizontalLayout {
            spacing: 8px;
            Text { text: root.label; width: 110px; font-size: 11px; color: #667085; vertical-alignment: center; }
            Rectangle {
                background: #EEF1FD;
                HorizontalLayout { @children }
            }
        }
    }

    component Row inherits HorizontalLayout {
        // The mini view's row pattern: tile, name, size.
        in property <string> name;
        spacing: 10px;
        padding-left: 6px;
        padding-right: 6px;
        Rectangle { width: 24px; height: 24px; y: 5px; border-radius: 6px; background: #3352E0;
            Text { text: "PDF"; color: white; font-size: 8px; font-weight: 700; }
        }
        Text { text: root.name; vertical-alignment: center; horizontal-stretch: 1; }
        Text { text: "2.1 م.ب"; vertical-alignment: center; color: #667085; }
    }

    export component Spike inherits Window {
        default-font-family: "Crate Inter";
        default-font-size: 14px;
        default-font-weight: 500;
        background: white;
        VerticalLayout {
            padding: 12px;
            spacing: 4px;
            Case { label: "Arabic, Crate Inter";
                Text { text: "استخراج الكل"; vertical-alignment: center; } }
            Case { label: "Noto Sans Arabic";
                Text { text: "استخراج الكل"; font-family: "Noto Sans Arabic"; vertical-alignment: center; } }
            Case { label: "IBM Plex Sans Ar.";
                Text { text: "استخراج الكل"; font-family: "IBM Plex Sans Arabic"; vertical-alignment: center; } }
            Case { label: "weight 600";
                Text { text: "تقرير الربع الثالث.docx"; font-weight: 600; vertical-alignment: center; } }
            Case { label: "bidi: guillemets";
                Text { text: "استخراج الكل إلى «عقود-2019/»"; vertical-alignment: center; } }
            Case { label: "bidi: path";
                Text { text: "~/Downloads/عقود-2019/"; vertical-alignment: center; } }
            Case { label: "bidi: numbers";
                Text { text: "24 عنصرًا · 48 ميغابايت ← 31 ميغابايت"; vertical-alignment: center; } }
            Case { label: "Arabic digits";
                Text { text: "المساحة الموفّرة ٣٥٪ · 35%"; vertical-alignment: center; } }
            Case { label: "align: default";
                Text { text: "عقد الإيجار.pdf"; vertical-alignment: center; } }
            Case { label: "align: start";
                Text { text: "عقد الإيجار.pdf"; horizontal-alignment: start; vertical-alignment: center; } }
            Case { label: "align: end";
                Text { text: "عقد الإيجار.pdf"; horizontal-alignment: end; vertical-alignment: center; } }
            Case { label: "align: right";
                Text { text: "عقد الإيجار.pdf"; horizontal-alignment: right; vertical-alignment: center; } }
            Case { label: "elide: Arabic";
                Text { text: "تقرير الربع الثالث للعام المالي ٢٠٢٦ النسخة النهائية المعتمدة من مجلس الإدارة.docx"; overflow: elide; vertical-alignment: center; } }
            Case { label: "elide: right-aligned";
                Text { text: "تقرير الربع الثالث للعام المالي ٢٠٢٦ النسخة النهائية المعتمدة من مجلس الإدارة.docx"; overflow: elide; horizontal-alignment: right; vertical-alignment: center; } }
            Case { label: "wrap: Arabic";
                height: 52px;
                Text { text: "الأسماء في ملفات ZIP القديمة من Windows تُحفظ بلغة النظام. اختر الترميز الذي يجعلها مقروءة."; wrap: word-wrap; vertical-alignment: center; } }
            Case { label: "HorizontalLayout";
                Row { name: "عقد الإيجار.pdf"; } }
            Case { label: "Flexbox row-reverse";
                FlexboxLayout {
                    flex-direction: row-reverse;
                    flex-wrap: no-wrap;
                    padding-left: 6px;
                    padding-right: 6px;
                    spacing: 10px;
                    alignment: stretch;
                    cross-axis-alignment: center;
                    Rectangle { width: 24px; height: 24px; border-radius: 6px; background: #3352E0;
                        Text { text: "PDF"; color: white; font-size: 8px; font-weight: 700; } }
                    Text { text: "عقد الإيجار.pdf"; horizontal-alignment: right; horizontal-stretch: 1; }
                    Text { text: "2.1 م.ب"; color: #667085; }
                } }
            Case { label: "TextInput";
                TextInput { text: "البحث في الأرشيف"; vertical-alignment: center; } }
        }
    }
}

#[test]
fn rtl_spike() {
    let h = Harness::new();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("عقود-2019.zip");
    arabic_zip(&path);

    // The real mini view and full view with Arabic names (English UI).
    let ui = h.open(&path);
    h.until("listing", || g(&ui).get_status() == "ready");
    println!("rows: {:?}", rows(&ui));
    h.shot("rtl-01-mini");
    g(&ui).invoke_toggle_full();
    g(&ui).invoke_row_activated(0);
    h.shot("rtl-02-full");

    // The test sheet; it gets its own window from the harness platform.
    let spike = Spike::new().unwrap();
    spike
        .window()
        .set_size(slint::LogicalSize::new(520.0, 740.0));
    spike.show().unwrap();
    h.shot("rtl-03-cases-1x");
    h.window()
        .dispatch_event(slint::platform::WindowEvent::ScaleFactorChanged { scale_factor: 2.0 });
    spike
        .window()
        .set_size(slint::LogicalSize::new(520.0, 740.0));
    h.shot("rtl-03-cases-2x");
}

/// The names from `MiniViewRTL.dc.html`, in UTF-8.
fn arabic_zip(path: &std::path::Path) {
    use zip::write::SimpleFileOptions;
    let mut zip = zip::ZipWriter::new(std::fs::File::create(path).unwrap());
    let opts = SimpleFileOptions::default();
    for name in [
        "عقود/عقد الإيجار.pdf",
        "عقود/قائمة الأسعار.xlsx",
        "عقود/تقرير الربع الثالث.docx",
        "عقود/ملحق-أ.pdf",
        "عقود/صور الرحلة/scan_0042.jpg",
        "عقود/README.txt",
    ] {
        zip.start_file(name, opts).unwrap();
        zip.write_all("مرحبا\n".repeat(200).as_bytes()).unwrap();
    }
    zip.finish().unwrap();
}
