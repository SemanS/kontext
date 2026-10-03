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
    assert!(!brief.contains("May need a refresh"), "one matching commit, despite multiple files and overlapping paths: {brief}");

    // Renaming out of a governed path still changes that path.
    s.ok_git(&["mv", "src/cache.rs", "cache.rs"]);
    commit("2025-01-03T12:00:00Z");
    let brief = s.kontext(&["brief", "--no-adapters", "--budget", "300"]);
    assert!(
        brief.contains("## State")
            && brief.contains("May need a refresh (2+ commits on their paths since they were made): [use-cache] Use cache (2 commits)"),
        "{brief}"
    );
    let status = s.kontext(&["status"]);
    assert!(
        status.contains("freshness 1 decision(s) may need a refresh") && status.contains("[use-cache] Use cache (2 commits)"),
        "{status}"
    );

    // Reviewing the decision and advancing its date resets the warning, even before commit.
    let decision_path = ".ai/decisions/use-cache.md";
    let original = std::fs::read_to_string(s.repo.join(decision_path)).unwrap();
    s.write(decision_path, &original.replace("date: 2025-01-01", "date: 2025-01-03"));
    assert!(!s.kontext(&["brief", "--no-adapters"]).contains("May need a refresh"));
    s.write(decision_path, &original);

    s.write(".ai/kontext.toml", "[freshness]\nthreshold_commits = 3\n");
    s.write("src/other.rs", "uncommitted change\n");
    assert!(!s.kontext(&["brief", "--no-adapters"]).contains("May need a refresh"));
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
    assert_eq!(brief.matches("(2 commits)").count(), 3, "{brief}");
    assert!(brief.contains("+1 more (`kontext status`)"), "a small brief still has its state: {brief}");
    let trace = std::fs::read_to_string(trace_path).unwrap();
    let history_calls = trace
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .filter(|event| event["event"] == "start" && event["argv"].as_array().is_some_and(|args| args.iter().any(|a| a == "log")))
        .count();
    assert_eq!(history_calls, 1, "all decisions must share one git log call");
    let status = s.kontext(&["status"]);
    assert!(status.contains("freshness 4 decision(s) may need a refresh"), "{status}");
    for id in ["cache-a", "cache-b", "cache-c", "use-cache"] {
        assert!(status.contains(&format!("[{id}]")), "{status}");
    }
}

