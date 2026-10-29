# Application API

Applications can use kontext as their knowledge engine. They supply source text, external identities
and repository paths; kontext handles the configured store, Markdown/ADR formats, queries, path
matching, extraction and Git staging. The application keeps its UI, authentication, shared queues
and agent-session delivery.

## Transports and compatibility

```sh
kontext -C /path/to/repository api status
kontext api entries.list '{"limit":20,"offset":0}'
kontext api drafts.stage - < request.json
```

The last argument is a JSON object, or `-` to read one object from stdin (at most 2 MiB). It defaults
to `{}`. Each invocation writes one complete JSON envelope to stdout:

```json
{"schema_version":1,"ok":true,"result":{"entries":[],"total":0}}
```

Failures use `{"schema_version":1,"ok":false,"error":{"code":"invalid_input","message":"…"}}`
and a nonzero exit status. Clients branch on `error.code`, not message wording. Unknown operations
return `unsupported_operation`; missing records return `not_found`. Other failures have their own
code or `operation_failed`. Unknown response fields must be tolerated.

The MCP tool `ctx_api` accepts `{operation,input?,dir?}` and invokes the same application service.
Its result has `structuredContent` containing the envelope, an `outputSchema`, a complete text JSON
copy for older clients, and `isError: true` for failures. Machine results are never clipped or
prefixed with human repository banners. Existing human CLI commands and agent tools remain available.

Call `status` to check `api_version: 1`, supported `capabilities` and the actual LLM selection.
An installed kontext binary alone does not establish that an LLM adapter is configured. Applications
should provide or locate a compatible binary and retain their last successful snapshot with an
explicit stale state when it cannot be refreshed.

### Compatibility policy {#compatibility}

Within `api_version: 1`:

- **May be added:** operations, optional inputs, response fields, error codes, enum values and
  capability strings. Clients ignore what they do not know.
- **Bumps the version:** removing or renaming an operation, input or field, or changing a field's
  type or meaning.
- **Narrowing an input** is allowed only where the old behaviour contradicted a recorded decision or a
  security promise. It is announced with a capability string and a migration note in the changelog.
  The narrowings so far: `drafts.stage amends` accepts only additive bodies (`amend_not_additive`,
  capability `drafts.stage.amends.additive`); a revision guard refuses `HEAD` and other moving names
  (`stale_entry`); `drafts.verify` runs only pinned commands (`untrusted_verify`, capability
  `verify.trust`).

Parameters gain aliases, never renames. `api.describe {}` returns the JSON Schema of every
operation's input, `{"operations": {"<name>": {"input": {…}}}}`, so a client can check what an installed
binary accepts. Capability strings to look for in `status.capabilities` besides the operation names:
`api.describe`, `verify.trust`, `drafts.stage.amends`, `drafts.stage.amends.additive`,
`drafts.stage.extends`, `verification.last_verified`, `drift.ordered`.

**One notion of revision.** A revision is a blob id: `revision` from `entries.related` and
`entries.changed`, `entry_revision` in `drafts.extend`, `drafts.revise` and `retire`, and
`amends.revision` all take the entry file's blob id or a full commit hash whose file content equals
the current one. A moved revision is `stale_entry` everywhere; the older `operation_conflict` for a
moved revision stays as an alias in `error.details.alias`.


## Operations

