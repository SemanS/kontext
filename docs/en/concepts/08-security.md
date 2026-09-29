# Security and privacy

kontext runs on your machine with your permissions and handles text that may end up in a shared repository. These are the guarantees it tries to give.

## What leaves your machine

Nothing, unless you configure it:

- the local index, inbox, outbox and caches stay in `.git/kontext/`,
- knowledge reaches others only through commits you push,
- adapters send data only where their configuration says (and only for the ops and events they declare),
- `kontext distill` sends a thread's compacted transcript to the `llm` adapter you choose, with secrets redacted and email addresses, phone numbers, IBANs, card numbers, IP addresses and your `secrets.redact` patterns masked (see [sensitive threads](../guides/08-distill-threads.md#sensitive-threads)).

## Trust for repository-declared adapters

Adapters can start processes and call URLs. A repository you clone must not be able to make kontext, or its git hooks, run arbitrary commands. Therefore:

- adapters in your user config (`~/.config/kontext/…`) and in the clone-local config are trusted,
- adapters declared in a repository's **shared** config (`.ai/kontext.toml`) are **skipped** until you review them and run `kontext trust`,
- trust is pinned to a hash of the adapters table (in `~/.config/kontext/trust.toml`); any change requires trusting again.

`kontext status` and `kontext adapters list` show untrusted adapters as a warning.

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
- `ctx_capture` and deepen submissions redact findings automatically (`[redacted:<rule>]`).

## Files kontext never reads

`.env` files (except `.example`, `.sample`, `.template`, `.dist`), SSH keys, `*.pem`, `*.key`, `*.p12`, `*.pfx`, `*.keystore`, `*.tfstate`, `*.tfvars`, `credentials.json`, `.npmrc`, `.pypirc`, `.netrc`, service-account JSON files and `*secret*` configuration files are excluded from scanning, indexing, `ctx_read` and autopilot prompts. Environment variable *names* are read from example files; values never are.

## Agents and the repository

- `ctx_read` only returns files inside the repository.
- Tools that write (`ctx_capture`, `ctx_inbox promote`, `ctx_prepare_commit`, `ctx_init_submit`) modify the working tree and the git index only, never history, never remotes.
- Commands run by adapters receive arguments as an argument vector, never through a shell.

## Reporting a vulnerability

Please do not open a public issue. Use GitHub's private vulnerability reporting: see [SECURITY.md](https://github.com/SemanS/kontext/blob/main/SECURITY.md).
