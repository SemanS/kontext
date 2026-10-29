# Changelog

All notable changes are listed here. The project follows [Semantic Versioning](https://semver.org/); until 1.0 minor versions may change configuration or the entry format, always with a migration note.

## 0.10.1 (2026-10-29)

**Fix: a person's unstaged edit of an entry is a conflict, not a stale revision.** `drafts.extend`, `drafts.stage` with `amends`, `drafts.retire` and `drafts.revise` checked the revision first, so an entry file with uncommitted edits in the checkout answered `stale_entry` ("read it again") instead of `operation_conflict` ("stage or revert your edits"), which 0.9.0 said. Now an unstaged or untracked change to the entry's file is reported first, as `operation_conflict` with "unstaged" in the message; a revision that moved in a commit or in what kontext itself staged is still `stale_entry`. Nothing else changes.

## 0.10.0 (2026-10-29)

**Promises hold again: trust-pinned verify commands, honest verification, a brief that keeps every governing entry, a fresh index, additive amendments, and a 2026 MCP surface.**

### Security

- **`verify:` runs only after a pin.** A command in an entry's `verify:` field is code from the repository. It runs only when `trust.toml` holds a SHA-256 pin of the entry path and the command text (`[repos.<slug>.verify]`). Without one nothing runs: `kontext verify` and `drafts.verify` fail with `untrusted_verify`, whose error carries `id`, `command`, `command_sha256` and `hint` (`kontext trust --verify <id>`). Editing the command drops the pin. Capability `drafts.verify.pinned`.
- **`kontext trust --verify [<id>|--all]`** pins verify commands from an interactive terminal only and prints what it pins. No MCP tool and no `ctx_api` operation pins.
- **Over MCP an unpinned command is returned, not run**, and so is a pinned one while the entry's file or governed paths have uncommitted changes (`ran: false`, `reason: untrusted_verify | dirty_tree`). The agent runs it in its own sandbox; the result is recorded as an `agent` stamp, not as `verify`.
- **The runner** passes an environment allowlist (`PATH`, `HOME`, `USER`, `LANG`, `LC_*`, `TMPDIR`, `TERM`, plus `[verify] pass_env` from your own config only), redacts `output_tail`, names uncommitted governed paths in the ledger line (`[dirty: <paths>]`, and `dirty`, `dirty_paths` in the result), stops reading stdout 2 s after the shell exits and kills the process group. The MCP server kills a running check when stdin closes or the request is cancelled.
- **`ctx_verify`** is annotated `destructiveHint: true`, `openWorldHint: true`. The `dir` of `ctx_verify`, `ctx_inbox` and `ctx_prepare_commit` is confined to this repository, its worktrees and submodules.
- **Adapter trust pins are SHA-256** over the canonical TOML of the shared `adapters`, `vars` and `project.name`. A legacy 12-hex FNV pin is refused with a message, never upgraded silently. Shared `[vars]` never override the vars of a user or local adapter; a shared `project.name` must be a slug; `{{repo.remote}}` carries no userinfo. `kontext trust` runs from a terminal only.
- `ctx_threads` takes only `claude:`, `codex:`, `superset:`, `last` and paths under known transcript roots, and refuses `-` over MCP. `ctx_read file:` refuses `.git/`, `.git-credentials` and `.ssh`. `ctx_prepare_commit drop` checks every id first and refuses a candidate another worktree wrote.

### What the agent is told

- **A focused brief keeps every governing entry.** A reserved *Governs &lt;focus&gt; (N)* section comes first: one compact line per entry (full id, title, marks, commits since), ordered by path specificity (file, directory, glob, `**`), then oldest first. What does not fit is named on one always-printed `also governing:` line; below the section's size only ids are printed. Other kinds trim line by line and end with `… N more (K govern your focus)`. The footer counts against the budget.
- **Paths stay paths.** A focus item with `/`, a glob character or a known extension is a path even before the file exists. Overlap works in both directions, including leading wildcards and a leading slash. `[slug]` matches literally first, then as a glob.
- **Honest currency lines.** The brief says `read by <who>`, `stamped by agent`, `bulk stamp`, `check passed` or `CHECK FAILING`; "verified" appears only where the verification holds. A brief line uses `rule_text` when the summary lacks the rule.
- **The MCP server sees other processes.** The index reloads when another process (a CLI capture, `kontext status`, a second session) refreshed it; a new schema is built in a temporary directory and renamed. The branch is read per request; a config parse error comes back as a warning.
- **Visible from the first turn.** `ctx_brief` carries `_meta["anthropic/alwaysLoad"]`; the first 512 characters of the server instructions stand alone and the whole stays under 4 096 characters.

### Supersession and parsing

- **Supersession follows active successors.** An entry named in the `supersedes` of an active entry is superseded, and `superseded_by` is filled in. The adr-tools `## Status` section is read and written in its own style. References are written as full ids; an ambiguous target is refused. `kontext check` reports a one-sided supersede and `kontext repair [--dry-run]` writes the missing side.
- **Entries parse the way people and formatters write them:** multi-line flow lists, block lists, trailing comments, a BOM. An unparsable entry is a `check` error. `retired`, `archived` or emoji statuses read as `deprecated` with a warning; non-Latin titles get a short hash id.
- **Amendments only add.** `drafts.stage amends` accepts a body only when the old body is its byte prefix and the rest are `## Added <YYYY-MM-DD>` sections; otherwise `amend_not_additive`, pointing to supersede. The title and every field outside the body must stay the same. `drafts.extend` with `llm` no longer folds: it adds the finding as written (`as_written: true`). `extends` passes through `ctx_capture`, the inbox and promotion.
- **Challenges stay resolving** until the entry's new blob lands in `HEAD`.
- **`since` counts by ancestry.** `since` and freshness count non-merge commits on governed paths reachable from `HEAD` and not from the commit of the look, so a branch merged after a confirmation is counted. A failed check is shown but is not a look.
- **What counts as verified:** a read by a person or a passed check. A `checked:` date without a ledger line is kind `cache` and reports `verified: false`; `kontext confirm --migrate-legacy` writes bulk lines for such entries. The CLI writes `read` only on a terminal (otherwise `agent`; `--kind read` forces it with a warning); MCP never writes `read`; `kontext api` records `by: application:<name>`.
- **Drift needs a baseline** and a look at or after the last governed change in the range; without a baseline it says why, and `--fail` exits 3.

### Redaction and outbox

- **Redaction masks what it promises and fails closed:** whole PEM blocks, national phone numbers, IPv6 and every match on a line. An invalid `secrets.redact` pattern is reported by `status`, and distill, threads and autopilot refuse to send anything to a model. Your own `redact` and `allow` lists are joined with the repository's, not replaced. Adapter errors drop query strings and userinfo. Bash-mode output and compaction summaries of Claude threads are dropped.
- **The outbox loses nothing silently.** An attempt counts only for the adapter that was tried; after repeated failures events move to `outbox.dead.jsonl`, which `status` shows, and `kontext outbox retry` requeues them. Events for an adapter skipped by `when` stay queued.

### Init

- `kontext init skip mod:<module>` takes effect. `dec:` clusters whose commits an entry or a `Decision:` trailer already cites are not planned again; `dec:` task state is derived from git and from `<store>/init-covered.log` (committed, union-merged), so a colleague and a fresh clone agree. `dec:` ids are `dec:<module>-<sha7>` of the cluster's oldest commit.
- Re-rendering a module doc keeps hand-edited `paths`, `tags` and `summary`. Module choice and cluster ids are deterministic.
- Bot authors are left out of scoring; Slovak and Czech decision words are bundled, `[init] decision_words = { strong, rationale, contrast }` adds stems in your language and `[init] bot_authors` names more bots. Only an explicit `--no-hooks` records a hooks opt-out. `init.jobs` is honoured.

### Search and eval

- **Shadow fields.** Folded and English-stemmed `title_f` and `body_f` are searched at a lower boost, so `namietka` finds `námietka` and `adapter` finds "Adapters". The index schema version is bumped; the index rebuilds on first use.
- **Reviewed knowledge first.** Local sources are weighed before they are fused (entries 1.0, doc chunks 0.7, commits 0.6; built in, `[search]` keys are planned); superseded hits are weighed down and read `→ superseded by [id] Title`, never hidden; copies are found by a digest of title and text; deduplication never keeps a superseded twin over its active successor; adapter hits sort by mapped score.
- **Graded eval.** `tests/eval/qrels.tsv` with nDCG@5, recall@10 and MRR per slice (identifiers, plurals, Slovak with and without diacritics, paraphrase, focused briefs from 200 to 1 400 tokens). Expectations are written by a different session than the one that implements the change.

### API v1 policy

- **The compatibility policy is written down.** Within version 1, operations, optional inputs, fields, error codes, enum values and capability strings may be added, and clients ignore what they do not know. Narrowing an input is allowed only where the old behaviour contradicted a recorded decision or a security promise, and is announced with a capability string and a migration note. See [Compatibility](../reference/05-application-api.md#compatibility).
- **`api.describe`** returns the JSON Schema of every operation's input.
- **One notion of revision:** the blob id. A moved revision is `stale_entry` everywhere, with the old `operation_conflict` in `error.aliases`; `model_unavailable` lists `llm_unavailable`. The guard takes a blob id, a full commit hash or an older 12-hex revision; `HEAD` or a branch name is `invalid_input`.
- **Verification fields** `last_verified`, `since_verified`, `failing` and `amended_since` sit beside the existing ones. `entries.changed` carries `status` and reads each revision's committed config; committed entries carry `date`, `verify`, `review_by`, `supersedes`, `commits` and `author`. A replay returns the stored receipt even after a later edit; `operations.get` finds routed receipts.
- **New error codes:** `untrusted_verify`, `amend_not_additive`; `stale_entry` everywhere.
- **`drift.ordered`:** `entries.drift` carries `base_source` (`explicit`, `origin-head`, `origin-main`, `upstream`, `default-remote`, `fallback-none`) and, without a base, `warning`; `kontext drift --fail` and `kontext ci --strict` exit 3 then.
- **`drafts.confirm` takes `operation_id`**, so a retry does not append a second ledger line; `drafts.capture` keeps `extends`.
- **New capability strings:** `api.describe`, `drafts.stage.amends.additive`, `drafts.capture.extends`, `drafts.confirm.operation_id`, `drafts.verify.pinned`, `verification.last_verified`, `drift.ordered`.

### MCP, hooks, structured results, skills and plugins

- **Dual-era MCP.** Capabilities no longer include `logging`. The server answers the stateless 2026-07-28 revision (`server/discover`, `_meta` on requests without `initialize`, `resultType`, `ttlMs`/`cacheScope`) and legacy `initialize`, which falls back to 2025-11-25. An unknown version is `-32022`, an unknown tool `-32602`; `resources/list` pages with a cursor; `tools.listChanged` may be true: it is when adapters expose tools.
- **Structured results.** `ctx_brief`, `ctx_search`, `ctx_read`, `ctx_why`, `ctx_inbox` and `ctx_drift` return `structuredContent` with an `outputSchema` beside the text, and `resource_link` blocks to `kontext://entry/{id}`. `kontext brief --format json` returns the same structure.
- **Hooks.** `kontext hook --host claude|codex|cursor` reads the host's hook JSON on stdin and answers in its shape. SessionStart (startup, resume, clear, compact) and SubagentStart get the brief; PreToolUse on `Edit|Write|MultiEdit` (Codex: `apply_patch|Edit|Write`; Cursor: `postToolUse`) gets the governing entries for the edited files, only those not yet told in the session. One budget per session, shared with `ctx_brief` when it knows the session (`session_id`, `CLAUDE_SESSION_ID`): `[hooks] session_budget_tokens` (6000), `edit_budget_tokens` (900). Fail-open; kill switch `KONTEXT_HOOKS=off` or `[hooks] enabled = false`. Installed only by `kontext connect <host> --hooks`; any `--write` also installs a shell guard that denies an agent's `kontext trust`. See [Hooks](../guides/09-hooks.md).
- **Honest approvals.** Exposed adapter tools keep their upstream annotations; missing ones default to `readOnlyHint: false`, `openWorldHint: true`. `kontext connect codex --write` writes `default_tools_approval_mode = "prompt"` and a `[mcp_servers.kontext.tools.<tool>]` table per tool: `approval_mode = "approve"` for the read and inbox tools, `"prompt"` for `ctx_verify`, `ctx_api` and `ctx_init`.
- **Skills and plugins.** Skills go to `.claude/skills` and `.agents/skills`; a `kontext-skill-hash` marker keeps team edits, and `connect skills --write --force` replaces them. A Claude Code plugin (`.claude-plugin/plugin.json`) and a Codex plugin are built from the same sources as `connect`. Parameters gained aliases (`id`/`uri`, `focus`/`paths`, `kind`/`kinds` (also a string), `limit`/`max`, `budget_tokens`, `reason`/`why`/`because`); nothing was renamed. `ctx_read` pages a long text (`part`, `offset`).
- **A sturdier server.** A panic in one request answers `-32603` and the server keeps running; the HTTP MCP client re-initialises after a 404; outside a git repository the server says so and `dir` still works.
- **New commands and flags:** `kontext repair [--dry-run]`, `kontext trust [--yes]` and `--verify [<id>|--all]`, `kontext confirm --kind read|agent` and `--migrate-legacy`, `kontext distill --force`, `kontext outbox retry`, `kontext hook --host`, `kontext connect <target> [--write] [--hooks] [--force]`.

**Migration.**

- Rust 1.90 is the minimum (`rust-version` in `Cargo.toml`).
- Legacy FNV adapter pins are refused: run `kontext trust` again in every repository with shared adapters.
- Verify commands do not run until pinned: `kontext trust --verify <id>` or `--all`, in a terminal.
- Peer: `drift.ts` must map `untrusted_verify` to "not verified, show the command".
- An entry whose only evidence is a `checked:` date now reports `verified: false` (kind `cache`); `kontext confirm --migrate-legacy` records bulk lines for them.
- `since` counts may grow: commits merged after a look are now counted.
- `drafts.extend` never folds a finding into the body: with or without `llm` it is added as written and `as_written` is always true.
- `entries.export` without a committed configuration reports `config_source.kind: "defaults"` (store `.ai`), not `local_configuration`.
- A challenge stays listed as *resolving* (`resolving` in `inbox.list`) until the entry's new blob lands in `HEAD`, not when `prepare_commit` stages the change.
- API fields are additive and the API version stays 1. A client that passed `HEAD` as a revision gets `invalid_input`, a non-additive `amends` body `amend_not_additive`.
- The search index rebuilds once (schema version bump).

## 0.9.0 (2026-10-29)

**Additions amend entries in place, new entries link what they build on, and changes between commits are listed.**

- **`drafts.extend`** drafts an entry with a finding added to its body: same id, file and fields, the old body beside it, nothing written. With `llm` the model folds the finding in; otherwise it is added as written under `## Added <date>` (`as_written: true`). Conventions, learnings and incidents only: a decision is refused with `decision_not_amended`, since it changes only by supersession. A moved revision is `stale_entry`. See [Adding to an entry](../reference/05-application-api.md#extend).
- **`drafts.stage`** takes `amends: {id, revision}` to write that body into the entry's own file, with the front matter kept byte for byte (exclusive with `supersedes`), and `entry.extends: [ids]` to link a new entry to the entries it builds on (`unknown_entry` for an id the store lacks). See [Amending and linking](../reference/05-application-api.md#amend).
- **`entries.changed {base, head}`** lists the committed entries added, revised, superseded (with `superseded_by`) or deprecated between two commits, without a model. See [Changed entries](../reference/05-application-api.md#changed).
- `entries.related` candidates carry their `revision`, the value the two operations above take. `status.capabilities` lists `drafts.extend`, `entries.changed`, `drafts.stage.amends` and `drafts.stage.extends`. API version stays 1.

**Known issues** (fixed in 0.10.0): a `verify:` command runs through `sh` without a trust gate, so do not call `ctx_verify`, `drafts.verify` or `kontext verify` in a repository you do not trust; `drafts.extend` with `llm` and `amends` with an arbitrary body rewrite an entry in place, against the 2026-10-27 decision.

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
