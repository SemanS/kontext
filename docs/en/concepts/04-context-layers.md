# Context layers and budgets

Agents pay for every token they read. kontext answers in small, layered pieces and lets the agent decide when to pay for more.

## The brief

`ctx_brief` (or `kontext brief`) is the entry point, one block of about 1–2k tokens (`brief.budget_tokens`, default 1400):

| Section | Content |
| --- | --- |
| Header | project, repository id, branch, *About* (overview summary), *Stack* |
| Rules for agents | `AGENTS.md`, `CLAUDE.md`, `.cursorrules`, Copilot instructions, and nested `AGENTS.md` files on the focus paths |
| Changed since … | knowledge added, changed or removed since this worktree's last brief, at most five lines ([below](#changed)) |
| Governs &lt;focus&gt; (N) | with a focus, every active entry whose `paths` govern it, one compact line each, before anything else ([below](#governing)) |
| Decisions | active decisions, most relevant to the focus first, one line each (up to `brief.max_decisions`) |
| Conventions · Learnings & pitfalls · Incidents | one line each, focus-ranked |
| Flows through the focus | with a focus, the [flows](02-knowledge-store.md#flows) whose `paths` overlap it (up to four, with their step count); an unfocused brief says in one line how many flows exist |
| Modules | module summaries (up to `brief.max_modules`); for focused modules the start of their overview |
| Submodules | a section for a submodule with its own knowledge when focus paths lie in it, or when this repository has none; otherwise a one-line pointer |
| Local notes | notes in this clone's inbox (team candidates and private notes) that match the focus, or the newest three |
| Related docs and history | in a repository without a store, once a search has built the local index: the docs and commits that match the focus |
| From adapters | sections from adapters that have a `brief` op (e.g. a session primer) |
| State | inbox counts, bootstrap progress, entries due for review, entries that may need a refresh, missing git hooks, warnings; its room is kept aside, so a full brief does not drop it |

`focus` accepts paths (files or directories) and topic words. Entries whose `paths` overlap a focus path rank first; topic words match titles, summaries and tags. When every focus path lies in another repository, the brief is that repository's ([Several repositories](09-several-repositories.md)).

A focus item with `/`, a glob character or a known file extension is a path even when the file does not exist yet, so an agent about to create `src/billing/refunds.rs` gets the entries governing `src/billing/**`. Overlap works both ways: a directory focus matches a pattern below it, including patterns that start with a wildcard (`**/migrations/*.sql` against `src/db`) or a leading slash.

### What governs the focus {#governing}

A focused brief never drops a governing entry silently. The *Governs &lt;focus&gt; (N)* section is reserved before every other section and lists one line per entry: full id, title, marks and `· N commits since`. It is ordered by how specifically the entry's `paths` name the focus (a file, then a directory, then a glob, then `**`), and then oldest first, since the oldest constraints are the ones a new change most often breaks. No word list ("never", "must") decides the order.

When the section does not fit the budget, the entries that do not fit are named on one line that is always printed: `also governing: [id] [id] …`. Below the size of the section itself only ids are printed. The other kinds then trim line by line, and each trimmed section ends with a count line: `… 7 more (2 govern your focus)`. A whole section never disappears without that line. The footer that tells the agent where to look next counts against the budget, and superseded module docs are left out.

Budgets in practice run from 200 to 1 400 tokens: an orchestrator such as Peer sends `max(200, 1500 − tokens already used)`, so 200 to 1 000 is the common case, not the edge. The *What changed since* line under a governing entry costs about 25 tokens; at 200 tokens the governing section is ids only.

### Marks {#marks}

A short line must not let a proposal pass for an accepted rule. An entry's line is marked when it is anything short of accepted, approved knowledge:

| Mark | When |
| --- | --- |
| `(proposed)`, `(deprecated)`, … | its status is not `accepted` |
| `local, unapproved` | its file is new, edited or staged in the working tree: nobody has approved that text yet |
| `review due` | its [`review_by`](./02-knowledge-store.md#review-by) date has passed |
| `challenged locally` | this clone's inbox holds a [challenge](./03-capture-and-review.md#challenges) against it |
| `N sources` | it carries [evidence](./02-knowledge-store.md#evidence) |

Several marks share one pair of parentheses: `Use queues for exports (proposed; local, unapproved)`. An accepted, committed entry carries none, so the common line costs no extra tokens. Search results carry the same marks in their brackets.

### Changed since the last brief {#changed}

Each brief records the commit it saw in a cache of the worktree's local state (`.git/kontext/…/brief-head`). The next brief lists, in at most five lines, the knowledge entries added, changed or removed since then; a new entry that supersedes another names it (`(new, supersedes [0007] Prices are integer cents)`). Without the cache — a new worktree, a deleted cache — it shows the last 14 days instead. A brief without changes has no such section, so a routine step adds nothing.

The section has its own small budget and is the first thing a full brief drops; the cache moves on only when the section was shown, so a dropped change comes up in the next brief.

An entry is referred to by its full id (`[2026-09-28-adapters-are-configuration]`), or by its ADR number. A shorter prefix would stop being unique once a longer id is added, and agents copy these references into `supersedes` and `[[links]]`.

Claude Code, Codex and Cursor can receive the brief automatically at session start and the governing entries before an edit: see [Hooks](../guides/09-hooks.md). Hooks and `ctx_brief` draw from one `[hooks] session_budget_tokens` budget per session.

### Freshness {#freshness}

An entry is written once, but the code it governs keeps moving. The brief's *State* and `kontext status` list active decisions, conventions and learnings whose `paths` were touched by at least `freshness.threshold_commits` commits (default 20) since someone last looked at them, most-changed first:

```text
- May need a refresh (20+ commits on their paths since they were made, edited or confirmed): [0007] Prices are integer cents (34 commits), … — check them: confirm what still holds (`ctx_confirm`), supersede what no longer does.
```

The count starts at the latest of three moments: the entry's `date`, its [`checked`](./02-knowledge-store.md#checked) confirmation, and the last commit that changed the entry's own file (an edit counts as a look). Commits from the day of the date (or of a confirmation written as a day), commits up to a confirmation's second, and merge commits are not counted. With a confirmation or check that names a commit, the count is by ancestry: commits `HEAD` reaches and that commit does not, so a branch merged after the look counts. A failed check is not a look and does not reset the count; an agent or bulk stamp is a look. A commit counts once however many of its files match, and renaming code out of a governed path counts too. Entries without `paths`, or without a `date` or `checked` to count from, are never flagged; incidents and module docs are not watched. A flag is a prompt to check, not a verdict: if the entry still holds, confirm it (`kontext confirm <id>`) and the count starts again; if it does not, supersede or retire it.

For a range of commits rather than the whole history, see [drift](./03-capture-and-review.md#drift).

Entries of any kind can also carry a [review date](./02-knowledge-store.md#review-by). Past it, the brief's *State* lists them as *Due for review* and their lines are marked `review due`; `kontext status` lists them all. Both signals live in one place (`src/freshness.rs`).

The counts come from one `git log` for all watched entries, from the oldest one's date (bounded by `index.max_commits`), and are cached per `HEAD` under the worktree's state directory: a brief adds no git call per entry, and none at all until the next commit. A confirmation or an edit in the working tree resets the count before it is committed. Set `threshold_commits = 0` to turn it off.

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
- Host hooks share one `[hooks] session_budget_tokens` budget with `ctx_brief`; an edit hook repeats only entries not yet told in the session or whose blob changed.

Token counts are estimated at four characters per token (deliberately simple and conservative).
