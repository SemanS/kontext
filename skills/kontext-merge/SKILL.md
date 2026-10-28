---
name: kontext-merge
description: Find knowledge entries that say the same thing, contradict each other or extend one another, and propose one superseding entry per group for a person to review. Use when asked to deduplicate, merge, consolidate or find conflicts in the team knowledge (.ai/).
---

# Merge what says the same thing

A store that grew by capture holds near-duplicates: the same rule recorded from two sessions, a learning that a later decision already settled, two decisions that quietly disagree. Merging is not editing: a new entry `supersedes` the old ones, the old ones get marked and stay in history, and every `[[id]]`, trailer and ledger line still resolves.

## 1. Find the groups

For each active decision, convention and learning (`kontext api entries.list '{"v":1}'`, paged), ask what reads alike:

```sh
kontext api entries.related '{"v":1,"text":"<title and summary>","paths":[…],"exclude_ids":["<its id>"],"llm":true}'
```

Without a model the answer lists up to five entries that read alike or govern overlapping paths, labelled `related, check`. With `llm: true` it names the relation: `same`, `extends`, `conflicts`, `new` or `uncertain`. The answer is cached per text, so a second pass is cheap. Keep only groups where the model or your own reading says `same` or `conflicts`; `extends` is usually fine as two entries.

Order the groups: conflicts first (two active rules that disagree mislead every reader), then duplicates with the most paths, then the rest. Agree with the person how many to resolve in this pass.

## 2. Read every member in full, then decide the shape

Read each member at level 2 (`ctx_read kx:<id>`). Decide:

- **Same claim** → one entry that keeps the stronger wording, the union of the paths, the evidence of both, and the most recent verification's reasoning in its body.
- **Conflict** → do not pick a winner alone. Say which one the code follows today (look at the paths, `git log`), draft the entry that states the current rule and names the rejected one under `## Alternatives`, and ask the person before capturing.
- **A learning a decision already settled** → the decision supersedes the learning; its body keeps the pitfall in one sentence so the lesson is not lost.

## 3. Capture the superseding entry

```sh
kontext capture --kind <kind> --title "…" --supersedes <id-a>,<id-b> --paths <union> --verify "<command, when one proves it>" --body …
```

(`ctx_capture` with `supersedes`.) Nothing changes in the store until the capture is promoted: `kontext prepare-commit` marks the superseded entries when the new one ships. Never edit the old entries, never delete their files. A flow or another entry that links an old id keeps working; `kontext check` reports the changed premise as an info line, which is the cue to point the trail at the new entry in the same change.

## 4. Hand over

Report each group: members, relation, what the new entry says, what it dropped. The captures wait in the local inbox; the person reviews them with `ctx_prepare_commit` and commits. One pull request per pass, so the superseded entries and their replacements are reviewed together.
