---
name: kontext-flows
description: Reorganise this repository's team knowledge (.ai/) into flows - curated trails through the existing entries, one per process, so the store reads as flows instead of a pile sorted by date. Use when asked to reorganise, group or map the knowledge into areas or flows, or to add a flow for a process.
---

# Reorganise the knowledge into flows

A flow entry is a curated trail through the knowledge in the order a reader meets it along one process: a request from upload to its published result, a release from tag to deploy, an inference call from the registry to a worker. It changes nothing about the entries it links. Ids are the lineage (trailers, the verification ledger and `[[id]]` links point at them), so a flow never copies, renames, moves or retires an entry.

## Shape of a flow entry

`kind: architecture`, file `.ai/architecture/flows/flow-<name>.md`:

```markdown
---
id: flow-<name>
kind: architecture
title: "Flow: <process>, from <start> to <end>"
summary: "One line: where it starts, what it passes, where it ends."
tags: [flow, <area>, …]
paths: [<the module globs the process passes through>]
author: <who>
origin: agent
date: <today>
---

One short paragraph: what the process is, where it starts and ends.

## Trail

### <stage>
1. [[<entry-id>]] — one sentence: what this entry settles at this step.
2. [[<entry-id>]] — …
```

A `###` heading groups a few steps into a stage. An entry may stand on more than one trail. The brief lists the flows whose `paths` overlap the focus.

## 1. Inventory

```sh
kontext api entries.list '{"v":1}'           # paged: pass `offset` from `next_offset` until it is null
```

Keep id, kind, title, summary, tags, paths and status for every active entry. Skip `mod-*` module docs, `overview`, superseded and retired entries. Agree with the person which processes get a flow (from tags, paths and module names: a handful, not one per tag) and that every entry lands on at least one trail, or is named as deliberately left out.

## 2. Assign, then write from the full entries

Assign each entry to its flows and write the assignment down (a JSON file in a scratch directory). Then, for each flow, read the **full** entries (`ctx_read kx:<id>` at level 2), not the summaries: a sentence written from a summary is wrong about one time in four. Write the trail in the order a reader meets the entries along the process. Each sentence says what the entry *settles* at that step ("Fixes the shape every later step relies on: …", "Bites when …"), never restates its title.

Check before you show it: every assigned id linked exactly once per flow, no stranger ids, no `[[id]]` that does not exist, `kontext check` clean. A second pass that compares each sentence against the full entry is worth its cost.

## 3. Record the convention once

If the store has no convention for flows yet, capture one (`kontext capture --kind convention --title "Flow entries are curated trails through the knowledge, one per process" --paths .ai/architecture/flows/**`) with the shape above, and promote it with the first flow.

## 4. Land it as a stack, one change per flow

With jj (colocated): `jj new main`, write all the flow files, then `jj split -m "docs(ai): flow, <process>" .ai/architecture/flows/flow-<name>.md` per file, the convention first; refresh the README index with `git add .ai && kontext hook pre-commit`; push the bookmark and open a pull request. With git: one commit per flow on a branch. `kontext prepare-commit` lists the trailers for each change; the hook adds them on git commits, Peer adds them on jj landings.

The person reviews and merges. A new decision or learning that belongs to a process is added to its flow in the same change, at the step where a reader meets it.
