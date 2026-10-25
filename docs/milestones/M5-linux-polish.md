# M5: Linux polish

**Goal:** everything around the core that makes Crate feel finished on Ubuntu.
**Done when:** a fresh Ubuntu user can install the dev build, go through first run, and use Crate in English or Arabic, in light or dark mode.

## Tasks
- [ ] Operations panel: queue, pause/resume, cancel, history, desktop notifications (`Progress.dc.html`)
- [ ] Batch extract, each archive into its own folder (`Batch.dc.html`)
- [ ] Nested archives and missing volumes (`EdgeCases.dc.html`)
- [ ] Preferences screen (`Settings.dc.html`), saved settings
- [ ] First run (`FirstRun.dc.html`): when-opening choice, file types, **"Make Crate the default"** on Linux:
  - .deb/.rpm/AppImage: run `xdg-mime default crate.desktop <mime types>` after the user confirms
  - Flatpak: can't set defaults from inside the sandbox. Show the steps instead (Files → Properties → Open With → Set as default)
- [ ] Dark mode on every screen (token table in `design/DESIGN.md`)
- [ ] Translations with Slint's `@tr()` + gettext `.po` files, + **Arabic** with a mirrored right-to-left layout (`BrowseRTL.dc.html`, `MiniViewRTL.dc.html`). [Hypothesis] Check Slint's right-to-left support early (bidi text, whether layouts mirror on their own); mirror layouts explicitly if they don't
- [ ] Filename encoding picker (`MiniViewRTL.dc.html`)
- [ ] Keyboard shortcuts sheet (`Shortcuts.dc.html`)
- [ ] Accessibility pass: everything reachable by keyboard, `accessible-label` on icon buttons, Orca screen-reader spot check (Slint → AccessKit → AT-SPI)
- [ ] About & support screen (`About.dc.html`)

## Notes
### Right-to-left spike (Slint 1.18.1, checked 2026-10-26)
Spike: `app/tests/rtl_spike.rs`. It opens an archive with the Arabic names from `MiniViewRTL.dc.html` in the real mini and full views, then draws a sheet of test cases. Captures are in [`m5-rtl/`](m5-rtl/): `real-mini-1x.png` (the real window, femtovg at 1×, dark theme), `headless-mini-2x.png`, `headless-full-2x.png`, `cases-1x.png`, `cases-2x.png`. Both femtovg and the software renderer lay out text with the same parley + harfrust code, so the results match.

**Works on its own**
- Arabic shaping: letters join, harakat and lam-alef are right, Arabic-Indic digits (٣٥٪) are fine.
- Bidi (Unicode bidi algorithm): each paragraph takes its base direction from its first strong character. So `عقود-2019.zip` displays as `zip.2019-عقود`, the same as a browser with `dir="auto"`. A path like `~/Downloads/عقود-2019/` stays LTR, with the Arabic run reordered inside it. Wrapped Arabic paragraphs break correctly.
- Font fallback: "Crate Inter" has no Arabic glyphs, so fontique asks fontconfig per script and gets the system's **Noto Sans Arabic** on Ubuntu. Spaces still come from Inter. Weight 500 renders Noto Sans Arabic *Regular*, because this Ubuntu install has only the Regular and Bold files. Next to Inter Medium, Arabic text therefore looks lighter.
- A font the app registers itself is *not* used for fallback. Tested by registering Noto Naskh Arabic in the spike: the system Noto Sans Arabic still won. A `Text` takes one `font-family`, followed by the generic families only, with no family list.
- Window titles in Arabic are fine (checked under XWayland).

