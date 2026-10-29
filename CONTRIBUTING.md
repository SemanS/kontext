# Contributing to kontext

English / [Slovenčina](CONTRIBUTING_SK.md)

Thanks for helping! Bug fixes, new presets, documentation, translations and ideas are all welcome.

## Before you start

- **Questions and ideas** → [Discussions](https://github.com/SemanS/kontext/discussions).
- **Bugs** → an [issue](https://github.com/SemanS/kontext/issues/new/choose) with the output of `kontext --version` and `kontext status`, and the steps to reproduce.
- **Larger changes** (new capabilities, config or entry-format changes) → open an issue or discussion first so we can agree on the design. The project's own decisions live in [`.ai/decisions/`](.ai/decisions/); a change that contradicts one should come with a superseding decision.

## Development setup

Requirements: Rust 1.90+ (`rustup`, the `rust-version` in `Cargo.toml`), git 2.31+, and Node 20+ for the docs site.

```sh
git clone https://github.com/SemanS/kontext && cd kontext
cargo build --locked
cargo test --locked                                   # unit tests + an end-to-end test on a throwaway repository
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

English sources are in `docs/en/`, Slovak in `docs/sk/`. The two trees mirror each other file by file. When you change a page, update the other language too or mention it in the pull request so a translator can follow up.

## Project layout

| Path | What |
| --- | --- |
| `src/main.rs`, `src/cli.rs` | thin binary entry and CLI (exit 2 for a usage error or listed drift, 3 for drift without a base) |
| `src/lib.rs`, `src/application.rs` | private engine modules and the stable embedding facade |
| `src/app.rs` | process-wide context: repository, configuration, adapter registry and search index |
| `src/api.rs`, `src/api/schema.rs`, `src/api_read.rs`, `src/api_write.rs` | versioned application service shared by CLI, MCP and library; input schemas for `api.describe` |
| `src/mcp.rs`, `src/tools.rs` | MCP server (both protocol eras) and the agent tool surface |
| `src/ops.rs` | brief, search, read, why, log, capture, promote, prepare-commit, check |
| `src/model.rs` | search hits from the index and adapters, merged and budgeted |
| `src/route.rs`, `src/repo.rs` | which repository a path belongs to (submodules, worktrees, `dir`); git discovery and plumbing |
| `src/glob.rs` | git-style globs for `sources`, entry `paths` and allowlists |
| `src/store.rs`, `src/inbox.rs` | knowledge entries (both styles, supersession, parsing) and local candidates |
| `src/index.rs` | Tantivy index, including the folded shadow fields |
| `src/freshness.rs` | entries whose `paths` changed a lot since someone last looked at them |
| `src/drift.rs` | entries a range of commits (or the working tree) changed the code of without a look |
| `src/ledger.rs`, `src/verification.rs` | the verification ledger (`.ai/confirmations.log`); how each entry was verified, what changed since, changed premises, `verify` commands and their trust pins |
| `src/evidence.rs`, `src/related.rs` | where an entry comes from; how a new text relates to recorded entries |
| `src/signals.rs` | decision-shaped commit heuristics (`why`, init history) |
| `src/secrets.rs` | secret detection and redaction |
| `src/threads.rs`, `src/distill.rs` | agent transcripts and distilling knowledge from them |
| `src/connect.rs`, `src/skills.rs` | wiring agent clients, host hooks and skills |
| `src/adapters/` | registry, drivers, MCP client, result mapping |
| `src/init/` | scan, history, render, deepen, symbols, the covered-commits log (`init-covered.log`) |
| `src/hooks.rs`, `src/events.rs` | git hooks; outbox |
| `src/hook.rs`, `src/lint.rs` | agent host hooks (`kontext hook --host`); the instruction lint that withholds an entry's text from a hook |
| `src/config.rs`, `src/template.rs`, `src/jpath.rs` | configuration, templates, JSON paths |
| `src/presets.rs`, `presets/` | adapter presets (plain TOML) |
| `src/util.rs` | small shared helpers |
| `skills/` | `SKILL.md` recipes compiled into the binary |
| `tests/` | integration tests: `cli.rs` (end to end), `api.rs`, `api_contract.rs` (with `api_contract.snap`), `drift.rs`, `hooks.rs`, `init.rs`, `maintenance.rs`, `mcp.rs`, `eval.rs` with `tests/eval/`; shared sandbox helpers in `tests/common/` |
| `docs/` | documentation site (VitePress) |

Regenerate this table when you add or remove a module: `ls src src/*/` and the `//!` line at the top of each file.

## Pull requests

- Keep them focused; one topic per pull request.
- Add or update tests: unit tests next to the code, the end-to-end test for cross-cutting behaviour.
- `cargo fmt`, `cargo clippy --all-targets -- -D warnings` and `cargo test` must pass (CI runs them).
- Update the docs (both languages when you can) and `docs/*/about/02-changelog.md` for user-visible changes.
- Commit messages follow [Conventional Commits](https://www.conventionalcommits.org/) (`feat:`, `fix:`, `docs:`, `refactor:` …).
- If your change embodies a design decision, record it: `kontext capture --kind decision …` and promote it into the pull request. The kontext hooks add the trailer.

## Fixes start with a failing test

- **Red first.** The first commit of a fix is a test or eval scenario that fails on the current code; paste its failing output into the pull request. The fix follows in the next commit.
- **Eval expectations come from a different session.** Whoever implements a change to ranking, the brief or verification does not write its eval expectations or qrels labels; another session (or person) writes them, before the change, and they are committed first.
- **One owner per hot file.** `src/ops.rs`, `src/api_write.rs`, `src/store.rs`, `src/verification.rs` and `src/config.rs` are changed by one session or pull request at a time. Disjoint modules can be worked on in parallel.
- **Release gates.** A release is tagged only from a green `main`: fmt, clippy, tests on Linux and macOS (the API contract snapshot among them), the MSRV job (reads `rust-version`), the eval set, the docs build, EN/SK parity, `kontext ci` over this repository's own `.ai/` and cargo deny. The release workflow tests the tag again on Linux and macOS before it builds anything. See [RELEASE.md](RELEASE.md).

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
