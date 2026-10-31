# M8: macOS port

**Goal:** Crate on macOS 12+ (Apple Silicon + Intel), signed and notarized.
**Done when:** the universal .dmg installs with no Gatekeeper warning and the Homebrew cask is submitted.

## Tasks
- [x] Build and CI on `macos-latest` (universal binary)
- [ ] Window: native traffic lights placed inside Crate's own header (overlay title bar through winit's macOS window attributes in `platform::select_backend`); a native menu bar (File / Edit / Archive / View / Window / Help), using Slint's `MenuBar` if it maps to the native macOS menu
- [ ] Document types in the app bundle's Info.plist (file associations) so Crate appears in "Open With"
- [x] Default app: first run calls the macOS API for setting the default app per file type (small native code); the system may show its own confirmation. Verify on the current macOS version
- [x] Ask for folder access when needed (`FileIssues.dc.html`, permission card)
- [x] Code signing + notarization in CI (Apple Developer account ✔); store the secrets in GitHub
- [x] Dock progress
- [x] Homebrew cask

## Acceptance (macOS)
- [ ] Download the .dmg → open → no "unidentified developer" warning
- [ ] First run → make default → double-clicking a .zip opens the mini view

## Notes

### Build, CI and the bundle (2026-10-29, 0.2.1)
- `ci.yml` has a `macos` job (clippy and every test on `macos-latest`, Apple Silicon). It passed the first time: nothing in the code was Linux- or Windows-only without a fallback. The RAR reader already passes wide characters as `wchar_t` (32-bit on macOS).
- `scripts/package-macos.sh` builds both targets with `MACOSX_DEPLOYMENT_TARGET=12.0`, joins them with `lipo`, and lays out `Crate.app`: `Contents/MacOS/crate-app` (the bundle's executable) and `crate` (the CLI, for the Homebrew cask to link), `Info.plist` from `packaging/macos/Info.plist`, `crate.icns`, `LICENSE`. Then the `.dmg` (the app and a link to /Applications), `Crate-<version>-universal.dmg`, and `SHA256SUMS`.
- Signing: `MACOS_SIGN_IDENTITY` signs with the hardened runtime and a timestamp; without it the app is signed ad hoc (Apple Silicon won't run an unsigned binary at all). Notarization comes with the signing task.
- `release.yml` has a `macos` job: tests, the `.dmg`, then it mounts it, checks the signature, runs the CLI natively and under Rosetta, and uploads it as an artifact. `publish` leaves it out until it's signed and notarized.
- Dry run 40931301588 (2026-10-29): every job green; `Crate-0.2.1-universal.dmg` mounted, signature valid, `crate 0.2.1` on arm64 and under Rosetta.
- `Info.plist` already declares the document types (task 3): the Windows installer's extensions. Types macOS knows (zip, tar, gz, bz2) rank "Alternate", so Crate is in "Open With" without taking them from Archive Utility; 7z, rar, xz, zst, tzst, tbz2, txz and .001 are declared (`UTImportedTypeDeclarations`) and rank "Default".

### Opening files from Finder (2026-10-29)
- Finder doesn't pass the file as an argument: it launches the app (or finds it running) and sends an "open documents" Apple Event, which AppKit hands to the app delegate's `application:openURLs:`. winit 0.30 has neither the method nor an event for it.
- `platform/macos.rs` adds the method to winit's delegate class (`class_addMethod`, as winit does for `sendEvent:`) right after `select_backend()` made the event loop, then sets the delegate again so AppKit sees it. Files that arrive before anyone asks are queued (`platform::take_opened_files`); after that `platform::on_open_files` gets them. Other systems have no-op versions.
- `main.rs`: launched with no arguments on macOS (`run_launched`), Crate decides on the event loop's first turn, after the launch event: the first archive opens in the mini view in this process (with the usual one-window-per-archive claim), any others in new processes, and with none, Home. Files sent to a running Crate open in new processes (`open_each`), as Home's Open does.
- **To check on the Mac:** double-click a .zip with Crate as its app (mini view, no Home flash); a second archive while it's open; the same archive twice (the first window comes to the front); "Open With → Crate" for each type.

### Dock progress (2026-10-29, 0.2.2)
- `platform::taskbar_progress` on macOS puts a content view on the Dock tile: the app icon (`NSImageView`) with a determinate `NSProgressIndicator` bar along the bottom, then `display()`. `None` removes the view, which brings the plain icon back. Each window is its own process, so the Dock icon belongs to one archive's work.
- **To check on the Mac:** the bar's size and position on the icon, at both Dock sizes; it goes away when the work ends or is cancelled.

### Default app (2026-10-29, 0.2.3)
- `Defaults::Confirm` (5): Crate asks with `NSWorkspace setDefaultApplicationAtURL:toOpenContentType:completionHandler:` (macOS 12+) for the chosen types' UTIs (the ones in `Info.plist`), and macOS asks the user to confirm each. The answer arrives later, so Crate doesn't wait for it; Preferences shows the new state the next time it reads it. Reading uses `URLForApplicationToOpenContentType:` and compares the bundle id.
- Outside Crate.app (`cargo run`) there's no bundle id, so it's `NotInstalled`, as on Linux without a .desktop file.
- New strings for both screens, with the Arabic.
- **To check on the Mac:** first run → Get started with all four types → macOS's prompts → double-click a .zip opens Crate; Preferences shows each type as Crate's; the `.tar.*` row covers tar, tgz, txz, tzst and tbz2.

### Menu bar (2026-10-29, 0.2.4)
- Slint's `MenuBar` goes into the macOS menu bar through muda (enabled in Slint's winit backend on macOS). Both windows have one inside `if native-menu:` (`platform::NATIVE_MENU`, macOS only), so Linux and Windows draw nothing and keep their header menus.
- Home: Crate (About Crate, Preferences ⌘,, Quit ⌘Q), File (New archive ⌘N, Open archive ⌘O, Close window ⌘W), Help (Keyboard shortcuts). Archive window: Crate (Preferences, Quit), File (Close window), Edit (Select all, Filter / search ⌘F, Add files ⇧⌘A, New folder ⇧⌘N, Rename, Delete from archive), Archive (Extract all ⌘E, Extract selected ⇧⌘E, Test integrity ⌘T, Properties ⌘I), View (Mini ↔ full view ⌘↩, List / grid view ⌘G, Toggle sidebar, Quick preview), Help.
- Each item calls the same callback as its key in the window's `FocusScope` and is enabled under the same conditions. Slint's `Control` is ⌘ on macOS.
- A menu shortcut fires before the focused text field sees the key, so ⌘A, Space, ⌘⌫ and Return have no menu shortcut: in the search field they must still edit text. The window's key handler keeps them.
- Slint menus can't hold AppKit's own items (the Window menu came later with Crate's own Minimize and Zoom, 0.2.16). Hide/Services in the app menu are missing for the same reason: our `MenuBar` replaces Slint's default app menu.
- **To check on the Mac:** the first menu shows as "Crate"; each shortcut runs once (not twice through the menu and the key handler); items grey out while a dialog is open; ⌘A in the search field selects the text; the Shortcuts sheet's macOS keys match (⌘↑, ⌘↓, Return to rename and ⌘⌫ aren't wired to keys yet).

### Window frame (2026-10-29): left for the Mac
- The mini view mockup's macOS frame is the plain native title bar (traffic lights, centred title), which Crate has now (`CUSTOM_FRAME = false`). The full view's ("unified toolbar · full-height sidebar · traffic lights left", `PlatformFull.dc.html`) needs winit's overlay title bar (`with_titlebar_transparent`, `with_fullsize_content_view`, `with_title_hidden` through Slint's window-attributes hook) and the header laid out around the buttons: their inset and the drag area can only be set while looking at it. Done on the Mac.

