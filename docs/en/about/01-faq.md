# FAQ

## Is kontext a memory system like OpenViking or Mem0?

No. It owns only the small, reviewed part (the team's decisions, conventions and lessons, kept in git) and connects everything else. Semantic memory, session archives and code graphs plug in as adapters. Many teams run kontext *with* a memory system: see the [OpenViking guide](../guides/03-openviking.md).

## Why not store everything in a vector database?

Because what the team relies on has to be reviewable, diffable and versioned with the code it describes. Vector stores are great at recall and bad at being the truth. kontext keeps truth in Markdown and gets semantic recall from adapters.

## Does kontext call an LLM?

Not in the core. Brief, search, drift, freshness, check and the hooks are deterministic: no model ranks, writes or judges anything there. The agent you already use does the thinking; kontext hands it small tasks and budgeted context.

A model is called only where you ask for it, always through an `llm` adapter you configure (`llm-claude`, `llm-codex`, a local Ollama):

- `kontext init --deepen` (the [autopilot](../guides/06-autopilot.md)) drafts module docs and decisions,
- `kontext distill` turns agent threads into inbox candidates ([sensitive threads](../guides/08-distill-threads.md#sensitive-threads)),
- the application API's `entries.related` labels possible conflicts, and `drafts.revise` drafts a replacement for a person to review.

A model never writes into an existing entry: `drafts.extend` adds a finding as written, and every draft goes through the inbox and a pull request.

## What does it cost in tokens?

The brief is about 1–2k tokens (configurable). Searches return one line per hit within a budget. Details are read only on demand (L1/L2).

## Does it send my code anywhere?

No. Everything is local unless you configure an adapter that sends data (and then only what that adapter's ops and events declare). The autopilot sends the attached excerpts to the LLM you chose. Secret files are never read, and captured text is redacted.

## Our repository already has ADRs.

kontext detects them and writes new decisions into the same directory, in the same style and numbering. See [Bootstrap an existing repository](../guides/01-bootstrap-an-existing-repository.md#existing-adrs).

## Do teammates have to install kontext?

No. `.ai/` is plain Markdown, and hook blocks do nothing on machines without the binary. Installing it gives them the tools, the hooks and sync to their own adapters.

## Can I use it in a repository I do not own?

Yes, privately: skip hooks, keep `.ai/` out of commits, or move the store to a locally excluded directory. See [Repositories you do not own](../guides/01-bootstrap-an-existing-repository.md#repositories-you-do-not-own).

## What about monorepos?

Workspaces (Nx, moon, Turborepo, pnpm/npm, Cargo, Go, uv) are recognized; every declared project becomes a module, big ones are split into areas, and the brief is scoped with `focus`.

## Windows?

The binary builds for Windows, and the brief, search, the MCP server and the CLI do not need a shell. Git hooks, host hooks and `verify` commands run through `sh`, and none of them is tested on native Windows; use Git for Windows' `sh` at your own risk, or WSL, which works.

## What does kontext write, and where? {#what-kontext-writes-where}

| Where | What | Written by |
| --- | --- | --- |
| `.ai/` (committed) | entries, `kontext.toml`, `README.md`, `.gitattributes`, `confirmations.log`, `init-covered.log` (the commits `dec:` tasks covered) | `init`, captures you promote, `confirm`, `verify`, `repair` |
| repository root (committed) | a marked block in `AGENTS.md`, an `@AGENTS.md` import in an existing `CLAUDE.md`, `.mcp.json`, `.cursor/mcp.json`, `opencode.json` | `kontext connect … --write` |
| `.claude/`, `.agents/`, `.codex/`, `.cursor/` (committed) | `settings.json` hooks and the deny rule for `kontext trust`, `skills/kontext-*/SKILL.md`, `hooks.json` | `kontext connect … --write`, `connect skills`, `--hooks` |
| git hooks (`.git/hooks` or `core.hooksPath`) | a marked block in `pre-commit`, `prepare-commit-msg`, `post-commit`, `post-merge`, `post-rewrite` | `init`, `kontext hooks install` |
| `.git/kontext/` (this clone) | search index, inbox, outbox and `outbox.dead.jsonl`, operation receipts, hook session state, caches, clone-local `config.toml` | everything at run time |
| `~/.config/kontext/` (you) | `config.toml`, `repos/<slug>.toml`, `trust.toml` (adapter and verify pins) | you, `kontext trust`, `adapters add --scope user` |
| `~/.codex/config.toml` | the `[mcp_servers.kontext]` entry, with a timestamped backup | `kontext connect codex --write` |
| `~/.local/bin/kontext` | the binary | `install.sh` |

To remove kontext from a repository:

1. `kontext hooks uninstall` (restores chained hooks),
2. delete the kontext entries from `.mcp.json`, `.claude/settings.json`, `.codex/hooks.json`, `.cursor/`, `opencode.json`, and the marked block in `AGENTS.md`,
3. delete `.claude/skills/kontext-*` and `.agents/skills/kontext-*`,
4. delete `.ai/` if you do not want the knowledge (it stays readable Markdown if you keep it),
5. delete `.git/kontext/`,
6. remove the repository's section from `~/.config/kontext/trust.toml` and `repos/<slug>.toml`.

To remove it from the machine as well: the `kontext` binary, `~/.config/kontext/`, and the `kontext` entry in `~/.codex/config.toml`.

## How is this different from CLAUDE.md, AGENTS.md or Claude Code's memory?

CLAUDE.md and AGENTS.md are prose: the root file loads every session, a nested CLAUDE.md when the agent reads files below it, and Claude Code reads AGENTS.md natively. `.claude/rules` files can be limited to paths, but have no status or history and only Claude Code reads them. Claude Code's auto memory is written by the model and stays on one machine. kontext keeps many small entries with paths, statuses and authors, approved in pull requests, ranked for the task at hand and linked to commits. Keep the few instructions every session needs in CLAUDE.md; kontext also adds a short block to `AGENTS.md` that tells agents to use it. See [Why not something else?](../getting-started/01-introduction.md#why-not)
