//! The MCP server over stdio, driven like a client would: both protocol eras, list order and
//! paging, structured results, the error codes, and the boundaries of the read tools.
//!
//! `Server::start(dir)` runs `kontext mcp` in `dir`; `request` sends one JSON-RPC request and
//! waits (at most 60 s) for its reply, keeping notifications that arrive meanwhile in `notes`.
use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{Receiver, channel};
use std::time::Duration;

struct Repo {
    root: PathBuf,
    repo: PathBuf,
    config: PathBuf,
}

impl Repo {
    /// A repository with a knowledge store holding one decision, committed.
    fn new(name: &str) -> Repo {
        let unique = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let root = std::env::temp_dir().join(format!("kontext-mcp-{name}-{}-{unique}", std::process::id()));
        let repo = root.join("repo");
        let config = root.join("config");
        std::fs::create_dir_all(repo.join(".ai/decisions")).unwrap();
        std::fs::create_dir_all(&config).unwrap();
        let r = Repo { root, repo, config };
        r.write(".ai/kontext.toml", "[project]\nname = \"shop\"\n");
        r.write(
            ".ai/decisions/2026-01-01-prices-are-integer-cents.md",
            "---\nid: 2026-01-01-prices-are-integer-cents\nkind: decision\ntitle: Prices are integer cents\nstatus: accepted\ndate: 2026-01-01\npaths: [src/**]\n---\n\nAll money is integer cents; never floats.\n",
        );
        r.write("src/money.ts", "export type Cents = number;\n");
        r.git(&["init", "-q", "-b", "main"]);
        r.git(&["add", "-A"]);
        r.git(&["commit", "-q", "-m", "feat: start"]);
        r
    }

    fn cmd(&self, program: &str) -> Command {
        let mut c = Command::new(program);
        c.current_dir(&self.repo)
            .env("KONTEXT_CONFIG_DIR", &self.config)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_AUTHOR_NAME", "Test")
            .env("GIT_AUTHOR_EMAIL", "test@example.com")
            .env("GIT_COMMITTER_NAME", "Test")
            .env("GIT_COMMITTER_EMAIL", "test@example.com")
            .env_remove("KONTEXT_SKIP");
        c
    }

    fn git(&self, args: &[&str]) {
        let o = self.cmd("git").args(args).output().unwrap();
        assert!(o.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&o.stderr));
    }

    fn write(&self, rel: &str, text: &str) {
        let p = self.repo.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, text).unwrap();
    }

    fn server(&self) -> Server {
        Server::start(&self.repo, &self.config, &[])
    }
}

impl Drop for Repo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

struct Server {
    child: Child,
    stdin: Option<ChildStdin>,
    lines: Receiver<Value>,
    /// notifications received while waiting for replies
    notes: Vec<Value>,
    next: i64,
}

