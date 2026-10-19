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

1. the staged change (or the working tree, when nothing is staged),
2. recorded knowledge whose `paths` cover the changed files (*confirm it still holds*),
3. challenges raised in this worktree, with the two ways to resolve them ([below](#challenges)),
4. inbox candidates, the ones touching changed files first,
5. validation and secret-scan results for staged knowledge (errors will block the commit),
6. module docs that may need a refresh (many changed files, or files added/removed),
7. the commit trailers, and whether the prepare-commit-msg hook adds them (a clone without kontext's hooks needs them added by hand, or `kontext hooks install`),
8. a nudge when a large change carries no knowledge at all.

A capture also names the recorded entries it may duplicate, extend or contradict (`related, check: … <kx:id> — governs the same paths (src/pricing/**)`). It is a prompt to look, never an instruction to replace: whether the new entry supersedes one is the author's call, and the reviewer's.

## Challenges {#challenges}

Sometimes an agent finds that recorded knowledge no longer holds. It says so with `ctx_challenge <id> "why"` (or `kontext challenge`): a local inbox candidate of kind `challenge`, against the entry and the commit it was read at. The challenge changes nothing by itself and cannot be promoted. The brief marks the entry `challenged locally`, and `ctx_prepare_commit` offers two ways to resolve it in the change under review:

- **retire** the entry (`retire=[id]`): its file gets status `deprecated` and a `## Retired` note with the challenge as the reason, staged;
- **replace** it: capture the new rule with `supersedes: [id]` and promote it.

Either takes the challenge out of the inbox with the commit. If the entry changed since it was challenged, the report says so; drop a challenge that turned out wrong. A challenge is resolved only when the change is reviewed and merged, not when one worktree prepared it. Shared challenges with counting of independent voices belong to an application such as Peer Hub; kontext keeps the local, reviewed part.

## Review

Knowledge travels in the same pull request as the code. Reviewers see the decision next to the diff it explains; they can ask for changes, reject it (delete the file or set `status: rejected`), or propose a superseding decision later. `git log --format='%(trailers:key=Decision,valueonly)'` and `kontext log` show which commit introduced which decision.

## After merge

When teammates pull, their `post-merge` / `post-rewrite` hooks compare knowledge at `HEAD` with their last snapshot and emit `sync` events, so their personal memory systems learn the decisions the team just accepted. Promoted inbox items are removed once their file is part of `HEAD`.
