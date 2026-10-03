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
fn knowledge_freshness_in_brief_and_status() {
    let s = Sandbox::new("freshness");
    s.ok_git(&["init", "-q", "-b", "main"]);
    s.write(".ai/kontext.toml", "[freshness]\nthreshold_commits = 2\n");
    s.write(
        ".ai/decisions/use-cache.md",
        "---\nid: use-cache\nkind: decision\ntitle: Use cache\nstatus: accepted\ndate: 2025-01-01\npaths: [src/**, src/cache.rs]\n---\n\nCache reads.\n",
    );
    for (id, kind, status, date, paths) in [
        ("newer", "decision", "accepted", "2025-01-02", "src/**"),
        ("superseded", "decision", "superseded", "2025-01-01", "src/**"),
        ("deprecated", "decision", "deprecated", "2025-01-01", "src/**"),
        ("rejected", "decision", "rejected", "2025-01-01", "src/**"),
        ("convention", "convention", "accepted", "2025-01-01", "src/**"),
        ("undated", "decision", "accepted", "", "src/**"),
        ("bad-date", "decision", "accepted", "yesterday", "src/**"),
        ("unscoped", "decision", "accepted", "2025-01-01", ""),
        ("unrelated", "decision", "accepted", "2025-01-01", "lib/**"),
    ] {
        s.write(
            &format!(".ai/decisions/{id}.md"),
            &format!("---\nid: {id}\nkind: {kind}\ntitle: {id}\nstatus: {status}\ndate: {date}\npaths: [{paths}]\n---\n\nA rule.\n"),
        );
    }
    s.write("src/cache.rs", "first\n");
    s.write("src/other.rs", "first\n");
    let commit = |date: &str| {
        s.ok_git(&["add", "-A"]);
        let o = s
            .cmd("git")
            .env("GIT_AUTHOR_DATE", date)
            .env("GIT_COMMITTER_DATE", date)
            .args(["commit", "-q", "-m", "test change"])
            .output()
            .unwrap();
        assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    };
    commit("2025-01-01T12:00:00Z");
    s.write("src/cache.rs", "same day\n");
    commit("2025-01-01T20:00:00Z");
    s.write("README.md", "Unrelated change\n");
    commit("2025-01-02T12:00:00Z");
    s.write("src/cache.rs", "second day\n");
    s.write("src/other.rs", "second day\n");
    commit("2025-01-02T13:00:00Z");
    let brief = s.kontext(&["brief", "--no-adapters"]);
    assert!(!brief.contains("may need a refresh"), "one matching commit, despite multiple files and overlapping paths: {brief}");

    // Renaming out of a governed path still changes that path.
    s.ok_git(&["mv", "src/cache.rs", "cache.rs"]);
    commit("2025-01-03T12:00:00Z");
    let brief = s.kontext(&["brief", "--no-adapters", "--budget", "300"]);
    assert!(brief.contains("## State") && brief.contains("use-cache may need a refresh"), "{brief}");
    assert!(brief.contains("at least 2 commits"), "{brief}");
    let status = s.kontext(&["status"]);
    assert!(status.contains("warning   Decision use-cache may need a refresh"), "{status}");
    assert_eq!(status.matches("may need a refresh").count(), 1, "{status}");

    // Reviewing the decision and advancing its date resets the warning, even before commit.
    let decision_path = ".ai/decisions/use-cache.md";
    let original = std::fs::read_to_string(s.repo.join(decision_path)).unwrap();
    s.write(decision_path, &original.replace("date: 2025-01-01", "date: 2025-01-03"));
    assert!(!s.kontext(&["brief", "--no-adapters"]).contains("may need a refresh"));
    s.write(decision_path, &original);

    s.write(".ai/kontext.toml", "[freshness]\nthreshold_commits = 3\n");
    s.write("src/other.rs", "uncommitted change\n");
    assert!(!s.kontext(&["brief", "--no-adapters"]).contains("may need a refresh"));
    s.write(".ai/kontext.toml", "[freshness]\nthreshold_commits = 0\n");
    assert!(!s.kontext(&["status"]).contains("may need a refresh"));

    // Several stale decisions share one history scan; the small brief points to the full list.
    s.write(".ai/kontext.toml", "[freshness]\nthreshold_commits = 2\n");
    for id in ["cache-a", "cache-b", "cache-c"] {
        s.write(&format!(".ai/decisions/{id}.md"), &original.replace("id: use-cache", &format!("id: {id}")));
    }
    let trace_path = s.root.join("git-trace.jsonl");
    s.ok_git(&["config", "log.showSignature", "true"]);
    let output = s
        .cmd(env!("CARGO_BIN_EXE_kontext"))
        .env("GIT_TRACE2_EVENT", &trace_path)
        .args(["brief", "--no-adapters", "--budget", "300"])
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let brief = String::from_utf8_lossy(&output.stdout);
    assert_eq!(brief.matches("may need a refresh").count(), 3, "{brief}");
    assert!(brief.contains("1 more freshness warnings: `kontext status`"), "{brief}");
    let trace = std::fs::read_to_string(trace_path).unwrap();
    let history_calls = trace
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .filter(|event| event["event"] == "start" && event["argv"].as_array().is_some_and(|args| args.iter().any(|a| a == "log")))
        .count();
    assert_eq!(history_calls, 1, "all decisions must share one git log call");
    assert_eq!(s.kontext(&["status"]).matches("may need a refresh").count(), 4);
}

