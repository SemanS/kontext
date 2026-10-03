---
id: 2026-10-03-tools-answer-from-the-repository-that-owns-the-paths
kind: decision
title: Tools answer from the repository that owns the paths
status: accepted
date: 2026-10-03
summary: Every repository-scoped tool takes dir; without it, paths in a submodule or another repository are answered from there, and captures go to the repository that owns them when it keeps team knowledge.
paths: [src/route.rs, src/tools.rs, src/ops.rs, src/app.rs, src/mcp.rs]
author: SemanS
origin: cli
---

## Context

Agents start a session in one directory but work across repositories. camp-bot's extractor/ submodule keeps 38 decisions while the session runs in camp-bot, which has no store. A dog-ai agent edited the sibling extractor project, and a turnkey agent worked in another worktree of the same clone. kontext answered only for the directory it started in. In one thread its brief said "not set up" ten times, and captures about extractor code landed in dog-ai's inbox, where they could not be promoted.

## Decision

Every repository-scoped tool takes `dir`, a path inside another worktree, submodule or repository. Without it, the paths decide:

- Focus paths, `ctx_why` targets and capture `paths` inside a submodule (also when named relative to the submodule, as its own AGENTS.md teaches), or absolute paths into another repository or worktree, are answered from the repository that owns them.
- A capture moves only when every path belongs to that repository and it keeps team knowledge; otherwise it stays here with a note.
- A submodule with its own store gets a section of the outer brief (first, and all of the brief when the outer repository has none), and its search hits carry `kx:<submodule>/<id>`, which `ctx_read` resolves.
- Only repositories really inside the worktree count as nested: no absolute or symlinked `.gitmodules` paths, and a worktree of the same clone kept inside this one is another worktree.
- Other repositories are opened once per process (`app::open_shared`), and reopened when a submodule appears.

## Consequences

A capture is reviewed with a commit of the repository whose code it governs. Answers from elsewhere say where they came from. A brief opens a submodule only after a file check, so a repository without submodules pays nothing.
