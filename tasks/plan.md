# Generalized knowledge application API

## Scope

Kontext owns knowledge parsing, configuration, querying, applicability, extraction, draft creation,
Git staging and reviewed-revision export. Peer keeps its UI, Hub candidates, identities, coordination,
session delivery and authorization. Existing human CLI commands and agent tools remain compatible.

## Contract

`kontext -C <checkout> api <operation> <JSON|->` and MCP `ctx_api` use the same dispatcher.
Success: `{schema_version:1,ok:true,result:{...}}`.
Failure: `{schema_version:1,ok:false,error:{code,message}}`; CLI exits nonzero, MCP sets `isError`.
MCP additionally returns the envelope as `structuredContent`, with an `outputSchema`.
JSON is never extracted from human prose, clipped or prefixed with repository banners.

Operations: `status`, `setup`, `entries.list`, `entries.get`, `entries.search`, `entries.governing`,
`entries.export`, `inbox.list`, `inbox.drop`, `drafts.capture`, `drafts.stage`, `drafts.extract`,
`drafts.formalize`, `guidance.propose`. Entry metadata includes structured paths, tags, supersession
and provenance. Publication evidence is distinct from decision status.

## Ownership and order

1. Read service agent: `src/api_read.rs`, including focused tests. Queries honor configured stores,
   ADR styles and canonical path semantics; export validates an exact committed review-ref tip.
2. Write service agent: `src/api_write.rs`, `src/distill.rs`, necessary shared preparation in
   `src/ops.rs`. Draft operations preserve metadata and use durable operation receipts and process
   locking; extraction/formalization do not implicitly publish knowledge.
3. Peer agent: recover a valid source checkout, then migrate Knowledge and Memory to this API.
   Keep Hub decisions, session delivery, independent findings, guidance, editing and review behavior.
4. Root: contract/CLI/MCP, transport integration tests, bilingual docs, cross-repository verification,
   review and merge. Agents do not commit into the shared kontext index.
   `main.rs` is a thin binary over private `cli.rs` and engine modules in `lib.rs`; the public
   `application::KnowledgePolicy` facade also serves independent trusted-mirror verification in Hub.

## Acceptance and verification

- CLI and MCP expose identical complete structured results, errors and repository routing.
- Configured store paths, ADR styles, inactive entries, pagination and applicability are tested.
- Reviewed export excludes dirty checkout content and rejects stale or unproven revisions.
- Retry, restart and concurrent staging cannot duplicate a logical operation; private notes cannot
  enter the team store; unrelated index content is preserved.
- Text extraction is read-only and preserves source paths; missing LLM is reported explicitly.
- Peer retains its current workflows while removing its own knowledge format/query/write engine.
- Run focused tests per slice, then `cargo test`, `cargo clippy --all-targets -- -D warnings`,
  `cargo fmt --check`, bilingual documentation build, and Peer-required tests/type/lint checks.
- Record the durable architecture decision, prepare knowledge with the commit, and merge only
  verified changes into each repository's actual integration branch without overwriting user work.
