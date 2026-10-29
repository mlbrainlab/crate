# M9: Pro: Glass theme + alternate icons

**Goal:** the first Pro features: the Liquid Glass theme and alternate app icons, unlocked with a license key.
**Done when:** buying a key, entering it, and switching to Glass works on all three platforms.

## Decisions to make first
- [ ] Pro model: honor-system license key, with all code public (recommended)
- [ ] Payment provider that pays out to Saudi Arabia (Lemon Squeezy / Paddle / Gumroad: verify)
- [ ] Pro price

## Tasks
- [ ] License check: signed key verified offline with a public key in the app; no account, no phone-home
- [ ] Upgrade sheet, license entry, "PRO" labels on locked items (to design first: `design/BACKLOG.md` 5.5)
- [ ] Glass theme, the same window on every platform (`GlassMini*.dc.html`, `GlassBrowse*.dc.html`):
  - Crate draws its own frame (`no-frame`, transparent window background), with rounded corners and a shadow drawn in Slint
  - Its own resize handles + header drag region
  - Windows: keep Snap Layouts. Windows shows them only when `WM_NCHITTEST` answers `HTMAXBUTTON` over the maximize button; winit has no hook for that, so subclass the window procedure (`SetWindowSubclass`) and hit-test Crate's own maximize button. Caption buttons 46 × 32, Close red on hover (DESIGN.md). Kept the native frame in M7 for this reason (2026-10-29)
  - Material: macOS vibrancy, Windows Acrylic (`window-vibrancy` crate on the winit window handle), Linux Frosted fallback (true blur only through the compositor / a blur extension). Slint can't blur inside the window, so panes are translucent fills over the OS material
  - Respect "Reduce transparency" → solid panes
- [ ] Alternate app icons (4 concepts): macOS can switch the Dock icon live; Windows/Linux update the shortcut/desktop icon (may need a restart)
- [ ] Give GitHub sponsors a free key (manual at first)
- [ ] README note for blur on Linux: "For the Glass theme's see-through blur on GNOME, install a blur extension such as Blur my Shell; KDE Plasma blurs natively." Test the exact extension name and setting yourself before publishing (moved from M6, 2026-10-28: the note ships with the theme)

## Acceptance
- [ ] Glass looks the same on all 3 OSes (shape, layout, controls); only the blur strength differs
- [ ] Without a key, Glass and the icons show a "PRO" label and the upgrade sheet; nothing else is locked

## Prompt for Claude Code
> Read CLAUDE.md, docs/milestones/M9-pro-glass.md and the Glass section of design/DESIGN.md. Build the license check in crate-core-independent code (`app/src/license.rs`).