impl Server {
    fn start(dir: &Path, config: &Path, env: &[(&str, &str)]) -> Server {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_kontext"));
        cmd.arg("mcp")
            .current_dir(dir)
            .env("KONTEXT_CONFIG_DIR", config)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        for (k, v) in env {
            cmd.env(k, v);
        }
        let mut child = cmd.spawn().unwrap();
        let stdin = child.stdin.take();
        let stdout = child.stdout.take().unwrap();
        let (tx, lines) = channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if let Ok(v) = serde_json::from_str::<Value>(&line)
                    && tx.send(v).is_err()
                {
                    break;
                }
            }
        });
        Server { child, stdin, lines, notes: Vec::new(), next: 1 }
    }

    fn send(&mut self, v: &Value) {
        let stdin = self.stdin.as_mut().expect("stdin open");
        writeln!(stdin, "{v}").unwrap();
        stdin.flush().unwrap();
    }

    /// The whole reply (result or error) to one request.
    fn request(&mut self, method: &str, params: Value) -> Value {
        let id = self.next;
        self.next += 1;
        self.send(&json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}));
        loop {
            let v = self.lines.recv_timeout(Duration::from_secs(60)).unwrap_or_else(|_| panic!("no reply to {method}"));
            if v["id"] == id {
                return v;
            }
            self.notes.push(v);
        }
    }

    fn initialize(&mut self, version: &str) -> Value {
        let r = self
            .request("initialize", json!({"protocolVersion": version, "capabilities": {}, "clientInfo": {"name": "test", "version": "0"}}));
        self.send(&json!({"jsonrpc": "2.0", "method": "notifications/initialized"}));
        r
    }

    fn call(&mut self, tool: &str, args: Value) -> Value {
        let r = self.request("tools/call", json!({"name": tool, "arguments": args}));
        assert!(r.get("error").is_none(), "{tool}: {r}");
        r["result"].clone()
    }

    fn text(result: &Value) -> String {
        result["content"][0]["text"].as_str().unwrap_or("").to_string()
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        drop(self.stdin.take());
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[test]
fn legacy_initialize_each_version() {
    let repo = Repo::new("legacy");
    for v in ["2024-11-05", "2025-03-26", "2025-06-18", "2025-11-25"] {
        let mut s = repo.server();
        let r = s.initialize(v);
        assert_eq!(r["result"]["protocolVersion"], v, "{r}");
        let caps = &r["result"]["capabilities"];
        assert!(caps.get("logging").is_none(), "no logging capability: {caps}");
        assert_eq!(caps["tools"]["listChanged"], false, "no adapter exposes tools: {caps}");
        assert_eq!(r["result"]["serverInfo"]["name"], "kontext");
        assert!(r["result"].get("resultType").is_none(), "legacy results keep their shape");
        // a legacy client may still set a log level
        assert!(s.request("logging/setLevel", json!({"level": "info"}))["result"].is_object());
    }
    // an older version this server does not know falls back to the newest legacy one
    let mut s = repo.server();
    assert_eq!(s.initialize("2024-01-01")["result"]["protocolVersion"], "2025-11-25");
}

#[test]
fn discover_then_call_without_initialize() {
    let repo = Repo::new("discover");
    let mut s = repo.server();
    let d = s.request("server/discover", json!({}));
    let r = &d["result"];
    assert!(r["supportedVersions"].as_array().unwrap().iter().any(|v| v == "2026-07-28"), "{d}");
    assert_eq!(r["serverInfo"]["name"], "kontext");
    assert!(r["instructions"].as_str().unwrap().chars().take(512).collect::<String>().contains("ctx_brief"));
    assert!(r["capabilities"]["tools"].is_object());
    assert_eq!(r["resultType"], "complete");
    let meta = json!({"io.modelcontextprotocol/protocolVersion": "2026-07-28",
        "io.modelcontextprotocol/clientInfo": {"name": "modern", "version": "1"}, "io.modelcontextprotocol/clientCapabilities": {}});
    let c = s.request("tools/call", json!({"name": "ctx_brief", "arguments": {"focus": ["src/money.ts"]}, "_meta": meta}));
    let res = &c["result"];
    assert!(Server::text(res).contains("Prices are integer cents"), "{c}");
    assert_eq!(res["resultType"], "complete");
    assert_eq!(res["_meta"]["io.modelcontextprotocol/serverInfo"]["name"], "kontext");
    let list = s.request("tools/list", json!({"_meta": meta}));
    assert_eq!(list["result"]["resultType"], "complete");
    // logging is gone on the modern path
    assert_eq!(s.request("logging/setLevel", json!({"level": "info", "_meta": meta}))["error"]["code"], -32601);
    // a missing resource is invalid params on the modern path, -32002 on the legacy one
    assert_eq!(s.request("resources/read", json!({"uri": "kontext://entry/nope", "_meta": meta}))["error"]["code"], -32602);
    assert_eq!(s.request("resources/read", json!({"uri": "kontext://entry/nope"}))["error"]["code"], -32002);
}

#[test]
fn unknown_version_returns_32022() {
    let repo = Repo::new("version");
    let mut s = repo.server();
    let r =
        s.request("initialize", json!({"protocolVersion": "2099-01-01", "capabilities": {}, "clientInfo": {"name": "t", "version": "0"}}));
    assert_eq!(r["error"]["code"], -32022, "{r}");
    assert!(r["error"]["data"]["supported"].as_array().unwrap().iter().any(|v| v == "2025-11-25"));
    let r = s.request("tools/list", json!({"_meta": {"io.modelcontextprotocol/protocolVersion": "1999-01-01"}}));
    assert_eq!(r["error"]["code"], -32022, "{r}");
}

#[test]
fn tools_list_order_is_stable() {
    let repo = Repo::new("order");
    let mut s = repo.server();
    s.initialize("2025-11-25");
    let names = |r: &Value| -> Vec<String> {
        r["result"]["tools"].as_array().unwrap().iter().map(|t| t["name"].as_str().unwrap().to_string()).collect()
    };
    let first = s.request("tools/list", json!({}));
    let second = s.request("tools/list", json!({}));
    assert_eq!(names(&first), names(&second));
    assert_eq!(names(&first)[0], "ctx_api");
    assert_eq!(first["result"]["cacheScope"], "private");
    assert!(first["result"]["ttlMs"].as_u64().unwrap() > 0);
    let prompts = s.request("prompts/list", json!({}));
    assert_eq!(prompts["result"]["prompts"][0]["name"], "kontext-init");
    let tools = first["result"]["tools"].as_array().unwrap();
    let brief = tools.iter().find(|t| t["name"] == "ctx_brief").unwrap();
    assert_eq!(brief["_meta"]["anthropic/alwaysLoad"], true);
    let size = serde_json::to_string(&first["result"]["tools"]).unwrap().len();
    assert!(size < 20_000, "tools/list is {size} chars");
}

#[test]
fn resources_list_pages_past_300() {
    let repo = Repo::new("pages");
    for i in 0..350 {
        repo.write(
            &format!(".ai/learnings/note-{i:03}.md"),
            &format!("---\nid: note-{i:03}\nkind: learning\ntitle: Note number {i}\ndate: 2026-01-01\n---\n\nBody {i}.\n"),
        );
    }
    let mut s = repo.server();
    s.initialize("2025-11-25");
    let mut uris = Vec::new();
    let mut cursor: Option<String> = None;
    for _ in 0..20 {
        let params = match &cursor {
            Some(c) => json!({"cursor": c}),
            None => json!({}),
        };
        let r = s.request("resources/list", params);
        for res in r["result"]["resources"].as_array().unwrap() {
            uris.push(res["uri"].as_str().unwrap().to_string());
        }
        cursor = r["result"]["nextCursor"].as_str().map(str::to_string);
        if cursor.is_none() {
            break;
        }
    }
    assert!(cursor.is_none(), "paging ends");
    assert_eq!(uris[0], "kontext://brief");
    assert_eq!(uris.iter().filter(|u| u.starts_with("kontext://entry/note-")).count(), 350);
    let unique: std::collections::HashSet<&String> = uris.iter().collect();
    assert_eq!(unique.len(), uris.len(), "no resource listed twice");
    assert_eq!(s.request("resources/list", json!({"cursor": "not-a-cursor"}))["error"]["code"], -32602);
}

#[test]
fn unknown_tool_is_rpc_error() {
    let repo = Repo::new("unknown");
    let mut s = repo.server();
    s.initialize("2025-11-25");
    for name in ["ctx_nope", "not_a_tool"] {
        let r = s.request("tools/call", json!({"name": name, "arguments": {}}));
        assert_eq!(r["error"]["code"], -32602, "{r}");
    }
    // a known tool with bad arguments stays a tool error the model can read
    let r = s.request("tools/call", json!({"name": "ctx_search", "arguments": {}}));
    assert_eq!(r["result"]["isError"], true, "{r}");
}

#[test]
fn panicking_request_gets_reply() {
    let repo = Repo::new("panic");
    let mut s = Server::start(&repo.repo, &repo.config, &[("KONTEXT_TEST_PANIC", "1")]);
    s.initialize("2025-11-25");
    let r = s.request("kontext/test_panic", json!({}));
    assert_eq!(r["error"]["code"], -32603, "{r}");
    // the server goes on, and its locks are usable
    assert!(s.request("ping", json!({}))["result"].is_object());
    assert!(Server::text(&s.call("ctx_brief", json!({}))).contains("Prices are integer cents"));
    // without the variable the method does not exist
    let mut plain = repo.server();
    assert_eq!(plain.request("kontext/test_panic", json!({}))["error"]["code"], -32601);
}

#[test]
fn structured_content_present_for_read_tools() {
    let repo = Repo::new("structured");
    let mut s = repo.server();
    s.initialize("2025-11-25");
    let id = "2026-01-01-prices-are-integer-cents";
    let brief = s.call("ctx_brief", json!({"focus": ["src/money.ts"]}));
    assert!(brief["structuredContent"]["text"].as_str().unwrap().contains("Prices are integer cents"), "{brief}");
    assert!(brief["structuredContent"]["entries"].as_array().unwrap().iter().any(|e| e == id), "{brief}");
    let search = s.call("ctx_search", json!({"query": "integer cents"}));
    assert!(search["structuredContent"]["hits"].as_array().unwrap().iter().any(|h| h["uri"] == format!("kx:{id}")), "{search}");
    assert!(
        search["content"].as_array().unwrap().iter().any(|c| c["type"] == "resource_link" && c["uri"] == format!("kontext://entry/{id}")),
        "{search}"
    );
    assert_eq!(search["content"][0]["type"], "text", "the text block stays first");
    let read = s.call("ctx_read", json!({"uri": format!("kx:{id}")}));
    assert_eq!(read["structuredContent"]["uri"], format!("kx:{id}"));
    assert_eq!(read["structuredContent"]["entry"], id);
    let why = s.call("ctx_why", json!({"target": "src/money.ts"}));
    assert!(why["structuredContent"]["text"].is_string(), "{why}");
    let inbox = s.call("ctx_inbox", json!({}));
    assert_eq!(inbox["structuredContent"]["action"], "list");
    let drift = s.call("ctx_drift", json!({"base": "HEAD"}));
    assert!(drift["structuredContent"]["entries"].is_array(), "{drift}");
    // the declared schemas say which fields are there
    let list = s.request("tools/list", json!({}));
    for name in ["ctx_brief", "ctx_search", "ctx_read", "ctx_why", "ctx_inbox", "ctx_drift"] {
        let tool = list["result"]["tools"].as_array().unwrap().iter().find(|t| t["name"] == name).unwrap();
        assert_eq!(tool["outputSchema"]["type"], "object", "{name}");
    }
}

#[test]
fn ctx_verify_annotations_are_destructive_and_open_world() {
    let repo = Repo::new("verify-hints");
    let mut s = repo.server();
    s.initialize("2025-11-25");
    let list = s.request("tools/list", json!({}));
    let verify = list["result"]["tools"].as_array().unwrap().iter().find(|t| t["name"] == "ctx_verify").unwrap().clone();
    assert_eq!(verify["annotations"]["destructiveHint"], true);
    assert_eq!(verify["annotations"]["openWorldHint"], true);
    assert_eq!(verify["annotations"]["readOnlyHint"], false);
    // its `dir` stays in this repository
    let elsewhere = Repo::new("verify-elsewhere");
    let r = s.call("ctx_verify", json!({"id": "2026-01-01-prices", "dir": elsewhere.repo.to_str().unwrap()}));
    assert_eq!(r["isError"], true, "{r}");
    assert!(Server::text(&r).contains("outside"), "{r}");
}

#[test]
fn ctx_threads_refuses_stdin() {
    let repo = Repo::new("threads");
    let mut s = repo.server();
    s.initialize("2025-11-25");
    let r = s.call("ctx_threads", json!({"thread": "-"}));
    assert_eq!(r["isError"], true, "{r}");
    assert!(Server::text(&r).contains("stdin"), "{r}");
    // a file outside this repository's transcripts is not a thread
    let r = s.call("ctx_threads", json!({"thread": repo.repo.join("src/money.ts").to_str().unwrap()}));
    assert_eq!(r["isError"], true, "{r}");
    // the server still reads its own input
    assert!(s.request("ping", json!({}))["result"].is_object());
}

#[test]
fn ctx_read_denies_git_internals() {
    let repo = Repo::new("git-internals");
    let mut s = repo.server();
    s.initialize("2025-11-25");
    for uri in ["file:.git/config", ".git/HEAD", "file:./.git/hooks/pre-commit", ".git-credentials", "file:.ssh/id_ed25519"] {
        let r = s.call("ctx_read", json!({"uri": uri}));
        assert_eq!(r["isError"], true, "{uri}: {r}");
    }
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(repo.repo.join(".git/config"), repo.repo.join("innocent.txt")).unwrap();
        let r = s.call("ctx_read", json!({"uri": "file:innocent.txt"}));
        assert_eq!(r["isError"], true, "a symlink into .git: {r}");
    }
    let ok = s.call("ctx_read", json!({"uri": "file:src/money.ts"}));
    assert!(Server::text(&ok).contains("Cents"), "{ok}");
}

