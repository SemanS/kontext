# Capture and review

kontext separates **remembering** from **sharing**. Anything can be captured quickly; only what goes through a commit becomes team knowledge.

```text
 agent / you                    local (this clone)                     team (git)
─────────────                   ─────────────────                      ──────────
 ctx_capture ─────────────────▶ inbox candidate ──┐
 kontext capture                 (team | private)  │ promote (ctx_prepare_commit,
                                                   │  ctx_inbox, kontext promote)
                                                   ▼
                                 .ai/decisions/…md staged ──▶ commit ──▶ PR review ──▶ merge
                                                               │ Decision: <id> trailer
                                                               ▼
                                                 teammates pull → sync events → their adapters
```

## Capturing

A capture needs a kind, a title and a body (or a summary). Good captures are short (*what, why, consequences*) and name the paths they govern:

```sh
kontext capture --kind decision --title "Use Tantivy for local recall" \
  --paths "src/index.rs" --body "BM25 is enough for code-shaped queries; semantic search comes from adapters."
```

Agents call `ctx_capture` with the same fields. On capture kontext:

- redacts secret-looking values (`capture.redact = true`),
- warns when a similar entry already exists (so it can be updated instead),
- defaults decisions to `status: accepted` and records the author (`git config user.name`) and origin (`agent` or `cli`),
- queues a `capture` event for adapters that subscribe to it,
- files it with the repository that owns its `paths`, such as a submodule with its own store ([Several repositories](09-several-repositories.md)).

