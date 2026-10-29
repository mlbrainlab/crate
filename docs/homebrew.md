# Homebrew cask

`scripts/homebrew-cask.py <version>` writes `target/homebrew/Casks/c/crate-archiver.rb` for a published release, with the `.dmg`'s SHA-256 from the release's `SHA256SUMS`. Run it once the release has the signed and notarized `.dmg`: Homebrew doesn't take apps that Gatekeeper blocks.

```bash
scripts/homebrew-cask.py 0.3.0
brew install --cask target/homebrew/Casks/c/crate-archiver.rb   # try it on a Mac
```

The cask installs `Crate.app` and links the `crate` command onto PATH. The token is `crate-archiver`, the Linux package name, because `crate` is already taken in homebrew-cask.

Two ways to publish it, both from your account:

- **Your own tap** (no review): a repository named `mlbrainlab/homebrew-tap` with the file at `Casks/crate-archiver.rb`. People install with `brew install --cask mlbrainlab/tap/crate-archiver`. New versions: run the script again and commit.
- **Homebrew/homebrew-cask** (`brew install --cask crate-archiver`): a pull request with the file at `Casks/c/crate-archiver.rb`. Homebrew asks that the project be notable enough, for example in GitHub stars, so a new project may be turned down. Run `brew audit --new --cask crate-archiver` and `brew style --fix` first. After it's merged, Homebrew's bot picks up new versions from GitHub Releases (`livecheck`).