#[test]
fn search_sees_entry_written_by_another_process() {
    let repo = Repo::new("other-process");
    let mut s = repo.server();
    s.initialize("2025-11-25");
    let before = s.call("ctx_search", json!({"query": "zebra quagga"}));
    assert!(!Server::text(&before).contains("Zebras are quaggas"), "{before}");
    repo.write(
        ".ai/learnings/zebras-are-quaggas.md",
        "---\nid: zebras-are-quaggas\nkind: learning\ntitle: Zebras are quaggas\ndate: 2026-01-02\n---\n\nThe zebra quagga rule.\n",
    );
    // another process indexes it first (the CLI), then the server answers
    let o = repo.cmd(env!("CARGO_BIN_EXE_kontext")).args(["search", "zebra", "quagga"]).output().unwrap();
    assert!(String::from_utf8_lossy(&o.stdout).contains("Zebras are quaggas"), "{}", String::from_utf8_lossy(&o.stdout));
    std::thread::sleep(Duration::from_millis(1_200));
    let after = s.call("ctx_search", json!({"query": "zebra quagga"}));
    assert!(Server::text(&after).contains("Zebras are quaggas"), "{after}");
}

#[test]
fn a_branch_switch_reaches_the_running_server() {
    let repo = Repo::new("branch");
    let mut s = repo.server();
    s.initialize("2025-11-25");
    s.call("ctx_brief", json!({}));
    repo.git(&["checkout", "-q", "-b", "feature/late-branch"]);
    let r = s.call("ctx_capture", json!({"kind": "learning", "title": "Late branch note", "body": "Captured after a checkout."}));
    assert!(r.get("isError").is_none() || r["isError"] == false, "{r}");
    let listed = Server::text(&s.call("ctx_inbox", json!({})));
    let id = listed.split('`').nth(1).unwrap_or("").to_string();
    let inbox = s.call("ctx_inbox", json!({"action": "show", "id": id}));
    assert!(Server::text(&inbox).contains("feature/late-branch"), "the capture records the current branch: {inbox}");
}

