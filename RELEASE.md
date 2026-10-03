# Releasing kontext

Releases are cut from `main` by pushing a version tag; CI builds the binaries and publishes the GitHub release.

## Checklist

1. Make sure `main` is green (`ci` workflow) and the docs build (`cd docs && npm run docs:build`).
2. Update the version in `Cargo.toml` (`version = "0.2.1"`) and run `cargo build` so `Cargo.lock` follows.
3. Add the release to the changelog in both languages: `docs/en/about/02-changelog.md` and `docs/sk/about/02-changelog.md`.
4. Commit: `git commit -am "chore(release): v0.2.1"`.
5. Tag and push:

   ```sh
   git tag -a v0.2.1 -m "kontext v0.2.1"
   git push origin main v0.2.1
   ```

6. The `release` workflow builds `kontext-<target>.tar.gz` for macOS (arm64, x86_64) and Linux (x86_64, arm64) and attaches them to the GitHub release with generated notes. Edit the release text if needed, and point to the changelog.
7. Check the install script against the new release:

   ```sh
   curl -fsSL https://raw.githubusercontent.com/SemanS/kontext/main/scripts/install.sh | KONTEXT_VERSION=v0.2.1 KONTEXT_BIN_DIR=/tmp/kx sh
   /tmp/kx/kontext --version
   ```

## Versioning

[Semantic Versioning](https://semver.org/). Before 1.0, a minor version may change configuration keys, the entry format or tool parameters, always with a migration note in the changelog. Patch versions are fixes only.