A learning or decision you proved with a command carries that command, so it can be [proved again](#verify) later:

```sh
kontext capture --kind learning --title "Totals round per line, not per invoice" \
  --paths "src/billing/**" --body "…" --verify "cargo test -p billing rounding_is_per_line"
```

`--verify` (`verify` in `ctx_capture` and `drafts.capture`) is validated like the entry field: one line, at most 500 characters; anything else is refused (`invalid_input` in the API). The draft keeps it and so does the entry it is promoted into, in either file style. A draft captured without one takes it later: `kontext inbox verify <id> "<command>"` (`ctx_inbox` with `action=verify`, the `inbox.verify` operation); `--clear` (an empty string) removes it.

## Visibility

| Visibility | Stays | Can be promoted | Typical use |
| --- | --- | --- | --- |
| `team` (default) | in the inbox until promoted | yes | decisions, conventions, pitfalls the team should know |
| `private` | in the inbox, and in personal adapters | no | your own notes, half-formed hypotheses, preferences |

Private captures never enter the repository. Adapters can receive them (for example as OpenViking memories) by subscribing to `capture` with `visibility = "private"`.

Inbox notes, team or private, stay findable: `ctx_search` finds them (marked *local note*) and the brief lists those that match its focus. Finding a note does not promote it: a commit still takes only the candidates its own worktree captured.

## Promoting

Promotion writes a candidate into the store (with the right id, directory, style and numbering) and stages it:

- `ctx_prepare_commit` with `promote=[ids]` (what agents use before committing),
- `ctx_inbox` with `action=promote`,
- `kontext promote <id>…` or `kontext prepare-commit --promote <id>`.

`capture --promote` (or `promote=true` in `ctx_capture`) skips the inbox and writes the entry into the working tree directly, which is useful when you already know it belongs to the current change. Either way nothing is shared until you commit.

## Before committing: `ctx_prepare_commit`

The report lists:

1. the staged change, or, when nothing is staged, the working tree's changes against `HEAD` (untracked files included) — the report says which it read,
2. recorded knowledge whose `paths` cover the changed files (*confirm it still holds*: `ctx_confirm` / `kontext confirm <id>`),
3. challenges raised in this worktree, with the two ways to resolve them ([below](#challenges)),
4. inbox candidates, the ones touching changed files first,
5. validation and secret-scan results for staged knowledge (errors will block the commit),
6. module docs that may need a refresh (many changed files, or files added/removed),
7. the commit trailers, and whether the prepare-commit-msg hook adds them (a clone without kontext's hooks needs them added by hand, or `kontext hooks install`),
8. a nudge when a large change carries no knowledge at all.

A capture also names the recorded entries it may duplicate, extend or contradict (`related, check: … <kx:id> — governs the same paths (src/pricing/**)`). It is a prompt to look, never an instruction to replace: whether the new entry supersedes one is the author's call, and the reviewer's.

## Drift: knowledge a change left behind {#drift}

`ctx_prepare_commit` looks at one change. `kontext drift` (`ctx_drift`, `entries.drift`) looks at a range: it lists the active entries whose `paths` cover files changed in `base..head` and whose own file was **not** changed in the same range. Editing an entry, retiring it or [confirming](./02-knowledge-store.md#checked) it counts as having looked at it.

```text
$ kontext drift
Knowledge drift in 3f2a91c4..working tree: 1 entry governs changed files but was not edited or confirmed in the range.

- [convention] Prices are integer cents <kx:prices-are-integer-cents> — .ai/conventions/prices-are-integer-cents.md
  changed: src/billing/invoice.rs, src/billing/tax.rs
  commits: 9c1d2e3f feat(billing): VAT per line

For each, read it against the change: if it still holds, confirm it (`kontext confirm <id>`); if it needs a correction, edit it; if it no longer holds, supersede it (`kontext capture --supersedes <id> …`) or retire it (`kontext challenge <id> "why"`, then `kontext prepare-commit --retire <id>`).
```

- `--base` defaults to where `HEAD` left the remote's default branch (the merge base with `origin/HEAD`, else `origin/main`, else `origin/master`), and to `HEAD` without a remote.
- Without `--head` the working tree counts as well: commits `base..HEAD` plus uncommitted and untracked changes. That is how a [jj](./07-git-integration.md#jj) working copy looks to git, so drift works without commits or hooks. With `--head` only committed changes count, and the entries are read as committed there.
- Decisions, conventions, learnings and incidents are checked (`--kind` to choose); module docs are refreshed by `kontext init`, not confirmed.
- No model is involved: the answer is a deterministic function of the repository. `--json` prints the `entries.drift` result; `--fail` exits with status 2 when an entry is listed. [`kontext ci`](../guides/07-ci.md) runs it in CI.
- **Gate with it, not with a hook.** A team that commits with jj, or with clients that skip git hooks, gets no commit-time check, so the gate belongs in the team's check script (`bun run check`, `make check`) or CI: `kontext drift --fail` (or `kontext ci --strict`). kontext adds no hook that blocks a commit for drift.

## Verifications: the ledger {#verifications}

A `checked:` date in an entry's file says that someone confirmed it, not what they looked at or how, and a merge that resolves a conflict in it by taking the older side loses the newer look. So every confirmation and every executable check is also appended to **`.ai/confirmations.log`**, one tab-separated line each:

```text
2026-10-27T09:12:44Z	prices-are-integer-cents	read	a7450a5c…	Jane Doe	-	checked against the VAT change
2026-10-27T09:40:02Z	invoice-totals-tested	verify	a7450a5c…	Jane Doe	1	
```

The fields are the time (UTC, to the second), the entry id, how it was looked at, the full commit `HEAD` named at that moment (`-` before the first commit), who (`git config user.name`), whether a `verify` command passed (`1`/`0`, `-` otherwise) and an optional one-line reason. The log is append-only: nothing rewrites a line. `kontext init` writes `.ai/.gitattributes` with `confirmations.log merge=union` (keeping the lines already there), and the first write makes sure the attribute is present, so two branches that confirm entries merge without a conflict and keep both lines. The log is part of the store: drift and freshness never count it as code, and `kontext check` does not read it as an entry.

How an entry was looked at:

| Kind | Written by | Counts as verified |
| --- | --- | --- |
| `read` | `kontext confirm` (a person at the terminal) | yes |
| `agent` | `ctx_confirm`, and `drafts.confirm` unless it says `kind: read` | no: a stamp, kept and shown as one |
| `bulk` | any confirmation of more than five entries in one call | no: a stamp |
| `verify` | `kontext verify`, `ctx_verify`, `drafts.verify` | when the command passed |

`kontext confirm <id…> --because "…"` (`because` in the tool and the API) stores the reason. The entry's `checked:` is still written and staged as before, so older readers keep working; it is a cache of the ledger, never the proof. When the cache is behind its ledger (a merge took the older side, or the line came from another branch), `kontext check` warns (`checked date … behind its ledger; run kontext confirm --repair`) and **`kontext confirm --repair`** rewrites the cached `checked:` of every such active entry from the ledger and stages the files. A `verify` line is not a confirmation and leaves the cache alone.

### An entry that checks itself {#verify}

An entry may name one shell command that shows it still holds:

```yaml
verify: cargo test -p billing totals_are_integer_cents
```

One line, at most 500 characters (a longer check belongs in a script the command calls); `kontext check` refuses anything else. `kontext verify <id…>` (`ctx_verify`, `drafts.verify`) runs it with `sh -c` from the repository root, stdin closed, stopped after 120 seconds (`KONTEXT_VERIFY_TIMEOUT_SECS` changes it), and appends a `verify` line with the outcome and the commit it ran against. It returns whether it passed, the exit code and the last 2 KB of the output. The command of a superseded or retired entry never runs, and an entry without `verify:` is refused: read it and confirm it instead. Machine-observable facts are best verified this way; what someone intended is confirmed by a person.

### What changed since {#since}

For each entry that governs the focus paths, [`ctx_brief`](./04-context-layers.md) adds how it was last verified and what happened to its paths since:

```text
- [prices-are-integer-cents] Prices are integer cents — Every amount is an integer number of cents.
  verified read at a7450a5 (2026-10-27); 3 commits on its paths since: feat(billing): VAT per line; fix(billing): round totals; …
```

The count starts at the commit the verification named, or at the commit that landed its ledger line if that is newer (the change it was reviewed with), and takes non-merge commits on the entry's governed files, knowledge files left out. A verification known only from a `checked:` date counts as freshness does. An entry never verified says `never verified`. When any listed entry has commits since, the brief adds once: *Read these before changing their paths: an entry that limits the direction you are about to take outranks a fresh one.* Unlike freshness there is no threshold: one commit on a governed path is already a reason to read.

The machine API returns the same with every entry (`entries.list`, `entries.get`, `entries.governing`, `entries.drift`): `verification`, `since`, `verify` and `premises`. See [the application API](../reference/05-application-api.md#verification).

`verification.landed` says where the verification is part of history: the full id of the commit in `HEAD`'s history that first added the entry's newest ledger line, or `null` while that line is only staged or in the working tree. A consumer tells "this check passed and the pass is committed" from "passed, not committed yet" without reading the ledger. For a verification known only from a `checked:` date it is the last commit that changed the entry's file (`null` while the file has uncommitted changes or was never committed).

### Changed premises {#premises}

Superseding or retiring an entry changes nothing in the entries that build on it. An active entry that links to one that is now superseded, retired or deprecated — by `[[id]]` in its body, by `supersedes`, or by an `extends` field — has a changed premise: the brief says `premise changed: <id> is superseded`, `kontext check` prints an info line (`<id> links to <old>, which is superseded by <new>: does it still hold?`), and the API lists it under `premises`. The entry that did the superseding is not flagged for it. It is a question for whoever reads the entry next, never an automatic edit: agents add to an entry or supersede it, they do not rewrite it.

## Challenges {#challenges}

Sometimes an agent finds that recorded knowledge no longer holds. It says so with `ctx_challenge <id> "why"` (or `kontext challenge`): a local inbox candidate of kind `challenge`, against the entry and the commit it was read at. The challenge changes nothing by itself and cannot be promoted. The brief marks the entry `challenged locally`, and `ctx_prepare_commit` offers two ways to resolve it in the change under review:

- **retire** the entry (`retire=[id]`): its file gets status `deprecated` and a `## Retired` note with the challenge as the reason, staged;
- **replace** it: capture the new rule with `supersedes: [id]` and promote it.

Either takes the challenge out of the inbox with the commit. If the entry changed since it was challenged, the report says so; drop a challenge that turned out wrong. A challenge is resolved only when the change is reviewed and merged, not when one worktree prepared it. Shared challenges with counting of independent voices belong to an application such as Peer Hub; kontext keeps the local, reviewed part.

## Review

Knowledge travels in the same pull request as the code. Reviewers see the decision next to the diff it explains; they can ask for changes, reject it (delete the file or set `status: rejected`), or propose a superseding decision later. `git log --format='%(trailers:key=Decision,valueonly)'` and `kontext log` show which commit introduced which decision.

## After merge

When teammates pull, their `post-merge` / `post-rewrite` hooks compare knowledge at `HEAD` with their last snapshot and emit `sync` events, so their personal memory systems learn the decisions the team just accepted. Promoted inbox items are removed once their file is part of `HEAD`.