### Folder access (2026-10-29, 0.2.5)
- macOS asks by itself the first time an app reads or writes Desktop, Documents, Downloads, removable or network volumes (TCC); there's no API to ask first. `Info.plist` now says why (`NS…UsageDescription`), with the Arabic in `ar.lproj/InfoPlist.strings`, copied into the bundle.
- If the user said no, writing fails with "Operation not permitted" (`PermissionDenied`), which already opens the "Crate can't write to …" card (`FileIssues.dc.html`). On macOS its text names Privacy & Security → Files and Folders, and an extra button opens that page (`platform::PRIVACY_SETTINGS`, an `x-apple.systempreferences:` link); the card stays, so Retry works once access is on.
- **To check on the Mac:** extract to Desktop on a fresh install (macOS's prompt with Crate's reason, in Arabic too); say no, extract again → the card → the button opens the right page → allow → Retry.

### Homebrew cask (2026-10-29, 0.2.6)
- `scripts/homebrew-cask.py <version>` writes `crate-archiver.rb` (the token the Linux packages use; `crate` is taken), reading the `.dmg`'s SHA-256 from the release's `SHA256SUMS`. It installs the app, links the `crate` CLI, requires macOS 12, and `zap`s `~/.config/crate`, `~/.local/state/crate` and `~/.cache/crate` (where `dirs.rs` puts them on macOS).
- Publishing it waits for a signed and notarized `.dmg`. Own tap or homebrew-cask, from the user's account: `docs/homebrew.md`. As with winget, it's the user's call whether to publish it.

