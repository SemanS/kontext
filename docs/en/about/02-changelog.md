# Changelog

All notable changes are listed here. The project follows [Semantic Versioning](https://semver.org/); until 1.0 minor versions may change configuration or the entry format, always with a migration note.

## 0.1.0 — 2026-09-28

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
