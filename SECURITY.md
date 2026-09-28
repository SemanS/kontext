# Security policy

## Supported versions

Security fixes are released for the latest minor version (currently 0.1.x). Please upgrade before reporting.

## Reporting a vulnerability

**Please do not open a public issue.** Report privately through GitHub:

1. Go to the repository's **Security** tab → **Report a vulnerability** ([direct link](https://github.com/SemanS/kontext/security/advisories/new)).
2. Describe the issue, the affected version (`kontext --version`), how to reproduce it and the impact you expect.

You will get an acknowledgement within a few days and a fix or mitigation plan once the report is assessed. We coordinate disclosure with you and credit you in the advisory unless you prefer otherwise.

## Scope

kontext runs locally with the permissions of the user who starts it. Reports are especially welcome for:

- ways a cloned repository could make kontext or its git hooks execute commands without `kontext trust`,
- secret-scanning bypasses that let credentials into committed knowledge files,
- path traversal through `ctx_read` or other tools,
- MCP tool inputs that lead to unintended writes outside the working tree, the git index or `.git/kontext/`.

Adapters you configure yourself run what you tell them to run; misconfigured adapters are not vulnerabilities in kontext.
