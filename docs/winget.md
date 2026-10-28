# Submitting Crate to winget

winget installs from manifests in [microsoft/winget-pkgs](https://github.com/microsoft/winget-pkgs). Each version is a pull request there from your GitHub account; a bot validates it (downloads the installer, scans it, installs it in a VM) and a moderator merges it.

Do this after the release workflow has published the version (the manifests point at its installer).

## 1. Make the manifests

```bash
scripts/winget-manifest.py 0.2.0
```

It writes three files into `target/winget/manifests/m/mlbrainlab/Crate/0.2.0/`, with the installer's URL and its SHA-256 from the release's `SHA256SUMS`. The package id is `mlbrainlab.Crate`; one installer serves both scopes (`/CURRENTUSER` for user, `/ALLUSERS` for machine).

## 2. Check them (on Windows)

```powershell
winget validate --manifest target\winget\manifests\m\mlbrainlab\Crate\0.2.0
winget install --manifest target\winget\manifests\m\mlbrainlab\Crate\0.2.0
```

(`winget settings --enable LocalManifestFiles` once, as administrator, allows the second command.)

## 3. Open the pull request

1. Fork [microsoft/winget-pkgs](https://github.com/microsoft/winget-pkgs).
2. Copy the folder to the same path in the fork (`manifests/m/mlbrainlab/Crate/0.2.0/`), commit, push.
3. Open a pull request titled "New package: mlbrainlab.Crate version 0.2.0" and fill in its checklist.

Unsigned installers are accepted. If the validation bot flags SmartScreen or a scanner, say in the pull request that the installer is built by GitHub Actions from the tagged source (link the release workflow run).

## Afterwards

- New versions: run the script with the new version and open a pull request titled "New version: mlbrainlab.Crate version x.y.z" (or use `wingetcreate update mlbrainlab.Crate --version x.y.z --urls <installer url> --submit`).
- Once it's merged, add `winget install mlbrainlab.Crate` to the README's install table.