#[test]
fn knowledge_freshness_without_commits() {
    let s = Sandbox::new("freshness-unborn");
    s.ok_git(&["init", "-q", "-b", "main"]);
    s.write(".ai/decisions/new.md", "---\nid: new\nkind: decision\ntitle: New\ndate: 2025-01-01\npaths: [src/**]\n---\n\nA new rule.\n");
    let brief = s.kontext(&["brief", "--no-adapters"]);
    assert!(!brief.contains("freshness") && !brief.contains("May need a refresh"), "{brief}");
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
fn stale_decisions() {
    let s = Sandbox::new("fresh");
    s.ok_git(&["init", "-q", "-b", "main"]);
    s.write(".ai/kontext.toml", "[freshness]\nthreshold_commits = 3\n");
    s.write(
        ".ai/decisions/2020-01-01-prices-are-integer-cents.md",
        "---\nid: 2020-01-01-prices-are-integer-cents\nkind: decision\nstatus: accepted\ndate: 2020-01-01\npaths:\n  - libs/pricing/**\n---\n# Prices are integer cents\n\nAll money is integer cents.\n",
    );
    s.write(
        ".ai/decisions/2020-01-01-routes-live-in-one-file.md",
        "---\nid: 2020-01-01-routes-live-in-one-file\nkind: decision\nstatus: accepted\ndate: 2020-01-01\npaths:\n  - apps/api/**\n---\n# Routes live in one file\n\nOne routes file.\n",
    );
    s.ok_git(&["add", "-A"]);
    s.ok_git(&["commit", "-q", "-m", "docs: decisions"]);
    let fresh = s.kontext(&["brief", "--no-adapters"]);
    assert!(!fresh.contains("May need a refresh"), "{fresh}");

    for i in 0..3 {
        s.write("libs/pricing/src/index.ts", &format!("export const v = {i};\n"));
        s.ok_git(&["add", "-A"]);
        s.ok_git(&["commit", "-q", "-m", &format!("feat(pricing): change {i}")]);
    }
    s.write("apps/api/src/index.ts", "export {};\n");
    s.ok_git(&["add", "-A"]);
    s.ok_git(&["commit", "-q", "-m", "feat(api): start"]);

    let brief = s.kontext(&["brief", "--no-adapters"]);
    assert!(brief.contains("May need a refresh (3+ commits on their paths since they were made): [2020-01-01-prices]"), "{brief}");
    assert!(brief.contains("Prices are integer cents (3 commits)") && !brief.contains("Routes live in one file (1"), "{brief}");
    // the second run is answered from the cache keyed by HEAD
    assert!(s.repo.join(".git/kontext/worktrees").exists());
    assert_eq!(s.kontext(&["brief", "--no-adapters"]), brief);
    let status = s.kontext(&["status"]);
    assert!(status.contains("freshness 1 decision(s) may need a refresh"), "{status}");

    s.write(".ai/kontext.toml", "[freshness]\nthreshold_commits = 0\n");
    assert!(!s.kontext(&["brief", "--no-adapters"]).contains("May need a refresh"));
    assert!(s.kontext(&["status"]).contains("freshness off"));
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

/// A repository that consumes another as a submodule, the way camp-bot and city-bot consume the
/// shared extractor: the team knowledge lives in the submodule, the agent's session in the outer one.
#[test]
fn knowledge_of_the_repository_that_owns_the_paths() {
    let s = Sandbox::new("nested");
    let inner = s.root.join("inner");
    std::fs::create_dir_all(inner.join(".ai/decisions")).unwrap();
    std::fs::create_dir_all(inner.join("src")).unwrap();
    std::fs::write(inner.join(".ai/kontext.toml"), "[project]\nname = \"lib\"\n").unwrap();
    std::fs::write(
        inner.join(".ai/decisions/2026-09-01-lib-amounts-are-integer-cents.md"),
        "---\nid: 2026-09-01-lib-amounts-are-integer-cents\nkind: decision\ntitle: Lib amounts are integer cents\nstatus: accepted\ndate: 2026-09-01\nsummary: Every amount in lib is an integer number of cents.\npaths: [src/**]\n---\n\n## Decision\n\nFloats broke rounding, so lib keeps integer cents.\n",
    )
    .unwrap();
    std::fs::write(inner.join("src/money.ts"), "export type Cents = number;\n").unwrap();
    let git_in = |dir: &Path, args: &[&str]| {
        let o = s.cmd("git").arg("-C").arg(dir).args(args).output().unwrap();
        assert!(o.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&o.stderr));
    };
    git_in(&inner, &["init", "-q", "-b", "main"]);
    git_in(&inner, &["add", "-A"]);
    git_in(&inner, &["commit", "-q", "-m", "feat: money in cents"]);

    s.ok_git(&["init", "-q", "-b", "main"]);
    s.write("README.md", "# Outer\n\nAn app that uses lib.\n");
    s.write("app/main.ts", "import { Cents } from '../lib/src/money';\n");
    s.ok_git(&["add", "-A"]);
    s.ok_git(&["commit", "-q", "-m", "feat: app"]);
    s.ok_git(&["-c", "protocol.file.allow=always", "submodule", "add", "-q", inner.to_str().unwrap(), "lib"]);
    s.ok_git(&["commit", "-q", "-m", "chore: add lib"]);
    assert!(!s.repo.join(".ai").exists() && s.repo.join("lib/.ai/decisions").is_dir());

    // the brief of a repository without knowledge of its own carries its submodule's
    let brief = s.kontext(&["brief", "--no-adapters"]);
    assert!(brief.contains("## lib/ — a nested repository") && brief.contains("Lib amounts are integer cents"), "{brief}");
    // focus inside the submodule: its brief answers, and its references resolve from here
    let brief = s.kontext(&["brief", "--no-adapters", "--focus", "lib/src"]);
    assert!(brief.contains("answered from lib at lib/") && brief.contains("[lib/2026-09-01-lib]"), "{brief}");
    assert!(!brief.contains("lib-amounts-are-integer-cents]"), "references are short: {brief}");
    // …also when the agent names it as the submodule's own AGENTS.md would, relative to the submodule
    let brief = s.kontext(&["brief", "--no-adapters", "--focus", "src/money.ts"]);
    assert!(brief.contains("answered from lib at lib/") && brief.contains("Lib amounts are integer cents"), "{brief}");
    let read = s.kontext(&["call", "ctx_read", r#"{"uri":"kx:lib/2026-09-01-lib"}"#]);
    assert!(read.contains("Floats broke rounding"), "{read}");
    let read = s.kontext(&["call", "ctx_read", r#"{"uri":"kx:2026-09-01-lib","level":0}"#]);
    assert!(read.contains("(entry of lib/)"), "an unprefixed id of a submodule entry: {read}");
    let search = s.kontext(&["search", "integer", "cents"]);
    assert!(search.contains("kx:lib/2026-09-01-lib-amounts-are-integer-cents") && search.contains("lib/]"), "{search}");
    let why = s.kontext(&["why", "lib/src/money.ts"]);
    assert!(why.contains("answered from lib") && why.contains("Lib amounts are integer cents") && why.contains("money in cents"), "{why}");

    // what an agent learns about the submodule's code lands in the submodule's inbox
    let cap = s.kontext(&[
        "call",
        "ctx_capture",
        r#"{"kind":"learning","title":"Rounding happens last","body":"Round once, after summing cents.","paths":["lib/src/money.ts","lib/src/rounding.ts"]}"#,
    ]);
    assert!(cap.contains("Captured in lib (lib/)") && cap.contains(r#"dir: "lib""#), "{cap}");
    let inbox = s.kontext(&["inbox", "-C", s.repo.join("lib").to_str().unwrap()]);
    assert!(inbox.contains("Rounding happens last"), "{inbox}");
    let entry = std::fs::read_dir(s.repo.join(".git/modules/lib/kontext/inbox")).unwrap().flatten().next().unwrap();
    let text = std::fs::read_to_string(entry.path()).unwrap();
    assert!(text.contains("paths: [src/money.ts, src/rounding.ts]"), "paths are rewritten for the submodule, new files too: {text}");
    let cap = s.kontext(&[
        "call",
        "ctx_capture",
        r#"{"kind":"learning","title":"Exports stream CSV","body":"Large exports stream rows instead of building a file.","paths":["reports/export.ts"],"visibility":"private"}"#,
    ]);
    assert!(cap.contains("not found in") && cap.contains("reports/export.ts"), "{cap}");

    // a private note in a repository without a store is found again
    let search = s.kontext(&["search", "stream", "csv", "exports"]);
    assert!(search.contains("Exports stream CSV") && search.contains("local note, private"), "{search}");
    let brief = s.kontext(&["brief", "--no-adapters"]);
    assert!(brief.contains("Local notes") && brief.contains("Exports stream CSV"), "{brief}");

    // `dir`, and absolute paths into another repository
    let brief = s.kontext(&["call", "ctx_brief", r#"{"dir":"lib"}"#]);
    assert!(brief.contains("answered from lib") && brief.contains("Lib amounts are integer cents"), "{brief}");
    let focus = format!(r#"{{"focus":["{}"]}}"#, inner.join("src").display());
    let brief = s.kontext(&["call", "ctx_brief", &focus]);
    assert!(brief.contains("answered from lib at") && brief.contains("Lib amounts are integer cents"), "{brief}");

    // the submodule's clone has no hooks: the report says so instead of promising trailers
    let id = inbox.split_whitespace().next().unwrap().to_string();
    let report = s.kontext(&["prepare-commit", "-C", s.repo.join("lib").to_str().unwrap(), "--promote", &id]);
    assert!(report.contains("Learning: ") && report.contains("hook is not installed in this clone"), "{report}");
    let brief = s.kontext(&["brief", "-C", s.repo.join("lib").to_str().unwrap(), "--no-adapters"]);
    assert!(brief.contains("Git hooks are not installed"), "{brief}");

    // `-C` names the repository, not the hooks directory
    let o = s
        .cmd(env!("CARGO_BIN_EXE_kontext"))
        .current_dir(&s.root)
        .args(["-C", s.repo.to_str().unwrap(), "hooks", "install"])
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    assert!(
        s.repo.join(".git/hooks/pre-commit").is_file() && !s.repo.join("pre-commit").exists(),
        "{}",
        String::from_utf8_lossy(&o.stdout)
    );

    // the MCP server tells an agent where the knowledge is
    let instructions = mcp_instructions(&s.repo, &s.config);
    assert!(instructions.contains("lib/ keeps team knowledge of its own") && instructions.contains("`dir`"), "{instructions}");
}

fn mcp_instructions(repo: &Path, config: &Path) -> String {
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
    writeln!(
        stdin,
        r#"{{"jsonrpc":"2.0","id":1,"method":"initialize","params":{{"protocolVersion":"2025-06-18","capabilities":{{}},"clientInfo":{{"name":"test","version":"0"}}}}}}"#
    )
    .unwrap();
    stdin.flush().unwrap();
    let mut line = String::new();
    BufReader::new(child.stdout.take().unwrap()).read_line(&mut line).unwrap();
    drop(stdin);
    let _ = child.wait();
    let v: serde_json::Value = serde_json::from_str(&line).unwrap();
    v["result"]["instructions"].as_str().unwrap_or("").to_string()
}

fn git_in(s: &Sandbox, dir: &Path, args: &[&str]) {
    let o = s.cmd("git").arg("-C").arg(dir).args(args).output().unwrap();
    assert!(o.status.success(), "git {args:?} in {}: {}", dir.display(), String::from_utf8_lossy(&o.stderr));
}

/// A repository with `.ai/` holding these decisions (id, title, paths) and one file per path root.
fn repo_with_store(s: &Sandbox, dir: &Path, name: &str, decisions: &[(&str, &str, &str)]) {
    std::fs::create_dir_all(dir.join(".ai/decisions")).unwrap();
    std::fs::write(dir.join(".ai/kontext.toml"), format!("[project]\nname = \"{name}\"\n")).unwrap();
    for (id, title, paths) in decisions {
        std::fs::write(
            dir.join(format!(".ai/decisions/{id}.md")),
            format!("---\nid: {id}\nkind: decision\ntitle: {title}\nstatus: accepted\ndate: {}\nsummary: {title}, for the reasons the body gives in some detail so that the line is long.\npaths: [{paths}]\n---\n\nBody.\n", &id[..10]),
        )
        .unwrap();
    }
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(dir.join("src/money.ts"), "export type Cents = number;\n").unwrap();
    git_in(s, dir, &["init", "-q", "-b", "main"]);
    git_in(s, dir, &["add", "-A"]);
    git_in(s, dir, &["commit", "-q", "-m", "feat: start"]);
}

#[test]
fn routing_edges() {
    let s = Sandbox::new("edges");
    let lib = s.root.join("lib-src");
    repo_with_store(&s, &lib, "lib", &[("2026-09-01-lib-amounts-are-integer-cents", "Lib amounts are integer cents", "src/**")]);
    let vend = s.root.join("vend-src");
    std::fs::create_dir_all(vend.join("src")).unwrap();
    std::fs::write(vend.join("src/x.ts"), "export const x = 1;\n").unwrap();
    git_in(&s, &vend, &["init", "-q", "-b", "main"]);
    git_in(&s, &vend, &["add", "-A"]);
    git_in(&s, &vend, &["commit", "-q", "-m", "feat: vendored"]);

    // an outer repository with plenty of knowledge of its own
    let outer: Vec<(String, String)> =
        (0..24).map(|i| (format!("2026-08-{:02}-outer-rule-number-{i}", i % 28 + 1), format!("Outer rule number {i}"))).collect();
    let refs: Vec<(&str, &str, &str)> = outer.iter().map(|(id, t)| (id.as_str(), t.as_str(), "app/**")).collect();
    repo_with_store(&s, &s.repo, "outer", &refs);
    s.write("app/main.ts", "export const app = 1;\n");
    s.ok_git(&["add", "-A"]);
    s.ok_git(&["commit", "-q", "-m", "feat: app"]);
    s.ok_git(&["-c", "protocol.file.allow=always", "submodule", "add", "-q", lib.to_str().unwrap(), "lib"]);
    s.ok_git(&["-c", "protocol.file.allow=always", "submodule", "add", "-q", vend.to_str().unwrap(), "vend"]);
    s.ok_git(&["commit", "-q", "-m", "chore: submodules"]);

    // focus in the submodule comes first, before the outer repository's unfocused knowledge
    let brief = s.kontext(&["brief", "--no-adapters", "--focus", "lib/src/money.ts"]);
    assert!(brief.contains("## lib/ — a nested repository") && brief.contains("Lib amounts are integer cents"), "{brief}");
    assert!(brief.contains("Outer rule number"), "the outer knowledge still follows: {brief}");
    // without focus there, the submodule is a pointer
    let brief = s.kontext(&["brief", "--no-adapters"]);
    assert!(brief.contains("- lib/ keeps its own team knowledge (1 decision)") && !brief.contains("Lib amounts are"), "{brief}");

    // a submodule without a store takes no captures: the outer repository keeps them
    let brief = s.kontext(&["brief", "--no-adapters", "--focus", "vend/src"]);
    assert!(!brief.contains("vend/ — a nested repository"), "{brief}");
    let cap = s.kontext(&[
        "call",
        "ctx_capture",
        r#"{"kind":"learning","title":"Vendored code is patched in place","body":"We patch vend directly.","paths":["vend/src/x.ts"]}"#,
    ]);
    assert!(cap.starts_with("Captured as inbox:") && cap.contains("vend/ keeps no team knowledge"), "{cap}");
    let mixed = s.kontext(&[
        "call",
        "ctx_capture",
        r#"{"kind":"learning","title":"Cents cross the app boundary","body":"The app keeps lib's cents.","paths":["lib/src/money.ts","app/main.ts"]}"#,
    ]);
    assert!(mixed.starts_with("Captured as inbox:") && mixed.contains("span more than one repository"), "{mixed}");
    // a file this repository is about to get keeps the capture here too
    let new_file = s.kontext(&[
        "call",
        "ctx_capture",
        r#"{"kind":"learning","title":"The new feature reads cents","body":"It takes lib's cents.","paths":["lib/src/money.ts","app/new_feature.ts"]}"#,
    ]);
    assert!(new_file.starts_with("Captured as inbox:") && new_file.contains("not found in outer: app/new_feature.ts"), "{new_file}");
    assert!(!cap.contains("with `dir: \"vend\"`") || cap.contains("a private note can go there"), "no advice that fails: {cap}");
    let kept: Vec<String> = std::fs::read_dir(s.repo.join(".git/kontext/inbox"))
        .unwrap()
        .flatten()
        .filter(|f| f.path().is_file())
        .map(|f| std::fs::read_to_string(f.path()).unwrap())
        .collect();
    assert!(kept.iter().any(|t| t.contains("paths: [vend/src/x.ts]")), "{kept:?}");
    assert!(kept.iter().any(|t| t.contains("paths: [lib/src/money.ts, app/main.ts]")), "{kept:?}");

    // `.gitmodules` cannot pull in a repository outside this one: an absolute path, a symlink
    std::os::unix::fs::symlink(&lib, s.repo.join("evil")).unwrap();
    let mut gm = std::fs::read_to_string(s.repo.join(".gitmodules")).unwrap();
    gm.push_str(&format!("[submodule \"abs\"]\n\tpath = {}\n[submodule \"evil\"]\n\tpath = evil\n", lib.display()));
    std::fs::write(s.repo.join(".gitmodules"), gm).unwrap();
    let search = s.kontext(&["search", "integer", "cents"]);
    assert!(search.contains("kx:lib/") && !search.contains("kx:evil/") && !search.contains("kx:/"), "{search}");
    s.ok_git(&["checkout", "-q", "--", ".gitmodules"]);
    std::fs::remove_file(s.repo.join("evil")).unwrap();

    // the old `hooks install --dir <hooks dir>` now names the repository: refused, with the new flag
    std::fs::create_dir_all(s.repo.join(".githooks")).unwrap();
    let o = s.cmd(env!("CARGO_BIN_EXE_kontext")).args(["hooks", "install", "--dir", ".githooks"]).output().unwrap();
    let err = String::from_utf8_lossy(&o.stderr);
    assert!(!o.status.success() && err.contains("--hooks-dir .githooks"), "{err}");

    // a store kept in docs/adr, its config committed, no overview: this branch is not "another branch"
    let adr = s.root.join("adr");
    std::fs::create_dir_all(adr.join("docs/adr")).unwrap();
    std::fs::create_dir_all(adr.join(".ai")).unwrap();
    std::fs::write(
        adr.join(".ai/kontext.toml"),
        "[project]\nname = \"adr\"\n\n[store.kinds.decision]\npath = \"docs/adr\"\nstyle = \"fields\"\nnumbering = \"sequential\"\n\n[hooks]\ntrailers = false\n",
    )
    .unwrap();
    std::fs::write(adr.join("docs/adr/0001-record-decisions.md"), "# 1. Record decisions\n\n**Status:** Accepted\n\nWe keep ADRs.\n")
        .unwrap();
    git_in(&s, &adr, &["init", "-q", "-b", "main"]);
    git_in(&s, &adr, &["add", "-A"]);
    git_in(&s, &adr, &["commit", "-q", "-m", "docs: adr"]);
    let a = adr.to_str().unwrap();
    let brief = s.kontext(&["brief", "-C", a, "--no-adapters"]);
    assert!(!brief.contains("Team knowledge exists on"), "{brief}");
    // …and a team that turned trailers off is not told to write them by hand
    let cap =
        s.kontext(&["capture", "-C", a, "--kind", "decision", "--title", "Use cents", "--paths", "src/**", "--body", "Integer cents."]);
    let id = cap.trim().rsplit(':').next().unwrap().trim().to_string();
    let report = s.kontext(&["prepare-commit", "-C", a, "--promote", &id]);
    assert!(report.contains("promoted") && !report.contains("Commit trailers"), "{report}");
}

/// A long-running server sees a submodule that was initialized after it started.
#[test]
fn a_running_server_sees_a_submodule_initialized_later() {
    let s = Sandbox::new("late");
    let lib = s.root.join("lib-src");
    repo_with_store(&s, &lib, "lib", &[("2026-09-01-lib-amounts-are-integer-cents", "Lib amounts are integer cents", "src/**")]);
    s.ok_git(&["init", "-q", "-b", "main"]);
    s.write("README.md", "# Outer\n");
    s.ok_git(&["add", "-A"]);
    s.ok_git(&["commit", "-q", "-m", "feat: outer"]);
    s.ok_git(&["-c", "protocol.file.allow=always", "submodule", "add", "-q", lib.to_str().unwrap(), "lib"]);
    s.ok_git(&["commit", "-q", "-m", "chore: lib"]);
    s.ok_git(&["submodule", "deinit", "-q", "-f", "lib"]);

    let mut child = Command::new(env!("CARGO_BIN_EXE_kontext"))
        .arg("mcp")
        .current_dir(&s.repo)
        .env("KONTEXT_CONFIG_DIR", &s.config)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let mut reader = BufReader::new(child.stdout.take().unwrap());
    let mut call = |id: i64, args: &str| -> String {
        writeln!(stdin, r#"{{"jsonrpc":"2.0","id":{id},"method":"tools/call","params":{{"name":"ctx_brief","arguments":{args}}}}}"#)
            .unwrap();
        stdin.flush().unwrap();
        loop {
            let mut line = String::new();
            assert!(reader.read_line(&mut line).unwrap() > 0, "server closed");
            let v: serde_json::Value = serde_json::from_str(&line).unwrap();
            if v["id"] == id {
                return v["result"]["content"][0]["text"].as_str().unwrap_or("").to_string();
            }
        }
    };
    let before = call(1, r#"{"dir":"lib"}"#);
    assert!(!before.contains("Lib amounts are integer cents"), "{before}");
    s.ok_git(&["-c", "protocol.file.allow=always", "submodule", "update", "-q", "--init"]);
    let after = call(2, r#"{"dir":"lib"}"#);
    assert!(after.contains("Lib amounts are integer cents"), "{after}");
    let outer = call(3, "{}");
    assert!(outer.contains("## lib/ — a nested repository"), "{outer}");
    drop(stdin);
    let _ = child.kill();
    let _ = child.wait();
}

/// Knowledge pushed by a teammate and fetched, not pulled; a worktree kept inside the main one.
#[test]
fn upstream_knowledge_and_inner_worktrees() {
    let s = Sandbox::new("upstream");
    let up = s.root.join("up");
    repo_with_store(&s, &up, "shop", &[("2026-09-01-prices-are-integer-cents", "Prices are integer cents", "src/**")]);
    // the clone's main still points at a commit before the knowledge arrived
    std::fs::remove_dir_all(&s.repo).unwrap();
    let o = s.cmd("git").current_dir(&s.root).args(["clone", "-q", up.to_str().unwrap(), "repo"]).output().unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    std::fs::write(s.repo.join("README.md"), "# Shop\n").unwrap();
    s.ok_git(&["checkout", "-q", "--orphan", "fresh"]);
    s.ok_git(&["rm", "-rq", "--cached", "."]);
    std::fs::remove_dir_all(s.repo.join(".ai")).unwrap();
    s.ok_git(&["add", "README.md"]);
    s.ok_git(&["commit", "-q", "-m", "chore: start"]);
    s.ok_git(&["branch", "-q", "-D", "main"]);
    s.ok_git(&["branch", "-q", "-m", "main"]);
    let brief = s.kontext(&["brief", "--no-adapters"]);
    assert!(brief.contains("Team knowledge exists on `origin/main`"), "{brief}");
    s.ok_git(&["remote", "set-head", "origin", "-d"]);
    let brief = s.kontext(&["brief", "--no-adapters"]);
    assert!(brief.contains("Team knowledge exists on `origin/main`"), "without origin/HEAD too: {brief}");

    // a worktree of the same clone kept inside the main one is another worktree, not a submodule
    s.ok_git(&["reset", "-q", "--hard", "origin/main"]);
    let inner = s.repo.join(".claude/worktrees/wt2");
    s.ok_git(&["worktree", "add", "-q", "-b", "wt2", inner.to_str().unwrap()]);
    let file = inner.join("src/money.ts");
    let why = s.kontext(&["why", file.to_str().unwrap()]);
    assert!(why.contains("answered from shop at .claude/worktrees/wt2/") && why.contains("Prices are integer cents"), "{why}");
    let cap = s.kontext(&[
        "call",
        "ctx_capture",
        &format!(r#"{{"kind":"learning","title":"Cents survive refunds","body":"Refunds stay in cents.","paths":["{}"]}}"#, file.display()),
    ]);
    assert!(cap.contains("Captured in shop (.claude/worktrees/wt2/)"), "{cap}");
    let theirs = s.kontext(&["prepare-commit", "-C", inner.to_str().unwrap()]);
    let ours = s.kontext(&["prepare-commit"]);
    std::fs::write(inner.join("src/money.ts"), "export type Cents = number; // refunds too\n").unwrap();
    let theirs_changed = s.kontext(&["prepare-commit", "-C", inner.to_str().unwrap()]);
    assert!(
        theirs_changed.contains("Cents survive refunds") && !ours.contains("## Inbox candidates"),
        "{theirs}\n---\n{ours}\n---\n{theirs_changed}"
    );
    let brief = s.kontext(&["brief", "--no-adapters"]);
    assert!(!brief.contains("a nested repository"), "{brief}");
}