**Has to be done by hand**
- **No layout mirroring.** Slint has no layout-direction setting. `HorizontalLayout` always places children left to right (case "HorizontalLayout"). `FlexboxLayout { flex-direction: rtl ? row-reverse : row; flex-wrap: no-wrap; }` does mirror, and its direction can change at runtime (case "Flexbox row-reverse"). Elements placed with `x:` need `rtl ? parent.width - self.width - x : x`.
- **`horizontal-alignment: start` isn't direction-aware.** The docs say it follows the text direction, but `i-slint-core/textlayout/sharedparley/layout.rs` maps `Start` to parley `Left` (and `End` to `Right`). Arabic text with the default or `start` alignment sits on the left. Every `Text` and `TextInput` (the search field too) needs `horizontal-alignment: Locale.rtl ? right : left`.
- **`overflow: elide` is broken for RTL lines.** No "…" is drawn and the line spills out of its box to the left, over the tile and other neighbours (see the long name in `real-mini-1x.png` and the "elide" cases). The elision code walks glyphs left to right in visual order and assumes the line's end is on the right. This hits Arabic file names in the *English* UI too, so it's a bug today, not only in M5. Proposed: report it upstream with the spike's minimal case. Meanwhile, put `clip: true` on name cells so the overflow can't paint over neighbours.
- Directional icons (chevrons, the `→` in sizes, breadcrumb `›`, back) must be swapped by hand in RTL.

**Font: not chosen yet (needs a decision)**
Without a bundled font, Arabic depends on what the system has. Ubuntu desktop has Noto Sans Arabic; a minimal Flatpak runtime, Windows and macOS each have their own (or none). Because registered fonts aren't used for fallback, a bundled Arabic font only shows where it is named. So the plan would be `default-font-family: Locale.rtl ? "Crate Arabic" : "Crate Inter"` (registered under its own name, as Inter is), with Latin glyphs coming from the Arabic font in the Arabic UI. Arabic file names in the English UI keep using the system fallback. Candidates, both OFL:
  - **IBM Plex Sans Arabic**: what the mockups use; static weights including Medium (500) and SemiBold (600), so it matches the 500/600 weights; has its own Latin set.
  - **Noto Sans Arabic**: what Ubuntu shows today, so the look wouldn't change; the static files have Regular and Bold only.
  - (Later, if Arabic names in the English UI must match: merge the Arabic glyphs into "Crate Inter" with fontTools. That's more work, and Inter's units-per-em differ from both fonts above.)

**Proposed approach for `@tr()` and gettext**
- Mark strings in `.slint` with `@tr()`, with plurals as `@tr("{n} item" | "{n} items" % n)`. Extract them with `slint-tr-extractor` into `app/translations/crate-app.pot`, with `app/translations/ar/LC_MESSAGES/crate-app.po` for Arabic (Arabic has 6 plural forms).
- **Bundle the translations** (`slint_build::CompilerConfiguration::with_bundled_translations("translations")`) instead of the runtime `gettext` feature. Nothing to install in `share/locale`, the same in AppImage, Flatpak, Windows and macOS, and no libintl. Slint picks the bundled language from the system locale on its own. `slint::select_bundled_translation("ar")` switches it at runtime, for the Preferences language setting.
- Slint has no public API to translate a string from Rust (`translate` is in `private_unstable_api`). About 120 `format!` calls in `app/src` build text, so text the user sees moves into a Slint global with pure functions, e.g. `Strings.extracted(n, name)` with `@tr` inside. Rust calls `invoke_extracted(…)`, so there's one catalog for both sides.
- Add a `Locale` global (`rtl: bool`, set from Rust from the chosen or system language). Every mirrored layout, alignment and icon reads it, as described above.

## Acceptance (on Ubuntu)
- [ ] First run → "Make Crate the default" → double-clicking any .zip opens Crate
- [ ] Switching the language to Arabic mirrors the whole UI, with no clipped text
- [ ] The whole mini view → extract flow works with the keyboard only

## Prompt for Claude Code
> Read CLAUDE.md and docs/milestones/M5-linux-polish.md. One task per session; open the referenced design board for each. Detect Flatpak via the `FLATPAK_ID` env var to choose the default-app flow.
