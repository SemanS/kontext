# Hooks: knowledge at the moment of an edit

An agent that never calls `ctx_brief` never sees the decisions that govern the file it edits. Host hooks deliver that knowledge without a tool call: at session start, after a compaction, for a subagent, and right before a file is edited.

Hooks are **opt-in**. They are installed only by `kontext connect <host> --hooks` and stay opt-in until an A/B comparison of agent outcomes with and without them is published; that report also sets the default budget.

## Install

```sh
kontext connect claude --hooks --write    # .claude/settings.json
kontext connect codex --hooks --write     # .codex/hooks.json
kontext connect cursor --hooks --write    # .cursor/hooks.json
```

Commit the file so the team gets the same hooks. Every hook runs one command:

```sh
kontext hook --host claude    # or codex, cursor
```

It reads the hook event as JSON on stdin and prints the answer in the shape that host expects.

## What each event gets

| Event | Claude Code | Codex | Cursor | Answer |
| --- | --- | --- | --- | --- |
| session start | `SessionStart` with `startup`, `resume`, `clear`, `compact` | `SessionStart` | session start | the brief |
| subagent | `SubagentStart` | | | the brief, for the subagent |
| before an edit | `PreToolUse` on `Edit\|Write` | before `apply_patch` | `postToolUse` after an edit | the governing section for the edited file |
| stop (optional) | `Stop`, with `[hooks] stop_note = true` | | | a one-line capture reminder |

The edit hook answers with the entries that govern `tool_input.file_path`, the same section a focused brief starts with. It sends only entries not yet told in this session, or whose file changed since (a new blob). Session state lives in `.git/kontext/sessions/<id>.json`.

## One budget

There is one budget per session, `[hooks] session_budget_tokens`, and it is the same budget `ctx_brief` draws from. A hook is not a second channel with its own allowance: what a hook injected is not injected again by the next brief, and the other way round. Host limits apply on top: at most 10 000 characters of `additionalContext` in Claude Code, about 2 500 tokens in Codex.

## What hooks never do

- **Block.** A hook never denies a tool call and never stops a session; the Stop note is `additionalContext` only.
- **Fail loudly.** On any error (no repository, no store, a timeout) the hook exits 0 and prints nothing. The target is 150 ms on a 250-entry store.
- **Call a model**, or send anything over the network.
- **Inject unreviewed text.** Only committed, accepted entries are injected. An entry whose body a linter flags (imperatives aimed at the agent, `curl | sh`, verify lines in the body) is referenced by id only, so a poisoned entry cannot instruct the agent through a hook.

## Turn them off

- For everyone: `[hooks] enabled = false` in `.ai/kontext.toml`.
- For you, now: `KONTEXT_HOOKS=off` in the environment.
- For good: remove the entries from the host's hook file (`kontext connect <host>` without `--hooks` prints the file without them).

## Check

```sh
echo '{"hook_event_name":"PreToolUse","tool_name":"Edit","tool_input":{"file_path":"src/billing/invoice.rs"},"session_id":"t1"}' \
  | kontext hook --host claude
```

prints exactly what Claude Code would receive. `kontext status` shows whether hooks are enabled and how much of the session budget is used.