#[test]
fn knowledge_freshness_without_commits() {
    let s = Sandbox::new("freshness-unborn");
    s.ok_git(&["init", "-q", "-b", "main"]);
    s.write(".ai/decisions/new.md", "---\nid: new\nkind: decision\ntitle: New\ndate: 2025-01-01\npaths: [src/**]\n---\n\nA new rule.\n");
    let brief = s.kontext(&["brief", "--no-adapters"]);
    assert!(!brief.contains("freshness") && !brief.contains("may need a refresh"), "{brief}");
}

#[test]
fn end_to_end() {
    let s = Sandbox::new("e2e");
    seed(&s);

    // before anyone sets kontext up, agents only read: no team captures, no bootstrap by accident
    let run = |args: &[&str]| {
        let o = s.cmd(env!("CARGO_BIN_EXE_kontext")).args(args).output().unwrap();
        format!("{}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr))
    };
    let refused = run(&["call", "ctx_capture", r#"{"kind":"decision","title":"Use cents","body":"Integer cents everywhere."}"#]);
    assert!(refused.contains("not set up in this repository"), "{refused}");
    let asked = run(&["call", "ctx_init", "{}"]);
    assert!(asked.contains("bootstrap=true") && !s.repo.join(".ai").exists(), "{asked}");
    let private =
        run(&["call", "ctx_capture", r#"{"kind":"learning","title":"Note to self","body":"Check VAT rounding.","visibility":"private"}"#]);
    assert!(private.contains("Saved privately"), "{private}");

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

    // the same task submitted twice records its learning once
    let module = r#"{"task_id":"mod:libs-pricing","summary":"Prices in integer cents.","overview":"One place for money math.","learnings":[{"title":"VAT is applied last","body":"Round the net amount first, then add VAT.","paths":["libs/pricing/**"]}]}"#;
    s.kontext(&["call", "ctx_init_submit", module]);
    s.kontext(&["call", "ctx_init_submit", module]);
    let learnings: Vec<_> = std::fs::read_dir(s.repo.join(".ai/learnings")).unwrap().flatten().map(|e| e.file_name()).collect();
    assert_eq!(learnings.len(), 1, "{learnings:?}");

    // while an autopilot run owns a task, another process cannot record it
    let mut owner = Command::new("sleep").arg("30").spawn().unwrap();
    let lock = s.repo.join(".git/kontext/autopilot.json");
    std::fs::write(&lock, format!(r#"{{"pid":{},"tasks":["mod:apps-api"]}}"#, owner.id())).unwrap();
    let o = s
        .cmd(env!("CARGO_BIN_EXE_kontext"))
        .args(["call", "ctx_init_submit", r#"{"task_id":"mod:apps-api","summary":"The HTTP API."}"#])
        .output()
        .unwrap();
    let refused = format!("{}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr));
    assert!(refused.contains("is being written by `kontext init --deepen`"), "{refused}");
    owner.kill().unwrap();
    let _ = owner.wait();
    std::fs::remove_file(&lock).unwrap();

    s.ok_git(&["checkout", "-q", "-b", "kontext/bootstrap"]);
    s.ok_git(&["add", ".ai"]);
    s.ok_git(&["commit", "-q", "-m", "docs: bootstrap team knowledge"]);
    let msg = s.ok_git(&["log", "-1", "--format=%B"]);
    assert!(!msg.contains("Learning:"), "entries mined by init get no trailers: {msg}");

    // a branch without the bootstrap is told where it is instead of bootstrapping again
    let bare = s.repo.with_file_name("e2e-bare");
    s.ok_git(&["worktree", "add", "-q", bare.to_str().unwrap(), "main"]);
    let brief = s.kontext(&["brief", "-C", bare.to_str().unwrap(), "--no-adapters"]);
    assert!(brief.contains("Team knowledge exists on `kontext/bootstrap`"), "{brief}");
    let init = s.kontext(&["call", "-C", bare.to_str().unwrap(), "ctx_init", "{}"]);
    assert!(init.contains("Nothing was bootstrapped") && !bare.join(".ai").exists(), "{init}");
    s.ok_git(&["worktree", "remove", "--force", bare.to_str().unwrap()]);

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

    // a decision captured without paths governs the change it is promoted with
    let cap = s.kontext(&[
        "capture",
        "--kind",
        "decision",
        "--title",
        "Carts expire after a day",
        "--body",
        "Abandoned carts are dropped after 24 hours to keep the store small.",
    ]);
    let pathless = cap.trim().rsplit(':').next().unwrap().trim().to_string();
    s.write("apps/api/src/cart.ts", "export class Cart { total() { return 0; } expires = 86400; }\n");
    s.kontext(&["prepare-commit", "--promote", &pathless]);
    let promoted = std::fs::read_dir(s.repo.join(".ai/decisions"))
        .unwrap()
        .flatten()
        .find(|f| f.file_name().to_string_lossy().contains("carts-expire"))
        .unwrap();
    let text = std::fs::read_to_string(promoted.path()).unwrap();
    assert!(text.contains("paths: [apps/api/src/cart.ts]"), "{text}");
    s.ok_git(&["add", "-A"]);
    s.ok_git(&["commit", "-q", "-m", "feat(cart): carts expire after a day"]);

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

#[test]
fn distill_threads() {
    let s = Sandbox::new("distill");
    seed(&s);
    s.kontext(&["init", "--no-hooks"]);
    // the choice holds for later runs: no hooks appear behind the user's back
    let again = s.kontext(&["init"]);
    assert!(again.contains("hooks skipped (this clone chose --no-hooks") && !s.repo.join(".git/hooks/pre-commit").exists(), "{again}");
    let home = s.root.join("home");
    let top = s.ok_git(&["rev-parse", "--show-toplevel"]).trim().to_string();

    // a Claude Code thread that ran in this repository
    let slug: String = top.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect();
    let dir = home.join(".claude/projects").join(&slug);
    std::fs::create_dir_all(&dir).unwrap();
    let id = "abcd1234-0000-4000-8000-000000000000";
    let session = [
        format!(
            r#"{{"type":"user","sessionId":"{id}","cwd":"{top}","gitBranch":"main","timestamp":"2026-09-28T10:00:00Z","message":{{"role":"user","content":"Round prices half-even: finance reconciles with the bank that way."}}}}"#
        ),
        format!(
            r#"{{"type":"assistant","message":{{"role":"assistant","content":[{{"type":"text","text":"Rounding is half-even now."}},{{"type":"tool_use","name":"Edit","input":{{"file_path":"{top}/libs/pricing/src/round.ts"}}}}]}}}}"#
        ),
        r#"{"type":"ai-title","aiTitle":"Half-even rounding"}"#.to_string(),
    ]
    .join("\n");
    std::fs::write(dir.join(format!("{id}.jsonl")), session).unwrap();

    // a model that finds one decision in whatever it reads
    let answer = s.root.join("answer.json");
    std::fs::write(
        &answer,
        r#"{"entries":[{"kind":"decision","title":"Round prices half-even","body":"Finance reconciles with the bank using half-even rounding.","paths":["libs/pricing/src/round.ts"]}]}"#,
    )
    .unwrap();
    std::fs::write(
        s.config.join("config.toml"),
        format!(
            "[adapters.fake-llm]\ndriver = \"command\"\n\n[adapters.fake-llm.ops.llm]\ncommand = [\"sh\", \"-c\", \"cat > /dev/null; cat '{}'\"]\nstdin = \"{{{{prompt}}}}\"\nformat = \"text\"\n",
            answer.display()
        ),
    )
    .unwrap();

    let run = |args: &[&str], workspace: Option<&str>| -> String {
        let mut c = s.cmd(env!("CARGO_BIN_EXE_kontext"));
        c.env("HOME", &home)
            .env("SUPERSET_HOME_DIR", home.join(".superset"))
            .env_remove("CLAUDE_CONFIG_DIR")
            .env_remove("CODEX_HOME")
            .env_remove("SUPERSET_ORGANIZATION_ID")
            .env_remove("SUPERSET_WORKSPACE_ID");
        if let Some(w) = workspace {
            c.env("SUPERSET_WORKSPACE_ID", w);
        }
        let o = c.args(args).output().unwrap();
        assert!(
            o.status.success(),
            "kontext {args:?} failed: {}{}",
            String::from_utf8_lossy(&o.stdout),
            String::from_utf8_lossy(&o.stderr)
        );
        String::from_utf8_lossy(&o.stdout).to_string()
    };

    let list = run(&["distill", "--list"], None);
    assert!(list.contains("claude:abcd1234") && list.contains("Half-even rounding"), "{list}");
    let out = run(&["distill", "--claude", "last", "--llm", "fake-llm"], None);
    assert!(out.contains("Round prices half-even") && out.contains("inbox:"), "{out}");
    let inbox = run(&["inbox"], None);
    assert!(inbox.contains("from claude:abcd1234"), "{inbox}");
    // once in the inbox, the same finding is not captured again
    let again = run(&["distill", "--claude", "abcd1234", "--llm", "fake-llm"], None);
    assert!(again.contains("Nothing durable found"), "{again}");
    // any text works as a thread, e.g. a copied chat
    s.write("chat.md", "Anna: let's drop the XML export, nobody uses it.\nBoris: agreed, JSON only from now on.\n");
    let text = run(&["distill", "chat.md", "--llm", "fake-llm", "--dry-run"], None);
    assert!(text.contains("text:chat"), "{text}");

    // a Superset workspace names its threads through the terminals that ran them
    if Command::new("sqlite3").arg("--version").output().is_ok_and(|o| o.status.success()) {
        let db_dir = home.join(".superset/host/org");
        std::fs::create_dir_all(&db_dir).unwrap();
        let sql = format!(
            "create table workspaces (id text primary key, name text, branch text, type text, worktree_path text, last_activity_at integer);\
             create table terminal_agent_bindings (terminal_id text, workspace_id text, agent_id text, agent_session_id text, transcript_path text, started_at integer);\
             insert into workspaces values ('ws1111aa-0000', 'Rounding fix', 'main', 'worktree', '{top}', 1);\
             insert into terminal_agent_bindings values ('t1', 'ws1111aa-0000', 'claude', '{id}', null, 1);"
        );
        let o = Command::new("sqlite3").arg(db_dir.join("host.db")).arg(&sql).output().unwrap();
        assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
        let ws = run(&["distill", "--superset", "ws1111", "--list"], None);
        assert!(ws.contains("Rounding fix") && ws.contains("claude:abcd1234"), "{ws}");
        // in a Superset terminal the current workspace is the default
        let current = run(&["distill", "--superset", "--list"], Some("ws1111aa-0000"));
        assert!(current.contains("Rounding fix"), "{current}");
    }
}
