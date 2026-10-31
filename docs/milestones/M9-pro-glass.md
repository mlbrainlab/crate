# M9: Pro: Glass theme + alternate icons

**Goal:** the first Pro features: the Liquid Glass theme and alternate app icons, unlocked with a license key.
**Done when:** buying a key, entering it, and switching to Glass works on all three platforms.

## Decisions (2026-10-31)
- [x] Pro model: honor-system license key, with all code public
- [x] Payment provider: Lemon Squeezy (merchant of record; its docs list bank payouts to Saudi Arabia in USD, and PayPal). Confirm when signing up that it still takes new sellers.
- [x] Pro price: $9, one-time
- [x] Key delivery: the buyer pastes Lemon Squeezy's license key into Crate, which exchanges it once with a small signer (a serverless function) for a key signed with Crate's private key. From then on the check is offline. Signed keys can also be pasted directly (sponsors, keys made by hand). This replaces "no phone-home" with "one call at activation, none after".

## Tasks
- [x] License check: signed key verified offline with a public key in the app; no account, one call at activation (0.2.18)
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

## Notes

### License check (2026-10-31, 0.2.18)
- `app/src/license.rs`: `verify` (offline, Ed25519 with `PUBLIC_KEY`), `current` / `is_pro` (the key saved as `pro.key` in settings), `activate` (a `CRATE-PRO-…` key is checked and saved; anything else goes to the signer as a Lemon Squeezy key; blocking, for a worker thread) and `remove`. Errors are an enum (`Rejected`, `Network`, `Unavailable`) for the upgrade sheet to word.
- Key: `CRATE-PRO-` + base64url(payload + signature), payload `1|<id>|<YYYY-MM-DD>`, id `ls:<order>` or `gh:<sponsor>`. No name or email in the key.
- The private key is `~/crate-license/license-ed25519.pem` on the dev machine, made on 2026-10-31. It needs an offline backup, like ~/apple-signing.
- `signer/`: the Cloudflare Worker (asks Lemon Squeezy's License API, checks the store and product id, signs), `sign.mjs` for keys made by hand, and the deploy steps. The release workflow passes the repository variable `CRATE_LICENSE_URL` to the build. Until it's set, builds take only signed keys.
- Tests: good and tampered keys, and `sign.mjs` keys checked by the Rust code. The Worker was tried by hand against a mocked Lemon Squeezy. Not tried yet: the real Lemon Squeezy API, which needs the store set up.
- **To do (you):** create the Lemon Squeezy store and the Crate Pro product, deploy the Worker (signer/README.md), set `CRATE_LICENSE_URL`, and back up the private key.

## Acceptance
- [ ] Glass looks the same on all 3 OSes (shape, layout, controls); only the blur strength differs
- [ ] Without a key, Glass and the icons show a "PRO" label and the upgrade sheet; nothing else is locked

## Prompt for Claude Code
> Read CLAUDE.md, docs/milestones/M9-pro-glass.md and the Glass section of design/DESIGN.md. Build the license check in crate-core-independent code (`app/src/license.rs`).
