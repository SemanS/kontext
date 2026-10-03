# Agents working on kontext

How to build, check and document a change: [CONTRIBUTING.md](CONTRIBUTING.md).

<!-- kontext:start -->
## Team context (kontext)

- Start a task with `ctx_brief` (MCP server `kontext`): decisions, conventions, pitfalls and the module map, within a small token budget. Pass `focus` with the paths you will touch.
- Need more? `ctx_search` (decisions, docs, commit history, connected memory systems) and `ctx_read <uri>` for detail. `ctx_why <path[:line]>` explains why code is the way it is.
- Decided something durable, or hit a non-obvious gotcha? `ctx_capture` it (kind: decision | convention | learning | incident; paths you touched). Personal notes: `visibility: private`.
- Before committing: `ctx_prepare_commit` — it lists relevant inbox candidates, validates knowledge files and promotes the ones that belong to the change so they ship in the same commit and get reviewed with it.
- Working in another worktree, a submodule or a sibling repository? Pass `dir` (a path inside it) so the tools answer from there.
- Shared knowledge lives in `.ai/` and changes only through commits and review — never edit it to win an argument; propose a superseding decision instead.
<!-- kontext:end -->
