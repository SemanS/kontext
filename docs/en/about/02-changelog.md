# Changelog

All notable changes are listed here. The project follows [Semantic Versioning](https://semver.org/); until 1.0 minor versions may change configuration or the entry format, always with a migration note.

## Unreleased

**Additions amend entries in place, new entries link what they build on, and changes between commits are listed.**

- **`drafts.extend`** drafts an entry with a finding added to its body: same id, file and fields, the old body beside it, nothing written. With `llm` the model folds the finding in; otherwise it is added as written under `## Added <date>` (`as_written: true`). Conventions, learnings and incidents only: a decision is refused with `decision_not_amended`, since it changes only by supersession. A moved revision is `stale_entry`. See [Adding to an entry](../reference/05-application-api.md#extend).
- **`drafts.stage`** takes `amends: {id, revision}` to write that body into the entry's own file, with the front matter kept byte for byte (exclusive with `supersedes`), and `entry.extends: [ids]` to link a new entry to the entries it builds on (`unknown_entry` for an id the store lacks). See [Amending and linking](../reference/05-application-api.md#amend).
- **`entries.changed {base, head}`** lists the committed entries added, revised, superseded (with `superseded_by`) or deprecated between two commits, without a model. See [Changed entries](../reference/05-application-api.md#changed).
- `entries.related` candidates carry their `revision`, the value the two operations above take. `status.capabilities` lists `drafts.extend`, `entries.changed`, `drafts.stage.amends` and `drafts.stage.extends`. API version stays 1.

## 0.8.0 (2026-10-28)

**Skills for knowledge maintenance, and flows in the brief.**

- **Skills.** `kontext connect skills --write` installs four `SKILL.md` recipes into `.claude/skills/` (project scope, to commit): `kontext-audit` (a budgeted look at the entries most likely stale: confirm, challenge or supersede, never delete), `kontext-flows` (reorganise the store into flows), `kontext-verify` (give entries a `verify` command) and `kontext-merge` (find duplicates and conflicts with `entries.related`, draft one superseding entry per group). They are compiled into the binary; re-running the command after an upgrade brings the files back to the shipped text. See [Claude Code](../agent-integrations/02-claude-code.md#skills).
- **Flows.** An `architecture` entry with id `flow-<name>` (or tagged `flow`) is a curated trail through the knowledge along one process: a `## Trail` of `[[id]] — sentence` steps. A focused `ctx_brief` lists the flows whose `paths` overlap the focus, with their step count, under *Flows through the focus*; an unfocused brief says in one line how many flows exist. See [Flows](../concepts/02-knowledge-store.md#flows).

## 0.7.0 (2026-10-27)

**A capture names the command that proved it, and a verification says where it landed.**

- **Capture with its proof.** `kontext capture --verify "<command>"`, `verify` in `ctx_capture` and in the `entry` of `drafts.capture` record the command that proved a learning or decision; it is validated like the entry field (one line, at most 500 characters) and kept through the inbox and promotion. The capture guidance (the tool description, the server instructions and the `AGENTS.md` block) says so. See [Capturing](../concepts/03-capture-and-review.md#capturing).
- **A draft takes it later.** `kontext inbox verify <id> "<command>"` (`--clear`), `ctx_inbox` with `action=verify` and the new operation `inbox.verify` set or clear the command of an inbox draft.
- **Where it landed.** `verification.landed` is the commit in `HEAD`'s history that first added the entry's newest ledger line, or null while it is only staged or in the working tree; for a `checked:` date alone, the last commit that changed the entry's file. `since` is unchanged. API version stays 1. See [the application API](../reference/05-application-api.md#verification).

## 0.6.0 (2026-10-27)

**A verification is a commit in a ledger, and the brief says what changed since.** A `checked:` date was a stamp anyone could set, often in bulk, and a merge could silently replace it with an older one. Now:

- **Ledger.** `kontext confirm`, `ctx_confirm` and `drafts.confirm` also append a line to `.ai/confirmations.log`: the UTC time, the entry, how it was looked at (`read` from the CLI, `agent` from the MCP tool, `bulk` for more than five entries in one call), the commit `HEAD` named, who, and an optional reason (`--because`). `kontext init` and the first write add `confirmations.log merge=union` to `.ai/.gitattributes`, so branches merge without a conflict. `checked:` is still written as a cache. See [Verifications](../concepts/03-capture-and-review.md#verifications).
- **Repair.** `kontext check` warns when an entry's `checked` is behind its ledger; `kontext confirm --repair` (`drafts.confirm` with `repair: true`) rewrites it from the ledger.
- **Executable checks.** An entry may carry `verify:`, one shell command. `kontext verify <id>`, `ctx_verify` and the new `drafts.verify` run it from the repository root with a timeout (`KONTEXT_VERIFY_TIMEOUT_SECS`, default 120) and record pass or fail in the ledger.
- **What changed since.** For each entry governing the focus paths, `ctx_brief` says how it was last verified (kind, short commit, day) and how many commits touched its paths since, with their first subjects; once per brief it asks to read those before changing their paths.
- **API.** Entries from `entries.list`, `entries.get`, `entries.search`, `entries.governing` and `entries.drift` carry `verification`, `since`, `verify` and `premises`; `checked` is the newer of the file and the ledger. `drafts.confirm` takes `because`, `kind`, `ids` and `repair`. API version stays 1. See [the application API](../reference/05-application-api.md#verification).
- **Changed premises.** An active entry that links to a superseded, retired or deprecated one (`[[id]]`, `supersedes`, `extends`) is flagged in the brief, the API and as an info line of `kontext check`; nothing is rewritten.
- **Gate.** `kontext hooks status` and the docs point teams to `kontext drift --fail` in their check script or CI, which also covers jj commits; no hook blocks a commit for drift.

**Migration.** None needed. The first confirmation creates `.ai/confirmations.log` and adds a line to `.ai/.gitattributes`; commit both. `kontext check` counts info lines as neither errors nor warnings.

## 0.5.0 (2026-10-26)

**Decisions can say what was rejected.** A decision entry may carry an optional `## Alternatives` section between `## Decision` and `## Consequences`: the options considered and why each was rejected, so a later challenge or drift does not reopen one that already failed. It is free Markdown, not front matter, and nothing requires it. See [the entry format](../concepts/02-knowledge-store.md).

- `ctx_init_submit` takes an optional `alternatives` string for `dec:` tasks and writes the section; an empty value writes nothing.
- The `ctx_capture` body hint and the distill prompt mention the section, so agents fill it when there were real alternatives.
- The one-line summary and the action line still come from `## Decision`, so the brief and search read entries as before.

**Migration.** None: entries without the section stay valid, and older kontext versions read the section as ordinary body text.

## 0.4.1 (2026-10-20)

**Fix: confirming an entry again the same day cleared nothing.** `kontext confirm` wrote only a date, so a second confirmation on the same day left the file byte-identical, and drift, which counts an entry as looked at only when its file changes in the range, kept listing it after later commits. Confirm (`kontext confirm`, `ctx_confirm`, `drafts.confirm`) now writes the UTC time to the second, `checked: 2026-10-20T14:05:09Z`, and returns the same string. `checked` accepts that form or a bare `YYYY-MM-DD` (both validated, neither in the future); freshness counts after a time's second and from the next UTC day after a date. Nothing needs to change: dates already written stay valid. See [Confirmations](../concepts/02-knowledge-store.md#checked).

## 0.4.0 (2026-10-20)

**Knowledge that follows the code it governs.** A review of one repository found about ten entries that no longer matched the code. kontext already listed the entries covering a staged change, but only from git hooks, which a team committing with jj never runs; freshness never reset once someone checked an entry, and watched decisions only; and nothing answered which entries a range of commits left behind. Now, without a model and the same under git and jj:

- **Confirmations.** An entry can carry `checked: YYYY-MM-DD`, the last day someone confirmed it still holds. `kontext confirm <id>` / `ctx_confirm` / `drafts.confirm` set it to today in the entry's own file (a one-line diff) and stage it, so the confirmation is reviewed with the change. See [Confirmations](../concepts/02-knowledge-store.md#checked).
- **Drift.** `kontext drift` / `ctx_drift` / `entries.drift` list the active entries whose `paths` cover files changed in `base..head` while their own file was not edited, retired or confirmed in the range. The base defaults to where the branch left the remote's default branch; without a head the working tree counts too, which is what a jj working copy is. See [Drift](../concepts/03-capture-and-review.md#drift).
- **CI.** `kontext ci` checks every entry and lists the drift of the pull request's commits, as text or as Markdown for a job summary; drift is a warning unless `--strict`. A ready-to-copy GitHub Actions workflow is in [Validate knowledge in CI](../guides/07-ci.md).
- **Freshness** watches conventions and learnings as well as decisions, and counts commits since the latest of the entry's date, its confirmation and the last commit that changed its file. See [Freshness](../concepts/04-context-layers.md#freshness).
- **prepare-commit without a staged change** says that it read the working tree's changes against `HEAD`, and tells how to confirm an entry that still holds. With jj, run it (or `kontext drift`) before `jj commit` / `jj git push`: see [jj](../concepts/07-git-integration.md#jj).
- **Application API.** New operations `entries.drift` and `drafts.confirm` (listed in `status.capabilities`); read entries and the embedding facade's `CommittedEntry` carry `checked`.

**Migration.** `checked` is a new optional field; nothing in `.ai/` needs to change. Things you may notice:

- `kontext status` and the brief may flag conventions and learnings now, and say "N entr(y|ies) may need a refresh … since they were made, edited or confirmed" instead of "N decision(s) …". Counts are often lower: the commit that last changed an entry's own file (usually the one that added it) and the commits before it no longer count.
- `checked` is an entry field: an existing `checked:` key moves from `extra` to its own field in entry JSON and in `CommittedEntry`, `kontext check` rejects a `checked` that is not a date or lies in the future, and `drafts.*` refuse it as an `extra` key.
- The `prepare-commit` report's working-tree line reads "Nothing staged yet — read the working tree's changes against HEAD (untracked files included): …".

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
