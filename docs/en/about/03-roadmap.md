# Roadmap

Ideas under consideration, roughly in order. Opinions and pull requests are welcome: open a [discussion](https://github.com/SemanS/kontext/discussions) or an [issue](https://github.com/SemanS/kontext/issues).

## Next

- **Distribution**: Homebrew tap, Linux ARM binaries, crates.io.
- **More presets**: Mem0 / Zep-style memory services, GitHub and GitLab issue search, Linear/Jira, Confluence/Notion as read-only knowledge sources.
- **Tree-sitter symbols** as an optional feature for more precise module facts without a code adapter.
- **Decision refresh tasks**: propose `refresh:` tasks for decisions flagged by [knowledge freshness](../reference/03-configuration.md#freshness). Freshness warnings are available in the brief and `kontext status`.

## Later

- **Review helpers**: a GitHub Action that comments on pull requests with the knowledge covering the change (`ctx_prepare_commit` for reviewers).
- **Cross-repository knowledge**: organization-wide conventions shared by many repositories, pulled into each brief.
- **Windows** support for hooks.
- **Embeddings adapter example**: a small local embedding service as a `search` adapter for teams without a memory system.

## Not planned

- A built-in vector database or LLM in the core: those stay adapters.
- A hosted service: kontext is a local tool; your git host is the server.
