<div align="center">

<a href="https://semans.github.io/kontext/" target="_blank">
  <img alt="kontext" src="docs/images/logo.svg" width="128" height="128">
</a>

### kontext: declarative team knowledge for coding agents

English / [Slovenčina](README_SK.md)

<a href="https://semans.github.io/kontext/">Docs</a> · <a href="https://semans.github.io/kontext/getting-started/03-quickstart">Quick start</a> · <a href="https://github.com/SemanS/kontext/releases">Releases</a> · <a href="https://github.com/SemanS/kontext/issues">Issues</a> · <a href="https://github.com/SemanS/kontext/discussions">Discussions</a>

<p>
  <a href="https://github.com/SemanS/kontext/releases"><img src="https://img.shields.io/github/v/release/SemanS/kontext?color=4f46e5&labelColor=black&logo=github&style=flat-square" alt="release"></a>
  <a href="https://github.com/SemanS/kontext/actions/workflows/ci.yml"><img src="https://img.shields.io/github/actions/workflow/status/SemanS/kontext/ci.yml?branch=main&label=ci&labelColor=black&style=flat-square" alt="ci"></a>
  <a href="#license"><img src="https://img.shields.io/badge/license-MIT%20%2F%20Apache--2.0-white?labelColor=black&style=flat-square" alt="license"></a>
  <a href="https://github.com/SemanS/kontext"><img src="https://img.shields.io/github/stars/SemanS/kontext?labelColor=black&style=flat-square&color=ffcb47" alt="stars"></a>
  <a href="https://github.com/SemanS/kontext/issues"><img src="https://img.shields.io/github/issues/SemanS/kontext?labelColor=black&style=flat-square&color=ff80eb" alt="issues"></a>
  <a href="https://github.com/SemanS/kontext/commits/main"><img src="https://img.shields.io/github/last-commit/SemanS/kontext?color=0d9488&labelColor=black&style=flat-square" alt="last commit"></a>
  <img src="https://img.shields.io/badge/rust-1.88%2B-orange?labelColor=black&style=flat-square&logo=rust" alt="rust 1.88+">
  <img src="https://img.shields.io/badge/MCP-stdio-0d9488?labelColor=black&style=flat-square" alt="MCP">
</p>

</div>

***

## What is kontext

kontext is a declarative bridge between your coding agents and git. The decisions, conventions and pitfalls your agents follow are short Markdown files in the repository. Agents propose new ones while they work, people approve them in the pull request of the change, and every agent (Claude Code, Codex, Cursor, OpenCode, any MCP client) gets the result in a short brief for the paths it is about to touch.

It is one Rust binary with three faces: an **MCP server** (`kontext mcp`), a **CLI** and a set of **git hooks**.

```text
          Claude Code · Codex · Cursor · OpenCode · any MCP client
                                 │  MCP (stdio) or CLI
                        ┌────────┴────────┐
                        │     kontext     │  ctx_brief · ctx_why · ctx_search · ctx_read
                        │  (Rust, 1 bin)  │  ctx_capture · ctx_prepare_commit · ctx_init …
                        └────────┬────────┘
                                 │  git hooks: validate + secret scan · Decision: trailers · index
                                 ▼
   TEAM KNOWLEDGE (git, reviewed)                   LOCAL, DERIVED (.git/kontext/)
   .ai/decisions/*.md   ◄── commit + pull request ── inbox: candidates agents captured
   .ai/conventions/ · learnings/ · incidents/       search index (Tantivy), caches
   .ai/architecture/                                 deleted any time, rebuilt from git
```

## Why kontext

