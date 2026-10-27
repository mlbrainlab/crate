# Changelog

Every finished task bumps the version (see "Versions" in `docs/ROADMAP.md`): the patch number per task, 0.1.0 for the first Linux release (M6), 0.2.0 Windows (M7), 0.3.0 macOS (M8), and 1.0.0 with the final milestone (M9).

## 0.0.2 — 2026-10-27
M5: accessibility pass.
- Every control can be reached with Tab and pressed with Space or Enter, with a focus ring that shows only for keyboard users; Up / Down change drop-downs and segmented controls.
- Screen readers get a name and role for every control, including icon buttons, checkboxes, switches, tabs and list rows (with size and selected state); toasts are announced.
- A test fails if any control on the main screens has no name.

## 0.0.1 — up to 2026-10-27
Everything before per-task versions:
- M0–M4: the Slint app, safe listing and extraction (`crate-core`), the CLI, the mini view, the full view with preview and grid, creating archives, editing inside archives, Properties, testing and converting.
- M5 so far: mini view folder navigation, Preferences, First run with "Make Crate the default", the Arabic UI with a mirrored layout, the keyboard shortcuts sheet, dark mode on every screen, the filename encoding picker with "Remember for this archive", and the About & support screen.
