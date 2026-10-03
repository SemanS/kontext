---
layout: home

hero:
  name: kontext
  text: Declarative team knowledge for coding agents
  tagline: "A bridge between your agents and git. Decisions are files in the repository, agents propose new ones, people approve them in pull requests, and every agent works from the result."
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
    title: Declared, not remembered
    details: A decision is a Markdown file that says what holds, since when and for which paths. Briefs, search and kontext why are derived from these files and the git history. Check out last year's release and agents see the decisions that held then.
  - icon: ✅
    title: Agents propose, people approve
    details: An agent's capture waits in a local inbox. It becomes team knowledge only when it is promoted into a commit and merged through a pull request, under the same CODEOWNERS and CI as the code it explains.
  - icon: 🔗
    title: Traceable to commits
    details: Each decision has an author, a date and a status, and the commit that ships it carries a Decision:&nbsp;trailer. A reversed decision is superseded, not deleted, so the record shows what held when.
  - icon: 🧭
    title: One brief, every agent
    details: ctx_brief gives Claude Code, Codex, Cursor, OpenCode or any MCP client the decisions for the paths it is about to touch, in about 1–2k tokens. Paths in a submodule or another worktree are answered by the repository that owns them.
    link: /concepts/09-several-repositories
    linkText: Several repositories
  - icon: ⏳
    title: Stale decisions surface
    details: A decision is written once while its code keeps moving. One whose paths saw 20+ commits since it was made shows up in the brief and in kontext status.
    link: /concepts/04-context-layers#freshness
    linkText: Freshness
  - icon: 🔒
    title: Local, no service
    details: One Rust binary is the MCP server, the CLI and the git hooks. No account, no server, and nothing leaves the machine unless you configure it. Briefs take ~50 ms, hooks ~0.1 s.
---

## One file per decision

```markdown
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

From this file alone:

- an agent about to edit `src/billing/` gets the decision first in its brief,
- `kontext why src/billing/round.ts:40` shows it next to the blame and the commits behind it,
- after 20 commits on `src/billing/**` it is flagged for a check,
- a newer decision that supersedes it takes its place, and `kontext log --all` keeps the chain.

## Why not Claude Code's memory or CLAUDE.md?

| | CLAUDE.md, AGENTS.md | Claude Code auto memory | Memory services | kontext |
| --- | --- | --- | --- | --- |
| Written by | people, as prose | the model, on its own | the model, from sessions | agents propose, people approve |
| Shared and reviewed | in pull requests | no: one user, one machine | per deployment, no review | in the pull request of the change |
| Reaches the agent | the whole file, every session | first 200 lines of its index, every session | by similarity to the query | by the paths it is changing |
| An entry records | no status, date or owner | when it was written | depends on the store | paths, status, date, author, commits |
| When it goes stale | stays until someone notices | the model may rewrite it | the model may overwrite it | flagged after 20 commits on its paths |
| Works with | Claude Code; others read AGENTS.md | Claude Code | their plugin, SDK or MCP server | every MCP client and the CLI |

kontext does not replace CLAUDE.md: keep the few instructions every session needs there. Decisions are too many to load whole and too important to leave unreviewed. [More on the alternatives](/getting-started/01-introduction#why-not)

## Under your control

- **Approval.** Knowledge changes only through commits. With `/.ai/ @acme/architects` in CODEOWNERS and code-owner review required, no agent changes what every agent follows without that team's approval.
- **Audit trail.** `git log -- .ai` and `kontext log` show who decided what and when. `git log --grep "Decision: <id>"` finds the commit that shipped a decision.
- **Policy in CI.** `kontext check` fails a pull request on malformed entries, duplicate ids or leaked secrets.
- **No lock-in.** The store is plain Markdown in your repository and stays readable without kontext.
