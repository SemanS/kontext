# Releasing kontext

Releases are cut from `main` by pushing a version tag; CI tests the tag, builds the binaries and publishes the GitHub release.

## Checklist

1. Make sure `main` is green (`ci` workflow: tests on Linux and macOS, MSRV, clippy, eval, cargo deny) and the docs build (`cd docs && npm run docs:build`).
2. Update the version in `Cargo.toml` (`version = "0.2.1"`) and run `cargo build` so `Cargo.lock` follows. The plugin manifests (`.claude-plugin/`, `.codex-plugin/`, `.mcp.json`, `hooks/`) embed the version: regenerate them with `cargo run -- connect plugin --write` (or `KONTEXT_REGEN_PLUGINS=1 cargo test plugin_files`), or the plugin tests fail.
3. Add the release to the changelog in both languages: `docs/en/about/02-changelog.md` and `docs/sk/about/02-changelog.md`.
4. Run the release check against the candidate (see below) and read what it reports.
5. Commit: `git commit -am "chore(release): v0.2.1"`.
6. Tag and push. **The tag must be `v` plus the version in `Cargo.toml`**: the `release` workflow fails before building anything when they differ.

   ```sh
   git tag -a v0.2.1 -m "kontext v0.2.1"
   git push origin main v0.2.1
   ```

7. The `release` workflow:
   - checks the tag against `Cargo.toml`, then runs `cargo test --locked` on Linux and macOS; nothing is built when either fails;
   - builds `kontext-<target>.tar.gz` and `kontext-<target>.manifest.json` for macOS (arm64, x86_64), Linux (x86_64, arm64) and Windows x64. Linux builds on `ubuntu-22.04`, so no binary needs a glibc newer than 2.35; the job summary prints each Linux binary's highest `GLIBC_` symbol (its glibc floor). Check it did not rise;
   - publishes the archives, the manifests and `SHA256SUMS` (the archives' hashes) with generated notes. Only this last job has write access. Edit the release text if needed, and point to the changelog.
8. Check the install script against the new release. It verifies the archive against `SHA256SUMS` and runs the binary before installing it:

   ```sh
   curl -fsSL https://raw.githubusercontent.com/SemanS/kontext/main/scripts/install.sh | KONTEXT_VERSION=v0.2.1 KONTEXT_BIN_DIR=/tmp/kx/bin sh
   /tmp/kx/bin/kontext --version
   ```

## Release check

`scripts/release-check.sh` builds the candidate (`cargo build --release --locked`; or `KONTEXT_CANDIDATE=<binary>`) and shows what it changes for the people and applications that read it:

```sh
PEER_DIR=~/src/peer \
KONTEXT_PREV=~/.local/bin/kontext \
KONTEXT_STORES=~/src/shop:~/src/lib \
scripts/release-check.sh
```

- `PEER_DIR`: a Peer checkout. Its three integration suites (`kontext.integration`, `knowledge/challenges.integration`, `knowledge/keeping.integration`) run with `KONTEXT_BIN` set to the candidate; a failure fails the check. Skipped when unset. `PEER_TEST_CMD` replaces the runner (`vp test run`).
- `KONTEXT_STORES`: repositories with knowledge, separated by `:`. Each entry is read through paged `kontext api entries.list` and `entries.get` with the previous binary (`KONTEXT_PREV`, default `kontext` on PATH) and with the candidate. Per entry, the script prints what differs in status, `verification.kind`, `verification.verified`, `since.commits` and the governing paths, then totals. Differences are printed, not judged: each one should be explained by the changelog.

## API contract

`tests/api_contract.rs` holds the response shape (key paths and JSON types) of every operation in `status.capabilities` to `tests/api_contract.snap`. A new, removed or retyped field fails the test. After an intended change, regenerate it and review the diff in the same pull request:

```sh
KONTEXT_UPDATE_CONTRACT=1 cargo test --test api_contract
```

API v1 changes are additive: a removed or retyped field needs a new capability string and a migration note.

## Versioning

[Semantic Versioning](https://semver.org/). Before 1.0, a minor version may change configuration keys, the entry format or tool parameters, always with a migration note in the changelog. Patch versions are fixes only.
