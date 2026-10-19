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
| `inbox.list` | `state?`: `pending` (default), `dismissed`, `promoted`, `all` | local candidates and retained workflow history |
| `inbox.drop` | `id`, `reason?` | dismiss a local candidate |
| `drafts.capture` | `entry`, `source?`, `operation_id?` | inbox candidate, notes, redaction count, `evidence` and `related_entries` |
| `drafts.stage` | `entry` or `candidate_id`, `operation_id`, `source?`, `requested_path?` | entry, path, staging result, notes, replay flag, `evidence`, `related_entries` and `resolved_challenges` |
| `drafts.retire` | `operation_id`, `entry_id`, `entry_revision`, `reason` | the entry staged with status `deprecated` and a `## Retired` note ([details](#retire)) |
| `drafts.revise` | `entry_id`, `entry_revision`, `objections`, `llm?` | a draft that supersedes the entry; no writes ([details](#revise)) |
| `drafts.extract` | `text`, `source?`, `llm?`, `max?` | extracted candidates and notes; no capture or staging |
| `drafts.formalize` | `entry`, `sources?`, `context?`, `llm?` | formulated entry, notes and `related_entries`; no capture or staging |
| `guidance.propose` | `kept`, `dismissed`, `missed`, `current?`, `profile_tag?`, `llm?` | proposed convention; no staging |
| `operations.get` | `operation_id`, `fingerprint?` | the original completed write result, or an explicit incomplete/conflict error |

The `entry` object includes `kind`, `title`, `body`, and optional `summary`, `paths`, `tags`, `status`,
`supersedes`, `visibility`, `commits`, `review_by` and `evidence` ([below](#maintenance)). Keep paths as arrays throughout the workflow. A selected
finding can be formalized separately from extracting new findings, so deduplication does not discard
something the user explicitly chose to keep. Extraction and guidance generation require a usable LLM;
formalization can preserve the supplied finding when no model is selected.

Read entries include `id`, `kind`, `title`, `body`, `summary`, `status`, `date`, `review_by`, `paths`, `tags`, `file`,
`markdown`, `style`, supersession fields and extra metadata (`extra.external_refs` holds the sources). The engine owns parsing and matching;
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
near the best hit) or govern overlapping `paths`, each `{id, title, kind, status?, reason, label}`
with `label: "related, check"`. With `llm: true` (or an adapter name) one model question names the
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
`entry_revision` is the commit the caller read the entry at (`publication.revision` of `entries.get`).
The operation fails with `operation_conflict` when the file at that revision differs from HEAD, when
the file has local or staged changes, or when the entry is already inactive. It needs an
`operation_id`, keeps a receipt and replays like `drafts.stage` (`replayed: true`,
`operations.get`). Pending challenges of this worktree against the entry are resolved by it and
listed in `resolved_challenges`. Retirement and supersession work for every kind, not only decisions.

A challenge is how an agent says an entry no longer holds without changing it: `ctx_challenge`
(or `kontext challenge`) writes an inbox candidate of kind `challenge` with `extra.against` (the entry
id), `extra.against_revision` (the commit) and `extra.against_blob`. `inbox.list` returns it; it cannot
be staged as an entry. A retirement or a staged replacement that supersedes the entry resolves it.

### Drafting a replacement {#revise}

`drafts.revise {entry_id, entry_revision, objections, llm?}` asks the model for a replacement that
answers the objections (1 to 20 strings of at most 300 characters). It writes nothing. The draft keeps
the entry's kind, paths and tags, sets `supersedes: [entry_id]` and is `proposed` when it is a
decision; the client shows it to a person and stages it with `drafts.stage`. A stale
`entry_revision` or a dirty file is `operation_conflict`, no model is `model_unavailable`, and a
model that returns the old body unchanged is `invalid_model_output`.

## Rust embedding

The `kontext` crate exposes `application::request(&Path, operation, &input)` with the same JSON
envelope. Engine internals stay private. A service verifying its own Git mirror can embed
`application::KnowledgePolicy` instead of spawning a binary:

```rust
use kontext::application::KnowledgePolicy;

let policy = KnowledgePolicy::from_config(".ai", Some(reviewed_config_text))?;
let entry = policy.inspect("docs/adr/0001-storage.md", verified_blob_text)?;
```

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
failure returns the original result with `replayed: true`. Reusing the ID with different input is a
conflict. The engine coordinates concurrent application writes and persists the operation plan and
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
of those states by itself proves team approval.

For a reviewed export, supply a full commit SHA and a full local review ref, for example
`refs/remotes/origin/main`. The revision must equal that ref's current tip. Export reads committed
blobs, not the checkout, excludes proposed/inactive entries, and returns the exact Markdown and blob
hashes. Dirty files and dirty shared configuration cannot replace the committed export configuration.
The response identifies the configuration source when local store configuration is needed.

The caller owns the policy that designates a ref as reviewed, plus any required fetch and hosting
provider review checks. An accepted frontmatter field or arbitrary `HEAD` is not proof of a reviewed
branch. Clients importing exported knowledge should retain its revision/ref/blob provenance and make
repeated imports idempotent.
