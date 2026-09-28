# Quick start

This walkthrough bootstraps kontext in an existing repository, connects an agent and records a first decision.

## 1. Bootstrap the repository

```sh
cd your-repo
kontext init
```

```text
kontext init · github.com/acme/shop
[1/5] scan     ✓ 3149 files · TypeScript 91% · Nx/npm workspaces · 94 modules · 63 manifests (1.2s)
[2/5] history  ✓ 1498 commits · 220 decision-shaped in 36 areas (0.7s)
[3/5] render   ✓ .ai/architecture: overview (written) + 40 module docs (40 new, 0 updated) · config .ai/kontext.toml (0.1s)
[4/5] wire     ✓ git hooks in .git/hooks: post-commit, post-merge, post-rewrite, pre-commit, prepare-commit-msg
[5/5] deepen   · 0/51 tasks done — let your agent continue with the `kontext-init` prompt …
```

The first four phases are deterministic and take seconds. They write an architecture overview and one document per module into `.ai/`, reuse an existing ADR directory if you have one, and install git hooks next to any hooks you already use. Nothing is committed.

Review the result and commit it on a branch, so the team sees the bootstrap in a pull request:

```sh
git switch -c kontext/bootstrap
git add .ai && git commit -m "docs: bootstrap team knowledge"
```

## 2. Connect your agent

```sh
kontext connect claude --write      # writes .mcp.json (project scope, shared with the team)
kontext connect agents-md --write   # adds a short "how to use kontext" block to AGENTS.md / CLAUDE.md
kontext connect codex               # prints the ~/.codex/config.toml snippet (--write appends it)
```

See [Set up your agents](./04-setup-for-agents.md) for Cursor, OpenCode and automatic briefs at session start.

## 3. Let the agent deepen the bootstrap

In Claude Code run the prompt **`/mcp__kontext__kontext-init`**, or ask any connected agent to "continue the kontext bootstrap". It pulls small tasks with `ctx_init` — describe the project, summarize a module, distill decisions from a cluster of commits — and answers each with `ctx_init_submit`. Stop whenever you like; progress is kept in the files.

No agent at hand? Use an LLM CLI as an autopilot:

```sh
kontext adapters add llm-claude
kontext init --deepen --llm llm-claude --jobs 3 --max 20
```

## 4. Work with it

```sh
kontext brief --focus src/billing      # what an agent sees first
kontext search "rounding of prices"    # knowledge, docs, commit history, adapters
kontext why src/billing/round.ts:40    # decisions, module summary, history, blame
kontext log                            # the decision timeline
```

## 5. Record a decision and ship it with the change

```sh
kontext capture --kind decision --title "Prices are integer cents" \
  --paths "src/billing/**" \
  --body "Floats broke VAT rounding. Every amount is an integer number of cents."

git add src/billing/round.ts
kontext prepare-commit                          # shows the candidate next to the staged change
kontext prepare-commit --promote <inbox-id>     # writes .ai/decisions/…md and stages it
git commit -m "fix(billing): round in cents"
git log -1 --format=%B                          # … Decision: 2026-09-28-prices-are-integer-cents
```

Agents do the same through `ctx_capture` and `ctx_prepare_commit`. The pre-commit hook validates the entry, scans it for secrets and refreshes `.ai/README.md`; the prepare-commit-msg hook adds the trailer.

## Next steps

- [Team workflow](../guides/02-team-workflow.md) — reviews, superseding decisions, onboarding teammates
- [Adapters](../adapters/01-overview.md) — plug in OpenViking, CodeGraph, Serena, session archives
- [Bootstrap an existing repository](../guides/01-bootstrap-an-existing-repository.md) — monorepos, existing ADRs, client repositories
