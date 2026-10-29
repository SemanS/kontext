# Roadmap

Ideas under consideration, roughly in order. Opinions and pull requests are welcome: open a [discussion](https://github.com/SemanS/kontext/discussions) or an [issue](https://github.com/SemanS/kontext/issues).

Shipped items leave this page; the [changelog](./02-changelog.md) lists them. Linux ARM binaries, freshness flags, drift, CI checks, hooks for agent hosts and a 2026 MCP surface have shipped.

## Next

- **Distribution**: Homebrew tap, crates.io.
- **More presets**: Mem0 / Zep-style memory services, GitHub and GitLab issue search, Linear/Jira, Confluence/Notion as read-only knowledge sources.
- **Tree-sitter symbols** as an optional feature for more precise module facts without a code adapter.
- **Knowledge freshness**: propose `refresh:` tasks for decisions whose `paths` changed a lot since the decision (they are already flagged in the brief and `kontext status`).

## Toward 1.0

Not scheduled; this starts after an A/B report on agent outcomes.

- **Outcome A/B harness**: pairs of issue → pull request, with and without the brief and hooks, measuring success, governing rules caught at commit, tokens and cost. It sets `session_budget_tokens` and decides whether hooks become default-on.
- **Anchored claims**: `anchors:` (a path and symbol, a quoted line, a test name) resolved once per `HEAD`; a broken anchor marks the entry "evidence moved" in the brief, drift and the API, never hides it.
- **Verify in CI for touched paths**: `kontext ci --verify-touched` runs the base revision's commands of entries governing changed files, lists new or changed commands as not run, needs no local trust and never writes the ledger.
- **Approvals for applications**: a person approves a verify command once in the client's UI (MCP elicitation) instead of at a terminal.
- **Native rule exports** (opt-in): `kontext export rules --target claude-rules|cursor-rules|copilot-instructions|agents-md`, derived files marked as such, `--check` fails when stale.
- **Organisation bundles** vendored in `.ai/org/` and bumped by a bot pull request; **a deterministic PR review action** that comments and never blocks; **provenance on capture**; **MCP Registry** listing; **as-of queries** from git objects.

## Later

- **Cross-repository knowledge**: organization-wide conventions shared by many repositories, pulled into each brief (see organisation bundles above).
- **Windows**: tests for git hooks, host hooks and verify commands on native Windows.
- **Embeddings adapter example**: a small local embedding service as a `search` adapter for teams without a memory system.

## Not planned

- A built-in vector database or LLM in the core: those stay adapters.
- A hosted service: kontext is a local tool; your git host is the server.
