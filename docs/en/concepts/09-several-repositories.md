# Several repositories

An agent's session starts in one directory. Its work often does not stay there:

- **A submodule.** The session runs in the shop repository; the decisions that govern the code live in `pricing/`, a submodule with its own `.ai/`.
- **Another worktree.** An orchestrator such as Superset starts the agent in one worktree. The agent works in another one, on the branch that has the team knowledge.
- **A sibling project.** The agent edits a shared library checked out next to the app.

A memory bound to the session's directory answers "no team knowledge here" while the decisions sit one directory away. Captures about that code land in an inbox where they can never be promoted.

## Paths decide

kontext answers from the repository that owns the paths it is given:

| The agent passes | kontext answers from |
| --- | --- |
| `ctx_brief` with `focus: ["pricing/src"]` | `pricing/`: its decisions, rules and modules, referred to as `kx:pricing/<id>` |
| `focus: ["src/round.ts"]`, a path only `pricing/` has | `pricing/` (this is how its own `AGENTS.md` names paths) |
| `ctx_why pricing/src/round.ts:40` | `pricing/`: its decisions, blame and history |
| an absolute path into another worktree or project | that worktree or project |
| `ctx_capture` with `paths: ["pricing/src/round.ts"]` | `pricing/`'s inbox, as `src/round.ts`, to be committed there |

`dir` names the repository outright, for every tool: `ctx_search` with `dir: "pricing"`, `ctx_prepare_commit` with `dir: "/work/shop-checkout"`. An answer from another repository starts with a line that says where it came from.

## What the outer repository shows

- A submodule with its own knowledge gets a section of the brief. It comes first when focus paths lie in it, and it fills the brief when the outer repository has no store. Otherwise one line points to it.
- `ctx_search` covers submodules with a store. Their hits carry the submodule's path, and `ctx_read` opens them as they are.
- A branch without the team knowledge is told which branch has it, and which local worktree:

  ```text
  - Team knowledge exists on `origin/main` but not on this branch yet … A worktree that has it: /work/shop-main (tools take `dir: "/work/shop-main"`).
  ```

- `kontext status` lists the submodules that keep their own knowledge.

## Limits

- A capture moves only when every path belongs to one repository that keeps team knowledge. Mixed paths, a submodule without a store, or paths that exist nowhere keep it where it is, and a note says why.
- Only submodules inside the worktree count. An absolute or symlinked `.gitmodules` path is ignored. A worktree of the same clone kept inside this one (`.claude/worktrees/<name>`) is another worktree, not a submodule.
