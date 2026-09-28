# Contributing to kontext

English / [Slovenčina](CONTRIBUTING_SK.md)

Thanks for helping! Bug fixes, new presets, documentation, translations and ideas are all welcome.

## Before you start

- **Questions and ideas** → [Discussions](https://github.com/SemanS/kontext/discussions).
- **Bugs** → an [issue](https://github.com/SemanS/kontext/issues/new/choose) with the output of `kontext --version` and `kontext status`, and the steps to reproduce.
- **Larger changes** (new capabilities, config or entry-format changes) → open an issue or discussion first so we can agree on the design. The project's own decisions live in [`.ai/decisions/`](.ai/decisions/); a change that contradicts one should come with a superseding decision.

## Development setup

Requirements: Rust 1.88+ (`rustup`), git 2.31+, and Node 20+ for the docs site.

```sh
git clone https://github.com/SemanS/kontext && cd kontext
cargo build
cargo test                                   # unit tests + an end-to-end test on a throwaway repository
cargo clippy --all-targets -- -D warnings
cargo fmt --check

# try your build on a scratch clone of a real repository
cargo build --release
git clone --no-hardlinks <some-repo> /tmp/scratch && cd /tmp/scratch
KONTEXT_CONFIG_DIR=/tmp/kontext-config ~/path/to/kontext/target/release/kontext init
```

`KONTEXT_CONFIG_DIR` keeps experiments away from your personal `~/.config/kontext`.

### Docs

```sh
cd docs
npm ci
npm run docs:dev        # http://localhost:5173/kontext/
npm run docs:build      # what CI runs; fails on dead links
```

English sources are in `docs/en/`, Slovak in `docs/sk/` — the two trees mirror each other file by file. When you change a page, update the other language too or mention it in the pull request so a translator can follow up.

## Project layout

| Path | What |
| --- | --- |
| `src/main.rs` | CLI |
| `src/mcp.rs`, `src/tools.rs` | MCP server and the agent tool surface |
| `src/ops.rs` | brief, search, read, why, log, capture, promote, prepare-commit, check |
| `src/store.rs`, `src/inbox.rs` | knowledge entries (both styles) and local candidates |
| `src/index.rs` | Tantivy index |
| `src/adapters/` | registry, drivers, MCP client, result mapping |
| `src/init/` | scan, history, render, deepen |
| `src/hooks.rs`, `src/events.rs` | git hooks, outbox |
| `src/config.rs`, `src/template.rs`, `src/jpath.rs` | configuration, templates, JSON paths |
| `presets/` | adapter presets (plain TOML) |
| `tests/cli.rs` | end-to-end test |
| `docs/` | documentation site (VitePress) |

## Pull requests

- Keep them focused; one topic per pull request.
- Add or update tests: unit tests next to the code, the end-to-end test for cross-cutting behaviour.
- `cargo fmt`, `cargo clippy --all-targets -- -D warnings` and `cargo test` must pass (CI runs them).
- Update the docs (both languages when you can) and `docs/*/about/02-changelog.md` for user-visible changes.
- Commit messages follow [Conventional Commits](https://www.conventionalcommits.org/) (`feat:`, `fix:`, `docs:`, `refactor:` …).
- If your change embodies a design decision, record it: `kontext capture --kind decision …` and promote it into the pull request — kontext's hooks add the trailer.

## Adding a preset

1. Create `presets/<name>.toml` with a comment header (what it is, which ops/events, variables).
2. Use `when` so it is inactive where its tool is missing.
3. Prefer schema-based argument binding over hard-coded `args` for MCP tools.
4. Register it in `src/presets.rs` (`BUNDLED`) and document it in `docs/en/adapters/03-presets.md` (and `docs/sk/…`).
5. Say in the pull request how you tested it (`kontext adapters test <name>` output).

## Style

- Rust: idiomatic, small functions, no `unwrap` on user-controlled input, errors with context (`anyhow`).
- Agent-facing text (tool descriptions, briefs, reports): short, concrete, budget-aware.
- Code comments explain *why*, not *what*.

## License

By contributing you agree that your contributions are dual-licensed under the MIT and Apache-2.0 licenses, as described in the [README](README.md#license).
