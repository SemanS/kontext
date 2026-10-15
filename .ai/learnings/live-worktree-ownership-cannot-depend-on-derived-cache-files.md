---
id: live-worktree-ownership-cannot-depend-on-derived-cache-files
kind: learning
title: Live worktree ownership cannot depend on derived cache files
date: 2026-10-15
summary: Use Git's worktree registry to determine candidate ownership even if local state was never created or was deleted.
tags: [worktrees, ownership, cache]
paths: [src/repo.rs, src/inbox.rs, src/api_write.rs]
author: SemanS
origin: agent
---

The legacy capture path warmed the index, which recorded a worktree's path in derived state. Application captures deliberately avoid that index side effect. A sibling worktree with no path cache was then classified as an orphan, allowing another worktree to adopt its candidate. Candidate ownership now uses Git's NUL-delimited worktree registry and the same canonical root-derived key; a failed registry read conservatively keeps foreign ownership. A regression deletes/omits derived state and verifies live ownership is retained, then removes the actual worktree and verifies orphan detection.
