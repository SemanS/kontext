---
layout: home

hero:
  name: kontext
  text: Team context bridge for coding agents
  tagline: "Short, reviewed memory of why your code is the way it is: kept in git, served over MCP to every agent you run, with any memory, history or code system plugged in as a configured adapter."
  image:
    src: /logo.svg
    alt: kontext
  actions:
    - theme: brand
      text: Get started
      link: /getting-started/01-introduction
    - theme: alt
      text: Quick start
      link: /getting-started/03-quickstart
    - theme: alt
      text: GitHub
      link: https://github.com/SemanS/kontext

features:
  - icon: 🧭
    title: One brief, every agent
    details: ctx_brief gives Claude Code, Codex, Cursor or OpenCode the active decisions, conventions, pitfalls and module map in ~1–2k tokens, ranked for the files the agent is about to touch.
  - icon: 🗂️
    title: Knowledge where the work is
    details: Agents leave the directory their session started in. Paths in a submodule, another worktree or a sibling project are answered from the repository that owns them, and captures land there too.
    link: /concepts/09-several-repositories
    linkText: Several repositories
  - icon: 🌿
    title: Git is the source of truth
    details: Decisions and learnings are short Markdown files in the repository. They change through commits and pull requests, so the team reviews knowledge together with the code it explains.
  - icon: 📥
    title: Capture, then promote
    details: Agents capture candidates into a local inbox. ctx_prepare_commit promotes the relevant ones into the commit they belong to and the hook adds Decision:&nbsp;trailers.
  - icon: ⏳
    title: Stale decisions surface
    details: A decision is written once while its code keeps moving. One whose paths saw 20+ commits since it was made shows up in the brief and in kontext status.
    link: /concepts/04-context-layers#freshness
    linkText: Freshness
  - icon: 🔌
    title: Adapters, not integrations
    details: "OpenViking, CodeGraph, Serena, Agent LCM, sessions, LLM CLIs: each is a few lines of TOML over generic MCP, HTTP or command drivers. The core never names a product."
  - icon: 🪜
    title: Step-by-step bootstrap
    details: kontext init scans the repository, mines history for decision-shaped commits and writes the facts in seconds; agents then deepen it one small, resumable task at a time.
  - icon: ⚡
    title: Fast, local, private
    details: A single Rust binary with an embedded Tantivy index. Briefs in ~50 ms, hooks in ~0.1 s, secret scanning on every commit, nothing leaves your machine unless an adapter sends it.
---
