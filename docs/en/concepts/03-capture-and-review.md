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

A capture needs a kind, a title and a body (or a summary). Good captures are short — *what, why, consequences* — and name the paths they govern:

```sh
kontext capture --kind decision --title "Use Tantivy for local recall" \
  --paths "src/index.rs" --body "BM25 is enough for code-shaped queries; semantic search comes from adapters."
```

Agents call `ctx_capture` with the same fields. On capture kontext:

- redacts secret-looking values (`capture.redact = true`),
- warns when a similar entry already exists (so it can be updated instead),
- defaults decisions to `status: accepted` and records the author (`git config user.name`) and origin (`agent` or `cli`),
- queues a `capture` event for adapters that subscribe to it.

## Visibility

| Visibility | Stays | Can be promoted | Typical use |
| --- | --- | --- | --- |
| `team` (default) | in the inbox until promoted | yes | decisions, conventions, pitfalls the team should know |
| `private` | in the inbox, and in personal adapters | no | your own notes, half-formed hypotheses, preferences |

Private captures never enter the repository. Adapters can receive them (for example as OpenViking memories) by subscribing to `capture` with `visibility = "private"`.

## Promoting

Promotion writes a candidate into the store — with the right id, directory, style and numbering — and stages it:

- `ctx_prepare_commit` with `promote=[ids]` (what agents use before committing),
- `ctx_inbox` with `action=promote`,
- `kontext promote <id>…` or `kontext prepare-commit --promote <id>`.

`capture --promote` (or `promote=true` in `ctx_capture`) skips the inbox and writes the entry into the working tree directly — useful when you already know it belongs to the current change. Either way nothing is shared until you commit.

## Before committing: `ctx_prepare_commit`

The report lists:

1. the staged change (or the working tree, when nothing is staged),
2. recorded knowledge whose `paths` cover the changed files — *confirm it still holds*,
3. inbox candidates, the ones touching changed files first,
4. validation and secret-scan results for staged knowledge (errors will block the commit),
5. module docs that may need a refresh (many changed files, or files added/removed),
6. the trailers the hook will add,
7. a nudge when a large change carries no knowledge at all.

## Review

Knowledge travels in the same pull request as the code. Reviewers see the decision next to the diff it explains; they can ask for changes, reject it (delete the file or set `status: rejected`), or propose a superseding decision later. `git log --format='%(trailers:key=Decision,valueonly)'` and `kontext log` show which commit introduced which decision.

## After merge

When teammates pull, their `post-merge` / `post-rewrite` hooks compare knowledge at `HEAD` with their last snapshot and emit `sync` events — so their personal memory systems learn the decisions the team just accepted. Promoted inbox items are removed once their file is part of `HEAD`.
