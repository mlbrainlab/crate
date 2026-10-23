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
- [ ] Translations setup (Fluent or i18next) + **Arabic** with a mirrored right-to-left layout (`BrowseRTL.dc.html`, `MiniViewRTL.dc.html`)
- [ ] Filename encoding picker (`MiniViewRTL.dc.html`)
- [ ] Keyboard shortcuts sheet (`Shortcuts.dc.html`)
- [ ] Accessibility pass: everything reachable by keyboard, labels on icon buttons, Orca screen-reader spot check
- [ ] About & support screen (`About.dc.html`)

## Acceptance (on Ubuntu)
- [ ] First run → "Make Crate the default" → double-clicking any .zip opens Crate
- [ ] Switching the language to Arabic mirrors the whole UI, with no clipped text
- [ ] The whole mini view → extract flow works with the keyboard only

## Prompt for Claude Code
> Read CLAUDE.md and docs/milestones/M5-linux-polish.md. One task per session; open the referenced design board for each. Detect Flatpak via the `FLATPAK_ID` env var to choose the default-app flow.
