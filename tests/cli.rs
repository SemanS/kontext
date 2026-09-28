//! End-to-end: a throwaway repository goes through init, capture, promote, a hooked commit,
//! log/why, secret blocking and an MCP session.
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

struct Sandbox {
    root: PathBuf,
    repo: PathBuf,
    config: PathBuf,
}

impl Sandbox {
    fn new(name: &str) -> Sandbox {
        let root = std::env::temp_dir().join(format!("kontext-it-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let repo = root.join("repo");
        let config = root.join("config");
        std::fs::create_dir_all(&repo).unwrap();
        std::fs::create_dir_all(&config).unwrap();
        Sandbox { root, repo, config }
    }

    fn bin_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_BIN_EXE_kontext")).parent().unwrap().to_path_buf()
    }

    fn path_env() -> String {
        format!("{}:{}", Self::bin_dir().display(), std::env::var("PATH").unwrap_or_default())
    }

    fn cmd(&self, program: &str) -> Command {
        let mut c = Command::new(program);
        c.current_dir(&self.repo)
            .env("PATH", Self::path_env())
            .env("KONTEXT_CONFIG_DIR", &self.config)
            .env("GIT_AUTHOR_NAME", "Test")
            .env("GIT_AUTHOR_EMAIL", "test@example.com")
            .env("GIT_COMMITTER_NAME", "Test")
            .env("GIT_COMMITTER_EMAIL", "test@example.com")
            .env_remove("KONTEXT_SKIP");
        c
    }

    fn git(&self, args: &[&str]) -> Output {
        self.cmd("git").args(args).output().unwrap()
    }

    fn ok_git(&self, args: &[&str]) -> String {
        let o = self.git(args);
        assert!(o.status.success(), "git {args:?} failed: {}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr));
        String::from_utf8_lossy(&o.stdout).to_string()
    }

    fn kontext(&self, args: &[&str]) -> String {
        let o = self.cmd(env!("CARGO_BIN_EXE_kontext")).args(args).output().unwrap();
        assert!(
            o.status.success(),
            "kontext {args:?} failed: {}{}",
            String::from_utf8_lossy(&o.stdout),
            String::from_utf8_lossy(&o.stderr)
        );
        String::from_utf8_lossy(&o.stdout).to_string()
    }

    fn write(&self, rel: &str, text: &str) {
        let p = self.repo.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, text).unwrap();
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn seed(s: &Sandbox) {
    s.ok_git(&["init", "-q", "-b", "main"]);
    s.ok_git(&["remote", "add", "origin", "https://github.com/example-org/shop.git"]);
    s.write(
        "package.json",
        r#"{"name": "shop", "description": "A tiny shop used by kontext's tests", "workspaces": ["apps/*", "libs/*"]}"#,
    );
    s.write(
        "README.md",
        "# Shop\n\nA tiny web shop: a React storefront and an Express API sharing a pricing library.\n\n## Run\n\nnpm run dev\n",
    );
    s.write("apps/api/package.json", r#"{"name": "@shop/api", "dependencies": {"express": "^4.0.0", "@shop/pricing": "workspace:*"}}"#);
    s.write("apps/api/src/index.ts", "import { price } from '@shop/pricing';\nexport function start() { return price(1); }\n");
    s.write("apps/api/src/routes.ts", "export const routes = ['/cart', '/checkout'];\n");
    s.write("apps/api/src/cart.ts", "export class Cart { total() { return 0; } }\n");
    s.write("libs/pricing/package.json", r#"{"name": "@shop/pricing", "description": "Price calculation shared by API and storefront"}"#);
    s.write("libs/pricing/src/index.ts", "export function price(n: number) { return n * 100; }\n");
    s.write("libs/pricing/src/tax.ts", "export const VAT = 0.2;\n");
    s.write("libs/pricing/src/round.ts", "export function round(n: number) { return Math.round(n); }\n");
    // a package without README or manifest description: its docstring describes it
    s.write("worker/__init__.py", "\"\"\"Background jobs that recompute prices overnight.\"\"\"\n");
    s.write("worker/jobs.py", "def nightly():\n    return 0\n");
    s.write("worker/queue.py", "class Queue:\n    pass\n");
    s.ok_git(&["add", "-A"]);
    s.ok_git(&["commit", "-q", "-m", "feat: storefront and api"]);
    s.write("libs/pricing/src/money.ts", "export type Cents = number;\n");
    s.ok_git(&["add", "-A"]);
    s.ok_git(&[
        "commit",
        "-q",
        "-m",
        "refactor(pricing): use integer cents instead of floats",
        "-m",
        "Because float rounding broke VAT totals; all prices are integer cents now.",
    ]);
}

#[test]
fn end_to_end() {
    let s = Sandbox::new("e2e");
    seed(&s);

    let init = s.kontext(&["init"]);
    assert!(init.contains("[1/5] scan"), "{init}");
    assert!(init.contains("[4/5] wire"), "{init}");
    assert!(s.repo.join(".ai/architecture/overview.md").exists());
    assert!(s.repo.join(".ai/architecture/modules/libs-pricing.md").exists(), "{init}");
    assert!(s.repo.join(".ai/kontext.toml").exists());
    let pricing = std::fs::read_to_string(s.repo.join(".ai/architecture/modules/libs-pricing.md")).unwrap();
    assert!(pricing.contains("Price calculation shared by API and storefront"), "{pricing}");
    let api = std::fs::read_to_string(s.repo.join(".ai/architecture/modules/apps-api.md")).unwrap();
    assert!(api.contains("`libs/pricing`"), "dependency edge from the workspace dep/import: {api}");

    let overview = std::fs::read_to_string(s.repo.join(".ai/architecture/overview.md")).unwrap();
    assert!(overview.contains("Background jobs that recompute prices overnight."), "{overview}");

    let brief = s.kontext(&["brief", "--no-adapters"]);
    assert!(brief.contains("# shop — team context"), "{brief}");
    assert!(brief.contains("A tiny web shop"), "{brief}");

    // agent-style deepen submission
    let next = s.kontext(&["init", "next"]);
    assert!(next.contains("Task `purpose`"), "{next}");
    let submit = s.kontext(&["call", "ctx_init_submit", r#"{"task_id":"purpose","summary":"A demo shop: storefront + API over one pricing library.","overview":"Two apps share `libs/pricing`."}"#]);
    assert!(submit.contains("Recorded `purpose`"), "{submit}");

    s.ok_git(&["checkout", "-q", "-b", "kontext/bootstrap"]);
    s.ok_git(&["add", ".ai"]);
    s.ok_git(&["commit", "-q", "-m", "docs: bootstrap team knowledge"]);

    // capture → inbox → prepare-commit → promote → commit with trailer
    let cap = s.kontext(&[
        "capture",
        "--kind",
        "decision",
        "--title",
        "Prices are integer cents",
        "--paths",
        "libs/pricing/**",
        "--body",
        "Floats broke VAT rounding; every amount is an integer number of cents.",
    ]);
    assert!(cap.contains("captured inbox:"), "{cap}");
    let id = cap.trim().rsplit(':').next().unwrap().trim().to_string();
    s.write("libs/pricing/src/round.ts", "export function round(n: number) { return Math.round(n); }\n// cents only\n");
    s.ok_git(&["add", "libs/pricing/src/round.ts"]);
    let report = s.kontext(&["prepare-commit"]);
    assert!(report.contains(&id), "{report}");
    assert!(report.contains("touches 1 changed file"), "{report}");
    let promoted = s.kontext(&["prepare-commit", "--promote", &id]);
    assert!(promoted.contains("Decision: "), "{promoted}");
    s.ok_git(&["commit", "-q", "-m", "fix(pricing): round in cents"]);
    let msg = s.ok_git(&["log", "-1", "--format=%B"]);
    assert!(msg.contains("Decision: "), "trailer added by prepare-commit-msg: {msg}");
    let files = s.ok_git(&["show", "--name-only", "--format=", "HEAD"]);
    assert!(files.contains(".ai/decisions/"), "{files}");
    assert!(files.contains(".ai/README.md"), "index refreshed and staged by pre-commit: {files}");
    // rewording keeps the trailer: an amend measures from HEAD's parent
    s.ok_git(&["commit", "-q", "--amend", "-m", "fix(pricing): round in cents, reworded"]);
    let msg = s.ok_git(&["log", "-1", "--format=%B"]);
    assert_eq!(msg.matches("Decision: ").count(), 1, "trailer kept once on amend -m: {msg}");
    // …and the index forgets the commit the amend replaced
    let search = s.kontext(&["search", "round", "in", "cents"]);
    assert!(search.contains("reworded") && !search.contains("round in cents —"), "{search}");

    // a candidate captured in another worktree is not offered for this commit
    let wt = s.repo.with_file_name("e2e-worktree");
    s.ok_git(&["worktree", "add", "-q", "-b", "other", wt.to_str().unwrap()]);
    let other = s.kontext(&[
        "capture",
        "-C",
        wt.to_str().unwrap(),
        "--kind",
        "learning",
        "--title",
        "Tax tables load lazily",
        "--paths",
        "libs/pricing/**",
        "--body",
        "The VAT table is read on first use.",
    ]);
    assert!(other.contains("captured inbox:"), "{other}");
    s.write("libs/pricing/src/tax.ts", "export const VAT = 0.2; // standard rate\n");
    s.ok_git(&["add", "libs/pricing/src/tax.ts"]);
    let report = s.kontext(&["prepare-commit"]);
    assert!(report.contains("Captured in other worktrees") && !report.contains("## Inbox candidates"), "{report}");
    s.ok_git(&["restore", "-q", "--staged", "libs/pricing/src/tax.ts"]);
    s.ok_git(&["checkout", "-q", "--", "libs/pricing/src/tax.ts"]);
    s.ok_git(&["worktree", "remove", "--force", wt.to_str().unwrap()]);

    let log = s.kontext(&["log"]);
    assert!(log.contains("Prices are integer cents"), "{log}");
    let why = s.kontext(&["why", "libs/pricing/src/round.ts"]);
    assert!(why.contains("Prices are integer cents"), "{why}");
    let why_dir = s.kontext(&["why", "libs/pricing"]);
    assert!(why_dir.contains("use integer cents instead of floats"), "decision-shaped history: {why_dir}");
    let search = s.kontext(&["search", "cents", "rounding"]);
    assert!(search.contains("Prices are integer cents"), "{search}");

    // a secret in knowledge blocks the commit
    s.write(".ai/learnings/token.md", "---\nkind: learning\ntitle: Token\n---\nUse ghp_abcdefghijklmnopqrstuvwxyz0123456789AB\n");
    s.ok_git(&["add", ".ai/learnings/token.md"]);
    let blocked = s.git(&["commit", "-q", "-m", "oops"]);
    assert!(!blocked.status.success(), "the pre-commit hook must block secrets");
    assert!(String::from_utf8_lossy(&blocked.stderr).contains("possible secret"));
    s.ok_git(&["rm", "-q", "--cached", ".ai/learnings/token.md"]);
    std::fs::remove_file(s.repo.join(".ai/learnings/token.md")).unwrap();

    mcp_session(&s.repo, &s.config);
}

fn mcp_session(repo: &Path, config: &Path) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_kontext"))
        .arg("mcp")
        .current_dir(repo)
        .env("KONTEXT_CONFIG_DIR", config)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let requests = [
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"test","version":"0"}}}"#,
        r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#,
        r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"ctx_brief","arguments":{"focus":["libs/pricing"]}}}"#,
        r#"{"jsonrpc":"2.0","id":4,"method":"prompts/list"}"#,
    ];
    for r in requests {
        writeln!(stdin, "{r}").unwrap();
    }
    stdin.flush().unwrap();
    let mut reader = BufReader::new(child.stdout.take().unwrap());
    let mut replies = std::collections::HashMap::new();
    while replies.len() < 4 {
        let mut line = String::new();
        if reader.read_line(&mut line).unwrap() == 0 {
            break;
        }
        let v: serde_json::Value = serde_json::from_str(&line).unwrap();
        replies.insert(v["id"].as_i64().unwrap(), v);
    }
    drop(stdin);
    let _ = child.wait();
    assert_eq!(replies[&1]["result"]["serverInfo"]["name"], "kontext");
    let tools: Vec<&str> = replies[&2]["result"]["tools"].as_array().unwrap().iter().map(|t| t["name"].as_str().unwrap()).collect();
    for t in ["ctx_brief", "ctx_search", "ctx_capture", "ctx_prepare_commit", "ctx_init", "ctx_init_submit"] {
        assert!(tools.contains(&t), "{t} missing from {tools:?}");
    }
    // clients such as Codex decide approval by these hints
    for t in replies[&2]["result"]["tools"].as_array().unwrap() {
        let a = &t["annotations"];
        assert!(a["readOnlyHint"].is_boolean() && a["destructiveHint"] != true, "{t}");
    }
    let brief_tool = replies[&2]["result"]["tools"].as_array().unwrap().iter().find(|t| t["name"] == "ctx_brief").unwrap();
    assert_eq!(brief_tool["annotations"]["readOnlyHint"], true);
    let brief = replies[&3]["result"]["content"][0]["text"].as_str().unwrap();
    assert!(brief.contains("Prices are integer cents"), "{brief}");
    assert_eq!(replies[&4]["result"]["prompts"][0]["name"], "kontext-init");
}
