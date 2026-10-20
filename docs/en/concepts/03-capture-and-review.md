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

## Challenges {#challenges}

Sometimes an agent finds that recorded knowledge no longer holds. It says so with `ctx_challenge <id> "why"` (or `kontext challenge`): a local inbox candidate of kind `challenge`, against the entry and the commit it was read at. The challenge changes nothing by itself and cannot be promoted. The brief marks the entry `challenged locally`, and `ctx_prepare_commit` offers two ways to resolve it in the change under review:

- **retire** the entry (`retire=[id]`): its file gets status `deprecated` and a `## Retired` note with the challenge as the reason, staged;
- **replace** it: capture the new rule with `supersedes: [id]` and promote it.

Either takes the challenge out of the inbox with the commit. If the entry changed since it was challenged, the report says so; drop a challenge that turned out wrong. A challenge is resolved only when the change is reviewed and merged, not when one worktree prepared it. Shared challenges with counting of independent voices belong to an application such as Peer Hub; kontext keeps the local, reviewed part.

## Review

Knowledge travels in the same pull request as the code. Reviewers see the decision next to the diff it explains; they can ask for changes, reject it (delete the file or set `status: rejected`), or propose a superseding decision later. `git log --format='%(trailers:key=Decision,valueonly)'` and `kontext log` show which commit introduced which decision.

## After merge

When teammates pull, their `post-merge` / `post-rewrite` hooks compare knowledge at `HEAD` with their last snapshot and emit `sync` events, so their personal memory systems learn the decisions the team just accepted. Promoted inbox items are removed once their file is part of `HEAD`.
