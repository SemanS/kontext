---
id: 2026-10-19-model-help-on-related-knowledge-is-asked-once-cached-by
kind: decision
title: Model help on related knowledge is asked once, cached by content, and only labels
status: accepted
date: 2026-10-19
summary: entries.related always returns deterministic candidates; a model is asked only on request, once per text and compared revisions, and its answer is a label to check (possible conflict, check), never an instruction to replace.
tags: [knowledge-maintenance]
paths: [src/related.rs, src/api_write.rs, src/api_read.rs]
author: SemanS
origin: agent
---

## Context

Captures were compared with existing knowledge by title similarity only, and the note said "update it instead", which invites rewriting in place. Asking a model at capture, formalize and stage would cost a call each time and can point at the wrong entry.

## Decision

`entries.related {text, paths, llm?}` works in two steps: without a model, at most five entries that read alike (BM25, near the best score) or govern the same paths, each with its reason; with `llm`, one question naming relation and target together (new, same:<id>, extends:<id>, conflicts:<id>, uncertain; several allowed). Answers about entries the model was not shown are dropped (uncertain if nothing remains). The answer is cached by a hash of the text and of the compared entries' content under .git/kontext, and ctx_capture and drafts.capture/stage/formalize (`related_entries`) only reuse a cached answer, never ask. Labels read "possible conflict, check", "possibly the same, check", "extends it, check"; none says replace. drafts.revise is the same: a side-effect-free draft that supersedes the entry, model_unavailable without a model.

## Consequences

Everything works without a model. A person decides whether to supersede; the tool never prefills supersedes.