- **Declarative.** What agents follow is declared in the repository: one Markdown file per decision says what holds, since when and for which paths, and `.ai/kontext.toml` says how it is served (brief budget, freshness threshold, trailers, secret scanning). Nothing lives in a service or a UI. → [Knowledge store](https://semans.github.io/kontext/concepts/02-knowledge-store)
- **Reproducible.** The same commit gives every teammate and every agent the same team knowledge. The local index is derived: delete `.git/kontext/` and the next call rebuilds it. Check out an older commit and agents see the decisions that held at that commit. → [Architecture](https://semans.github.io/kontext/concepts/01-architecture)
- **Reviewed and reversible.** A capture waits in a local inbox until it is promoted into a commit and merged through a pull request, under the same CODEOWNERS and CI as the code. A reversed decision is superseded, not deleted; the commit that ships a decision carries a `Decision:` trailer. → [Capture and review](https://semans.github.io/kontext/concepts/03-capture-and-review)
- **One brief, every agent.** `ctx_brief` returns the decisions, conventions and pitfalls for the paths an agent is about to touch, in ~1–2k tokens, with details on demand. → [Context layers](https://semans.github.io/kontext/concepts/04-context-layers)
- **Stale decisions surface.** A decision whose paths saw 20+ commits since it was made is flagged in the brief and in `kontext status`. → [Freshness](https://semans.github.io/kontext/concepts/04-context-layers#freshness)
- **Step-by-step bootstrap.** `kontext init` scans the repository and mines its history for decision-shaped commits in seconds; your agent then deepens it one small, resumable task at a time. → [Init pipeline](https://semans.github.io/kontext/concepts/06-init-pipeline)

## Why not Claude Code's memory or CLAUDE.md?

| | CLAUDE.md, AGENTS.md | Claude Code auto memory | Memory services | kontext |
| --- | --- | --- | --- | --- |
| Written by | people, as prose | the model, on its own | the model, from sessions | agents propose, people approve |
| Shared and reviewed | in pull requests | no: one user, one machine | per deployment, no review | in the pull request of the change |
| Reaches the agent | the whole file, every session | the first 200 lines of its index, every session | by similarity to the query | ranked for the paths it names, within a budget |
| An entry records | no status, date or owner | when it was written | depends on the store | paths, status, date, author |
| When it goes stale | stays until someone notices | the model may rewrite it | the model may overwrite it | flagged after N commits on its paths (20 by default) |
| Works with | Claude Code; others read AGENTS.md | Claude Code | their plugin, SDK or MCP server | every MCP client and the CLI |

kontext does not replace CLAUDE.md: keep the few instructions every session needs there. Decisions are too many to load whole and too important to leave unreviewed. → [Why not something else?](https://semans.github.io/kontext/getting-started/01-introduction#why-not)

## Under your control

- **Approval.** Knowledge changes only through commits. With `/.ai/ @acme/architects` in CODEOWNERS and code-owner review required, no agent changes what every agent follows without that team's approval.
- **Audit trail.** `git log -- .ai` and `kontext log --all` show who decided what, when, and what replaced it. With kontext's hooks installed, `git log --grep "Decision: <id>"` finds the commit that shipped a decision.
- **Policy in CI.** `kontext check` fails a pull request on malformed entries, duplicate ids or high-confidence secrets. → [Validate knowledge in CI](https://semans.github.io/kontext/guides/07-ci)
- **Local and private.** No account and no server; briefs in ~50 ms, hooks in ~0.1 s. Nothing leaves the machine unless you configure it. → [Security](https://semans.github.io/kontext/concepts/08-security)
- **No lock-in.** The store is plain Markdown and stays readable without kontext.

## Quick start

```sh
curl -fsSL https://raw.githubusercontent.com/SemanS/kontext/main/scripts/install.sh | sh   # → ~/.local/bin/kontext

cd your-repo
kontext init                          # scan → history → render → wire (git hooks) → deepen plan
git switch -c kontext/bootstrap && git add .ai && git commit -m "docs: bootstrap team knowledge"

kontext connect claude --write        # .mcp.json — or: claude mcp add --scope user kontext -- kontext mcp
kontext connect agents-md --write     # a short "how to use kontext" block in AGENTS.md
```

Then run **`/mcp__kontext__kontext-init`** in Claude Code (or ask any connected agent to continue the kontext bootstrap), or let an LLM do a first pass: `kontext init --deepen --llm llm-claude`.

Daily use:

```sh
kontext brief --focus src/billing       # what an agent sees first
kontext why src/billing/round.ts:40     # decisions, module, history, blame
kontext capture --kind decision --title "Prices are integer cents" --paths "src/billing/**" --body "…"
kontext prepare-commit --promote <id>   # ships the decision in the same commit
kontext log                             # the decision timeline
```

Full walkthrough: [Quick start](https://semans.github.io/kontext/getting-started/03-quickstart) · build from source: [Installation](https://semans.github.io/kontext/getting-started/02-installation).

## Use it with your agent

| Agent | Connect | Guide |
| --- | --- | --- |
| **Claude Code** | `kontext connect claude --write` (+ `claude-hooks` for a brief at session start) | [Claude Code](https://semans.github.io/kontext/agent-integrations/02-claude-code) |
| **Codex** | `kontext connect codex --write` (+ `agents-md`: Codex follows `AGENTS.md`) | [Codex](https://semans.github.io/kontext/agent-integrations/03-codex) |
| **Cursor** | `kontext connect cursor --write` | [Other clients](https://semans.github.io/kontext/agent-integrations/04-other-clients) |
| **OpenCode** | `kontext connect opencode --write` | [Other clients](https://semans.github.io/kontext/agent-integrations/04-other-clients) |
| **Superset & orchestrators** | nothing extra: worktrees share inbox and progress | [Other clients](https://semans.github.io/kontext/agent-integrations/04-other-clients#superset-and-other-orchestrators) |
| **Any MCP client** | command `kontext`, args `["mcp"]` | [MCP reference](https://semans.github.io/kontext/reference/02-mcp) |

Agent tools: `ctx_brief` · `ctx_search` · `ctx_read` · `ctx_why` · `ctx_log` · `ctx_threads` · `ctx_capture` · `ctx_inbox` · `ctx_prepare_commit` · `ctx_init` · `ctx_init_submit`, plus prompts `kontext-init`, `kontext-commit`, `kontext-reflect`, `kontext-distill`.

## Optional adapters

kontext works without any. If you already run a semantic memory (OpenViking), a code graph (CodeGraph, Serena) or a session archive, an adapter connects it in a few lines of TOML, and `claude -p`, `codex exec` or a local Ollama model can draft the bootstrap. → [Adapters](https://semans.github.io/kontext/adapters/01-overview)

## Performance

Release build on an Apple Silicon laptop (init with a warm file cache; the first cold run of the monorepo took 3.3 s):

| Repository | init (scan + history + render) | first search (builds the index) | warm search | brief | pre-commit hook |
| --- | --- | --- | --- | --- | --- |
| TypeScript Nx monorepo (3.2k files, 1.5k commits) | 1.1 s | 0.47 s | 0.14 s | 0.04 s | 0.13 s |
| Rust/Python/TS moon workspace (640 files) | 1.1 s | 0.25 s | 0.11 s | 0.04 s | 0.11 s |

## Documentation

**[semans.github.io/kontext](https://semans.github.io/kontext/)**, in English and [Slovak](https://semans.github.io/kontext/sk/).

- Getting started: [Introduction](https://semans.github.io/kontext/getting-started/01-introduction) · [Installation](https://semans.github.io/kontext/getting-started/02-installation) · [Quick start](https://semans.github.io/kontext/getting-started/03-quickstart) · [Set up your agents](https://semans.github.io/kontext/getting-started/04-setup-for-agents)
- Concepts: [Architecture](https://semans.github.io/kontext/concepts/01-architecture) · [Knowledge store](https://semans.github.io/kontext/concepts/02-knowledge-store) · [Git integration](https://semans.github.io/kontext/concepts/07-git-integration)
- Guides: [Bootstrap an existing repository](https://semans.github.io/kontext/guides/01-bootstrap-an-existing-repository) · [Team workflow](https://semans.github.io/kontext/guides/02-team-workflow) · [OpenViking](https://semans.github.io/kontext/guides/03-openviking)
- Reference: [CLI](https://semans.github.io/kontext/reference/01-cli) · [MCP](https://semans.github.io/kontext/reference/02-mcp) · [Configuration](https://semans.github.io/kontext/reference/03-configuration)

The docs sources are in [`docs/`](docs/). This repository records its own design decisions with kontext. See [`.ai/`](.ai/README.md).

## Community & Contributing

- **Questions and ideas**: [Discussions](https://github.com/SemanS/kontext/discussions)
- **Bugs and feature requests**: [Issues](https://github.com/SemanS/kontext/issues)
- **Contributing**: bug fixes, presets, docs and translations are welcome. See [CONTRIBUTING.md](CONTRIBUTING.md) ([SK](CONTRIBUTING_SK.md)) and the [Code of Conduct](CODE_OF_CONDUCT.md)
- **Changelog**: [docs](https://semans.github.io/kontext/about/02-changelog) · **Roadmap**: [docs](https://semans.github.io/kontext/about/03-roadmap)

## Security

Please report vulnerabilities privately. See [SECURITY.md](SECURITY.md).

## License

kontext is dual-licensed under either of

- [Apache License, Version 2.0](LICENSE-APACHE)
- [MIT license](LICENSE-MIT)

at your option. Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in kontext by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.
