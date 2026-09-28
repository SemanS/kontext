# Retrieval

kontext searches locally with BM25 and merges whatever connected adapters return.

## The local index

A [Tantivy](https://github.com/quickwit-oss/tantivy) index per worktree, derived from three sources:

| Source | Documents | URI |
| --- | --- | --- |
| Store entries | one per entry: title, body, tags, paths, id | `kx:<id>` |
| Repository docs | Markdown matching `sources.include` (root `*.md`, every `README.md`, `AGENTS.md`, `CLAUDE.md`, `docs/**`), split at `##` headings | `file:<path>` or `file:<path>#<section>` |
| Commit history | subject, body and trailers of the last `index.max_commits` commits (5000); file names are searchable but kept out of snippets | `git:<sha>` |

Noise is filtered: "Merge branch …" commits, `Co-authored-by`/`Signed-off-by` lines, files over `sources.max_bytes`, and anything that looks like a secrets file.

The index refreshes incrementally before a query (entries and docs by modification time and size, commits by SHA) — at most once per second per process. `kontext reindex` rebuilds it from scratch.

### Identifiers

Code-shaped words are expanded so both forms match: `orderTotal` also indexes `order total`, `user_account_balances` indexes `user account balances`. A query for an identifier additionally searches its words as a *phrase*, so `RunContext` finds "run context" without matching every document that says "run".

### Ranking

BM25 over title (boost 2.5), identifier-expanded auxiliary text (1.5) and body. Local results have weight 1.0; adapters default to 0.9 (`weight` per adapter).

## Federated search

`ctx_search` sends the query to every adapter with a `search` op, in parallel, with a 12-second deadline. Results are normalized per source (the best hit of each source scores 1.0), weighted, merged, and de-duplicated by URI and by near-identical title/snippet. A slow or failing adapter never blocks the answer: it is reported in a note and cools down for 30 seconds.

Restrict a search with `kinds` (`decision`, `convention`, `learning`, `incident`, `architecture`, `doc`, `commit`) and `sources` (`local` and/or adapter names).

## `ctx_why`

"Why is this like this?" combines several signals, depending on the target:

| Target | Answer |
| --- | --- |
| `path` or directory | knowledge whose `paths` cover it, the module summary, decision-shaped commits among the last 60 touching it (plus the latest three), `history` adapters |
| `path:line` / `path:start-end` | the same, plus the commits that last changed those lines (`git blame`) |
| commit sha | message, the knowledge it relates to (via `commits` or `paths`), changed files |
| symbol or topic | `code` adapters locate it (CodeGraph, Serena), then local knowledge and history, then `history` adapters |

Commits are ranked by decision signals — wording such as *replace*, *migrate*, *instead of*, *because*; Conventional-Commit type; breaking changes; explanation length — and by kontext trailers (`Decision: …`).

## `ctx_log`

The decision timeline: date, id, title, status, supersession and the commit that added the file. `kontext log --json` for scripts.
