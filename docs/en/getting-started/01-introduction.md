# Introduction

kontext is a **team context bridge for coding agents**. It is one Rust binary that works as three things at once:

- an **MCP server** (`kontext mcp`) that Claude Code, Codex, Cursor, OpenCode and any other MCP client can call,
- a **CLI** for people (`kontext brief`, `kontext why`, `kontext log`, …),
- a set of **git hooks** that keep shared knowledge valid, linked to commits and in sync.

Its job is narrow and important: give every agent you run the same short, reviewed memory of *why the code is the way it is*, and turn "the agent figured something out" into "the team knows it" through ordinary commits.

## The problem

Coding agents forget everything between sessions, and each harness remembers differently. Teams answer that with memory tools — vector stores, session archives, knowledge graphs — and quickly end up with three new problems:

1. **Nobody reviews what the agent "learned".** A wrong conclusion stored once is recalled forever, for everyone.
2. **Memory is per machine and per tool.** What Claude Code knows on your laptop, Codex on your colleague's does not.
3. **Every tool wants to be the center.** Wiring five memory systems into five agents is a mesh nobody maintains.

## The idea

Split context into layers and let each layer do what it is good at:

| Layer | Typical systems | kontext's role |
| --- | --- | --- |
| Raw history — sessions, transcripts | Agent LCM, sessions, your harness | read through a `history`/`search`/`brief` adapter; never copied into git |
| Code intelligence | CodeGraph, Serena (LSP) | read through a `code` adapter, used by `ctx_why <symbol>` |
| Long-term / semantic memory | OpenViking, any HTTP or MCP store | federated into `ctx_search`; receives `sync` and private `capture` events |
| **Team truth** | **Markdown in the repository** | **owned by kontext**: capture → inbox → promote → commit → review → pull |

The first three layers are **adapters** — configuration, not code. The fourth is the part that has to be trustworthy, so it lives where the team already reviews things: in git.

## Principles

- **Git + Markdown is the source of truth; everything else is derived.** The search index, caches, sync snapshots and the event outbox can be deleted at any time and are rebuilt from files and history.
- **Agents never change shared truth on their own.** A capture lands in a local inbox. It becomes team knowledge only when it is promoted *into a commit* — so it shows up in the diff and is reviewed with the code it explains.
- **Brief beats complete.** Every agent-facing answer has a token budget. Details are available on demand through levels (L0 one line, L1 overview, L2 full).
- **Adapters are configuration.** The core knows capabilities (`search`, `read`, `store`, `history`, `code`, `brief`, `llm`), events (`capture`, `promote`, `sync`) and three drivers (`mcp`, `http`, `command`). A product is a preset you can edit.
- **Hooks never wait on the network.** Commits stay instant; deliveries to adapters go through an outbox that retries in the background.

## What a day with kontext looks like

1. An agent starts a task and calls `ctx_brief` with the paths it will touch. It gets the decisions and pitfalls that govern them.
2. While working it calls `ctx_search`, `ctx_read` and `ctx_why` instead of rediscovering history.
3. When something durable is settled — a decision, a convention, a gotcha — it calls `ctx_capture`.
4. Before committing it calls `ctx_prepare_commit`, which promotes the relevant candidates into the change. The hook validates them, scans them for secrets and adds `Decision: <id>` trailers.
5. The pull request carries code and knowledge together. After merge, teammates' hooks deliver the new knowledge to their own memory adapters.

## Where to go next

- [Install kontext](./02-installation.md)
- [Quick start](./03-quickstart.md) — bootstrap a repository in five minutes
- [Architecture](../concepts/01-architecture.md) — how the pieces fit
