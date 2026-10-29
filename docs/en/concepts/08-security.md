# Security and privacy

kontext runs on your machine with your permissions and handles text that may end up in a shared repository. These are the guarantees it tries to give, and where they stop.

## What leaves your machine

Nothing, unless you configure it:

- the local index, inbox, outbox and caches stay in `.git/kontext/`,
- knowledge reaches others only through commits you push,
- adapters send data only where their configuration says (and only for the ops and events they declare),
- `kontext distill` sends a thread's compacted transcript to the `llm` adapter you choose, after redaction. What is masked, and what is dropped before the prompt, is listed under [sensitive threads](../guides/08-distill-threads.md#sensitive-threads). An invalid `secrets.redact` pattern stops distill, threads and autopilot before anything is sent.

## Trust for repository-declared adapters

Adapters can start processes and call URLs. A repository you clone must not be able to make kontext, or its git hooks, run arbitrary commands. Therefore:

- adapters in your user config (`~/.config/kontext/…`) and in the clone-local config are trusted,
- adapters declared in a repository's **shared** config (`.ai/kontext.toml`) are **skipped** until you review them and run `kontext trust` from a terminal,
- trust is pinned (in `~/.config/kontext/trust.toml`) to a SHA-256 hash over the canonical TOML of everything shared that reaches an adapter: the `adapters` table, `[vars]` and `project.name`. Any change to them requires trusting again,
- a legacy 12-hex FNV pin from a version before 0.10 is refused with a message, never upgraded silently: run `kontext trust` again,
- shared `[vars]` never override the vars of a user or local adapter, a shared `project.name` must be a slug, and `{{repo.remote}}` carries no userinfo.

`kontext status` and `kontext adapters list` show untrusted adapters as a warning.

What trust covers:

- **The declared text, not the scripts it starts.** If a trusted adapter runs `./scripts/sync.sh`, the pin covers the line that names the script, not the script. A later commit that changes the script is not caught. Review scripts an adapter starts from the repository as you would any code you run.
- **Honour-based against a process that can edit `trust.toml`.** `kontext trust` refuses to run without a terminal, but an agent that can allocate a terminal or write your config can pin anything. `kontext connect` installs a host rule that denies `kontext trust` run by the agent (`PreToolUse` on `Bash` in Claude Code and Codex, `beforeShellExecution` in Cursor); it is a guard rail, not a boundary.

## Verify commands are a trust boundary {#verify}

An entry's `verify:` field is a shell command from the repository, and running it is running repository code. Therefore:

- a verify command runs only when `trust.toml` holds a pin of its text: `sha256(entry path, NUL, command)` under `[repos.<slug>.verify]`,
- `kontext trust --verify <id>` (or `--all`) pins from an interactive terminal only and prints each command it pins. No MCP tool and no `ctx_api` operation pins,
- without a pin, `kontext verify`, `drafts.verify` and `ctx_verify` run nothing and return `untrusted_verify` with the command, its SHA-256 (`command_sha256`) and the line to pin it (`hint`). Editing the command drops the pin,
- over MCP an unpinned command is returned to the agent, which runs it in its own sandbox; the result is recorded as an `agent` stamp, not as a passed check,
- the command runs with an environment allowlist: `PATH`, `HOME`, `USER`, `LANG`, `LC_*`, `TMPDIR`, `TERM`, plus the names in `[verify] pass_env` from your user config (the shared and the clone-local config cannot add any),
- over MCP a pinned command is not run either while the entry's file or governed paths have uncommitted changes; elsewhere such a run is marked `[dirty: <paths>]` in the ledger line's reason,
- `output_tail` is redacted, and the process group is killed at the timeout, when stdout stays open 2 s after the shell exits, and when the MCP request is cancelled or stdin closes,
- `ctx_verify` is annotated `destructiveHint: true` and `openWorldHint: true`, so hosts ask before calling it.

What the pin does **not** cover:

- **The pin binds consent to the command text, not to the code it executes.** `cargo test` or `npm test` runs whatever the working tree contains; a branch can change the tests without changing the command.
- **`HOME` stays readable and the network open.** The allowlist removes tokens from the environment, not access to files or the network.
- **In Codex, `ctx_verify` runs inside the MCP server, outside the agent's sandbox.** Prefer that the agent runs the returned command itself.

The only path where a pull request cannot smuggle a command into a run with CI credentials is running the base revision's text in CI; see [CI](../guides/07-ci.md).

## Secret scanning

Every staged knowledge file is scanned by the pre-commit hook; high-confidence findings block the commit.

| Rule | Severity |
| --- | --- |
| private keys (PEM, OpenSSH, PGP), GCP service-account JSON | high |
| AWS access keys, GitHub tokens and fine-grained PATs, GitLab, Slack, Stripe live keys | high |
| Anthropic (`sk-ant-…`) and OpenAI (`sk-…`) keys, dotenv-vault keys | high |
| `Authorization: Bearer …` headers, credentials in URLs | high |
| Google API keys, JWTs, `*_TOKEN=` / `password:` assignments (placeholders ignored) | medium |
| secrets in sentences ("the secret is …", "rotate the api key …") when the value looks random (mixed case, digits, high entropy; paths and placeholders ignored) | medium |

- `secrets.scan = "staged"` extends the scan to every staged text file (not only knowledge).
- A line containing `kontext:allow-secret`, or matching a regex in `secrets.allow`, is ignored.
- `secrets.redact` and `secrets.allow` from your user or local config are joined with the repository's, never replaced by them.
- `ctx_capture` and deepen submissions redact findings automatically (`[redacted:<rule>]`).

## Files kontext never reads

`.env` files (except `.example`, `.sample`, `.template`, `.dist`), SSH keys, `*.pem`, `*.key`, `*.p12`, `*.pfx`, `*.keystore`, `*.tfstate`, `*.tfvars`, `credentials.json`, `.npmrc`, `.pypirc`, `.netrc`, service-account JSON files and `*secret*` configuration files are excluded from scanning, indexing, `ctx_read` and autopilot prompts. Environment variable *names* are read from example files; values never are.

## Agents and the repository

- `ctx_read` only returns files inside the repository, and refuses `.git/`, `.git-credentials` and `.ssh`.
- `ctx_threads` reads only `claude:`, `codex:`, `superset:`, `last` and paths under known transcript roots.
- `dir` of `ctx_verify`, `ctx_inbox` and `ctx_prepare_commit` is confined to this repository, its worktrees and its submodules.
- Tools that write (`ctx_capture`, `ctx_inbox promote`, `ctx_prepare_commit`, `ctx_init_submit`, `ctx_confirm`, `ctx_challenge`) modify the working tree and the git index only, never history, never remotes. `ctx_prepare_commit drop` refuses a candidate another worktree wrote.
- `ctx_verify` runs a pinned command (see [above](#verify)); hosts should ask before it.
- Commands run by adapters receive arguments as an argument vector, never through a shell. Adapter error messages carry no query string or userinfo.

## Reporting a vulnerability

Please do not open a public issue. Use GitHub's private vulnerability reporting: see [SECURITY.md](https://github.com/SemanS/kontext/blob/main/SECURITY.md).