#[test]
fn an_invalid_config_edit_is_reported_once_and_in_the_brief() {
    let repo = Repo::new("bad-config");
    let mut s = repo.server();
    s.initialize("2025-11-25");
    assert!(!Server::text(&s.call("ctx_brief", json!({}))).starts_with("warning:"));
    repo.write(".ai/kontext.toml", "[project\nname = broken\n");
    let first = s.call("ctx_search", json!({"query": "cents"}));
    assert!(Server::text(&first).starts_with("warning:"), "{first}");
    let second = s.call("ctx_search", json!({"query": "cents"}));
    assert!(!Server::text(&second).starts_with("warning:"), "once per edit: {second}");
    let brief = s.call("ctx_brief", json!({}));
    assert!(Server::text(&brief).starts_with("warning:") && Server::text(&brief).contains("Prices are integer cents"), "{brief}");
}

#[test]
fn outside_a_repository_dir_still_works() {
    let repo = Repo::new("no-repo");
    let outside = repo.root.join("not-a-repo");
    std::fs::create_dir_all(&outside).unwrap();
    let mut s = Server::start(&outside, &repo.config, &[]);
    let init = s.initialize("2025-11-25");
    let instructions = init["result"]["instructions"].as_str().unwrap();
    assert!(instructions.contains("no git repository") && instructions.contains("`dir`"), "{instructions}");
    let r = s.call("ctx_brief", json!({"dir": repo.repo.to_str().unwrap()}));
    assert!(Server::text(&r).contains("Prices are integer cents"), "{r}");
    let without = s.call("ctx_brief", json!({}));
    assert_eq!(without["isError"], true);
}

