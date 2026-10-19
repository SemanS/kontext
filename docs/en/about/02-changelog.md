# Changelog

All notable changes are listed here. The project follows [Semantic Versioning](https://semver.org/); until 1.0 minor versions may change configuration or the entry format, always with a migration note.

## 0.3.0 (2026-10-19)

**Knowledge that stays correct after it is written.** The brief and search no longer let a proposal pass for an accepted rule, merging sources no longer drops a different claim that happens to start the same way, and recorded knowledge can now carry a review date and its sources, be challenged and be retired, always through review. Measured on a new [evaluation set](https://github.com/SemanS/kontext/tree/main/tests/eval) of 33 scenarios: 8 passed on 0.2.1 (23 of 64 expectations found, 2 hits wrongly suppressed, 1 noisy); all 33 pass now (69 of 69, none suppressed, no noise).

- **Authority marks.** A line in the brief and a search hit say when an entry is not accepted (`(proposed)`, `(deprecated)`), when its file is new, edited or staged and nobody approved it (`local, unapproved`), when its review date passed (`review due`), when it is challenged locally, and how many sources it has. Accepted, committed entries carry no mark. See [Marks](../concepts/04-context-layers.md#marks).
- **Safe deduplication.** Search merged hits whose first 90 normalized characters matched, so one of two different claims vanished. Now only proven copies are dropped (the same URI, an adapter's copy of a local entry, a full-text match); a look-alike is shown under the first one with what differs.
- **Changed since the last brief.** The brief lists, in at most five lines, the knowledge added, changed or removed since this worktree's last brief (the last 14 days without one), and says which entry a new one supersedes. A brief without changes adds nothing. See [Changed since the last brief](../concepts/04-context-layers.md#changed).
- **Review dates.** `review_by: YYYY-MM-DD` on any entry; past it the entry is marked in the brief, search and `kontext status`, never hidden. Optional per-kind default `[store.kinds.<kind>] review_after = "90d"`, off by default. See [Review dates](../concepts/02-knowledge-store.md#review-by).
- **Evidence.** Captures take `evidence: [{ref, quote?}]` with references `pr:`, `issue:`, `task:`, `code:<repo>@<rev>:<path>#L<a>-<b>` and `url:`, kept in `extra.external_refs`. A quote of a code reference into this repository is checked against that revision; anything else is marked unverifiable, never refused. See [Evidence](../concepts/02-knowledge-store.md#evidence).
- **Challenges and retirement.** `ctx_challenge` / `kontext challenge` record that an entry no longer holds; `ctx_prepare_commit retire=[id]` retires it (status `deprecated` and a `## Retired` note, staged) or a replacement supersedes it. Retire and supersede work for every kind. See [Challenges](../concepts/03-capture-and-review.md#challenges).
- **Related knowledge.** `ctx_capture` names the entries a new one may duplicate, extend or contradict, with the reason (similar wording, the same paths); the old note that said "update it instead" is gone. A model is asked only on request, through `entries.related`, and its answer is cached.
- **Search.** A whole-query phrase bonus ranks an exact wording first (on the evaluation set the exact-phrase query moved from rank 2 to 1; queries in one's own words kept their rank). `[search] adapters = "fallback"` asks the search adapters only when local hits are fewer than `min_local_hits`, or when asked (`adapters: true`, `--adapters`); the default stays `always`.
- **Application API.** New operations `entries.related`, `drafts.retire` and `drafts.revise` (listed in `status.capabilities`); `drafts.capture`, `drafts.stage` and `drafts.formalize` also return `evidence` and `related_entries`, and `drafts.stage` `resolved_challenges`; entries carry `review_by`, and working reads `publication.blob_hash`, a per-entry content revision. See [Knowledge maintenance](../reference/05-application-api.md#maintenance).

Applications can use a shared knowledge engine through the [application API](../reference/05-application-api.md):
versioned JSON CLI and structured MCP, configured-store queries, path applicability, read-only extraction,
idempotent draft staging and committed exports pinned to an explicit review ref. Existing human CLI
commands and agent tools remain compatible.

**Migration.** Nothing in `.ai/` needs to change. Things a script or client may notice:

- `kontext search` (text) no longer prints `accepted` among a hit's metadata, and prints a look-alike hit as an unnumbered `↳ similar, differs: …` line; `--json` hits may carry `similar_to`.
- The brief's decision lines may carry marks after the title, a new *Changed since …* section may appear, and the brief writes a small cache (`brief-head`) into the worktree's local state under `.git/kontext/`.
- `ctx_capture` and `kontext capture` notes say `related, check: … <kx:id> — <reason>` instead of `similar existing entry: … — update it instead`.
- `review_by` (and `review-by`) are entry fields now: as an `extra` key in `drafts.*` they are refused as reserved; pass `entry.review_by` instead.
- `inbox.list` can return candidates of kind `challenge`; they cannot be staged as entries (`invalid_input`).

## 0.2.1 (2026-10-03)

**Agents act on the decisions that govern a change.** In a test thread, an agent was given the brief injected at session start, which lists only the newest decisions, and made a change that contradicted an older one. It learned of the conflict only at commit time, from a one-line summary that left the rule out, and recorded nothing. Now:

- an unfocused brief says that `ctx_brief` with `focus` lists the decisions that govern the paths being changed,
- `ctx_prepare_commit` quotes each covering decision's rule (the `## Decision` paragraph) instead of its summary, and says how to record a superseding decision and promote it into the same commit.

The same task then ended with the superseding decision in the commit and a `Decision:` trailer.

## 0.2.0 (2026-10-03)

**Migration:** `kontext hooks install --dir <dir>` is now `--hooks-dir <dir>` (likewise `uninstall`). `--dir` names the repository, like `-C`.

**Knowledge in submodules and other worktrees.** A session that started in the app repository but worked in a submodule, another worktree or a sibling project got "not set up" from kontext, and its captures landed in an inbox they could not be promoted from. Now the paths decide which repository answers, and every tool takes `dir`. See [Several repositories](../concepts/09-several-repositories.md).

**Private notes are found again.** A private capture in a repository without a store was written once and never read, because search did not cover the inbox. `ctx_search` now finds inbox notes, and the brief lists the ones that match its focus.

**Stale decisions are flagged.** A decision is written once while its code keeps moving. The brief's *State* and `kontext status` list decisions whose paths have seen `[freshness] threshold_commits` (default 20) commits since they were made. See [Freshness](../concepts/04-context-layers.md#freshness).

**Shorter answers.** A decision line no longer repeats its title as an id: references are the shortest unique prefix (`[2026-09-28-adapters]`), which makes the line about 20% shorter. `ctx_threads` turns each Claude Code background-task notification into one line. A thread that fanned out work went from 81,000 to 30,000 characters, and a part (20,000 characters by default) now fits one tool result.

**Fresh clones.** A clone made by an orchestrator or by CI has no git hooks. The brief says so, and `ctx_prepare_commit` no longer promises trailers that no hook will add.

**Codex** follows `AGENTS.md`, not the MCP server's instructions. `kontext connect agents-md --write` belongs to its setup.

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
