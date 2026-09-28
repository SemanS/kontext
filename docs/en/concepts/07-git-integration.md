# Git integration

kontext uses git itself as the collaboration layer: files for truth, commits for review, trailers for links, hooks for enforcement.

## Hooks

| Hook | What kontext does | Can block? |
| --- | --- | --- |
| `pre-commit` | validates staged knowledge and scans it for secrets; re-renders `.ai/README.md` from the staged tree and stages it; reminds you of inbox candidates that touch staged paths | yes, on errors |
| `prepare-commit-msg` | adds `Decision:`, `Convention:`, `Learning:`, `Incident:` trailers for staged entries (skipped for merge commits; an amend is measured from the parent of `HEAD`) | no |
| `post-commit` | queues `sync` events for knowledge that changed at `HEAD`, removes promoted inbox items that are now committed, starts a background delivery | no |
| `post-merge`, `post-rewrite` | queues `sync` events after pulls, merges and rebases | no |

Bypass once with `KONTEXT_SKIP=1 git commit …` or `git commit --no-verify`.

### Installation rules

`kontext hooks install` (also run by `kontext init`) never replaces your hooks:

- The target is `core.hooksPath` when it is set (husky's `.husky/_` is mapped to `.husky/`), otherwise `.git/hooks` (`git rev-parse --git-path hooks`, shared by all worktrees).
- In an existing shell hook a marked block is inserted right after the shebang, so it runs even if the rest of the script exits early.
- A non-shell hook (e.g. a Node script) is renamed to `<hook>.kontext-chained` and called after kontext's block.
- Each block looks for `kontext` on `PATH`, in `~/.local/bin` and `~/.cargo/bin`, and does nothing when it is missing — so shared hook directories do not break teammates who have not installed kontext.
- If the hook directory is tracked (e.g. `.githooks/`), the change shows up in `git status`: commit it to share the hooks with the team.
- If a tracked hook directory exists but is not active yet (for example `npm install` sets `core.hooksPath` later), kontext tells you and `kontext hooks install --dir .githooks` covers it.
- Hook managers that regenerate hook files (lefthook, pre-commit) are detected; add `kontext hook <name>` to their configuration instead.

`kontext hooks status` shows what is installed; `kontext hooks uninstall` removes the blocks and restores chained originals.

## Trailers

For every knowledge file staged in a commit, `prepare-commit-msg` adds a trailer (`git interpret-trailers --if-exists addIfDifferent`):

```text
fix(billing): round in cents

Decision: 2026-09-28-prices-are-integer-cents
```

Trailer keys come from `store.kinds.<kind>.trailer`. Plain git answers questions with them:

```sh
git log --format='%h %s%n%(trailers:key=Decision,valueonly)' -- src/billing
git log --grep 'Decision: 0007'
```

`kontext why` ranks commits with kontext trailers first.

Rewording with `git commit --amend -m …` keeps the trailers: the hook recognizes the amend (from its arguments, or from the committing `git` process) and measures what the commit adds from `HEAD`'s parent. A `git merge --squash` commit gets the trailers of everything it carries.

## Sync events

After a commit, merge or rebase, kontext compares the knowledge files at `HEAD` with the snapshot of the last sync (per worktree) and queues a `sync` event for every added or changed entry. The first run in a worktree only records the snapshot; `kontext sync --all` mirrors every entry once. Nothing happens unless an adapter subscribes to `sync`.

## The outbox

Hooks never talk to the network. Events are appended to `<git-common-dir>/kontext/outbox.jsonl` and a detached `kontext outbox flush` delivers them. Deliveries are tracked per adapter and op, retried up to eight times, and only one flush runs per clone. `kontext outbox` lists what is pending and the last error.

## Merges

- Entries are separate files: parallel branches rarely touch the same one.
- `.ai/README.md` is regenerated and marked `merge=union` (in `.ai/.gitattributes`).
- Sequential ADR numbers can collide between branches — the same as with any ADR practice; date-based ids (the default for new stores) avoid it.

## Worktrees

Everything local lives under the git common directory, so all worktrees of a clone share the inbox, the outbox and bootstrap progress, while each worktree keeps its own search index and sync snapshot (they follow the checked-out branch; commits that leave the history through an amend or rebase are dropped from the index).

Each inbox candidate records the worktree and branch it was captured in. With parallel agents in separate worktrees (Superset, Codex, Claude Code), `ctx_prepare_commit`, the pre-commit reminder and the brief offer only the current worktree's own candidates; another live worktree's appear apart as "not for this commit", and a removed worktree's may be adopted by anyone. `kontext inbox` shows where each one came from.
