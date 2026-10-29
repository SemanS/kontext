# Installation

kontext is a single static-ish binary. It needs **git 2.31 or newer** at runtime and nothing else.

## Install script (recommended)

```sh
curl -fsSL https://raw.githubusercontent.com/SemanS/kontext/main/scripts/install.sh | sh
```

The script downloads the prebuilt binary for your platform from the latest [release](https://github.com/SemanS/kontext/releases) into `~/.local/bin/kontext` and checks it against the release's `SHA256SUMS`; a mismatch stops the install. When no binary matches your platform it builds that release's tag from source with `cargo --locked`. A failed download stops with an error; it never falls back to building `main`. The script ends non-zero when the installed binary does not run (`kontext --version`).

| Variable | Default | Meaning |
| --- | --- | --- |
| `KONTEXT_BIN_DIR` | `~/.local/bin` | where the binary goes |
| `KONTEXT_VERSION` | `latest` | a release tag such as `v0.1.0` |
| `KONTEXT_REPO` | `SemanS/kontext` | install from a fork |

Prebuilt binaries: macOS (Apple Silicon and Intel), Linux x86_64 and ARM64 (glibc 2.34 or newer: Ubuntu 22.04, Debian 12, RHEL 9 and later), and Windows x86_64. Every release carries `SHA256SUMS`; to check by hand: `shasum -a 256 -c SHA256SUMS --ignore-missing`.

## From source

Requires Rust **1.90** or newer ([rustup.rs](https://rustup.rs)); CI checks this version against `Cargo.lock`.

```sh
cargo install --locked --git https://github.com/SemanS/kontext --root ~/.local
# or from a checkout
git clone https://github.com/SemanS/kontext && cd kontext
cargo build --release --locked && install -m 755 target/release/kontext ~/.local/bin/
```

## Check the installation

```sh
kontext --version
kontext status        # inside any git repository
```

Make sure the directory is on your `PATH`. Git hooks installed by kontext also look in `~/.local/bin` and `~/.cargo/bin`, so commits made from GUI clients find the binary even when their `PATH` is minimal.

## Platforms

- **macOS** and **Linux** are supported.
- **Windows**: the binary builds and the brief, search, MCP server and CLI work. Git hooks, agent host hooks and `verify` commands run through `sh` and are untested on native Windows. WSL works like Linux.

## Updating

Rerun the install script (or `cargo install … --force`). Local state under `.git/kontext/` is versioned; an incompatible search index is rebuilt automatically.

## Uninstalling

```sh
kontext hooks uninstall          # in each repository where you installed hooks
rm ~/.local/bin/kontext
rm -rf ~/.config/kontext         # your personal adapters and trust records
```

Knowledge files in your repositories (`.ai/`) are ordinary Markdown and stay where they are. Local state lives in `$(git rev-parse --git-common-dir)/kontext/` and can be deleted at any time.
