# Hooks: knowledge at the moment of an edit

An agent that never calls `ctx_brief` never sees the decisions that govern the file it edits. Host hooks deliver that knowledge without a tool call: at session start, after a compaction, for a subagent, and right before a file is edited.

Hooks are **opt-in**. The context hooks are installed only by `kontext connect <host> --hooks --write` and stay opt-in until an A/B comparison of agent outcomes with and without them is published; that report also sets the default budget.

## Install

```sh
kontext connect claude --hooks --write    # .claude/settings.json
kontext connect codex --hooks --write     # .codex/hooks.json
kontext connect cursor --hooks --write    # .cursor/hooks.json
```

Commit the file so the team gets the same hooks. `--write` without `--hooks` installs only the shell guard that refuses an agent's `kontext trust`. Every hook runs one command:

```sh
kontext hook --host claude    # or codex, cursor
```

It reads the hook event as JSON on stdin and prints the answer in the shape that host expects.

## What each event gets

| Event | Claude Code (`.claude/settings.json`) | Codex (`.codex/hooks.json`) | Cursor (`.cursor/hooks.json`) | Answer |
| --- | --- | --- | --- | --- |
| session start | `SessionStart`, matcher `startup\|resume\|clear\|compact` | `SessionStart`, same matcher | `sessionStart` | the brief; after `compact` or `resume` a shorter brief and the ids already told |
| subagent | `SubagentStart` | | | the brief, for the subagent |
| an edit | `PreToolUse`, matcher `Edit\|Write\|MultiEdit` | `PreToolUse`, matcher `apply_patch\|Edit\|Write` | `postToolUse` (after the edit) | the entries governing the edited files |
| a shell command | `PreToolUse`, matcher `Bash` | `PreToolUse`, matcher `Bash` | `beforeShellExecution` | denies `kontext trust`; nothing otherwise |

Claude Code and Codex hooks have the shape `{"hooks": {"<Event>": [{"matcher": "…", "hooks": [{"type": "command", "command": "…", "timeout": 15}]}]}}` (timeout 15 s at session start, 5 s otherwise). Cursor's file is `{"version": 1, "hooks": {"<event>": [{"command": "…", "timeout": 5}]}}`. Every command is

```sh
command -v kontext >/dev/null 2>&1 && kontext hook --host <host> 2>/dev/null || true # kontext:managed
```

so a machine without kontext runs nothing; the marker lets `connect` replace its own entries and keep the team's. Claude Code and Codex read the answer as `{"hookSpecificOutput": {"hookEventName": "…", "additionalContext": "…"}}`, a refusal as `permissionDecision: "deny"`; Cursor reads `{"additional_context": "…"}` and `{"permission": "deny", "user_message": "…", "agent_message": "…"}`. Codex runs project hooks only with `[features] codex_hooks = true` in its `config.toml`; `connect` says so and does not write it.

The edit hook answers with the committed, accepted entries at `HEAD` that govern the edited files. It sends only entries not yet told in this session, or whose file changed since (a new blob). Session state lives in `.git/kontext/sessions/<session id>.json`.

There is no Stop hook in what `connect` installs. If you add one yourself and set `[hooks] stop_note = true`, it answers with one line: the governing entries told this session and the tokens spent.

## One budget

There is one budget per session, `[hooks] session_budget_tokens` (6000): everything the hooks inject in one session draws from it, and one edit answer carries at most `[hooks] edit_budget_tokens` (900). Host limits apply on top: at most 10 000 characters of `additionalContext` in Claude Code and Cursor, about 2 500 tokens in Codex. `ctx_brief` draws from the same budget when it knows the session (a `session_id` argument, or `CLAUDE_SESSION_ID` in the server's environment): its budget is capped by what the session has left and its answer is counted, so what a hook injected is not given twice.

## What hooks never do

- **Block an edit.** The only refusal is the guard on an agent running `kontext trust`; context answers never deny a tool call or stop a session.
- **Fail loudly.** On any error (no repository, no store, a timeout) the hook exits 0 and prints nothing.
- **Call a model**, or send anything over the network. The session-start brief runs without adapters.
- **Inject unreviewed text.** Only committed, accepted entries are injected. An entry whose text the instruction lint flags (instructions aimed at the agent, role markers, `curl | sh`, a `verify:` line in the body) is named by id with `text withheld`, so a poisoned entry cannot instruct the agent through a hook.

## Turn them off

- For everyone: `[hooks] enabled = false` in `.ai/kontext.toml`.
- For you, now: `KONTEXT_HOOKS=off` (or `0`, `false`, `no`) in the environment.
- For good: remove the entries marked `kontext:managed` from the host's hook file. `kontext connect <host> --write` without `--hooks` keeps the context hooks already there and only adds the `kontext trust` guard.

## Check

```sh
echo '{"hook_event_name":"PreToolUse","tool_name":"Edit","tool_input":{"file_path":"src/billing/invoice.rs"},"session_id":"t1"}' \
  | kontext hook --host claude
```

prints exactly what Claude Code would receive.
