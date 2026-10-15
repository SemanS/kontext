//! Contract tests use the real binary, Git index and MCP transport in isolated repositories.
use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};

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
