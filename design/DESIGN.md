# Crate: cross-platform archiver, design notes

Working name: **Crate**. Targets: most Linux distros, Windows 10/11, macOS 12+.
The canvas source files are in `design/canvas/`: one `.dc.html` file per screen, plus `canvas.json`.

## Screens and the actions each one shows

| Screen | File | Actions mocked |
|---|---|---|
| **Mini view (default on double-click)** | `MiniView.dc.html` (interactive) | Archive summary (size, ratio, tested, encrypted), search, top-level items, click to select, one-click "Extract all to name/", split menu, "Extract to…", "Open in full view". Drawn with Linux, Windows 11 and macOS window frames |
| Home | `Main.dc.html` | Drop to open or compress, New archive, Open, quick actions (Compress, Extract, Convert, Test), recent archives, locations, running-operations widget |
| Browse archive (full view) | `Browse.dc.html` (interactive) | Folder tree, breadcrumbs, filter, multi-select, Add files, New folder, Extract selected, Preview, Rename, Delete, Test, details pane (CRC, method, ratio), Copy path, Extract all, Properties |
| Dark mode sample | `Dark.dc.html` | The Browse screen with dark colors (reference for the token map below) |
| Add files | `AddFiles.dc.html` | Drag and drop into a folder, Shift = archive root, new files inherit compression and encryption, warning about name conflicts |
| Context menus | `ContextMenu.dc.html` | Right-click inside the app, plus file-manager menu entries |
| Extract dialog | `Extract.dc.html` | What to extract, destination, smart folder, if-file-exists rule, password, free-space check |
| Create archive | `Create.dc.html` | Presets, contents, exclude patterns, format, compression level, password, split into volumes, keep permissions |
| Prompts & errors | `Dialogs.dc.html` | Password, file conflict, delete, rename, damaged archive, convert format |
| Operations | `Progress.dc.html` | Progress, pause, resume, cancel, queue, history, desktop notification |
| Properties | `Properties.dc.html` | Size stats, breakdown by file type, integrity check, checksums, paste a checksum to verify it |
| Preferences | `Settings.dc.html` | Theme, defaults, CPU threads, excludes, integration, file associations |
| About & support | `About.dc.html` | Version and update status, GitHub Sponsors / Patreon / PayPal buttons, project links, "Support Crate…" in the main menu, a one-time support nudge |
| Crate Pro | `Pro.dc.html` | Upgrade sheet, license key entry and its errors, activated, the Crate Pro group in Preferences, PRO labels on the Glass theme and app icons |

## Mini view rules

- Double-clicking an archive opens the **mini view** (a 520 × 620 window). The full view is one click away and remembers the window size it was last opened at.
- The mini view and the full view are the **same component** in two density modes. Do not build a separate UI for each.
- It is not the same thing as a phone-width layout. The mini view is a separate window with a limited set of actions; responsive layout just reflows the full view.
- Preference: "When opening an archive: Mini view / Full view / Extract right away".

## Color tokens (light → dark)

In the app the tokens are properties of the Slint global `Tokens` (`app/ui/tokens.slint`), switched by the system color scheme or the theme setting. The mockups use CSS variables with the same names.

| Token | Light | Dark |
|---|---|---|
| `--bg` | `#FFFFFF` | `#0F1115` |
| `--surface-2` (sidebar, details) | `#F7F8FA` | `#12151B` |
| `--chrome` (header, footer) | `#FBFBFC` | `#14171D` |
| `--line` | `#E6E8EC` | `#232832` |
| `--ink` | `#0F172A` | `#E7EAF0` |
| `--ink-2` | `#475467` | `#A9B0BD` |
| `--ink-3` | `#667085` | `#8A93A3` |
| `--accent` (button background) | `#3352E0` | `#4A65F0` |
| `--accent-soft` / text on soft | `#EEF1FD` / `#2A44C2` | `#1E2747` / `#AFC0FF` |
| `--selection` | `#EEF1FD` | `#1A2240` |
| `--success` | `#0E7A4F` | `#3DD68C` |
| `--warning` | `#B45309` | `#F5A524` |
| `--danger` | `#C2261B` | `#FF8A80` |
| `--crypto` | `#6B3FC4` | `#CDB4FA` |
| `--pro` (the PRO label) | `#1E2F8F` | `#1E2F8F` |

