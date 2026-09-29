# Team workflow

## Roll out

1. One person bootstraps the repository on a branch (`kontext init`, deepen, review) and opens a pull request with `.ai/`, `.mcp.json` (`kontext connect claude --write`) and the `AGENTS.md` block (`kontext connect agents-md --write`).
2. The team reviews it like code: wrong module summaries or decisions are cheaper to fix now.
3. After merge, each teammate installs kontext and runs `kontext hooks install` once per clone (unless hooks are shared through a tracked hook directory).

Teammates without kontext are not affected: hook blocks are no-ops without the binary, and `.ai/` is plain Markdown.

## The daily loop

| Step | Agent | Person |
| --- | --- | --- |
| Start | `ctx_brief` with focus paths | `kontext brief --focus <path>` |
| Understand | `ctx_why`, `ctx_search`, `ctx_read` | `kontext why <path[:line]>`, `kontext search …` |
| Record | `ctx_capture` | `kontext capture …` |
| Before commit | `ctx_prepare_commit` (promote relevant candidates) | `kontext prepare-commit [--promote id]` |
| Commit | hooks validate, refresh the index, add trailers | same |

Good habits:

- capture when the decision is made, not at the end of the week (the inbox is cheap),
- one decision per entry, a few lines each; link longer write-ups instead of pasting them,
- give every entry `paths`: that is what makes it show up for the right change.

## Reviewing knowledge in pull requests

Treat entries like code:

- **Is it true and still intended?** An agent may have over-generalized from one commit.
- **Is it scoped?** `paths` should cover what the decision governs, not the whole repository.
- **Is it brief?** Context, decision, consequences: a few lines each.
- **Does it replace something?** Then it should `supersede` the old entry rather than contradict it silently.

## Changing your mind

Decisions are not edited to reverse them. Record a new one that supersedes the old:

```sh
kontext capture --kind decision --title "Orders move to event sourcing" \
  --supersedes 0007 --paths "src/orders/**" --body "…"
```

The old entry becomes `superseded` and points to the new one; `kontext log --all` shows the chain, and the brief only lists active decisions.

## Pitfalls and incidents

- `--kind pitfall` (a learning tagged `pitfall`) for gotchas that cost someone an afternoon.
- `--kind incident` for what broke, the cause and what changed (short; link the full post-mortem).

## Onboarding someone

```sh
kontext brief                 # the map
kontext log                   # the decisions, newest first
kontext why src/payments      # where to be careful
```

An agent given `ctx_brief` with the newcomer's first task as `focus` produces a good first tour.

## Keep it healthy

- `kontext check` (or [in CI](./07-ci.md)) validates all entries and finds paths that no longer match anything.
- `ctx_prepare_commit` flags module docs whose modules changed a lot; `ctx_init` then offers `refresh:` tasks.
- Rerun `kontext init` after large restructurings: facts are refreshed, written text is kept.
