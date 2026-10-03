# Introduction

kontext is **declarative team knowledge for coding agents**. The decisions, conventions and pitfalls your agents follow are files in the repository, approved in pull requests like code. kontext serves them to every agent and links them to the commits they govern.

It is one Rust binary that works as three things at once:

- an **MCP server** (`kontext mcp`) that Claude Code, Codex, Cursor, OpenCode and any other MCP client can call,
- a **CLI** for people (`kontext brief`, `kontext why`, `kontext log`, …),
- a set of **git hooks** that validate knowledge, link it to commits and keep the search index current.

## The problem

Coding agents now keep their own memory, and none of it is built for a team:

- **Claude Code's auto memory** is written by the model on its own and kept per user in `~/.claude/projects/<project>/memory/`. The first 200 lines of its index are loaded into every session. Teammates' agents never see it, and nobody approves it.
- **CLAUDE.md and AGENTS.md** are shared, but each is prose loaded whole into every session. A rule has no owner, date or status, and an outdated one stays until someone notices.
- **Memory services** such as mem0, claude-mem or Zep have a model extract facts from sessions and recall them by similarity. A wrong conclusion stored once is recalled for everyone, and nobody signed off on it.

None of them can answer what a tech lead or an auditor asks: *which decisions do our agents follow in the billing code, who made them, and when?*

## Declarative, reproducible, reviewed

kontext treats team knowledge the way Nix treats a system: the desired state is declared in files, and everything else is derived from them.

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

```toml
# .ai/kontext.toml: committed, reviewed like code
[brief]
budget_tokens = 1400     # what one brief may cost an agent

[freshness]
threshold_commits = 20   # flag a decision after this many commits on its paths

[hooks]
trailers = true          # `Decision: <id>` on the commit that ships a decision

[secrets]
scan = "staged"          # scan every staged file, not only knowledge
```

- **Declarative.** The decision file says what holds, since when and for which paths. `.ai/kontext.toml` says how knowledge is served and guarded. Both are committed; nothing lives in a service or a UI.
- **Reproducible.** The same commit gives every teammate and every agent the same team knowledge. What kontext keeps in `.git/kontext/` (the search index, caches) is derived: delete it and the next call rebuilds it with the same result. Check out an older commit and agents see the decisions that held at that commit.
- **Reviewed and reversible.** Agents capture into a local inbox. A capture becomes team knowledge only when it is promoted into a commit and merged through a pull request, so the people who review the code also review what agents will follow. A decision is reversed by a newer one that supersedes it, `kontext log --all` keeps the chain, and `git revert` undoes a bad change.

The brief also lists your own inbox notes and, if you configure them, adapter sections. Those are local; the team part is what the commit declares.

## Why not something else? {#why-not}

| | CLAUDE.md, AGENTS.md | Claude Code auto memory | Memory services | kontext |
| --- | --- | --- | --- | --- |
| Written by | people, as prose | the model, on its own | the model, from sessions | agents propose, people approve |
| Shared and reviewed | in pull requests | no: one user, one machine | per deployment, no review | in the pull request of the change |
| Reaches the agent | the whole file, every session | the first 200 lines of its index, every session | by similarity to the query | ranked for the paths it names, within a budget |
| An entry records | no status, date or owner | when it was written | depends on the store | paths, status, date, author |
| When it goes stale | stays until someone notices | the model may rewrite it | the model may overwrite it | flagged after N commits on its paths (20 by default) |
| Works with | Claude Code; others read AGENTS.md | Claude Code | their plugin, SDK or MCP server | every MCP client and the CLI |

- **Path-scoped rules.** Claude Code's `.claude/rules/` can limit a rule to paths. The rule still has no status, owner or history, and other agents do not read it.
- **An ADR folder.** Keep it. kontext reads existing ADRs where they are and serves them to agents by path: see [Bootstrap an existing repository](../guides/01-bootstrap-an-existing-repository.md#existing-adrs).
- **A wiki.** It lives outside the repository, so it is not reviewed with the code it describes, and an agent cannot tell which page still holds.

kontext does not replace CLAUDE.md. Keep there the few instructions every session needs. Decisions are too many to load whole and too important to leave unreviewed; `kontext connect claude-hooks` puts the brief at the start of every Claude Code session.

## Under your control

- **Approval.** Knowledge changes only through commits. With `/.ai/ @acme/architects` in CODEOWNERS and code-owner review required, no agent changes what every agent follows without that team's approval.
- **Audit trail.** `git log -- .ai` and `kontext log --all` show who decided what, when, and what replaced it. With kontext's hooks installed, `git log --grep "Decision: <id>"` finds the commit that shipped a decision.
- **Policy in CI.** `kontext check` fails a pull request on malformed entries, duplicate ids or high-confidence secrets: see [Validate knowledge in CI](../guides/07-ci.md).
- **Data stays local.** No account and no server. Nothing leaves the machine unless you configure it, and adapters declared in a repository run only after `kontext trust`: see [Security and privacy](../concepts/08-security.md).
- **No lock-in.** The store is plain Markdown and stays readable without kontext. Removing kontext leaves the files.

## A day with kontext

1. An agent starts a task and calls `ctx_brief` with the paths it will touch. It gets the decisions and pitfalls that govern them.
2. While working it calls `ctx_search`, `ctx_read` and `ctx_why` instead of rediscovering history.
3. When something durable is settled (a decision, a convention, a gotcha), it calls `ctx_capture`.
4. Before committing it calls `ctx_prepare_commit`, which promotes the relevant candidates into the change. The hook validates them, scans them for secrets and adds `Decision: <id>` trailers.
5. The pull request carries code and knowledge together. After the merge, every teammate's agents get the new decision in their next brief.

## Principles

- **Git + Markdown is the source of truth; everything else is derived.** The search index, caches, sync snapshots and the event outbox can be deleted at any time and are rebuilt from files and history.
- **Agents never change shared truth on their own.** A capture lands in a local inbox. It becomes team knowledge only when it is promoted *into a commit*, so it shows up in the diff and is reviewed with the code it explains.
- **Brief beats complete.** Every agent-facing answer has a token budget. Details are available on demand through levels (L0 one line, L1 overview, L2 full).
- **Hooks never wait on the network.** Commits stay instant.
- **Everything else is optional.** Session archives, code graphs and semantic memory can be connected as [adapters](../adapters/01-overview.md), a few lines of configuration each. kontext works without any.

## Where to go next

- [Install kontext](./02-installation.md)
- [Quick start](./03-quickstart.md): bootstrap a repository in five minutes
- [Architecture](../concepts/01-architecture.md): how the pieces fit