The Glass theme (Pro, archive windows) is light only and overrides these: `--bg` (panes) `rgba(255,255,255,.55)`, `--surface-2` `rgba(255,255,255,.45)`, `--chrome` transparent, `--line` `rgba(255,255,255,.7)`, `--ink-2` `#1D2939`, `--ink-3` `#334155`, `--accent` `rgba(51,82,224,.9)`, `--accent-soft` `rgba(51,82,224,.16)` / `#1E3399`, `--selection` `rgba(51,82,224,.18)`; the window itself `rgba(255,255,255,.40)`, and dialogs over it (`--sheet`) `rgba(255,255,255,.94)`.

Font: Inter 400/500/600/700, compiled into the app from `assets/fonts/inter` (Inter 4.1, OFL). Never load fonts at runtime. Monospace text uses the system monospace font.

## Cross-platform implementation (revised)

- **Decision (Oct 24): Slint** (Rust UI toolkit, `.slint` markup). The M0 startup spike showed the mini view in 282 ms cold with Slint against 576 ms with Tauri 2 + WebKitGTK; the budget is 500 ms (`docs/perf.md`). Cost: the HTML/CSS mockups no longer move over directly, so each screen is rebuilt in `.slint` with the mockup as the reference. Tauri was the earlier pick; GTK4/libadwaita looks wrong on Windows and macOS; Electron ships installers of roughly 100 MB or more.
- [Likely] Archive engine: pure-Rust crates (`zip`, `tar`, `zstd`, `xz2`, `bzip2`, a 7z crate such as `sevenz-rust2`). These avoid bundling native libarchive on Windows and macOS. Use the `unrar` crate for RAR, extract only.
- [Hypothesis] Check AES-256 *write* support in the current `zip` crate and 7z encryption support before committing to those features.
- Window frame: use the **native title bar** on Windows and macOS (macOS can use the overlay title-bar style). Draw the header bar only on Linux. The mini view board shows all three.
- Packaging:
  - Linux: AppImage + Flatpak (Flathub) + .deb/.rpm
  - Windows: MSI or NSIS installer + winget
  - macOS: universal .dmg
  - Builds via GitHub Actions on each OS. [Hypothesis] `cargo-packager` (AppImage, .deb, NSIS/MSI, .dmg) + `cargo-generate-rpm` (.rpm); confirm in M6
- [Likely] Code signing:
  - Unsigned Windows builds trigger SmartScreen warnings.
  - Unsigned macOS builds are blocked by Gatekeeper, so you need Apple Developer membership and notarization.
  - Budget for both, or say so up front in the README.
- [Likely] File-manager integration is three separate projects:
  - Linux: Nautilus / Dolphin / Thunar entries
  - Windows: registry verbs, which are easy but appear only under "Show more options" (the IExplorerCommand version is much harder)
  - macOS: a Finder Quick Action or a Finder Sync extension
  - Ship v1 with "Open with" file associations only, and add these menus later.

## Engineering flags

- [Certain] RAR creation needs a paid license, so RAR is extract-only.
- [Certain] tar.* has no built-in encryption. The age-encryption option in Create is non-standard (whoever opens the archive also needs age). The simpler choice is to offer passwords only for ZIP and 7z.
- [Hypothesis] "Repair" is realistic only for ZIP (rebuilding its file index) and for RAR files that include recovery data.
- **Grid view: kept.** Grid = thumbnails for archives that are mostly images or videos. Suggested default: switch to grid automatically when more than 60% of the items in a folder are media, and remember the user's choice per archive. Generate thumbnails lazily, only for the tiles on screen.

## Windows signing (no paid Windows certificate)

> **Decision (Oct 21):** ship Windows builds unsigned. The README explains the SmartScreen prompt and every release publishes checksums. The options below are kept for reference only.

- [Likely] Unsigned .exe/.msi files trigger SmartScreen's "Windows protected your PC" warning until the app builds up a download reputation.
- Options, simplest first:
  1. **Microsoft Store**: registration is free for individual developers (since Sept 2025), and the Store signs the MSIX package for you. That gives clean installs plus Store discovery.
  2. **SignPath Foundation**: free code signing for open-source projects, built into GitHub Actions. Requires an OSI-approved license and an approval step.
  3. Ship unsigned on GitHub Releases / winget and document the "More info → Run anyway" step in the README.
- [Hypothesis] Azure Trusted Signing (~$10/month) accepts individuals only from a few countries, which is likely not Saudi Arabia. Verify before relying on it.

## Not designed yet (backlog)

> The live tracker with status for every item is **BACKLOG.md**. The list below is the original snapshot.

