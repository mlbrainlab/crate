# Submitting Crate to Flathub

Flathub builds every app from a manifest in its own repository, `github.com/flathub/io.github.mlbrainlab.crate`, which Flathub creates once the submission is accepted. A submission is a pull request to [flathub/flathub](https://github.com/flathub/flathub) from your GitHub account; the reviewers talk to that account in the pull request.

Do this after the `v0.1.0` tag is on GitHub (the manifest builds from it).

## 1. Point the manifest at the release

In a copy of `packaging/flatpak/io.github.mlbrainlab.crate.yml`, replace the `dir` source with the tagged commit:

```yaml
    sources:
      - type: git
        url: https://github.com/mlbrainlab/crate.git
        tag: v0.1.0
        commit: <the commit v0.1.0 points at: git rev-parse v0.1.0^{commit}>
      - cargo-sources.json
```

`cargo-sources.json` must match the tag's `Cargo.lock`: run `scripts/flatpak-cargo-sources.py` on the tag and copy `packaging/flatpak/cargo-sources.json`.

## 2. Check it locally

```bash
flatpak run --command=flatpak-builder-lint org.flatpak.Builder manifest io.github.mlbrainlab.crate.yml
flatpak run org.flatpak.Builder --user --install --force-clean --disable-rofiles-fuse build-dir io.github.mlbrainlab.crate.yml
flatpak run io.github.mlbrainlab.crate
flatpak run --command=flatpak-builder-lint org.flatpak.Builder repo repo
```

(For the last one, build with `--repo=repo`.)

## 3. Open the pull request

1. Fork [flathub/flathub](https://github.com/flathub/flathub) (uncheck "Copy the master branch only").
2. In the fork, branch from **`new-pr`**:
   ```bash
   git clone --branch new-pr git@github.com:<you>/flathub.git && cd flathub
   git checkout -b io.github.mlbrainlab.crate
   ```
3. Add `io.github.mlbrainlab.crate.yml` (from step 1) and `cargo-sources.json`, commit, push.
4. Open a pull request **against `new-pr`** (not `master`), titled "Add io.github.mlbrainlab.crate".
5. In the description, explain the permission reviewers will ask about:
   > `--filesystem=host`: Crate is an archive manager. It extracts next to the archive and saves new archives wherever the user chooses, including removable and network drives; the file chooser portal only grants the chosen file, not its folder.
   >
   > `--talk-name=org.freedesktop.Notifications`: a notification when a long extraction finishes in the background.
   >
   > License: MIT, plus the UnRAR library (freeware, extraction only) for RAR; the metainfo says so.
6. Comment `bot, build` to start a test build; fix what the reviewers ask for. When it's merged, Flathub creates the app's repository and publishes the first build within a few hours.

## Afterwards

- Each release: update the tag and commit in the Flathub repository's manifest and regenerate `cargo-sources.json`.
- Add a `<release>` entry to `packaging/linux/io.github.mlbrainlab.crate.metainfo.xml` for every version.
- Once Crate is on Flathub, add `flatpak install flathub io.github.mlbrainlab.crate` and the Flathub badge to the README.