impl Repo {
    /// `kontext api <operation>` in `dir` with `input`, the whole JSON answer.
    fn api_in(&self, dir: &Path, operation: &str, input: Value) -> Value {
        let o = self.cmd(env!("CARGO_BIN_EXE_kontext")).current_dir(dir).args(["api", operation, &input.to_string()]).output().unwrap();
        serde_json::from_slice(&o.stdout).unwrap_or_else(|_| panic!("{operation}: {}", String::from_utf8_lossy(&o.stderr)))
    }
}

#[test]
fn inbox_drop_refuses_other_worktree_candidate() {
    let repo = Repo::new("drop-other");
    let other = repo.root.join("other");
    repo.git(&["worktree", "add", "-q", "-b", "feat/other", other.to_str().unwrap()]);
    let captured = repo.api_in(
        &other,
        "drafts.capture",
        json!({"entry": {"kind": "learning", "title": "Retries need jitter", "body": "Retries without jitter stampede."}}),
    );
    let id = captured["result"]["candidate"]["id"].as_str().or_else(|| captured["result"]["id"].as_str()).unwrap().to_string();
    let mut s = repo.server();
    s.initialize("2025-11-25");
    let r = s.request("tools/call", json!({"name": "ctx_inbox", "arguments": {"action": "drop", "ids": [id]}}));
    let text = Server::text(&r["result"]);
    assert!(r["result"]["isError"] == true || r.get("error").is_some(), "{r}");
    assert!(text.contains("another active worktree") || r.to_string().contains("another active worktree"), "{r}");
    // the other worktree still has its candidate, and nothing was archived
    let pending = repo.api_in(&other, "inbox.list", json!({}));
    assert!(pending["result"]["entries"].as_array().unwrap().iter().any(|e| e["id"] == id.as_str()), "{pending}");
    let dismissed = repo.api_in(&repo.repo, "inbox.list", json!({"state": "dismissed"}));
    assert_eq!(dismissed["result"]["entries"], json!([]), "{dismissed}");
}