**Correctness & security (before v1)**
- Path-traversal ("zip-slip") and symlink-escape protection; zip-bomb ratio warning
- Filename encoding picker for old ZIPs (CP1256 Arabic, CP437, Shift-JIS) to fix garbled names
- Windows reserved names (CON, AUX…), illegal characters, long paths (>260 characters); files whose names differ only by case on macOS/Windows
- Mark-of-the-Web: copy the Windows "downloaded from the internet" flag onto extracted files
- Remove partial output when an operation is cancelled or fails; disk-full and permission-denied errors; never touch the original archive until a rewrite succeeds

**Workflows not drawn**
- Drag files *out* of the archive to the desktop or file manager
- Open a file from inside the archive → edit → "Update archive with your changes?"
- Batch: select 10 archives → extract each into its own folder
- Nested archives (a zip inside a zip) and missing volumes in multi-part sets (".part2 missing — locate…")
- Quick preview for text, code, images and binary (hex)
- First run: set Crate as the default app (Windows 10/11 can only send the user to Settings), plus the auto-update prompt
- Keyboard shortcuts sheet; Windows taskbar and macOS Dock progress; keep the computer awake during long operations
- Command-line companion (`crate x file.zip`)

**Reach**
- Arabic and other right-to-left languages: localization + mirrored layout
- Accessibility: full keyboard navigation, screen-reader labels, high contrast
- App icon and brand; AppStream metainfo + screenshots for Flathub; Store listings; landing page
- Name check: "Crate" collides with Rust's word for a package, which hurts search

## Glass theme (Pro)

Boards: `GlassMini.dc.html`, `GlassBrowse.dc.html`.

- Recipe:
  - Window: `rgba(255,255,255,.34–.40)` + `backdrop-filter: blur(30px) saturate(180%)`
  - 1 px white border at 70% opacity; inset top highlight
  - Panes float inside the window with a 12 px gap and 22 px radius; controls are pill-shaped
- Accent controls: `rgba(51,82,224,.9)` with an inset highlight and a coloured shadow.
- Text stays solid ink (`#0F172A` / `#1D2939` / `#334155`). Never use grey text below `#334155` on glass.
- [Likely] Real see-through blur depends on the OS window. The `window-vibrancy` crate can request macOS vibrancy and Windows Mica/Acrylic through the window handle that Slint's winit backend exposes.
- [Hypothesis] Slint has no `backdrop-filter`: a pane inside the window can't blur what is behind it. Get the blur from the OS material behind a transparent window, and draw panes as translucent fills on top.
- [Hypothesis] Linux has no standard window-blur API: KDE can blur, GNOME generally can't. Plan a fallback there: a blurred copy of the wallpaper, or a frosted solid colour.
- Respect the OS "Reduce transparency" setting: switch to solid panes.

### Per-platform variants

| | macOS | Ubuntu (GNOME) | Windows 11 |
|---|---|---|---|
| Boards | GlassMini / GlassBrowse | GlassMiniUbuntu / GlassBrowseUbuntu | GlassMiniWin / GlassBrowseWin |
| Window radius | 28–30 px | 16 px | 8 px |
| Control radius | pills | 10–12 px | 4–6 px |
| Window controls | traffic lights, left | round buttons, right; menu in the header bar | 46 × 32 caption buttons; Close turns red on hover |
| Material | vibrancy (true blur) | **Frosted** by default: translucent window, near-opaque panes, no blur. **Blur** where the compositor provides it (KDE, or GNOME with a blur extension) | **Acrylic** = see-through blur (Win 10 + 11) · **Mica** = wallpaper-tinted, opaque (Win 11 only) |
| Selection | solid accent fill | solid accent fill | white fill + 3 px accent bar on the left (Fluent style) |

All variants share the same layout and colour tokens; only the shape, chrome and material change.

## Donations

> **Decision (Oct 22):** Crate is mainly an open-source showcase, with a small optional **Pro** tier (see BACKLOG.md section 5). Donations stay low-key.

- [Likely] Use **GitHub Sponsors** as the main button: it sits on the repo page and charges no fees on sponsorships from personal accounts. Keep Patreon and PayPal as alternatives.
- [Hypothesis] Check that GitHub Sponsors pays out to your country (payouts go through Stripe Connect). If it doesn't, make PayPal the main button.
- Placement:
  - `.github/FUNDING.yml`
  - README badge
  - About dialog
  - "Support Crate…" in the main menu
  - At most one gentle in-app nudge, after 50 successful operations, with a "Don't show again" option
  - Never block a task with a donation prompt
