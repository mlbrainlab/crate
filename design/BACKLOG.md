# Crate: design & product backlog

The single list of everything that is designed, still to design, or still to decide.
Status: ✅ designed (board name) · 🎨 to design · 🛠 engineering only (no new screen) · ❓ decision needed · ✔ decided

Last updated: 2026-10-24

## Decisions log

| Decision | Status |
|---|---|
| App name: **Crate** | ✔ |
| Stack: Rust — Slint UI + pure-Rust archive crates (Tauri 2 dropped after the M0 startup spike, Oct 24) | ✔ |
| Goal: open-source showcase on GitHub, **not** a money-maker | ✔ |
| Windows: ship **unsigned** (no certificate, no Store for now); README explains SmartScreen | ✔ |
| macOS: sign + notarize with the existing Apple Developer account | ✔ |
| Grid view: keep, with auto-switch for media-heavy folders | ✔ |
| Donations: GitHub Sponsors + Patreon + PayPal, low-key (About, README, FUNDING.yml) | ✔ |
| "Pro" tier: some features paid, small income expected | ✔ wanted · ❓ model (see section 5) |
| Pro includes extra themes (first: Liquid Glass) + alternate app icons | ✔ |
| App icon: concept #1 "Open Lid" (Icons board); the other 7 stay as Pro alternate icons | ✔ (files in `assets/icons/`) |
| Animations: free, short and purposeful, respect "reduce motion"; Glass theme's extra motion is Pro | ❓ proposed |
| Startup speed is a priority: budget + measurement in M0, Slint as the fallback UI | ✔ |
| License: **MIT** (README badge + About screen) | ✔ |
| UI toolkit: **Slint** (replaced Tauri + Svelte 5; Slint met the startup budget, Tauri didn't, see `docs/perf.md`) | ✔ |

## 1. Correctness & security (before v1)

| # | Item | Status | Notes |
|---|---|---|---|
| 1.1 | Path-traversal ("zip-slip") + symlink-escape protection | ✅ EdgeCases · 🛠 | Blocked silently, reported afterwards |
| 1.2 | Decompression-bomb warning | ✅ EdgeCases | Threshold to decide during implementation |
| 1.3 | Filename encoding picker | ✅ MiniViewRTL · Settings | |
| 1.4 | Windows reserved names, illegal characters, paths over 260 characters | ✅ FileIssues | Auto-rename with preview |
| 1.5 | Names that differ only by case (macOS/Windows) | ✅ FileIssues | |
| 1.6 | Copy the Windows "downloaded from the internet" flag (Mark-of-the-Web) to extracted files | 🛠 | |
| 1.7 | Clean up partial output after cancel or failure | 🛠 | Mentioned in FileIssues (disk full) |
| 1.8 | Disk-full and permission-denied errors | ✅ FileIssues | |
| 1.9 | Never touch the original archive until a rewrite succeeds (write to temp, then swap) | 🛠 | |
| 1.10 | Verify the `zip` crate can *write* AES-256 and that a 7z crate can encrypt | ✔ verified | Decides which formats get the password toggle. 2026-10-24: `zip` 9 writes AES-256 (legacy ZipCrypto writing isn't public API); `sevenz-rust2` 0.24 writes AES-256 7z, encrypted file lists included. Both are used to build the M2 test archives |
| 1.11 | tar.* encryption: age, or no password for tar | ✔ decided: age | 2026-10-25: tar.* gets passwords through age (passphrase, scrypt), as `name.tar.zst.age`. Verified with `age` 0.12: streaming encrypt and decrypt of a tar.zst, wrong passphrase fails cleanly, unlocking takes ~0.2 s. Only Crate and the age CLI open these; the Create dialog says so |
| 1.12 | "Repair" scope: ZIP index rebuild + RAR recovery records only | ❓ | |

## 2. Workflows

| # | Item | Status | Notes |
|---|---|---|---|
| 2.1 | Drag files out of the archive | ✅ DragOut · 🛠 blocked | Checked in M3 (2026-10-25): Slint 1.18’s `DragArea` only drops inside the same window, and winit 0.30 can only *receive* drops (X11, Windows, macOS; not Wayland). On Wayland a drag must start with the serial of the pointer press, which winit keeps internal, so it needs a winit/Slint patch or upstream support (winit issue “drag and drop source”). X11 alone could use a hand-written XDND source, but GNOME defaults to Wayland. Meanwhile: double-click opens an item, and Extract / Extract to Downloads cover the rest. Revisit when winit or Slint gains a drag source |
| 2.2 | Open → edit → update archive | ✅ EditUpdate | |
| 2.3 | Batch extract | ✅ Batch | |
| 2.4 | Nested archives | ✅ EdgeCases | |
| 2.5 | Missing volume in multi-part sets | ✅ EdgeCases | |
| 2.6 | First run + file associations | ✅ FirstRun | |
| 2.7 | Update prompt | ✅ FirstRun | Skipped for Store/Flatpak installs |
| 2.8 | Mini view: extract progress + done popup | ✅ MiniView | |
| 2.9 | Quick preview (image, code, hex) | ✅ Preview | |
| 2.10 | Grid view with thumbnails + auto-switch | ✅ Grid · Settings | |
| 2.11 | Keyboard shortcuts sheet (Linux/Windows + macOS keys) | ✅ Shortcuts | |
| 2.12 | Windows taskbar / macOS Dock progress; keep the computer awake | 🛠 | |
| 2.13 | Command-line companion (`crate x file.zip`) | 🛠 | Shown in README |
| 2.14 | "When opening an archive" preference | ✅ Settings | |
| 2.15 | Huge archives: loading state, search while loading, draw only visible rows | ✅ HugeArchive | |
| 2.16 | Full-view window frames for Windows & macOS | ✅ PlatformFull | |
| 2.17 | Dark mode for the remaining screens | 🛠 | Token table in DESIGN.md |

## 3. Reach

| # | Item | Status | Notes |
|---|---|---|---|
| 3.1 | Arabic UI + right-to-left layout | ✅ MiniViewRTL · BrowseRTL | |
| 3.2 | Other translations | 🛠 | |
| 3.3 | Accessibility: keyboard navigation, screen readers, high contrast | 🛠 | |
| 3.4 | App icon | ✅ Icons (4 concepts) · ❓ pick one | Then export .icns / .ico / Linux PNG sizes |
| 3.5 | Flathub AppStream metainfo + screenshots | 🛠 | Store listings dropped |
| 3.6 | README / landing page | ✅ Readme | GitHub Pages optional |
| 3.7 | Name check | ✔ keep "Crate" | |
| 3.8 | File-manager menus per OS | 🛠 | v1 = file associations only |

## 4. GitHub showcase (engineering)

| # | Item | Status |
|---|---|---|
| 4.1 | Workspace split: `crate-core` library + `crate-cli` + Slint app | 🛠 |
| 4.2 | CI on Linux/Windows/macOS; release builds and packages (tool chosen in M6) | 🛠 |
| 4.3 | Fuzz the archive parsers (cargo-fuzz) in CI | 🛠 |
| 4.4 | Reproducible benchmarks (`cargo bench`) against a reference tool | 🛠 |
| 4.5 | Security test suite: malicious archives (path escapes, bombs, symlinks) | 🛠 |
| 4.6 | Checksums on every release; signed macOS builds | 🛠 |
| 4.7 | CONTRIBUTING, issue templates, FUNDING.yml | 🛠 |

## 5. Pro tier

| # | Item | Status | Notes |
|---|---|---|---|
| 5.1 | Model: (A) honor-system license key, all code open · (B) open core, Pro code in a private repo · (C) paid Mac App Store build | ❓ | Recommended: A, optionally + C later |
| 5.2 | Rule: never put safety, extraction, creation, encoding fixes or the CLI behind Pro | ✔ proposed | |
| 5.3 | Candidate Pro features: watch folders (auto-extract Downloads), scheduled archive jobs, presets & per-folder rules, compare two archives, themes + alternate app icons | ❓ | |
| 5.4 | Payment provider (merchant of record) that pays out to Saudi Arabia | ❓ verify | Lemon Squeezy / Paddle / Gumroad |
| 5.5 | Screens: Pro upgrade sheet, license entry, Pro badges on locked features | 🎨 | After 5.1 and 5.3 |
| 5.6 | Liquid Glass theme: mini view + full view | ✅ macOS: GlassMini · GlassBrowse · Ubuntu: GlassMiniUbuntu · GlassBrowseUbuntu · Windows 11: GlassMiniWin · GlassBrowseWin | Ubuntu: Frosted / Blur toggle · Windows: Acrylic / Mica toggle |
| 5.7 | Glass on the remaining screens + a dark glass variant | 🎨 | Only if the theme ships |
| 5.8 | Native window blur per OS (macOS vibrancy, Windows Mica/Acrylic, Linux fallback) | 🛠 | See DESIGN.md "Glass theme". README tells Linux users to enable blur via a GNOME blur extension (verify name/setting) or KDE |
| 5.11 | Glass looks identical on all OSes: Crate draws its own frame, corners and buttons | ✔ decided | Build plan: docs/milestones/M9-pro-glass.md |
| 5.9 | "Reduce transparency" fallback: solid panes when the OS setting is on | 🛠 | |
| 5.10 | Alternate app icons picker (4 concepts) inside Pro | ✅ GlassBrowse (sidebar card) · 🛠 | macOS can change the Dock icon at runtime; Windows/Linux need a restart or a new shortcut |
