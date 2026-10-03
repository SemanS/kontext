# Changelog

All notable changes are listed here. The project follows [Semantic Versioning](https://semver.org/); until 1.0 minor versions may change configuration or the entry format, always with a migration note.

## Unreleased

From watching how agents used kontext across Superset workspaces: most of the time they called it, it had nothing to give, because the knowledge was in a submodule, another worktree or the local inbox.

**The repository that owns the paths**
- Every repository-scoped tool takes `dir`: a path inside another worktree, a submodule or a sibling repository. Answers from another repository start with where they came from.
- Focus paths, `ctx_why` targets and `ctx_capture` paths inside a submodule are answered from the submodule. Captures go into its inbox with the paths rewritten, when it keeps team knowledge. A brief of a repository without knowledge of its own carries its submodules'. Search covers them too (`kx:extractor/<id>`), and `ctx_read` opens their entries and commits.
- A path relative to a submodule (`apps/pipeline-runner`, as the submodule's `AGENTS.md` names it) counts as the submodule's when only one submodule has it. Absolute paths into another repository or worktree are answered from there. A capture with paths that exist nowhere says so.
- A branch without the team knowledge is told which branch, and which local worktree, has it. The server's instructions name submodules with their own knowledge.

**Local notes come back**
- `ctx_search` finds inbox notes (team candidates and private notes), and the brief lists the ones matching its focus. Private captures in a repository without a store were written and never found again.
- In a repository without a store, a brief with a focus lists the related docs and commits, once a search has built the local index (a brief does not build it).

**Smaller answers**
- Brief references are the shortest unique prefix of an id (`[2026-09-28-adapters]`), and a dated reference drops the repeated date: decision lines are about 20% shorter, and `ctx_read` resolves the prefix.
- `ctx_threads` renders Claude Code's `<task-notification>` (a finished background agent: ids, output file, note, usage, the whole result) as one line, and shortens per-session temp paths. On a thread that fanned out work, the transcript went from about 81,000 to 30,000 characters. Parts default to 20,000 characters, so that one fits a tool result (Codex cut 40,000-character parts and the agent read them twice).
- The server's instructions for a repository without a store are shorter. The tool descriptions are tighter, which pays for `dir`.

**Commits**
- `ctx_prepare_commit` says whether the prepare-commit-msg hook adds the trailers. A fresh clone has no hooks, and the report used to promise them anyway. The brief warns when a clone with a knowledge store has no hooks.
- `kontext hooks install` / `uninstall` take `--hooks-dir` instead of `--dir`, which collided with the global `-C/--dir`: `kontext -C <repo> hooks install` wrote the hooks into the repository root. The old `hooks install --dir .githooks` is refused with a pointer to `--hooks-dir`.
- The `AGENTS.md` block written by `kontext connect agents-md` mentions `dir`.

**Knowledge freshness**
- The brief's *State* and `kontext status` flag active decisions whose `paths` were touched by many commits since the decision's date (`[freshness] threshold_commits`, default 20, `0` turns it off), so an agent or reviewer checks whether they still hold. One `git log` covers every decision and is cached per `HEAD`; renames out of a governed path count, the decision's own day does not. See [Freshness](../concepts/04-context-layers.md#freshness).
- The brief keeps room for its *State*, so warnings survive a brief full of knowledge.

## 0.1.5 (2026-09-29)

**Autopilot**
- `llm-claude` runs on the subscription of the logged-in account (`ANTHROPIC_API_KEY` / `ANTHROPIC_AUTH_TOKEN` are dropped for its calls), with no tools, no MCP servers and a short system prompt (about a tenth of the tokens per task), and without saving the calls as Claude Code sessions. New `effort` variable (default `high`); timeout 10 minutes. Opus: `kontext adapters add llm-claude --force --var model=claude-opus-5-5`.

**Init**
- `kontext init --no-hooks` is remembered for the clone: a later `init` or `init --deepen`, from any worktree, no longer installs hooks where they were declined; `kontext hooks install` adds them.

## 0.1.4 (2026-09-29)

**Sensitive threads**
- Thread transcripts (`kontext distill`, `ctx_threads`) and what is distilled from them mask email addresses, phone numbers, IBANs, card numbers and IP addresses, besides secrets; `secrets.redact` adds your own patterns (a client's name, an integration id).
- The model is asked for roles instead of people's names and to leave out contact data, records, credentials and hosts; its entries are masked again.
- Measured on a client thread: 52 email addresses, 2 phone numbers and 13 IP addresses no longer reach the model, and the distilled entries named no one.

## 0.1.3 (2026-09-29)

**Knowledge from agent threads**
- `kontext distill` reads Claude Code and Codex threads (every Claude account, `$CODEX_HOME`), Superset workspaces (by id, worktree name, path or branch; `--superset` alone means the current one) or any text, and captures the durable decisions, conventions, pitfalls and incidents an `llm` adapter finds in them into the inbox, with paths, commits, `origin: thread` and the thread as source. Transcripts are compacted and redacted, split into parts for long threads, and checked against what is recorded already. See [Knowledge from agent threads](../guides/08-distill-threads.md).
- `ctx_threads` (read-only) gives connected agents the same transcripts, in parts; the `kontext-distill` prompt walks an agent through distilling one with `ctx_capture`, which now takes `source` and `commits`.
- A Superset workspace's threads that ran in another repository are skipped, and a workspace of another repository is refused.

## 0.1.2 (2026-09-29)

From deploying kontext into that monorepo with Claude Code and Codex connected.

**Bootstrap**
- The autopilot's LLM calls no longer reach kontext's own MCP server, and the prompt asks for the JSON answer only: with kontext connected to Codex, a model recorded each task itself through `ctx_init_submit` and again through its answer, so learnings and decisions appeared twice (`…-2`). The `llm-codex` preset passes `-c mcp_servers.kontext.enabled=false`, `llm-claude` passes `--strict-mcp-config`. Presets live in your config: refresh them with `kontext adapters add llm-codex --force` (and `llm-claude`).
- While `kontext init --deepen` runs, `ctx_init_submit` from another process refuses the run's tasks.
- A resubmitted task replaces its own earlier learnings and decisions instead of adding copies, and leaves entries people wrote alone.
- A branch without team knowledge is told when another branch has it (a bootstrap waiting for review): the brief says to merge it, and `ctx_init` refuses to bootstrap a second, conflicting `.ai/`.
- Entries mined by init get no commit trailers: they come from the older commits listed in their `commits`, and a bootstrap commit no longer carries dozens of `Decision:` / `Learning:` lines.

**Agents**
- Registered for all repositories (user scope), kontext only reads where nobody set it up: in a repository without a knowledge store the MCP instructions say so, `ctx_capture` refuses team entries (private notes still work), promotions are refused, and `ctx_init` bootstraps only with `bootstrap=true` (the `kontext-init` prompt passes it).
- A decision or learning captured without `paths` and promoted with a change governs that change's files (conventions stay repository-wide), so `ctx_why` finds it on the code it explains.
- `kontext connect codex` writes to `$CODEX_HOME/config.toml` when `CODEX_HOME` is set (as account-switching wrappers do), creates the directory if needed and mentions a backup only when it made one.

## 0.1.1 (2026-09-28)

Fixes from a field test on a 74-module moon/Cargo monorepo worked by parallel Claude Code and Codex agents.

**Agents**
- Codex no longer refuses kontext's tools under `approval_policy = "never"`: every tool carries MCP annotations (`readOnlyHint`, `destructiveHint`, `title`), and `kontext connect codex --write` adds `default_tools_approval_mode = "approve"` (also to an existing entry).
- Parallel worktrees: `ctx_prepare_commit`, the pre-commit reminder and the brief offer only the inbox candidates captured in the current worktree; candidates of another live worktree are listed apart ("not for this commit"), those of a removed worktree may be adopted.
- The brief lists every `AGENTS.md` / `CLAUDE.md` between the root and a focused path, and gives a focused module without a module doc its scan facts.
- Empty values in list arguments (`--paths a,`, an agent's `[""]`) are dropped instead of becoming entry paths, tags or supersessions.

**Git**
- `git commit --amend -m …` keeps the `Decision:` / `Convention:` … trailers (an amend is measured from the parent of `HEAD`); `git merge --squash` commits get them too.
- Hooks read staged knowledge in one `git cat-file --batch` and match entry paths without recompiling globs: a commit of 44 knowledge files went from 4.2 s to 0.4 s.
- The search index forgets commits that left the history (amended, rebased).

**Bootstrap**
- Decision mining recognizes choices phrased as rules ("X, not Y", "never", "no longer", "is gone"), more rationale words and long explanatory bodies, and the founding commit; when few commits pass, the next best fill up to a quarter of the history. Small clusters join their enclosing module, large ones are split into chronological parts of up to 8 commits; `init.max_decision_tasks` defaults to 20. Attribution trailers are dropped from mined messages.
- Module descriptions fall back to the package's own doc comment (`__init__.py` docstring, `//!` crate docs, `doc.go`); README introductions that end in a lead-in (`…:`) keep their first sentences.
- The overview names the modules without a module doc and counts the agent rule files it does not list.

**Retrieval**
- Federated search fuses sources by weighted reciprocal rank, so an adapter's compressed similarity scores no longer push local knowledge out; adapter copies of local entries (a `sync`ed `…/<id>.md`) collapse into the local hit.

**Security**
- Secrets written into sentences ("the secret is …", "rotate the token …") are detected when the value looks random; paths and placeholders are left alone.

## 0.1.0 (2026-09-28)

First public release.

**Knowledge store**
- Markdown entries (decision, convention, learning, incident, architecture) with flat front matter, or classic ADRs with `**Status:**` fields; date, sequential or no numbering; supersession; validation; generated, union-merged `.ai/README.md`.

**Agents**
- MCP server over stdio: `ctx_brief`, `ctx_search`, `ctx_read`, `ctx_why`, `ctx_log`, `ctx_capture`, `ctx_inbox`, `ctx_prepare_commit`, `ctx_init`, `ctx_init_submit`; prompts `kontext-init`, `kontext-commit`, `kontext-reflect`; resources `kontext://brief` and `kontext://entry/{id}`.
- `kontext connect` for Claude Code (project `.mcp.json`, SessionStart hook), Codex, Cursor, OpenCode and `AGENTS.md`.

**Bootstrap**
- `kontext init`: scan (languages, manifests incl. Nx and moon, stack, modules and areas, dependency edges, ADRs, agent rules), history (decision signals, dependency swaps, churn, conventions), render, wire, and a resumable deepen protocol for agents or an LLM adapter.

**Retrieval**
- Tantivy BM25 over entries, section-chunked docs and commit history with identifier expansion; federated search across adapters; `ctx_why` for paths, line ranges, commits and symbols; decision timeline.

**Adapters**
- Generic `mcp` (stdio and streamable HTTP), `http` and `command` drivers; ops, events (`capture`, `promote`, `sync`), schema-based MCP argument binding, result mapping (`items`, `split`, `where`, `map` with paths, `re:`, `tpl:`, literals), federated and declared tools, `when` conditions, trust for repository-declared adapters.
- Presets: OpenViking, CodeGraph, Serena, Agent LCM, sessions, Claude/Codex/Ollama LLMs, Slack webhook.

**Git**
- Hooks (pre-commit validation and secret scanning, prepare-commit-msg trailers, post-commit/merge/rewrite sync) installed alongside existing hooks, `core.hooksPath` and husky; an outbox with retries; worktree-aware local state.
