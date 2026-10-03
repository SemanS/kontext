---
layout: home

hero:
  name: kontext
  text: Declarative team knowledge for coding agents
  tagline: "Declare your team's decisions in the repository. Agents propose changes, people approve them in pull requests, and every agent at the same commit works from the same record."
  image:
    src: /logo.svg
    alt: kontext
  actions:
    - theme: brand
      text: Get started
      link: /getting-started/01-introduction
    - theme: alt
      text: Why not CLAUDE.md?
      link: /getting-started/01-introduction#why-not
    - theme: alt
      text: GitHub
      link: https://github.com/SemanS/kontext

features:
  - icon: 📄
    title: Declarative
    details: What agents follow is declared in the repository. One Markdown file per decision says what holds, since when and for which paths; .ai/kontext.toml says how it is served. Nothing lives in a service or a UI.
  - icon: 🔁
    title: Reproducible
    details: The same commit gives every teammate and every agent the same team knowledge. The local index is derived and can be deleted; the next call rebuilds it. Check out an older commit and agents see the decisions that held at that commit.
  - icon: ↩️
    title: Reviewed and reversible
    details: Agents capture into a local inbox; a capture becomes team knowledge only through a commit and a pull request. A reversed decision is superseded, not deleted, and git revert undoes a bad change like any other.
  - icon: 🧭
    title: One brief, every agent
    details: ctx_brief gives Claude Code, Codex, Cursor, OpenCode or any MCP client the decisions for the paths it names, within a token budget (1400 by default). Paths in a submodule or another worktree are answered by the repository that owns them.
    link: /concepts/09-several-repositories
    linkText: Several repositories
  - icon: ⏳
    title: Stale decisions surface
    details: A decision is written once while its code keeps moving. One whose paths saw 20 or more commits since its date (configurable) is flagged in the brief and in kontext status.
    link: /concepts/04-context-layers#freshness
    linkText: Freshness
  - icon: 🔒
    title: Local, no service
    details: One Rust binary is the MCP server, the CLI and the git hooks. No account and no server; nothing leaves the machine unless you configure it.
---

## Declare it

::: code-group

```markdown [.ai/decisions/2026-09-28-prices-are-integer-cents.md]
---
id: 2026-09-28-prices-are-integer-cents
kind: decision
title: Prices are integer cents
status: accepted
date: 2026-09-28
paths: [src/billing/**]
author: Jane Doe
---
Floats broke VAT rounding on invoices with many lines.
Every amount is stored and computed as an integer number of cents.
```

```toml [.ai/kontext.toml]
# committed, reviewed like code
[brief]
budget_tokens = 1400     # what one brief may cost an agent
max_decisions = 12

[freshness]
threshold_commits = 20   # flag a decision after this many commits on its paths

[hooks]
validate = true          # malformed entries and secrets block the commit
trailers = true          # `Decision: <id>` on the commit that ships a decision

[secrets]
scan = "staged"          # scan every staged file, not only knowledge
```

:::

From these two files, kontext derives the rest:

- an agent that calls `ctx_brief` with `focus: ["src/billing"]` gets the decision first,
- `kontext why src/billing/round.ts:40` shows it next to the blame and the commits behind it,
- the commit that ships it carries `Decision: 2026-09-28-prices-are-integer-cents`,
- after 20 commits on `src/billing/**` it is flagged for a check,
- a newer decision that supersedes it takes its place, and `kontext log --all` keeps the chain.

## Why not Claude Code's memory or CLAUDE.md?

| | CLAUDE.md, AGENTS.md | Claude Code auto memory | Memory services | kontext |
| --- | --- | --- | --- | --- |
| Written by | people, as prose | the model, on its own | the model, from sessions | agents propose, people approve |
| Shared and reviewed | in pull requests | no: one user, one machine | per deployment, no review | in the pull request of the change |
| Reaches the agent | the whole file, every session | the first 200 lines of its index, every session | by similarity to the query | ranked for the paths it names, within a budget |
| An entry records | no status, date or owner | when it was written | depends on the store | paths, status, date, author |
| When it goes stale | stays until someone notices | the model may rewrite it | the model may overwrite it | flagged after N commits on its paths |
| Works with | Claude Code; others read AGENTS.md | Claude Code | their plugin, SDK or MCP server | every MCP client and the CLI |

kontext does not replace CLAUDE.md: keep the few instructions every session needs there. Decisions are too many to load whole and too important to leave unreviewed. [More on the alternatives](/getting-started/01-introduction#why-not)

## Under your control

- **Approval.** Knowledge changes only through commits. With `/.ai/ @acme/architects` in CODEOWNERS and code-owner review required, no agent changes what every agent follows without that team's approval.
- **Audit trail.** `git log -- .ai` and `kontext log --all` show who decided what, when, and what replaced it. With kontext's hooks installed, `git log --grep "Decision: <id>"` finds the commit that shipped a decision.
- **Policy in CI.** `kontext check` fails a pull request on malformed entries, duplicate ids or high-confidence secrets.
- **No lock-in.** The store is plain Markdown in your repository and stays readable without kontext.
