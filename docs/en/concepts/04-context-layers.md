# Context layers and budgets

Agents pay for every token they read. kontext answers in small, layered pieces and lets the agent decide when to pay for more.

## The brief

`ctx_brief` (or `kontext brief`) is the entry point, one block of about 1–2k tokens (`brief.budget_tokens`, default 1400):

| Section | Content |
| --- | --- |
| Header | project, repository id, branch, *About* (overview summary), *Stack* |
| Rules for agents | `AGENTS.md`, `CLAUDE.md`, `.cursorrules`, Copilot instructions, and nested `AGENTS.md` files on the focus paths |
| Decisions | active decisions, most relevant to the focus first, one line each (up to `brief.max_decisions`) |
| Conventions · Learnings & pitfalls · Incidents | one line each, focus-ranked |
| Modules | module summaries (up to `brief.max_modules`); for focused modules the start of their overview |
| Nested repositories | a section for each submodule that keeps its own team knowledge: when focus paths lie in it, or when this repository has none of its own. Others get a one-line pointer |
| Local notes | notes in this clone's inbox (team candidates and private notes) that match the focus, or the newest three |
| Related docs and history | in a repository without a store, once a search has built the local index: the docs and commits that match the focus |
| From adapters | sections from adapters that have a `brief` op (e.g. a session primer) |
| State | inbox counts, bootstrap progress, missing git hooks, warnings |

`focus` accepts paths (files or directories) and topic words. Entries whose `paths` overlap a focus path rank first; topic words match titles, summaries and tags. When every focus path lies in one other repository (a submodule of a repository without knowledge of its own, another worktree, a sibling project), the brief is that repository's.

An entry is referred to by the shortest unique prefix of its id (`[2026-09-28-adapters]` rather than the id that repeats the whole title), or by its ADR number. `ctx_read` resolves either.

Claude Code can receive the brief automatically at session start: see [Claude Code](../agent-integrations/02-claude-code.md).

## Levels: L0 · L1 · L2

Every hit carries a URI. `ctx_read` returns it at the level the agent asks for:

| Level | Size | Use |
| --- | --- | --- |
| 0 | one line | is this relevant? |
| 1 | up to ~4k characters | enough to act on most of the time (default for `ctx_read`) |
| 2 | full content (capped at 80k characters) | when the details matter |

## URIs

| URI | Points to |
| --- | --- |
| `kx:<id>` | a knowledge entry (`kx:0007`, `kx:2026-09-28-prices-are-integer-cents`, or a unique prefix such as `kx:2026-09-28-prices`) |
| `kx:<submodule>/<id>` | an entry of a submodule's own store (`git:<submodule>/<sha>` for its commits) |
| `file:<path>` / `file:<path>#<section>` | a repository file, or one section of a Markdown document |
| `git:<sha>` | a commit (L1 = message and stat, L2 = full diff) |
| `inbox:<id>` | a local candidate |
| `viking://…`, … | anything an adapter declares it can read (`owns`) |

`file:` reads are limited to the repository and never return secret files (`.env*`, keys, tfvars, …).

## Budgets elsewhere

- `ctx_search` renders hits as single lines within `budget_tokens` (default 1200) and tells the agent how many were cut.
- `ctx_why` keeps each section short: at most `limit` items per section.
- `ctx_threads` returns a transcript in parts of `budget_chars` (default 20000), each small enough for one tool result.
- Adapter sections in the brief get at most a third of the remaining budget each.

Token counts are estimated at four characters per token (deliberately simple and conservative).