| Operation | Input | Result |
| --- | --- | --- |
| `status` | `{}` | version, root, revision, configured store, capabilities and LLM availability |
| `setup` | `{}` | initialize local knowledge without installing hooks implicitly |
| `entries.list` | `limit?`, `offset?`, `active_only?`, `kinds?`, `tags?` | entries, total, next offset and snapshot |
| `entries.get` | `id` (full/unique prefix, ADR number or entry path) | one entry |
| `entries.search` | `query`, `limit?`, `active_only?` | matching knowledge entries and total |
| `entries.governing` | `paths`, `exclude_ids?`, `limit?`, `min_specificity?` | matching entries with pattern, path and specificity, plus snapshot |
| `entries.export` | `revision`, `review_ref`, `paths?` | committed entries with exact Markdown, revision and Git blob hashes |
| `entries.related` | `text`, `paths?`, `exclude_ids?`, `llm?` | up to five related entries with reasons; with `llm`, the model's relations ([details](#related)) |
| `entries.drift` | `base?`, `head?`, `kinds?` | entries whose governed files changed in the range without a look at the entry ([details](#drift)) |
| `entries.changed` | `base`, `head`, `kinds?` | committed entries added, revised, superseded or deprecated between two commits ([details](#changed)) |
| `inbox.list` | `state?`: `pending` (default), `dismissed`, `promoted`, `all` | local candidates and retained workflow history |
| `inbox.drop` | `id`, `reason?` | dismiss a local candidate |
| `inbox.verify` | `id`, `verify` (`""` clears it) | `id`, `verify` and `entry`: the draft with the command that proved it; promotion keeps it ([details](#inbox-verify)) |
| `drafts.capture` | `entry`, `source?`, `operation_id?` | inbox candidate, notes, redaction count, `evidence` and `related_entries` |
| `drafts.stage` | `entry` or `candidate_id`, `operation_id`, `source?`, `requested_path?`, `amends?` ([details](#amend)) | entry, path, staging result, notes, replay flag, `evidence`, `related_entries` and `resolved_challenges` |
| `drafts.retire` | `operation_id`, `entry_id`, `entry_revision`, `reason` | the entry staged with status `deprecated` and a `## Retired` note ([details](#retire)) |
| `drafts.confirm` | `id` or `ids`, `because?`, `kind?`; or `repair: true` | `id`, `path`, `checked` and `ledger`: the entry confirmed now, its ledger line appended, both staged ([details](#confirm)) |
| `drafts.verify` | `id`, `because?` | the outcome of the entry's `verify` command, recorded in the ledger and staged ([details](#verify)) |
| `drafts.revise` | `entry_id`, `entry_revision`, `objections`, `llm?` | a draft that supersedes the entry; no writes ([details](#revise)) |
| `drafts.extend` | `entry_id`, `entry_revision`, `text`, `llm?` | the entry with a finding added to its body, same id and file; no writes ([details](#extend)) |
| `drafts.extract` | `text`, `source?`, `llm?`, `max?` | extracted candidates and notes; no capture or staging |
| `drafts.formalize` | `entry`, `sources?`, `context?`, `llm?` | formulated entry, notes and `related_entries`; no capture or staging |
| `guidance.propose` | `kept`, `dismissed`, `missed`, `current?`, `profile_tag?`, `llm?` | proposed convention; no staging |
| `operations.get` | `operation_id`, `fingerprint?` | the original completed write result, or an explicit incomplete/conflict error |

The `entry` object includes `kind`, `title`, `body`, and optional `summary`, `paths`, `tags`, `status`,
`supersedes`, `extends`, `visibility`, `commits`, `review_by`, `verify` and `evidence` ([below](#maintenance)). `verify` is the command that
proved the entry: one line of at most 500 characters, else `invalid_input`. Keep paths as arrays throughout the workflow. A selected
finding can be formalized separately from extracting new findings, so deduplication does not discard
something the user explicitly chose to keep. Extraction and guidance generation require a usable LLM;
formalization can preserve the supplied finding when no model is selected.

Read entries include `id`, `kind`, `title`, `body`, `summary`, `status`, `date`, `review_by`, `checked`, `verify`, `paths`, `tags`, `file`,
`markdown`, `style`, supersession fields and extra metadata (`extra.external_refs` holds the sources). `entries.list`, `entries.get`,
`entries.search`, `entries.governing` and `entries.drift` add `verification`, `since` and `premises` ([below](#verification)). The engine owns parsing and matching;
clients must not parse Markdown or implement a second matcher. `active_only` excludes superseded,
deprecated and rejected entries. It does not imply that a working-tree proposal passed review.
List queries default to 100 entries (maximum 500); follow `next_offset` until null. Snapshots change
when entry content changes, including edits that keep the number of entries unchanged.

## Knowledge maintenance {#maintenance}

Added in 0.3.0. `status.capabilities` lists `entries.related`, `drafts.retire` and `drafts.revise`
when a binary supports them; check it before calling them.

**Review date.** `entry.review_by` (`YYYY-MM-DD`) says when someone should look at the entry again,
for example a rule that holds during a migration. It is not an expiry: past the date the entry is
marked (`review due` in search, the brief and `kontext status`), never hidden. A malformed date or one
before the entry's `date` is `invalid_input`. A kind can default it with
[`review_after`](./03-configuration.md#store-kinds); without that, entries have none.

**Evidence.** `entry.evidence` is a list of `{ref, quote?}` (at most 50). The reference shapes are
`pr:<id>`, `issue:<id>`, `task:<id>`, `code:<repo>@<rev>:<path>#L<a>-<b>` (the line range is optional)
and `url:<http(s) URL>`; anything else is `invalid_input`. The references are stored in
`extra.external_refs`, after any references the client passes there itself. A `quote` (at most 200
characters) of a `code:` reference into this repository is checked with `git show <rev>:<path>`;
every result lists the references as `evidence: [{ref, kind, quote?, verification}]` with
`verification` one of `verified`, `quote_not_found`, `unverifiable` (another repository, an unknown
revision, or a reference that is not code) or `none` (no quote). Nothing is refused for being
unverifiable: quoted references and their verdicts go into a `## Evidence` section of the body, where
the reviewer reads them. Keep personal data out of references: e-mail addresses and session ids do
not belong in Git.

### Related entries {#related}

`entries.related {text, paths?, exclude_ids?, llm?}` answers how a text relates to recorded entries.
Without a model it returns `candidates`: at most five active entries that read alike (local BM25,
near the best hit) or govern overlapping `paths`, each `{id, title, kind, status?, reason, label, revision}`
with `label: "related, check"`. `revision` is a short hash of the entry's Markdown: pass it back as
`entry_revision` of `drafts.extend` or `amends.revision` of `drafts.stage`. With `llm: true` (or an adapter name) one model question names the
relation and the entry together; `relations` is a list of `{relation, id?, title?, label}` where
`relation` is `new`, `same`, `extends`, `conflicts` or `uncertain` and several may apply. An answer
about an entry the model was not shown is dropped; with nothing left the answer is `uncertain`.
The labels read `possible conflict, check`, `possibly the same, check`, `extends it, check` — a
person decides; nothing suggests or prefills `supersedes`. `model: {asked, cached}` tells whether a
model was called. The answer is cached in the worktree's local state by the hash of the text and of
the compared entries' content, so `drafts.capture`, `drafts.stage` and `drafts.formalize`, which
return the same object as `related_entries`, reuse a cached answer and never call a model themselves.
Without a configured model, `llm: true` fails with `model_unavailable`.

### Retiring an entry {#retire}

```json
{"operation_id":"retire-42","entry_id":"2026-01-10-use-cron","entry_revision":"<publication.revision>","reason":"Exports run on the queue since September."}
```

`drafts.retire` changes the entry's own file: status `deprecated` and a `## Retired` note with the
date and the reason, then stages it. Nothing else is rewritten; the old claim stays readable in Git.
`entry_revision` is the commit the caller read the entry at (`publication.revision` of `entries.get`)
or the entry's blob (`publication.blob_hash`); a later commit that did not touch the file does not
make it stale. The operation fails with `operation_conflict` when the file at that revision differs from HEAD, when
the file has local or staged changes, or when the entry is already inactive. It needs an
`operation_id`, keeps a receipt and replays like `drafts.stage` (`replayed: true`,
`operations.get`). Pending challenges of this worktree against the entry are resolved by it and
listed in `resolved_challenges`. Retirement and supersession work for every kind, not only decisions.

A challenge is how an agent says an entry no longer holds without changing it: `ctx_challenge`
(or `kontext challenge`) writes an inbox candidate of kind `challenge` with `extra.against` (the entry
id), `extra.against_revision` (the commit) and `extra.against_blob`. `inbox.list` returns it; it cannot
be staged as an entry. A retirement or a staged replacement that supersedes the entry resolves it.

### Drift {#drift}

Added in 0.4.0 (`status.capabilities` lists `entries.drift`).

```json
{"base": "origin/main", "head": "HEAD", "kinds": ["decision", "convention"]}
```

All fields are optional. `base` is a git revision; without it, the merge base of `HEAD` with
`origin/HEAD` (else `origin/main`, else `origin/master`), falling back to `HEAD`. `head` is a git
revision; without it the working tree counts: the commits `base..HEAD` plus uncommitted and untracked
changes (a jj working copy is uncommitted in git's terms). `kinds` defaults to
`["decision","convention","learning","incident"]`; architecture is checked only when named. An
unknown revision or one that starts with `-` is `invalid_input`; a repository without commits is
`no_commits`.

```json
{"base": "<full sha>", "head": "<full sha>" | "working", "entries": [
  {"id": "…", "kind": "convention", "title": "…", "path": ".ai/conventions/….md", "date": "2026-01-10", "checked": "2026-03-01",
   "changed": ["src/billing/invoice.rs"], "commits": [{"sha": "<full sha>", "subject": "feat(billing): …"}]}
]}
```

An entry is listed when it is active, has `paths`, at least one file changed in the range (other than
knowledge files) matches them — with the same matcher as `entries.governing` — and the entry's own
file did **not** change in the same range: an edit, a retirement or a confirmation counts as having
looked. `date` and `checked` appear when the entry has them; `verification`, `since`, `verify` and `premises`
are added as for the other reads ([below](#verification)). `commits` are the non-merge commits of
`base..head` that touched the matched files, newest first; it is empty when the change is only
uncommitted. With an explicit `head` the entries are read as committed at `head`. Entries are ordered
by id. The answer involves no model.

Added in 0.10.0 (capability `drift.ordered`): the result carries `base_source` (`input`, `merge_base`
or `none`). With `none` there was no baseline: `entries` is empty and `reason` says why, and
`kontext drift --fail` exits non-zero instead of reporting no drift. An entry counts as looked at only
when a look (an edit, a confirmation or a passed check) is at or after the last commit in the range
that changed its governed files; a look before that change no longer covers it.

### Confirming an entry {#confirm}

`drafts.confirm {id}` records that an entry still holds: it sets `checked` to the current UTC
time to the second (`YYYY-MM-DDTHH:MM:SSZ`) in the entry's own file, changing nothing else byte for
byte, appends a line to the [verification ledger](../concepts/03-capture-and-review.md#verifications)
(`.ai/confirmations.log`) and stages both like `drafts.retire`. The result is `{id, path, checked, ledger}`
with the same string in `checked` and `ledger.at`; its first ten characters are the day, so comparing it with a `YYYY-MM-DD` as a string stays
correct. Confirming again later the same day changes the file, so drift sees the new look. A
`checked` written by hand may also be a bare `YYYY-MM-DD`; read entries return it as written. An
unknown entry is `not_found`; an inactive one, one that is not committed yet, or a file with unstaged
edits is `operation_conflict`. `checked` appears on read entries and on
[`CommittedEntry`](#rust-embedding) in the embedding facade, so an application can recognise a
confirmation in a reviewed revision.

Added after 0.5.0, all optional:

- `because`: one line (at most 500 characters) kept in the ledger.
- `kind`: `read` when a person confirmed it in the client, `agent` (the default) when an agent did.
  More than five ids in one call are recorded as `bulk` whatever `kind` says.
- `ids`: several entries in one call. The result is then `{confirmed: [{id, path, checked, ledger}],
  failed: [{id, code, message}]}`.
- `repair: true` (alone): rewrites the cached `checked` of every active entry whose file is behind its
  newest ledger confirmation, and stages the files. The result is `{repaired: [{id, path, from, to}],
  skipped: [{id, path, reason}]}`.

`ledger` is `{at, kind, commit, because, path}`: the line's time, its kind, `HEAD` (null before the
first commit), the reason (null without one) and the ledger's path.

### Running an entry's check {#verify}

`drafts.verify {id, because?}` (added after 0.5.0) runs the entry's `verify` command with `sh -c` from the
repository root, stdin closed, stopped after `KONTEXT_VERIFY_TIMEOUT_SECS` (default 120), then appends a
`verify` line with the outcome and stages the ledger:

```json
{"id": "…", "command": "cargo test -p billing cents", "passed": true, "exit_code": 0, "timed_out": false,
 "output_tail": "…last 2 KB of stdout and stderr…", "at": "2026-10-27T09:40:02Z", "commit": "<full sha>",
 "path": ".ai/confirmations.log"}
```

`exit_code` is null when the command was stopped. An entry without `verify` is `invalid_input`; an
inactive one is `operation_conflict` and its command never runs; an unknown one is `not_found`. The
command runs without the write lock, so other writes wait only for the append.

The command runs only when its text is pinned in the user's `trust.toml` (`kontext trust --verify <id>`
from a terminal; no operation pins). Otherwise nothing runs and the error is `untrusted_verify` with
`details: {command, hash, pin: "kontext trust --verify <id>"}`; show the command to a person, do not
retry. The command gets an environment allowlist (`PATH`, `HOME`, `USER`, `LANG`, `LC_*`, `TMPDIR`,
`TERM` and the user's `[verify] pass_env`), its process group is killed at the timeout or 2 s after
the shell exits with stdout still open, `output_tail` is redacted, and the ledger line carries
`dirty: true` when the working tree had changes. See [Security](../concepts/08-security.md#verify).

### Verification, what changed since, premises {#verification}

Added after 0.5.0. Every entry returned by `entries.list`, `entries.get`, `entries.search`,
`entries.governing` and `entries.drift` carries:

```json
{"checked": "2026-10-27T09:12:44Z",
 "verify": "cargo test -p billing cents",
 "verification": {"at": "2026-10-27T09:12:44Z", "kind": "read", "commit": "<full sha>", "by": "Jane Doe",
                  "because": "checked against the VAT change", "passed": null, "source": "ledger", "verified": true,
                  "landed": "<full sha>"},
 "since": {"commits": 3, "head": "<full sha>", "paths": ["src/billing/invoice.rs"],
           "recent": [{"sha": "<full sha>", "subject": "feat(billing): VAT per line"}]},
 "premises": [{"id": "2026-09-01-prices-are-floats", "status": "superseded", "superseded_by": ["2026-09-28-prices-are-integer-cents"]}]}
```

- `verification` is the newest of the entry's ledger lines and its `checked` field; null when it has
  neither. `kind` is `read`, `agent`, `bulk` or `verify`; `passed` is a boolean for `verify` and null
  otherwise; `source` is `ledger`, or `frontmatter` for a `checked` with no newer ledger line (then
  `kind` is `read` and `commit`, `by` and `because` are null). `verified` is true for a `read` and a
  passed `verify`: `agent` and `bulk` lines are stamps.
- `verification.landed` (added after 0.6.0) is the full id of the commit in `HEAD`'s history that
  first added the entry's newest ledger line, or null while that line is only staged or in the working
  tree: "passed and committed" against "passed, not committed yet". For `source: "frontmatter"` it is
  the last commit in `HEAD`'s history that changed the entry's file, null while the file has
  uncommitted changes or was never committed. `since` does not change: it already counts from the
  later of `commit` and the commit that landed the line.
- `checked` stays a string: the newer of the field in the file and the entry's newest confirmation
  in the ledger (a `verify` line is not a confirmation).
- `since` counts the non-merge commits on the entry's governed files (knowledge files left out) after
  the verification: after the commit it named, or the commit that landed its ledger line when that is
  newer; by time when neither is in `HEAD`'s history; for a `checked` without a ledger line, the way
  freshness counts. `paths` are the changed governed files, sorted, at most 20; `recent` the newest
  of the commits, at most five; `head` is `HEAD`. Null for an entry never verified, an inactive one
  or one without `paths`.
- `verify` is the entry's command, or null.
- `premises` lists the entries this active entry links to (`[[id]]` in its body, `supersedes`, an
  `extends` field) that are superseded, retired or deprecated, except the one it superseded itself.
  Empty when there are none. Nothing is rewritten in either entry.

Added in 0.10.0 (capability `verification.last_verified`), beside the fields above:

- `verification.kind` is also `cache`: a `checked` date with no ledger line. It reports
  `verified: false`; `kontext confirm --migrate-legacy` records `bulk` lines for such entries.
  `checked` is still written, since clients read it.
- `verification.by` is `application:<name>` for a `read` written through `kontext api`, so it can be
  told apart from a read at a terminal.
- `last_verified` is the newest look that counts as verification (a `read` or a passed `verify`), in
  the shape of `verification`, or null; `since_verified` counts commits from it the way `since` does.
- `failing: true` when the newest `verify` line failed. A failed check is shown but is not a look: it
  neither replaces an earlier look nor resets `since`.
- `amended_since: true` when the entry's body changed after its last verifying look.
- `since` counts by ancestry: the non-merge commits on governed paths reachable from `HEAD` and not
  from the commit of the look, so a branch merged after a confirmation is counted. Time is used only
  without such a commit. `since.truncated` is true when `index.max_commits` cut the walk, and
  `since.lower_bound` in a shallow clone.

One `git log` and one `git log -p` of the ledger cover every entry, cached per `HEAD`; a list costs a
`rev-parse` while nothing is committed. `landed` adds, per `HEAD`, a `git log` of the files whose
verification is a `checked` date alone, and a `git status` of those files on each read. The working-tree
`snapshot` changes when the ledger does.

### Completing a draft's proof {#inbox-verify}

`inbox.verify {id, verify}` sets or replaces the `verify` command of a pending inbox draft that this
worktree captured, the way a draft captured without the command that proved it is completed without
capturing it again; an empty string clears it. The command is validated like the entry field
(`invalid_input`); an unknown draft is `not_found`, one another active worktree captured
`worktree_conflict`, a challenge `invalid_input`. Promotion (`drafts.stage` with `candidate_id`,
`ctx_prepare_commit`, `kontext promote`) keeps it.

```json
{"id": "2026-10-27-cache-keys-carry-the-tenant", "verify": "cargo test cache_key",
 "entry": {"id": "2026-10-27-cache-keys-carry-the-tenant", "verify": "cargo test cache_key", "publication": {"state": "local"}, "…": "…"}}
```

### Drafting a replacement {#revise}

`drafts.revise {entry_id, entry_revision, objections, llm?}` asks the model for a replacement that
answers the objections (1 to 20 strings of at most 300 characters). It writes nothing. The draft keeps
the entry's kind, paths and tags, sets `supersedes: [entry_id]` and is `proposed` when it is a
decision; the client shows it to a person and stages it with `drafts.stage`. A stale
`entry_revision` or a dirty file is `operation_conflict`, no model is `model_unavailable`, and a
model that returns the old body unchanged is `invalid_model_output`.

### Adding to an entry {#extend}

Added in 0.9.0 (`status.capabilities` lists `drafts.extend`, `drafts.stage.amends` and
`drafts.stage.extends`).

```json
{"entry_id":"2026-10-20-relay-tests-fake-the-node","entry_revision":"<candidate revision>","text":"Put the fake server behind the node.","llm":true}
```

`drafts.extend {entry_id, entry_revision, text, llm?}` drafts the entry with a finding added to it. It
writes nothing. The result is `{entry, old_body, as_written, notes, amends}`: `entry` keeps the id,
file, kind, paths, tags and every other field and has the new body; `old_body` is the body it
replaces; `amends` is `{id, revision}` to pass to `drafts.stage`. The finding is always added as
written under `## Added <YYYY-MM-DD>` and `as_written` is true; a model never writes into an entry.
`llm` is still accepted, so existing clients keep working, and `notes` says the finding was added as
written. The shape of the result is unchanged. `text` holds at most 4000 characters.

Only conventions, learnings and incidents are amended. A decision (or `adr`) is refused with
`decision_not_amended`: a decision changes only by supersession, so a finding that builds on one is
kept as a new entry that `extends` it. Another kind is `invalid_input`, an inactive entry
`operation_conflict`. `entry_revision` is the candidate's `revision` from `entries.related` (the file's
blob id) or a full commit hash that holds the same file content; `HEAD`, a branch name, a short hash
or anything else is `stale_entry` (compare again). A file with local, staged or uncommitted changes is `operation_conflict`.

### Amending and linking on staging {#amend}

```json
{"operation_id":"amend-42","amends":{"id":"2026-10-20-relay-tests-fake-the-node","revision":"<candidate revision>"},"entry":{"body":"<the body drafts.extend returned>"}}
```

`drafts.stage` with `amends: {id, revision}` writes `entry.body` into that entry's own file and stages
it; the rest of `entry` is ignored. Everything before the body (front matter or ADR fields, the title)
stays byte for byte, so the review diff is the body. The revision is checked again as in
`drafts.extend` (`stale_entry`, `decision_not_amended`, `operation_conflict`); a body equal to the
current one is `invalid_input`. An amendment only adds: the current body must be a byte prefix of
`entry.body` and the rest must be `## Added <YYYY-MM-DD>` sections; anything else is
`amend_not_additive`, which points to supersession. After the splice the entry is parsed again and the
title and every field outside the body must be unchanged. The only in-place edits kontext makes to
reviewed text are metadata lines: `verify`, `checked`, and `status` / `superseded_by` on retire,
supersede and `kontext repair`. `amends` with `supersedes` or `extends` on the same draft, or with
`candidate_id`, is `invalid_input`. The result is the usual staging result with `amended: <id>`;
challenges against the entry stay open, since an addition does not answer them. It needs an
`operation_id` and replays like any staging.

`entry.extends: [<ids>]` on a new entry links it to the entries it builds on: the ids (a full id, a
unique prefix, an ADR number or a path) are resolved and written as `extends: [...]` in its front
matter, at most 20. An id the store does not have is `unknown_entry`. A changed premise is flagged
when one of them is later superseded or retired ([verification](#verification)). `extends` is not
accepted in `extra`.

### Changed entries {#changed}

Added in 0.9.0 (`status.capabilities` lists `entries.changed`).

`entries.changed {base, head, kinds?}` compares the entries committed at two revisions, without a
model, and returns `{base, head, changes}` with the resolved commits. Each change is
`{id, kind, title, file, paths, revision, change, superseded_by?}`, ordered by id, where `change` is:

- `added`: the id is not at `base`;
- `revised`: the same id with other content (an amended entry); a change of `checked` alone is a look,
  not a revision, and is left out;
- `superseded`: active at `base`, superseded at `head`; `superseded_by` lists the entries at `head`
  that supersede it;
- `deprecated`: active at `base`, then deprecated, retired or rejected, or removed (`removed: true`).

`revision` is the entry's revision at `head` (for a removed entry, at `base`), in the form
`entries.related` gives. `kinds` defaults to decisions, conventions, learnings and incidents. Both
sides read committed trees: the working tree and staged files are never part of the answer.

Added in 0.10.0: each change also carries `status` (the entry's status at `head`, or at `base` for a
removed entry), and each side is read with the configuration committed at that revision, so a kind
moved to another directory between `base` and `head` is not reported as removed and added.

Committed entries in any read (`CommittedEntry`) carry `date`, `verify`, `review_by`, `supersedes`,
`commits` and `author` besides the earlier fields.

## Rust embedding

The `kontext` crate exposes `application::request(&Path, operation, &input)` with the same JSON
envelope. Engine internals stay private. A service verifying its own Git mirror can embed
`application::KnowledgePolicy` instead of spawning a binary:

```rust
use kontext::application::KnowledgePolicy;

let policy = KnowledgePolicy::from_config(".ai", Some(reviewed_config_text))?;
let entry = policy.inspect("docs/adr/0001-storage.md", verified_blob_text)?;
```

`inspect` returns a `CommittedEntry` with `id`, `kind`, `title`, `body`, `summary`, `status`,
`checked`, `paths`, `tags`, `file` and the remaining metadata in `extra`.
`config_candidates` gives the committed configuration search order. The caller reads those bytes
from the proven revision, then uses `inspect` to resolve configured kinds/paths, parse frontmatter
or fields-style ADRs, and enforce active team knowledge. Decisions must explicitly be accepted;
private, proposed, rejected and superseded entries cannot gain authority through import. Inspection
does not read files or execute models/adapters. The caller independently proves the origin, current
review ref, full revision, regular-file mode and exact blob bytes/hash. A client-provided parsed entry
is never a substitute for that proof. Pin the crate's revision when consuming its Git source.

Release archives include the standalone binary plus an API-version/target/SHA-256 manifest for
applications bundling that exact artifact, including cross-platform packaging. Supported release
targets cover macOS ARM64/x64, Linux ARM64/x64 and Windows x64. No release or download occurs merely
by calling the application API.

## Applicability and delivery

`entries.governing` uses kontext's canonical path matcher. The result explains which pattern matched
which file. Specificity allows a client to request narrow notices (`min_specificity: 3`), while keeping
broad repository knowledge in the regular brief/index. Session timing, already-delivered IDs and
notification text belong to the application. A long-running MCP connection avoids starting a process
for every query; clients can cache results by snapshot.

Every operation may select another worktree or repository through an input `dir`. MCP also accepts
`dir` beside `operation`. Captures and staging with entry paths use normal repository ownership
routing. Local operation receipts belong to the target worktree; never reuse a source worktree's
receipt to write into another checkout.

## Staging and safe retries

```json
{
  "operation_id": "external-candidate-42",
  "source": "external:task/42@v7",
  "entry": {
    "kind": "decision",
    "title": "Amounts use integer cents",
    "body": "Integer cents prevent floating-point rounding drift.",
    "paths": ["src/pricing/**"],
    "status": "proposed"
  }
}
```

Give each logical write a stable operation ID. Retrying after a client restart or a remote queue
failure returns the original result with `replayed: true`, also after a later legitimate edit of the
same entry: a replay returns the stored receipt and never stages again. Reusing the ID with different
input is a conflict. `operations.get` also finds the receipt of a capture that was routed to another
repository's store. The engine coordinates concurrent application writes and persists the operation plan and
result locally. The client still owns reconciliation with its remote queue: local Git and a remote
service do not form one transaction.

When a model may rewrite a finding before staging, supply `fingerprint` on `drafts.stage`: a stable
hash of the original candidate, not the generated wording. Query `operations.get` with the same ID
and fingerprint before rerunning the model. It returns the saved result, rejects an edited candidate
with `operation_conflict`, or reports `operation_incomplete` for a saved unfinished plan. Resume that
plan with `drafts.stage` containing only `operation_id` and the same `fingerprint`; the engine uses
the saved canonical entry. A missing receipt is `not_found`, so only then create a new plan.

Staging preserves unrelated index content, keeps private captures out of the team store and refuses
conflicting destinations. A requested path is constrained to the configured knowledge store. A staged
record remains subject to the same commit and review process as other repository changes.

CLI and application captures share the process lock. Operation receipts and inbox/history are local
workflow state, distinct from disposable search caches: retain them while operations can be retried.
When a supplied `requested_path` selects a configured kind directory, staging can infer an omitted
`entry.kind` from that configuration; clients do not infer kinds from hardcoded folder names.

## Publication evidence and export

An entry's `status` describes its content; publication evidence describes where that content exists.
Working reads distinguish local drafts, modified files, staged changes and committed content. None
of those states by itself proves team approval. `publication.revision` is the commit the read saw
(HEAD, the same for every entry); `publication.blob_hash` is the Git blob of the entry's own file
(null while the file has unstaged edits or is untracked), so it changes only when that entry changes.
Use it to tell whether an entry changed since a client last showed it.

For a reviewed export, supply a full commit SHA and a full local review ref, for example
`refs/remotes/origin/main`. The revision must equal that ref's current tip. Export reads committed
blobs, not the checkout, excludes proposed/inactive entries, and returns the exact Markdown and blob
hashes. Dirty files and dirty shared configuration cannot replace the committed export configuration.
The response identifies the configuration source when local store configuration is needed.

The caller owns the policy that designates a ref as reviewed, plus any required fetch and hosting
provider review checks. An accepted frontmatter field or arbitrary `HEAD` is not proof of a reviewed
branch. Clients importing exported knowledge should retain its revision/ref/blob provenance and make
repeated imports idempotent.
