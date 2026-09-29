# The init pipeline

`kontext init` bootstraps a repository in five phases. The first four are deterministic and finish in seconds; the fifth hands semantic work to an agent in small, resumable tasks.

```text
[1/5] scan     inventory: languages, manifests, stack, modules, dependencies, docs, ADRs
[2/5] history  decision-shaped commits, dependency swaps, churn, commit conventions
[3/5] render   .ai/kontext.toml, architecture overview, module docs, .ai/README.md
[4/5] wire     git hooks (and agent clients with --connect)
[5/5] deepen   purpose · mod:<module> · dec:<area> · refresh:<module> tasks
```

Re-running `kontext init` is safe: generated facts are refreshed, everything people or agents wrote is kept. While the bootstrap waits for review on its branch, agents on other branches are told it exists there (brief) and `ctx_init` does not start a second one.

## 1 · scan

Reads `git ls-files` (tracked plus untracked-but-not-ignored files). Submodules are reported, not descended; generated and vendored paths (`node_modules`, `dist`, `build`, `target`, `*.min.js`, lockfiles, `*.d.ts`, …) and anything that looks like a secrets file are skipped. Add your own exclusions with `init.exclude`.

It detects:

- **languages** by extension, with file and line counts,
- **manifests**: `package.json`, `Cargo.toml`, `pyproject.toml`, `setup.py`, `go.mod`, Nx `project.json`, moon `moon.yml`, `pom.xml`, `build.gradle(.kts)`, `composer.json`, `Gemfile`, `mix.exs`, `deno.json`,
- **workspace tooling** (Nx, moon, Turborepo, pnpm/npm workspaces, Cargo workspaces, Bun, uv, Deno, Lerna, Go workspaces),
- **stack and tooling** from dependencies and files (React, NestJS, Axum, FastAPI, Firebase, BigQuery, Terraform, Docker, Jest, Playwright, …),
- **modules**: every directory with a manifest (a *declared* project, kept even when small), top-level directories with at least `init.min_module_files` code files (*inferred*), and, for modules with 150+ files, *areas* one level below the module (or below its `src/`, `src/lib/`); each is described by its manifest, its README introduction or, failing both, its package doc comment (`__init__.py` docstring, `//!` crate docs, `doc.go`),
- **dependency edges** between modules from workspace dependencies, Cargo path dependencies, moon `dependsOn`, tsconfig path aliases and relative or package imports,
- **symbols** (exported functions, classes, types) with lightweight regexes for TypeScript/JavaScript, Python, Rust, Go and JVM languages (precise code intelligence is the job of [code adapters](../guides/04-code-intelligence.md)),
- **docs, ADR directories** (with their style and numbering), **agent rule files** (`AGENTS.md`, `CLAUDE.md`, …) and **environment variable names** from `.env.example`-style files (names only, never values).

## 2 · history

Mines the last `init.history_max_commits` non-merge commits (3000):

- **decision signals** in messages: *replace*, *migrate*, *switch to*, *instead of*, *deprecate*, *breaking*; choices phrased as rules (*X, not Y*, *never*, *no longer*, *is gone*); rationale (*because*, *so*, *since*, *otherwise*, *to keep*) and long explanatory bodies; Conventional-Commit types (`refactor`, `feat!`); the founding commit when its message explains the project,
- **dependency swaps** from manifest diffs: "removed X and added Y in the same commit" is one of the strongest signals,
- **churn** per file and module, and the last change date,
- **conventions**: the share of Conventional Commits, ticket keys in subjects.

Commits that only touch existing ADRs or `.ai/` are skipped (they already are knowledge), and attribution trailers (`Co-authored-by`, …) are dropped. When few commits pass the threshold (history written as rules rather than "we decided"), the next best fill up to a quarter of the history. Candidates are clustered per module; a module with one or two joins its enclosing module, and a large cluster is split into chronological parts of up to 8 commits. The strongest clusters become `dec:` tasks (`init.max_decision_tasks`, default 20).

## 3 · render

- `.ai/kontext.toml` (when no shared config exists yet), including a detected ADR directory as the home of decisions,
- `.ai/architecture/overview.md` with a *Purpose* section (the README introduction until an agent sharpens it) and generated facts: stack, a module table (plus the names of the modules without a doc), how to work in the repository, history,
- `.ai/architecture/modules/<module>.md` for the top `init.max_module_docs` modules (40) by rank, with an *Overview* placeholder and generated facts: kind, language, size, manifests, dependencies, dependents, key files, exports, packages, docs, activity,
- `.ai/README.md` and `.ai/.gitattributes`.

Generated facts sit between `<!-- kontext:facts:start … -->` and `<!-- kontext:facts:end -->`. Only that block is rewritten on later runs.

Modules are ranked by kind (apps and services first), size, how many modules depend on them, churn and whether they have docs.

## 4 · wire

Installs the [git hooks](./07-git-integration.md) unless `--no-hooks`. `--connect claude,agents-md` also wires agent clients.

## 5 · deepen

The semantic part is split into tasks that one agent turn can finish:

| Task | Asks for | Writes |
| --- | --- | --- |
| `purpose` | what the project is, for whom, its main moving parts | overview summary + *Purpose* |
| `mod:<module>` | the module's responsibility, flows, invariants, pitfalls (reading the listed docs and key files) | module summary + *Overview*, optional learnings |
| `dec:<area>` | at most three durable decisions distilled from a cluster of commits | decision entries with `commits` |
| `refresh:<module>` | an updated summary after the module's structure changed | module summary + *Overview* |

Order: purpose → the top eight modules → decision clusters → remaining modules → refreshes.

**Progress lives in the files.** A module doc carries `deepened` and `deepened_fingerprint`; decisions carry the commits they came from; skipped tasks are recorded in the clone's common dir. So you can stop at any time, continue in another worktree, or let a teammate finish after pulling. When a module's files change enough to alter its fingerprint, a `refresh:` task appears.

Who does the work:

- **the connected agent**: the `kontext-init` prompt or `ctx_init` / `ctx_init_submit` (and `ctx_init` bootstraps phases 1–3 by itself when `.ai/` does not exist yet),
- **an LLM adapter**: `kontext init --deepen --llm <adapter>`, which attaches the relevant file excerpts to each task and expects a JSON answer ([autopilot guide](../guides/06-autopilot.md)),
- **you**: `kontext init next` prints a task, `kontext init submit answer.json` records it, `kontext init skip <task>` skips it (skipping a `refresh:` task accepts the module's summary as still accurate).

Submissions are validated (length limits, secret redaction) and written into the working tree for review.
