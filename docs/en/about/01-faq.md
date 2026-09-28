# FAQ

## Is kontext a memory system like OpenViking or Mem0?

No. It owns only the small, reviewed part — the team's decisions, conventions and lessons, kept in git — and connects everything else. Semantic memory, session archives and code graphs plug in as adapters. Many teams run kontext *with* a memory system: see the [OpenViking guide](../guides/03-openviking.md).

## Why not store everything in a vector database?

Because what the team relies on has to be reviewable, diffable and versioned with the code it describes. Vector stores are great at recall and bad at being the truth. kontext keeps truth in Markdown and gets semantic recall from adapters.

## Does kontext call an LLM?

Not by itself. The agent you already use does the thinking; kontext hands it small tasks and budgeted context. Only the optional [autopilot](../guides/06-autopilot.md) calls an LLM, through an adapter you configure.

## What does it cost in tokens?

The brief is about 1–2k tokens (configurable). Searches return one line per hit within a budget. Details are read only on demand (L1/L2).

## Does it send my code anywhere?

No. Everything is local unless you configure an adapter that sends data (and then only what that adapter's ops and events declare). The autopilot sends the attached excerpts to the LLM you chose. Secret files are never read, and captured text is redacted.

## Our repository already has ADRs.

kontext detects them and writes new decisions into the same directory, in the same style and numbering. See [Bootstrap an existing repository](../guides/01-bootstrap-an-existing-repository.md#existing-adrs).

## Do teammates have to install kontext?

No. `.ai/` is plain Markdown, and hook blocks do nothing on machines without the binary. Installing it gives them the tools, the hooks and sync to their own adapters.

## Can I use it in a repository I do not own?

Yes, privately: skip hooks, keep `.ai/` out of commits, or move the store to a locally excluded directory. See [Repositories you do not own](../guides/01-bootstrap-an-existing-repository.md#repositories-you-do-not-own).

## What about monorepos?

Workspaces (Nx, moon, Turborepo, pnpm/npm, Cargo, Go, uv) are recognized; every declared project becomes a module, big ones are split into areas, and the brief is scoped with `focus`.

## Windows?

Not natively (hooks are POSIX shell). WSL works.

## How do I undo everything?

`kontext hooks uninstall`, delete `.ai/` if you do not want it, and remove `.git/kontext/`. There is no other state.

## How is this different from AGENTS.md / CLAUDE.md?

Those files are rules for agents, written by people, loaded whole. kontext keeps many small entries with paths, statuses and history, ranks them for the task at hand, links them to commits and keeps them valid — and it adds a short block to `AGENTS.md` that tells agents to use it.
