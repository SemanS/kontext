//! Contract tests use the real binary, Git index and MCP transport in isolated repositories.
use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};

mod common;

static NEXT: AtomicUsize = AtomicUsize::new(0);

struct Repo(PathBuf);
impl Repo {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("kontext-api-it-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
        std::fs::create_dir_all(&root).unwrap();
        let repo = Repo(root);
        repo.git(&["init", "-q", "-b", "main"]);
        std::fs::write(repo.0.join("README.md"), "# Application API test\n").unwrap();
        repo.git(&["add", "README.md"]);
        repo.git(&["commit", "-qm", "seed"]);
        repo
    }
    fn command(&self, program: &str) -> Command {
        let mut cmd = Command::new(program);
        // no global git config, HOME or agent directories of the machine running the tests
        common::isolate(&mut cmd, &PathBuf::from(format!("{}-home", self.0.display())));
        cmd.current_dir(&self.0)
            .env("KONTEXT_CONFIG_DIR", self.0.join(".config"))
            .env("GIT_AUTHOR_NAME", "API Test")
            .env("GIT_AUTHOR_EMAIL", "test@example.invalid")
            .env("GIT_COMMITTER_NAME", "API Test")
            .env("GIT_COMMITTER_EMAIL", "test@example.invalid")
            .env("KONTEXT_SKIP", "1");
        cmd
    }
    fn git(&self, args: &[&str]) -> String {
        let output = self.command("git").args(args).output().unwrap();
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        String::from_utf8(output.stdout).unwrap()
    }
    fn api(&self, operation: &str, input: Value) -> Value {
        let mut child = self
            .command(env!("CARGO_BIN_EXE_kontext"))
            .args(["api", operation, "-"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child.stdin.take().unwrap().write_all(input.to_string().as_bytes()).unwrap();
        let output = child.wait_with_output().unwrap();
        let value: Value = serde_json::from_slice(&output.stdout).unwrap_or_else(|err| {
            panic!("{err}: stdout={} stderr={}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr))
        });
        assert_eq!(output.status.success(), value["ok"] == true, "{value}");
        assert_eq!(value["schema_version"], 1);
        value
    }
    fn ok(&self, operation: &str, input: Value) -> Value {
        let value = self.api(operation, input);
        assert_eq!(value["ok"], true, "{value}");
        value["result"].clone()
    }
    fn code(&self, operation: &str, input: Value) -> Value {
        let value = self.api(operation, input);
        assert_eq!(value["ok"], false, "{value}");
        value["error"]["code"].clone()
    }
    /// The CLI, as a person or an agent's shell runs it.
    fn cli(&self, args: &[&str]) -> std::process::Output {
        self.command(env!("CARGO_BIN_EXE_kontext")).args(args).output().unwrap()
    }
    fn write(&self, path: &str, text: &str) {
        let p = self.0.join(path);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, text).unwrap();
    }
    fn read(&self, path: &str) -> String {
        std::fs::read_to_string(self.0.join(path)).unwrap()
    }
    fn commit(&self, message: &str) -> String {
        self.git(&["add", "-A"]);
        self.git(&["commit", "-qm", message]);
        self.git(&["rev-parse", "HEAD"]).trim().to_string()
    }
    /// A learning staged and committed; returns its id and file.
    fn committed_learning(&self, title: &str, body: &str) -> (String, String) {
        let staged = self.ok(
            "drafts.stage",
            json!({"operation_id":format!("seed-{title}"),"entry":{"kind":"learning","title":title,"body":body,"paths":["src/relay/**"]}}),
        );
        self.commit("seed");
        (staged["entry"]["id"].as_str().unwrap().to_string(), staged["path"].as_str().unwrap().to_string())
    }
    /// Operation receipts of every worktree of this clone.
    fn receipts(&self) -> Vec<PathBuf> {
        let mut out = Vec::new();
        let Ok(worktrees) = std::fs::read_dir(self.0.join(".git/kontext/worktrees")) else { return out };
        for w in worktrees.flatten() {
            if let Ok(files) = std::fs::read_dir(w.path().join("operations")) {
                out.extend(files.flatten().map(|f| f.path()).filter(|p| p.extension().is_some_and(|e| e == "json")));
            }
        }
        out.sort();
        out
    }
    fn staged(&self) -> String {
        self.git(&["diff", "--cached", "--name-only"])
    }
}
impl Drop for Repo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn machine_errors_are_json_and_never_report_success() {
    let repo = Repo::new();
    assert_eq!(repo.api("missing", json!({}))["error"]["code"], "unsupported_operation");
    assert_eq!(repo.api("status", json!([]))["error"]["code"], "invalid_input");
    let output = repo.command(env!("CARGO_BIN_EXE_kontext")).args(["api", "status", "{"]).output().unwrap();
    assert!(!output.status.success());
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["error"]["code"], "invalid_input");
}

#[test]
fn stage_survives_client_restart_and_preserves_metadata_and_unrelated_index() {
    let repo = Repo::new();
    repo.ok("setup", json!({}));
    std::fs::write(repo.0.join("unrelated.txt"), "staged by someone else").unwrap();
    repo.git(&["add", "unrelated.txt"]);
    let input = json!({"operation_id":"external-candidate-42", "source":"external:task/42@v7", "entry":{
        "kind":"decision","title":"Use integer cents","body":"Amounts are integer cents to avoid rounding drift.",
        "paths":["src/pricing/**"],"tags":["billing"],"status":"proposed"
    }});
    let first = repo.ok("drafts.stage", input.clone());
    let replay = repo.ok("drafts.stage", input.clone());
    assert_eq!(first["path"], replay["path"]);
    assert_eq!(replay["replayed"], true);
    assert_eq!(first["entry"]["paths"], json!(["src/pricing/**"]));
    assert_eq!(first["entry"]["status"], "proposed");
    assert!(repo.git(&["diff", "--cached", "--name-only"]).lines().any(|path| path == "unrelated.txt"));
    let listed = repo.ok("entries.list", json!({"active_only":false}));
    assert_eq!(listed["entries"].as_array().unwrap().iter().filter(|e| e["title"] == "Use integer cents").count(), 1);
    let mut conflict = input;
    conflict["entry"]["body"] = json!("Different data under the same operation ID");
    assert_eq!(repo.api("drafts.stage", conflict)["ok"], false);
}

#[test]
fn ownership_routing_reports_target_and_rejects_malformed_or_split_paths() {
    let repo = Repo::new();
    let target = Repo::new();
    repo.ok("setup", json!({}));
    target.ok("setup", json!({}));
    std::fs::create_dir_all(target.0.join("src")).unwrap();
    let file = target.0.join("src/scoped.rs");
    std::fs::write(&file, "// target code\n").unwrap();
    let staged = repo.ok(
        "drafts.stage",
        json!({"operation_id":"owned-repository","entry":{
            "kind":"learning","title":"This belongs to the target","body":"Route to the owning repository.","paths":[file]
        }}),
    );
    assert_eq!(staged["repository"]["root"], json!(target.0.canonicalize().unwrap()));
    assert_eq!(staged["entry"]["paths"], json!(["src/scoped.rs"]));
    let malformed = repo.api(
        "drafts.stage",
        json!({"operation_id":"bad-path-types","entry":{
            "kind":"learning","title":"Invalid paths","body":"Do not silently drop invalid paths.","paths":[file,42]
        }}),
    );
    assert_eq!(malformed["error"]["code"], "invalid_input");
    let mixed = repo.api(
        "drafts.stage",
        json!({"operation_id":"split-repositories","entry":{
            "kind":"learning","title":"Split ownership","body":"Require explicit repository choice.","paths":[file,"README.md"]
        }}),
    );
    assert_eq!(mixed["error"]["code"], "repository_conflict");
}

#[test]
fn legacy_and_application_captures_share_cross_process_allocation() {
    let repo = Repo::new();
    repo.ok("setup", json!({}));
    let mut clients = Vec::new();
    for n in 0..8 {
        let mut command = repo.command(env!("CARGO_BIN_EXE_kontext"));
        if n % 2 == 0 {
            command.args(["capture", "--kind", "learning", "--title", "Concurrent capture", "--body", "Keep every finding."]);
        } else {
            command.args([
                "api",
                "drafts.capture",
                &json!({"operation_id":format!("capture-{n}"),"entry":{
                    "kind":"learning","title":"Concurrent capture","body":"Keep every finding."
                }})
                .to_string(),
            ]);
        }
        clients.push(command.stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap());
    }
    for client in clients {
        let output = client.wait_with_output().unwrap();
        assert!(output.status.success(), "{}{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
    }
    let entries = repo.ok("inbox.list", json!({}));
    assert_eq!(entries["entries"].as_array().unwrap().iter().filter(|entry| entry["title"] == "Concurrent capture").count(), 8);
}

#[test]
fn cli_and_mcp_have_the_same_unclipped_contract_and_directory_routing() {
    let repo = Repo::new();
    let target = Repo::new();
    target.ok("setup", json!({}));
    let body = "A long knowledge body. ".repeat(4000);
    let staged = target.ok(
        "drafts.stage",
        json!({"operation_id":"large-body","entry":{
            "kind":"learning","title":"Retain complete machine results","body":body,"paths":["src/**"]
        }}),
    );
    let id = staged["entry"]["id"].as_str().unwrap();
    let cli = target.api("entries.get", json!({"id":id}));
    let mut child = repo
        .command(env!("CARGO_BIN_EXE_kontext"))
        .arg("mcp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap());
    let requests = [
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"contract-test","version":"1"}}}),
        json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
        json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"ctx_api","arguments":{"operation":"entries.get","input":{"id":id},"dir":target.0}}}),
        json!({"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"ctx_api","arguments":{"operation":"missing"}}}),
    ];
    for request in requests {
        writeln!(stdin, "{request}").unwrap();
        stdin.flush().unwrap();
        let mut line = String::new();
        stdout.read_line(&mut line).unwrap();
        let response: Value = serde_json::from_str(&line).unwrap();
        match response["id"].as_u64().unwrap() {
            2 => assert!(
                response["result"]["tools"].as_array().unwrap().iter().any(|t| t["name"] == "ctx_api" && t["outputSchema"].is_object())
            ),
            3 => {
                assert_eq!(response["result"]["structuredContent"], cli);
                let text: Value = serde_json::from_str(response["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
                assert_eq!(text, cli);
                assert!(text["result"]["entry"]["body"].as_str().unwrap().len() > 60_000);
            }
            4 => {
                assert_eq!(response["result"]["isError"], true);
                assert_eq!(response["result"]["structuredContent"]["error"]["code"], "unsupported_operation");
            }
            _ => {}
        }
    }
    drop(stdin);
    assert!(child.wait().unwrap().success());
}

#[test]
fn replay_after_confirm_returns_original_result() {
    let repo = Repo::new();
    repo.ok("setup", json!({}));
    let input = json!({"operation_id":"learn-1","entry":{"kind":"learning","title":"Relay tests fake the node","body":"Relay tests run against a fake node.","paths":["src/relay/**"]}});
    let first = repo.ok("drafts.stage", input.clone());
    let id = first["entry"]["id"].as_str().unwrap().to_string();
    repo.commit("learning");
    let confirm = json!({"id":id,"operation_id":"confirm-1"});
    let confirmed = repo.ok("drafts.confirm", confirm.clone());
    repo.commit("confirm");
    let ledger = repo.read(".ai/confirmations.log");
    // a later legitimate change of the file does not turn the committed write into a conflict
    let again = repo.ok("drafts.stage", input);
    assert_eq!(again["replayed"], true, "{again}");
    assert_eq!(again["changed_since"], true, "{again}");
    assert_eq!(again["path"], first["path"]);
    assert_eq!(again["publication"], "committed");
    assert_eq!(repo.ok("operations.get", json!({"operation_id":"learn-1"}))["path"], first["path"]);
    // a retried confirmation answers with the first result and writes no second ledger line
    let retried = repo.ok("drafts.confirm", confirm);
    assert_eq!(retried["replayed"], true);
    assert_eq!(retried["checked"], confirmed["checked"]);
    assert_eq!(repo.read(".ai/confirmations.log"), ledger);
}

#[test]
fn routed_capture_operation_is_found() {
    let repo = Repo::new();
    let target = Repo::new();
    repo.ok("setup", json!({}));
    target.ok("setup", json!({}));
    std::fs::create_dir_all(target.0.join("src")).unwrap();
    let file = target.0.join("src/owned.rs");
    std::fs::write(&file, "// owned\n").unwrap();
    let input = json!({"operation_id":"routed-1","fingerprint":"fp-routed","entry":{"kind":"learning","title":"Owned elsewhere","body":"Goes to the owner.","paths":[file]}});
    let captured = repo.ok("drafts.capture", input.clone());
    let owner = json!(target.0.canonicalize().unwrap());
    assert_eq!(captured["repository"]["root"], owner);
    // a client that lost the result asks where it called, as the docs say
    let found = repo.ok("operations.get", json!({"operation_id":"routed-1","fingerprint":"fp-routed"}));
    assert_eq!(found["id"], captured["id"]);
    assert_eq!(found["replayed"], true);
    assert_eq!(found["repository"]["root"], owner);
    // the original request replays in the owner too
    let again = repo.ok("drafts.capture", input);
    assert_eq!(again["id"], captured["id"]);
    assert_eq!(again["replayed"], true);
    assert_eq!(repo.code("operations.get", json!({"operation_id":"never-used"})), "not_found");
}

#[test]
fn stale_revision_rejects_symbolic_ref() {
    let repo = Repo::new();
    repo.ok("setup", json!({}));
    let (id, _) = repo.committed_learning("Relay tests fake the node", "Relay tests run against a fake node.");
    for rev in ["HEAD", "HEAD~0", "main"] {
        let retire = json!({"operation_id":format!("retire-{rev}"),"entry_id":id,"entry_revision":rev,"reason":"x"});
        assert_eq!(repo.code("drafts.retire", retire), "invalid_input", "{rev}");
        assert_eq!(repo.code("drafts.extend", json!({"entry_id":id,"entry_revision":rev,"text":"x"})), "invalid_input", "{rev}");
    }
    let stale = repo.api(
        "drafts.retire",
        json!({"operation_id":"retire-stale","entry_id":id,"entry_revision":"0123456789012345678901234567890123456789","reason":"x"}),
    );
    assert_eq!(stale["error"]["code"], "stale_entry");
    assert_eq!(stale["error"]["aliases"], json!(["operation_conflict"]));
    // the revision entries.get gives is the one every write takes
    let revision = repo.ok("entries.get", json!({"id":id}))["entry"]["revision"].clone();
    assert!(revision.as_str().is_some_and(|r| r.len() == 40), "{revision}");
    assert_eq!(repo.ok("drafts.extend", json!({"entry_id":id,"entry_revision":revision,"text":"x"}))["as_written"], true);
}

#[test]
fn entries_changed_reads_committed_config() {
    let repo = Repo::new();
    repo.write(".kontext.toml", "[store.kinds.decision]\npath = 'docs/adr'\nstyle = 'fields'\n");
    repo.write("docs/adr/0001-use-a-queue.md", "# 0001 — Use a queue\n\n**Status:** accepted\n\nQueue the jobs.\n");
    let base = repo.commit("base");
    repo.write("docs/adr/0002-retry-twice.md", "# 0002 — Retry twice\n\n**Status:** proposed\n\nRetry twice.\n");
    let head = repo.commit("head");
    // a clone-local layer that moves the decisions elsewhere does not change what the revisions hold
    repo.write(".git/kontext/config.toml", "[store.kinds.decision]\npath = 'elsewhere'\n");
    let out = repo.ok("entries.changed", json!({"base":base,"head":head}));
    let changes = out["changes"].as_array().unwrap();
    assert_eq!(changes.len(), 1, "{out}");
    assert_eq!(changes[0]["id"], "0002-retry-twice");
    assert_eq!(changes[0]["change"], "added");
    assert_eq!(changes[0]["status"], "proposed");
    assert_eq!(changes[0]["revision"].as_str().unwrap(), repo.git(&["rev-parse", &format!("{head}:docs/adr/0002-retry-twice.md")]).trim());
}

#[test]
fn promote_matches_drafts_stage_bytes() {
    let entry = json!({"kind":"learning","title":"Relay tests fake the node","body":"Relay tests run against a fake node.","paths":["src/relay/**"],"tags":["testing"],"verify":"true"});
    let by_agent = Repo::new();
    by_agent.ok("setup", json!({}));
    let id = by_agent.ok("drafts.capture", json!({"entry":entry}))["id"].as_str().unwrap().to_string();
    let o = by_agent.cli(&["promote", &id]);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let by_application = Repo::new();
    by_application.ok("setup", json!({}));
    let id = by_application.ok("drafts.capture", json!({"entry":entry}))["id"].as_str().unwrap().to_string();
    let staged = by_application.ok("drafts.stage", json!({"operation_id":"stage-it","candidate_id":id}));
    let path = staged["path"].as_str().unwrap();
    assert!(by_agent.staged().lines().any(|p| p == path), "{}", by_agent.staged());
    assert_eq!(by_agent.read(path), by_application.read(path));
}

#[test]
fn promote_strips_api_operation() {
    let repo = Repo::new();
    repo.ok("setup", json!({}));
    let capture = json!({"operation_id":"cap-9","entry":{"kind":"learning","title":"Queue drops on restart","body":"The queue drops jobs on restart.","paths":["src/queue/**"]}});
    let captured = repo.ok("drafts.capture", capture.clone());
    let id = captured["id"].as_str().unwrap();
    let candidate = repo.ok("inbox.list", json!({}))["entries"][0].clone();
    assert!(candidate["extra"]["api_operation"].is_string(), "{candidate}");
    let out = repo.cli(&["prepare-commit", "--promote", id]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let path = repo.staged().lines().find(|p| p.starts_with(".ai/learnings/")).unwrap().to_string();
    let text = repo.read(&path);
    assert!(!text.contains("api_operation"), "{text}");
    assert!(!text.contains("worktree") && !text.contains("captured"), "{text}");
    // the capture retried after the promotion still answers with its candidate
    let again = repo.ok("drafts.capture", capture);
    assert_eq!(again["id"], captured["id"]);
    assert_eq!(again["replayed"], true);
}

#[test]
fn promote_refuses_dirty_superseded_file() {
    let repo = Repo::new();
    repo.ok("setup", json!({}));
    let old = repo.ok(
        "drafts.stage",
        json!({"operation_id":"old","entry":{"kind":"decision","title":"Use cron","body":"Cron runs the exports.","status":"accepted"}}),
    );
    let (old_id, old_path) = (old["entry"]["id"].as_str().unwrap().to_string(), old["path"].as_str().unwrap().to_string());
    repo.commit("old");
    let wip = format!("{}USER WIP EDIT\n", repo.read(&old_path));
    repo.write(&old_path, &wip);
    let capture = json!({"entry":{"kind":"decision","title":"Use the queue","body":"The queue runs the exports.","status":"accepted","supersedes":[old_id]}});
    let id = repo.ok("drafts.capture", capture)["id"].as_str().unwrap().to_string();
    for args in [vec!["promote", id.as_str()], vec!["prepare-commit", "--promote", id.as_str()]] {
        let out = repo.cli(&args);
        assert!(!out.status.success(), "{args:?}");
        assert!(String::from_utf8_lossy(&out.stderr).contains("local or staged changes"), "{}", String::from_utf8_lossy(&out.stderr));
    }
    assert_eq!(repo.read(&old_path), wip, "the user's edit stays, unmarked");
    assert!(repo.staged().trim().is_empty(), "{}", repo.staged());
    assert_eq!(repo.ok("inbox.list", json!({}))["entries"][0]["id"], id.as_str());
}

#[test]
fn promote_fails_on_unknown_supersedes() {
    let repo = Repo::new();
    repo.ok("setup", json!({}));
    let capture = json!({"entry":{"kind":"decision","title":"Use parquet","body":"Exports are Parquet.","status":"accepted","supersedes":["no-such-entry-xyz"]}});
    let id = repo.ok("drafts.capture", capture)["id"].as_str().unwrap().to_string();
    let out = repo.cli(&["promote", &id]);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("no-such-entry-xyz"), "{}", String::from_utf8_lossy(&out.stderr));
    assert!(repo.staged().trim().is_empty());
    let written = std::fs::read_dir(repo.0.join(".ai/decisions"))
        .map(|d| d.flatten().any(|f| f.file_name().to_string_lossy().contains("parquet")))
        .unwrap_or(false);
    assert!(!written, "no file is written");
    assert_eq!(repo.ok("inbox.list", json!({}))["entries"][0]["id"], id.as_str());
}

#[test]
fn prepare_commit_bad_id_changes_nothing() {
    let repo = Repo::new();
    repo.ok("setup", json!({}));
    let capture = |title: &str| {
        repo.ok("drafts.capture", json!({"entry":{"kind":"learning","title":title,"body":"Something learned.","paths":["src/**"]}}))["id"]
            .as_str()
            .unwrap()
            .to_string()
    };
    let (a, b) = (capture("First finding"), capture("Second finding"));
    let before = repo.staged();
    let out = repo.cli(&["prepare-commit", "--drop", &a, "--promote", &b, "--promote", "2020-01-01-no-such-candidate"]);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("nothing was changed"), "{}", String::from_utf8_lossy(&out.stderr));
    let ids: Vec<String> =
        repo.ok("inbox.list", json!({}))["entries"].as_array().unwrap().iter().map(|e| e["id"].as_str().unwrap().to_string()).collect();
    assert!(ids.contains(&a) && ids.contains(&b), "{ids:?}");
    assert_eq!(repo.staged(), before);
    let retire = repo.cli(&["prepare-commit", "--drop", &a, "--retire", "no-such-entry"]);
    assert!(!retire.status.success());
    assert!(repo.ok("inbox.list", json!({}))["entries"].as_array().unwrap().iter().any(|e| e["id"] == a.as_str()));
}

#[test]
fn stage_amend_refuses_rewritten_body() {
    let repo = Repo::new();
    repo.ok("setup", json!({}));
    let (id, path) = repo.committed_learning("Relay tests fake the node", "Relay tests run against a fake node.");
    let before = repo.read(&path);
    let revision = repo.ok("entries.get", json!({"id":id}))["entry"]["revision"].clone();
    let rewritten = repo.api(
        "drafts.stage",
        json!({"operation_id":"rewrite","amends":{"id":id,"revision":revision},"entry":{"body":"Relay tests run against a real node.\n\n## Added 2026-10-29\n\nMore."}}),
    );
    assert_eq!(rewritten["error"]["code"], "amend_not_additive", "{rewritten}");
    assert_eq!(repo.read(&path), before);
    assert!(repo.staged().trim().is_empty());
    let status = repo.ok("status", json!({}));
    assert!(status["capabilities"].as_array().unwrap().iter().any(|c| c == "drafts.stage.amends.additive"));
    // the draft drafts.extend returns is additive and stages
    let draft = repo.ok("drafts.extend", json!({"entry_id":id,"entry_revision":revision,"text":"Put the fake behind the node."}));
    let staged = repo.ok("drafts.stage", json!({"operation_id":"extend","amends":draft["amends"],"entry":draft["entry"]}));
    assert_eq!(staged["amended"], id.as_str());
    assert!(repo.read(&path).starts_with(before.trim_end()));
}

#[test]
fn amend_cannot_change_fields_of_fields_style_entry() {
    let repo = Repo::new();
    repo.ok("setup", json!({}));
    repo.write(".kontext.toml", "[store.kinds.learning]\npath = 'docs/learnings'\nstyle = 'fields'\n");
    let text = "<!-- markdownlint-disable -->\n# Relay tests fake the node\n\n**Status:** accepted\n**Paths:** src/relay/**\n\nRelay tests run against a fake node.\n";
    repo.write("docs/learnings/relay-tests.md", text);
    repo.commit("fields-style learning");
    let revision = repo.ok("entries.get", json!({"id":"relay-tests"}))["entry"]["revision"].clone();
    let inject = json!({"operation_id":"inject","amends":{"id":"relay-tests","revision":revision},
        "entry":{"body":"**Status:** deprecated\n\nRelay tests run against a fake node.\n\n## Added 2026-10-29\n\nMore."}});
    assert_eq!(repo.code("drafts.stage", inject), "amend_not_additive");
    assert_eq!(repo.read("docs/learnings/relay-tests.md"), text);
    let added = json!({"operation_id":"added","amends":{"id":"relay-tests","revision":revision},
        "entry":{"body":"Relay tests run against a fake node.\n\n## Added 2026-10-29\n\n**Status:** deprecated"}});
    let out = repo.ok("drafts.stage", added);
    assert_eq!(out["entry"]["title"], "Relay tests fake the node");
    assert_eq!(out["entry"]["status"], "accepted");
    assert_eq!(out["entry"]["paths"], json!(["src/relay/**"]));
    assert!(repo.read("docs/learnings/relay-tests.md").starts_with(text.trim_end()));
}

#[test]
fn extend_with_llm_appends_as_written() {
    let repo = Repo::new();
    repo.ok("setup", json!({}));
    let (id, path) = repo.committed_learning("Relay tests fake the node", "Relay tests run against a fake node.");
    let revision = repo.ok("entries.get", json!({"id":id}))["entry"]["revision"].clone();
    let before = repo.read(&path);
    let out = repo.ok("drafts.extend", json!({"entry_id":id,"entry_revision":revision,"text":"Put the fake behind the node.","llm":true}));
    assert_eq!(out["as_written"], true);
    assert!(out["notes"][0].as_str().unwrap().contains("2026-10-27"), "{out}");
    let body = out["entry"]["body"].as_str().unwrap();
    assert!(body.starts_with("Relay tests run against a fake node.\n\n## Added "), "{body}");
    assert!(body.ends_with("\n\nPut the fake behind the node.\n"), "{body}");
    assert_eq!(repo.read(&path), before, "extend writes nothing");
}

#[test]
fn resume_amend_plan_keeps_fields_style_kind() {
    let repo = Repo::new();
    repo.ok("setup", json!({}));
    repo.write(
        ".kontext.toml",
        "[store.kinds.decision]\npath = 'docs/adr'\nstyle = 'fields'\n[store.kinds.learning]\npath = 'docs/learnings'\nstyle = 'fields'\n",
    );
    repo.commit("fields-style store");
    // an interrupted stage of a new fields-style decision resumes from its plan
    let known = repo.receipts();
    let input = json!({"operation_id":"adr-1","fingerprint":"fp1","entry":{"kind":"decision","title":"Use widgets for gadgets","body":"Widgets.","status":"accepted"}});
    let first = repo.ok("drafts.stage", input);
    let path = first["path"].as_str().unwrap().to_string();
    assert!(path.starts_with("docs/adr/"), "{path}");
    let receipt = repo.receipts().into_iter().find(|r| !known.contains(r)).unwrap();
    let crash = |receipt: &PathBuf| {
        let mut journal: Value = serde_json::from_str(&std::fs::read_to_string(receipt).unwrap()).unwrap();
        journal["result"] = Value::Null;
        std::fs::write(receipt, journal.to_string()).unwrap();
    };
    crash(&receipt);
    repo.git(&["rm", "-q", "--cached", "--", &path]);
    std::fs::remove_file(repo.0.join(&path)).unwrap();
    let resumed = repo.ok("drafts.stage", json!({"operation_id":"adr-1","fingerprint":"fp1"}));
    assert_eq!(resumed["path"], path.as_str());
    assert_eq!(resumed["entry"]["kind"], "decision");
    assert!(repo.staged().lines().any(|p| p == path));
    // an interrupted amendment of a fields-style learning resumes too
    repo.write(
        "docs/learnings/relay-tests.md",
        "# Relay tests fake the node\n\n**Status:** accepted\n\nRelay tests run against a fake node.\n",
    );
    repo.commit("learning");
    let revision = repo.ok("entries.get", json!({"id":"relay-tests"}))["entry"]["revision"].clone();
    let known = repo.receipts();
    let amend = json!({"operation_id":"amend-1","amends":{"id":"relay-tests","revision":revision},
        "entry":{"body":"Relay tests run against a fake node.\n\n## Added 2026-10-29\n\nMore."}});
    let done = repo.ok("drafts.stage", amend.clone());
    let receipt = repo.receipts().into_iter().find(|r| !known.contains(r)).unwrap();
    crash(&receipt);
    repo.git(&["reset", "-q", "--", "docs/learnings/relay-tests.md"]);
    repo.git(&["checkout", "-q", "--", "docs/learnings/relay-tests.md"]);
    let resumed = repo.ok("drafts.stage", amend);
    assert_eq!(resumed["path"], done["path"]);
    assert_eq!(resumed["entry"]["kind"], "learning");
    assert!(repo.read("docs/learnings/relay-tests.md").contains("## Added 2026-10-29"));
}

#[test]
fn capture_keeps_extends_through_promote() {
    let repo = Repo::new();
    repo.ok("setup", json!({}));
    let decision = repo.ok(
        "drafts.stage",
        json!({"operation_id":"decision","entry":{"kind":"decision","title":"Workers only run jobs","body":"Only workers run jobs.","status":"accepted"}}),
    );
    let decision_id = decision["entry"]["id"].as_str().unwrap().to_string();
    repo.commit("decision");
    let args = json!({"kind":"learning","title":"Workers need a queue","body":"A worker without a queue idles.","paths":["src/workers/**"],"extends":[decision_id]});
    let said = repo.cli(&["call", "ctx_capture", &args.to_string()]);
    assert!(said.status.success(), "{}", String::from_utf8_lossy(&said.stderr));
    let candidate = repo.ok("inbox.list", json!({}))["entries"][0].clone();
    assert_eq!(candidate["extra"]["extends"], json!([decision_id]), "{candidate}");
    let id = candidate["id"].as_str().unwrap();
    let out = repo.cli(&["prepare-commit", "--promote", id]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let path = repo.staged().lines().find(|p| p.starts_with(".ai/learnings/")).unwrap().to_string();
    assert!(repo.read(&path).contains(&format!("extends: [{decision_id}]")), "{}", repo.read(&path));
    // an application's capture names only entries the store has
    let unknown = json!({"entry":{"kind":"learning","title":"Links nothing","body":"x","extends":["2020-01-01-no-such-entry"]}});
    assert_eq!(repo.code("drafts.capture", unknown), "unknown_entry");
}

#[test]
fn api_describe_lists_every_operation_input() {
    let repo = Repo::new();
    let all = repo.ok("api.describe", json!({}));
    let status = repo.ok("status", json!({}));
    let capabilities: Vec<&str> = status["capabilities"].as_array().unwrap().iter().filter_map(Value::as_str).collect();
    assert!(capabilities.contains(&"api.describe"));
    for operation in capabilities.iter().filter(|c| all["operations"].get(**c).is_some()) {
        assert_eq!(all["operations"][*operation]["input"]["type"], "object", "{operation}");
    }
    assert!(all["operations"]["drafts.stage"]["input"]["properties"]["amends"].is_object());
    assert!(all["operations"]["setup"]["input"]["properties"]["hooks"].is_object());
    let one = repo.ok("api.describe", json!({"operation":"drafts.extend"}));
    assert_eq!(one["input"]["required"], json!(["entry_id", "entry_revision", "text"]));
    assert_eq!(repo.code("api.describe", json!({"operation":"nope"})), "unsupported_operation");
}

#[test]
fn a_challenge_stays_resolving_until_its_resolution_lands() {
    let repo = Repo::new();
    repo.ok("setup", json!({}));
    let (id, path) = repo.committed_learning("Relay tests fake the node", "Relay tests run against a fake node.");
    let said = repo.cli(&["call", "ctx_challenge", &json!({"id":id,"why":"The relay tests use a real node now."}).to_string()]);
    assert!(said.status.success(), "{}", String::from_utf8_lossy(&said.stderr));
    let out = repo.cli(&["prepare-commit", "--retire", &id]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let listed = repo.ok("inbox.list", json!({}))["entries"].clone();
    assert_eq!(listed.as_array().unwrap().len(), 1, "{listed}");
    assert_eq!(listed[0]["resolving"], path.as_str(), "{listed}");
    let offer = String::from_utf8_lossy(&repo.cli(&["prepare-commit"]).stdout).to_string();
    assert!(offer.contains("(resolving"), "{offer}");
    repo.commit("retire");
    // the post-commit hook as git runs it (the other calls here skip hooks)
    let hook = repo.command(env!("CARGO_BIN_EXE_kontext")).args(["hook", "post-commit"]).env_remove("KONTEXT_SKIP").output().unwrap();
    assert!(hook.status.success(), "{}", String::from_utf8_lossy(&hook.stderr));
    assert_eq!(repo.ok("inbox.list", json!({}))["entries"], json!([]));
}
