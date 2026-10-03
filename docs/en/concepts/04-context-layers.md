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
| From adapters | sections from adapters that have a `brief` op (e.g. a session primer) |
| State | inbox counts, bootstrap progress, decisions that may need a refresh, warnings |

`focus` accepts paths (files or directories) and topic words. Entries whose `paths` overlap a focus path rank first; topic words match titles, summaries and tags.

Claude Code can receive the brief automatically at session start: see [Claude Code](../agent-integrations/02-claude-code.md).

### Freshness

A decision is written once, but the code it governs keeps moving. The brief's *State* and `kontext status` list active decisions whose `paths` were touched by at least `brief.stale_after_commits` commits (default 20) since the decision's date, most-changed first:

```text
- May need a refresh (20+ commits on their paths since): [0007] Prices are integer cents (34 commits) …
```

Commits from the decision's own day and merge commits are not counted, and decisions without a `date` or `paths` are never flagged. A flag is a prompt to check, not a verdict: if the decision still holds, nothing needs to change; if it does not, supersede it. The count comes from one `git log` over all decisions (bounded by `index.max_commits`), cached per `HEAD` under the worktree's state directory, so a brief adds no git call per decision. Set `stale_after_commits = 0` to turn it off.

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
| `kx:<id>` | a knowledge entry (`kx:0007`, `kx:2026-09-28-prices-are-integer-cents`) |
| `file:<path>` / `file:<path>#<section>` | a repository file, or one section of a Markdown document |
| `git:<sha>` | a commit (L1 = message and stat, L2 = full diff) |
| `inbox:<id>` | a local candidate |
| `viking://…`, … | anything an adapter declares it can read (`owns`) |

`file:` reads are limited to the repository and never return secret files (`.env*`, keys, tfvars, …).

## Budgets elsewhere

- `ctx_search` renders hits as single lines within `budget_tokens` (default 1200) and tells the agent how many were cut.
- `ctx_why` keeps each section short: at most `limit` items per section.
- Adapter sections in the brief get at most a third of the remaining budget each.

Token counts are estimated at four characters per token (deliberately simple and conservative).
