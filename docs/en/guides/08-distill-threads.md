# Knowledge from agent threads

Much of a team's reasoning happens in agent threads: a colleague and Claude Code or Codex weigh two approaches, find a pitfall, settle a rule — and the thread ends. `kontext distill` reads such threads and turns what should outlive them into knowledge candidates. They land in the local inbox and are shared only when promoted with a commit and reviewed, like anything else captured.

Threads are read from this machine: every colleague distills their own. Raw transcripts never enter the repository; only the entries that come out of them do.

## From the command line

```sh
kontext distill                       # list this repository's threads on this machine
kontext distill --claude last         # the newest Claude Code thread of this repository
kontext distill --codex 01a0e6c1      # a Codex thread by id (or prefix)
kontext distill --superset            # every thread of the current Superset workspace
kontext distill --superset calm-river # a workspace by id, worktree name, path or branch
kontext distill slack-thread.md       # any text: a copied Slack thread, a PR discussion, notes
pbpaste | kontext distill -           # the same from stdin
```

```text
Superset workspace 7c41e0d2 · Fix rounding in the cart · fix/rounding · 1 thread(s) here
Distilling 1 thread(s)…
  superset:7c41e0d2/claude:4f1c2a9b · 1 part(s), 42s · 2 entries

[decision] Round prices half-even  (libs/pricing/src/round.ts)
→ inbox:2026-09-29-round-prices-half-even
```

What happens:

1. The thread is read into a compact transcript — what the developer asked, what the agent answered, what it edited and ran (`→ edit src/cache.ts`, `→ $ git commit …`). Tool output, reasoning and injected instructions (AGENTS.md, environment, hooks) are left out, paths are made relative to the repository, and secrets are redacted.
2. The transcript is split into parts of about 45,000 characters (`--max-parts`, default 12, spread over a longer thread) and sent to your LLM adapter, several at a time (`--jobs`).
3. The model names the decisions, conventions, pitfalls and incidents the team should still know later — at most `--max` per thread (5) — and is shown what is recorded already, so it does not repeat it. Findings similar to an existing entry or inbox candidate are dropped.
4. Each finding becomes an inbox candidate with its paths, the commits the thread made it in, `origin: thread` and its source (`claude:4f1c2a9b`, shown by `kontext inbox`, dropped on promotion).

Review them with `kontext inbox`, promote each with the change it belongs to (`kontext prepare-commit --promote <id>`, or `ctx_prepare_commit` from an agent), drop the rest (`kontext inbox drop <id>`). `--dry-run` prints the findings without capturing them (it also works where kontext is not set up yet).

| Option | Default | |
| --- | --- | --- |
| `--llm` | `init.llm` | adapter with an `llm` op (`llm-codex`, `llm-claude`, …); needed when several are configured |
| `--max` | 5 | most entries per thread |
| `--max-parts` | 12 | most transcript parts per thread |
| `--jobs` | 3 | parallel model calls |
| `--list` | | list the threads instead of distilling them |
| `--dry-run` | | print, do not capture |

## From an agent

The `kontext-distill` prompt (`/mcp__kontext__kontext-distill` in Claude Code), or simply asking — "distill the durable knowledge from thread claude:4f1c2a9b" — lets the connected agent do it with its own model, no LLM adapter needed:

1. `ctx_threads` without arguments lists this repository's recent threads and the current Superset workspace;
2. `ctx_threads thread="claude:4f1c2a9b"` returns the transcript, in parts (`part=2`, … when it says there are more);
3. the agent checks `ctx_search`, then records each finding with `ctx_capture` (with `source` and `commits`).

## Superset

Superset runs agents in workspaces — a worktree, or a session directory — and gives each an id (`$SUPERSET_WORKSPACE_ID` in its terminals). kontext reads Superset's own records (`~/.superset/host/<organization>/host.db`, with the `sqlite3` command): the workspace's path and branch, and the agent sessions its terminals ran. Their transcripts are found in every Claude Code account (`~/.claude`, `~/.claude-*`, `$CLAUDE_CONFIG_DIR`) and in Codex's sessions (`$CODEX_HOME`, `~/.codex`), plus any thread started in the workspace's directory.

- `--superset` alone means the workspace of the terminal you are in; a name can be the worktree directory (`calm-river`), a path, a branch or an id prefix.
- Superset binds agents to terminals, not directories: a thread one of its terminals ran in another repository is skipped ("ran outside this repository") — distill it from that repository.
- Runs that keep no transcript (`codex exec --ephemeral`, for example) are counted as "without a transcript".

## Where transcripts come from

| Agent | Files |
| --- | --- |
| Claude Code | `<config>/projects/<path as slug>/<session>.jsonl` for each config dir |
| Codex | `<CODEX_HOME>/sessions/YYYY/MM/DD/rollout-<time>-<id>.jsonl` and `archived_sessions/` |
| anything else | a file or stdin, read as plain text |
