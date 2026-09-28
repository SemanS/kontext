# Bootstrap an existing repository

`kontext init` works on any git repository. This guide covers what to check in larger or older ones.

## Run it on a branch

```sh
git switch -c kontext/bootstrap
kontext init
git status -- .ai              # what was written
kontext brief --no-adapters    # what an agent will see
```

Nothing is committed and nothing leaves the machine. If you do not like the result, delete `.ai/` (and `kontext hooks uninstall`) — there is no other state to clean up.

## Review the detection

`kontext init` prints what it found. Check three things in `.ai/architecture/overview.md`:

1. **Modules.** The *Layout* table should match how you think about the code. Declared projects (a manifest, an Nx `project.json`, a moon `moon.yml`, a Cargo crate) are always modules; large modules are split into *areas*. Exclude noise with `init.exclude` (globs) and rerun:

   ```toml
   [init]
   exclude = ["legacy/**", "**/generated/**", "tools/playground/**"]
   max_module_docs = 30
   ```

2. **Stack.** Detected from manifests and files. It is informational — agents read it in the brief.
3. **Purpose.** Taken from the README introduction; if your README starts with setup instructions it stays empty until the `purpose` task writes it.

## Existing ADRs

If the repository keeps decision records in `docs/adr`, `docs/decisions`, `adr/`, … kontext detects the directory, its style (front matter or `**Status:**` fields) and numbering, and writes this into `.ai/kontext.toml`:

```toml
[store.kinds.decision]
path = "docs/adr"
style = "fields"
numbering = "sequential"
trailer = "Decision"
```

Existing ADRs appear in `kontext log`, the brief and search right away, and new decisions get the next number in the same format. Commits that only touch ADRs are not mined again.

## Hooks in repositories that already have hooks

- A shell hook in `.git/hooks` or in `core.hooksPath` gets a kontext block after its shebang; your logic stays untouched.
- A **tracked** hook directory (e.g. `.githooks/` activated by `npm install`) is modified in the working tree — commit the change to share it, or keep kontext out of shared hooks and let each teammate run `kontext hooks install` locally.
- husky (`.husky/`) is supported; lefthook and pre-commit regenerate their hooks, so call `kontext hook pre-commit` (etc.) from their configuration instead.

`kontext hooks status` shows the result.

## Monorepos

- Nx, moon, Turborepo, pnpm/npm workspaces and Cargo workspaces are recognized; every declared project becomes a module and dependency edges come from the workspace graph and imports.
- Module docs are capped (`init.max_module_docs`, default 40) and ranked: apps and services, then heavily used libraries. The rest are listed in the overview.
- Scope `ctx_brief` with `focus` — the brief then ranks the knowledge of the modules being touched.

## Submodules

Submodules are reported but not scanned. Bootstrap each submodule in its own repository: decisions about shared code belong where that code lives.

## Repositories you do not own

For a client's repository, decide with its maintainers before committing `.ai/` or shared hooks. You can still use kontext privately:

- skip the shared parts: `kontext init --no-hooks`, keep `.ai/` uncommitted (add it to `.git/info/exclude`), or
- relocate the store to a path that is excluded locally, via clone-local config in `.git/kontext/config.toml`:

  ```toml
  [store]
  dir = ".kontext-local"
  ```

Captures, the inbox and adapters work the same either way; only sharing through commits is off.

## Deepen

Hand the rest to an agent (`/mcp__kontext__kontext-init`) or the [autopilot](./06-autopilot.md). Start with `purpose` and the top modules; decision tasks (`dec:`) turn the most decision-shaped commits into records. Review the result like any other pull request.