### Signing and notarization (2026-10-30, 0.2.7)
- The Developer ID certificate and the App Store Connect API key were made without a Mac (`openssl` on Linux) and stored as GitHub secrets: `docs/macos-signing.md`.
- `release.yml` imports the `.p12` into a temporary keychain and writes the `.p8` to `RUNNER_TEMP`. `package-macos.sh` signs the app and the `.dmg` (hardened runtime, timestamp), runs `notarytool submit --wait` (it reads the status itself, since notarytool exits 0 on a rejection, and prints Apple's log), and staples the ticket. The job checks the result with `spctl` and `stapler validate`. A tag push fails without the secrets, and `publish` now includes the `.dmg`.
- No entitlements were needed: nothing in Crate needs JIT or unsigned memory, and it was accepted the first time.
- Dry run 41043029091 (2026-10-30): every job green; notarization "Accepted"; `spctl` on the app and the `.dmg`: `accepted, source=Notarized Developer ID`.
- **To check on the Mac:** download the `.dmg` from a release in Safari (so it has the quarantine flag), open it, drag to Applications, and launch: no warning (acceptance item 1).

### First run on the Mac mini (2026-10-30, 0.2.8)
- The 0.2.7 `.dmg` passed Gatekeeper on macOS 27.0 (M4) but wouldn't start: `dyld: Library not loaded: /opt/homebrew/opt/xz/lib/liblzma.5.dylib`. `liblzma-sys` finds Homebrew's xz on the runner through pkg-config and links it dynamically (the arm64 build only; the x86_64 one is cross-compiled, so it built xz from source). CI didn't notice because the runner has Homebrew.
- Fix: `crate-core` turns on liblzma's `static` feature on macOS, and `package-macos.sh` fails when `otool -L` lists anything outside `/usr/lib` and `/System/Library`.

### Title bar and fonts (2026-10-30, 0.2.9)
- `platform::overlay_titlebar` turns the window's title bar transparent, hides the title and lets the content run under it (`FullSizeContentView`). Then it moves the traffic lights down to the middle of Crate's 56px header, with equal margins at the top and left: their container grows to the header's height (as Tauri's `trafficLightPosition` does). It returns the width they take, which becomes the header's `titlebar-inset`. AppKit lays them out again on resize, so each window's winit event filter passes `Resized` to `platform::window_event`, which places them again (not in full screen). The content size is kept when the mode changes, so the full view's remembered size doesn't grow.
- Home, the full view and (from 0.2.11) Preferences use it; the mini view (`toggle_full`) keeps the plain title bar. The inset is always on the left: the traffic lights follow the system's language, not Crate's.
- Fonts: `Locale.system-fonts` (`platform::SYSTEM_FONTS`) picks "System Font" (San Francisco) and "SF Arabic", which Slint finds through fontique's CoreText backend. A name it can't find falls back to Helvetica.
- 0.2.9 on the Mac mini (macOS 27.0): the lights stayed at the top of the bar. Moving the buttons by hand didn't hold, because AppKit lays the title bar out again after it.
- 0.2.10: `overlay_titlebar` sets an empty `NSToolbar` with the unified style instead (Electron's "hiddenInset"). The bar is then as tall as a current Mac app's, AppKit centres the lights in it and keeps them there, and there's no resize hook. Crate measures the bar's height (`frame` − `contentLayoutRect`) and the zoom button's right edge, and the header takes that height (`titlebar-height`), so its buttons share the lights' centre line. The title bar's separator is off, since the header draws its own line.
- 0.2.10 on the Mac: the lights still sat about 7px above the header's buttons (the bar's measured height wasn't twice the lights' centre). Preferences showed its title twice (the system bar and Crate's header), the header title had lost its centring (0.2.9 placed it after the lights, so Preferences' title touched the left edge), and the Operations page's Back button sat under the lights.
- 0.2.11: the header height is twice the close button's centre measured from the window's top, measured again 250ms later in case AppKit moves the lights after laying the toolbar out. The title is centred again, as in the mockups. Preferences gets the overlay title bar, and the Operations page takes the same inset and height as the header it covers.
- Liquid Glass: Crate draws all of its window itself (Slint), so AppKit's glass materials don't show through. Using them means native views behind a transparent Slint surface; it's a design question for later, not part of M8.
- **To check on the Mac:** the lights are centred against the header buttons and keep their place while resizing, after zoom, and after leaving full screen; clicks on the header's buttons work, and dragging or double-clicking the empty header moves or zooms the window; mini ↔ full keeps the window's size; the text is in San Francisco (and SF Arabic in Arabic), with the bold weights.

### Finder's keys (2026-10-31, 0.2.12)
- The archive window's key handler takes ⌘↑ (up a folder), ⌘↓ (open; in the mini view only folder tiles), ⌘⌫ (delete the selection, full view, editable archives) and Return (rename in the full view of an editable archive; otherwise it opens, as before), when `native-menu` is on. Slint reports ⌘ as `control` on macOS. F2, Delete and Alt+↑ keep working. The menu items for these have no shortcut, because text fields use the same keys (see the comment above the `MenuBar`).
- The title bar from 0.2.11 is confirmed on the Mac mini: the lights share the header's centre line in Home, the full view, Preferences and Operations.

### Window menu (2026-10-31, 0.2.16)
- Home and the archive window have a Window menu between View (archive window) or File (home) and Help: Minimize (⌘M) and Zoom. They are Crate's own items, not AppKit's, and call the same `minimize` / `toggle-maximize` callbacks as the header buttons (winit's `set_minimized` / `set_maximized`; on macOS the latter zooms). Bring All to Front is left out: each Crate window is its own process, so there is nothing else to bring forward.
- **To check on the Mac:** ⌘M minimizes to the Dock once; Zoom fills the screen and a second Zoom restores the size.

## Prompt for Claude Code
> Read CLAUDE.md and docs/milestones/M8-macos.md. Keep macOS-only code behind `#[cfg(target_os = "macos")]`.
