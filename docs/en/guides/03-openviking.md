# OpenViking

[OpenViking](https://github.com/volcengine/OpenViking) is a context database for agents with semantic retrieval and L0/L1/L2 summaries. With kontext it becomes the semantic, personal layer next to the reviewed team layer in git:

- `ctx_search` merges OpenViking's semantic hits with kontext's local BM25 hits,
- `ctx_read viking://…` reads any OpenViking URI,
- team knowledge that reaches `HEAD` is mirrored into the project's `resources/`,
- private captures become the project's `memories/`.

## Set up

Run an OpenViking server (see its [quick start](https://github.com/volcengine/OpenViking#quick-start)), then:

```sh
kontext adapters add openviking --var user=<your-openviking-user>
kontext adapters test openviking
```

| Variable | Default | Meaning |
| --- | --- | --- |
| `user` | `$OPENVIKING_USER` or `default` | `X-OpenViking-User` |
| `account` | `default` | `X-OpenViking-Account` |
| `peer` | the project name | the per-project space: `viking://user/<user>/peers/<peer>/` |

The server URL is `$OPENVIKING_URL` or `http://127.0.0.1:1933`. The preset uses the HTTP API with trusted-mode headers; add an `Authorization` header in `[adapters.openviking.headers]` if your server requires a key.

## What gets written

| Event | Op | OpenViking URI |
| --- | --- | --- |
| `sync` (decisions, conventions, learnings, incidents at `HEAD`) | `store` | `…/peers/<peer>/resources/kontext/<kind>/<id>.md` |
| `capture` with `visibility = private` | `remember` | `…/peers/<peer>/memories/kontext/<kind>/<id>.md` |

Writes use `mode: replace`, so re-syncing an entry updates it. To mirror module docs too, add `"architecture"` to the sync subscription's `kinds`. To backfill everything once:

```sh
kontext sync --all
kontext outbox        # pending deliveries and last errors
```

## Only some repositories

Restrict the adapter with `when`, for example to repositories you opted in:

```toml
[adapters.openviking]
when = { repo = "github.com/acme/(shop|billing)" }
```

and set a different peer per repository in `~/.config/kontext/repos/<slug>.toml`:

```toml
[adapters.openviking.vars]
peer = "shop"
```

## Division of labour

| Question | Best answered by |
| --- | --- |
| "What did the team decide about X?" | kontext (git, reviewed) |
| "Have I seen something like this before?" | OpenViking (semantic, personal history) |
| "Why is this line like this?" | `ctx_why`: git history + decisions + adapters |

Keep reviewed decisions in git. Let OpenViking remember everything else — and let it learn the team's decisions through `sync`.