#[test]
fn own_drop_is_archived() {
    let repo = Repo::new("drop-own");
    let mut s = repo.server();
    s.initialize("2025-11-25");
    let said = Server::text(&s.call(
        "ctx_capture",
        json!({"kind": "learning", "title": "Retries need jitter", "body": "Retries without jitter stampede.", "paths": ["src/money.ts"]}),
    ));
    let pending = repo.api_in(&repo.repo, "inbox.list", json!({}));
    let id = pending["result"]["entries"][0]["id"].as_str().unwrap_or_else(|| panic!("{said}\n{pending}")).to_string();
    let r = s.call("ctx_inbox", json!({"action": "drop", "ids": [id], "reason": "Covered by the retry convention."}));
    assert!(Server::text(&r).contains("Dropped"), "{r}");
    assert_eq!(repo.api_in(&repo.repo, "inbox.list", json!({}))["result"]["entries"], json!([]));
    // dropped through inbox.drop: kept under .dismissed with when and why, never proposed again
    let dismissed = repo.api_in(&repo.repo, "inbox.list", json!({"state": "dismissed"}));
    let row = &dismissed["result"]["entries"][0];
    assert_eq!(row["id"], id.as_str(), "{dismissed}");
    assert_eq!(row["extra"]["dismissal_reason"], "Covered by the retry convention.", "{dismissed}");
    assert!(row["extra"]["dismissed_at"].is_string(), "{dismissed}");
}
