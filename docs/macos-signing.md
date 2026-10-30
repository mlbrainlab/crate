# Signing and notarizing the macOS .dmg

`release.yml`'s `macos` job signs `Crate.app` and the `.dmg` with the Developer ID certificate (hardened runtime, timestamp), sends the `.dmg` to Apple's notary service and staples the ticket to it. Then it checks what Gatekeeper says (`spctl`) before the `.dmg` goes into the release. A tag push fails if the secrets are missing; a dry run without them builds an ad hoc signed `.dmg`.

## The secrets

| Secret | What it is |
|---|---|
| `MACOS_CERTIFICATE_P12` | The "Developer ID Application" certificate with its private key and Apple's Developer ID G2 intermediate, as a `.p12` in base64 |
| `MACOS_CERTIFICATE_PASSWORD` | The `.p12`'s export password |
| `MACOS_SIGN_IDENTITY` | The certificate's name: `Developer ID Application: <name> (<team id>)` |
| `APPLE_API_KEY_P8` | The App Store Connect API key (Developer role) that `notarytool` uses, the `.p8` file's text |
| `APPLE_API_KEY_ID`, `APPLE_API_ISSUER_ID` | That key's ID, and the issuer ID from the same page |
| `APPLE_TEAM_ID` | The team ID (not used by the workflow; kept for reference) |

The key and certificate were made on Linux with `openssl` (no Mac needed). The `.p12` is exported with `-legacy`, because macOS's `security import` can't read OpenSSL 3's default encryption. The originals are backed up offline by the account holder; they're never in the repository.

## On a Mac

With the certificate in the login keychain:

```bash
MACOS_SIGN_IDENTITY="Developer ID Application: …" \
APPLE_API_KEY_PATH=~/apple-signing/AuthKey_XXXXXXXXXX.p8 APPLE_API_KEY_ID=… APPLE_API_ISSUER_ID=… \
scripts/package-macos.sh
```

Without the `APPLE_API_*` variables the `.dmg` is signed but not notarized.

## Renewing

A Developer ID certificate is valid for 5 years. Before it expires, make a new one the same way and replace `MACOS_CERTIFICATE_P12`, `MACOS_CERTIFICATE_PASSWORD` and (if the name changed) `MACOS_SIGN_IDENTITY`. Apps signed with the old certificate keep working, because the timestamp shows they were signed while it was valid. If the API key is revoked, generate a new one and replace the three `APPLE_API_*` secrets. When notarization is rejected, the job prints Apple's log.
