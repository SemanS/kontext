# Deepen with an LLM

The deepen phase of `kontext init` is normally driven by the agent you work with. The autopilot runs the same tasks through any LLM you configure as an adapter with an `llm` op — useful for a first pass over a large repository.

```sh
kontext adapters add llm-claude              # or llm-codex, llm-ollama, or your own
kontext init --deepen --llm llm-claude --jobs 3 --max 20
```

```text
[5/5] deepen   … 1/47 done, running up to 20 task(s) with 'llm-claude' (3 at a time)
  [1/20] mod:libs-schemas ✓ (32.0s)
  [2/20] mod:apps-runner ✓ (41.3s)
  …
      deepen   partial ✓ 20 · ✗ 0 · progress 21/47
```

| Option | Default | Meaning |
| --- | --- | --- |
| `--llm` | `init.llm` | adapter to use |
| `--jobs` | 2 | tasks in parallel |
| `--max` | 50 | tasks in this run |

Set a default in config to drop `--llm`:

```toml
[init]
llm = "llm-claude"
```

## What the model receives

Each task as an agent would see it, plus **inlined sources** (up to `init.task_inline_chars`, 24000 characters): the module's docs and key files, or `git show --stat` of the commits in a decision cluster. Secret files are never attached and attached text is redacted. The model must answer with one JSON object — the same shape as `ctx_init_submit`:

```json
{
  "summary": "One line: what the module is for",
  "overview": "5–15 lines of Markdown",
  "decisions": [{ "title": "…", "decision": "…", "context": "…", "consequences": "…", "paths": ["…"], "commits": ["abc1234"] }],
  "learnings": [{ "title": "…", "body": "…", "paths": ["…"] }]
}
```

Prose or code fences around the JSON are tolerated. Failed tasks are reported and stay pending.

The answer is the result: the model is told not to call tools, and the presets keep kontext's own MCP server out of the call (`-c mcp_servers.kontext.enabled=false` for Codex, `--strict-mcp-config` for Claude Code) — with kontext connected to your client, the model could otherwise record the task itself through `ctx_init_submit` and again through its answer. While a run is going, `ctx_init_submit` from any other process refuses the run's tasks, and a resubmitted task replaces its own earlier entries instead of adding copies.

## Models

| Preset | Runs | Variable |
| --- | --- | --- |
| `llm-claude` | `claude -p --strict-mcp-config --output-format text --model <model>` | `model` (default `sonnet`; `haiku` is cheaper and good for module summaries) |
| `llm-codex` | `codex exec --sandbox read-only --ephemeral -c mcp_servers.kontext.enabled=false`, last message from a file | — |

Presets are copied into your config when added; after an upgrade, refresh them with `kontext adapters add llm-codex --force` (and `llm-claude`).
| `llm-ollama` | `POST /api/generate` on `$OLLAMA_HOST` | `model` (default `qwen2.5-coder:14b`) |

```sh
kontext adapters add llm-claude --var model=haiku --force
```

## Quality

- Review the diff before committing — the autopilot writes into the working tree only.
- Run a few tasks first (`--max 3`) and read them; adjust `init.task_max_files` or `init.task_inline_chars` if summaries are shallow.
- Decision tasks are the ones to check most carefully: they infer intent from commit messages.
- Mixing is fine: let the autopilot summarize modules and have your agent do the `dec:` tasks with full tool access.
