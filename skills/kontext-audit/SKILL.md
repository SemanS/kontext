---
name: kontext-audit
description: Audit this repository's team knowledge (.ai/) within a budget - confirm what still holds, challenge or supersede what does not, starting where the code moved most. Use when asked to review, clean up, refresh or audit the knowledge, or when the brief lists entries that may need a refresh.
---

# Audit the knowledge within a budget

Knowledge goes stale in silence: an entry was true when written, the code under it moved, and nothing says so until someone looks. Looking costs the same actions as the task, so an audit is an errand with a budget: check where a doubt is worth resolving, record every look, stop when the budget is spent. Nothing is deleted on the way; a wrong entry is superseded or retired, and the old text stays in history.

## 1. Agree the budget

Ask how many entries to look at in this pass (default 10) and whether to stay on some paths. One pass of ten careful looks beats a sweep that stamps everything.

## 2. Collect the candidates, most doubtful first

```sh
kontext status                       # "may need a refresh" (commits on their paths since a look) and "due for review"
kontext drift                        # entries whose governed files changed in the range without a look
kontext api entries.list '{"v":1}'   # every entry; `verification: null` means no recorded look at all
```

Order: challenged locally, past `review_by`, most commits on their paths since the last look, then entries with no verification at all. Constraint entries (a rule that says *never* or *only*) go before descriptive ones: a stale constraint silently steers every later decision. Skip module docs (`mod-*`) and flows (`flow-*`) unless asked; they are maps, not claims.

## 3. Look at each one, then record exactly one outcome

For each candidate within the budget:

1. Read it: `ctx_read kx:<id>` (level 1) or `kontext read kx:<id>`.
2. Look at what it governs: `git log --oneline -- <its paths>` since its last look, and the code itself where the claim can be seen. If it carries a `verify` command, `ctx_verify` runs it only when a person pinned it on this machine. When it answers `untrusted_verify` or hands the command back, show the person the command and stop on that entry: a verify command is code from the repository. Never run `kontext trust` yourself; the person pins it at their terminal with `kontext trust --verify <id>`.
3. Record one of:
   - **Still holds** → `ctx_confirm` with `because: "<what you saw>"`. Confirm only what you looked at.
   - **Wrong now, and you know the replacement** → `kontext capture --kind <kind> --title "…" --supersedes <id> --paths … --body …` (`ctx_capture` with `supersedes`), with the proof as `--verify` when a command shows it.
   - **Wrong now, replacement unknown** → `kontext challenge <id> "<why>"` (`ctx_challenge`). The brief marks it `challenged locally` until someone retires or supersedes it.
   - **Cannot tell** → leave it, say so in the report. A stamp you cannot stand behind is worse than none.

Never edit an entry's claim in place to make it true again. Never retire or supersede on a hunch: the look must name the commit or file that changed the premise.

## 4. Report and hand over

Stop at the budget. Report: entries confirmed, superseded, challenged, left; what is now in the local inbox (`kontext inbox`); which paths produced most doubt (the next pass starts there). Captures and challenges wait in the inbox until `ctx_prepare_commit` promotes them with a commit; confirmations are staged already. The person reviews and commits; nothing in the shared store changes before that.
