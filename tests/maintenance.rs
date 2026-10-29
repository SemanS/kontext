//! Knowledge maintenance end to end: authority marks, review dates, evidence, changes since the
//! last brief, challenges and retirement, against the real binary in throwaway repositories.
use serde_json::{Value, json};
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT: AtomicUsize = AtomicUsize::new(0);

struct Repo(PathBuf);

impl Repo {
    fn new() -> Repo {
        let root = std::env::temp_dir().join(format!("kontext-maint-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join(".ai/decisions")).unwrap();
        let repo = Repo(root);
        repo.write(".ai/kontext.toml", "[project]\nname = \"maint\"\n");
        repo.write("src/lib.rs", "pub fn f() {}\n");
        repo.git(&["init", "-q", "-b", "main"]);
        repo
    }

    fn cmd(&self, program: &str) -> Command {
        let mut c = Command::new(program);
        c.current_dir(&self.0)
            .env("KONTEXT_CONFIG_DIR", self.0.join(".git/user-config"))
            .env("GIT_AUTHOR_NAME", "Test")
            .env("GIT_AUTHOR_EMAIL", "test@example.invalid")
            .env("GIT_COMMITTER_NAME", "Test")
            .env("GIT_COMMITTER_EMAIL", "test@example.invalid")
            .env("KONTEXT_SKIP", "1");
        c
    }

    fn git(&self, args: &[&str]) -> String {
        let o = self.cmd("git").args(args).output().unwrap();
        assert!(o.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&o.stderr));
        String::from_utf8_lossy(&o.stdout).trim().to_string()
    }

    fn commit(&self, message: &str) -> String {
        self.git(&["add", "-A"]);
        self.git(&["commit", "-qm", message]);
        self.git(&["rev-parse", "HEAD"])
    }

    fn write(&self, rel: &str, text: &str) {
        let p = self.0.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, text).unwrap();
    }

    fn read(&self, rel: &str) -> String {
        std::fs::read_to_string(self.0.join(rel)).unwrap()
    }

    fn kontext(&self, args: &[&str]) -> String {
        let o = self.cmd(env!("CARGO_BIN_EXE_kontext")).args(args).output().unwrap();
        assert!(o.status.success(), "kontext {args:?}: {}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr));
        String::from_utf8_lossy(&o.stdout).to_string()
    }

    fn api(&self, operation: &str, input: Value) -> Value {
        let o = self.cmd(env!("CARGO_BIN_EXE_kontext")).args(["api", operation, &input.to_string()]).output().unwrap();
        serde_json::from_slice(&o.stdout).unwrap_or_else(|e| panic!("{e}: {}", String::from_utf8_lossy(&o.stdout)))
    }

    fn ok(&self, operation: &str, input: Value) -> Value {
        let v = self.api(operation, input);
        assert_eq!(v["ok"], true, "{v}");
        v["result"].clone()
    }

    fn decision(&self, id: &str, title: &str, status: &str, extra: &str) {
        self.write(
            &format!(".ai/decisions/{id}.md"),
            &format!("---\nid: {id}\nkind: decision\ntitle: {title}\nstatus: {status}\ndate: 2026-01-10\npaths: [src/**]\n{extra}---\n\n## Decision\n\n{title}, for the record.\n"),
        );
    }
}

impl Drop for Repo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn brief_and_search_say_when_an_entry_is_not_accepted_or_not_approved() {
    let r = Repo::new();
    r.decision("2026-01-10-use-queues", "Use queues for exports", "proposed", "");
    r.decision("2026-01-10-use-postgres", "Use postgres for orders", "accepted", "");
    r.decision("2026-01-10-use-memcached", "Use memcached for sessions", "deprecated", "");
    r.commit("seed");
    let brief = r.kontext(&["brief", "--focus", "src", "--no-adapters"]);
    assert!(brief.contains("Use queues for exports (proposed) —"), "{brief}");
    assert!(brief.contains("Use postgres for orders —"), "{brief}");
    assert!(!brief.contains("(accepted)"), "{brief}");
    let search = r.kontext(&["search", "memcached sessions"]);
    assert!(search.contains("deprecated"), "{search}");
    let accepted = r.kontext(&["search", "postgres orders"]);
    assert!(!accepted.contains("accepted"), "accepted costs no tokens: {accepted}");
    // an edit nobody approved yet
    r.decision("2026-01-10-use-postgres", "Use postgres for orders", "accepted", "summary: now with a replica\n");
    let brief = r.kontext(&["brief", "--focus", "src", "--no-adapters"]);
    assert!(brief.contains("Use postgres for orders (local, unapproved)"), "{brief}");
    assert!(r.kontext(&["search", "postgres orders"]).contains("local, unapproved"));
}

#[test]
fn evidence_is_kept_as_references_with_checked_quotes() {
    let r = Repo::new();
    r.write("src/retry.rs", "// one key per attempt\npub fn retry() {}\n");
    let head = r.commit("seed");
    let input = json!({"operation_id":"ev","requested_path":".ai/decisions/retry-with-one-key.md","entry":{
        "kind":"decision","title":"Retry with one key","body":"Each attempt reuses its key.",
        "extra":{"external_refs":["ticket://7"]},
        "evidence":[{"ref":"task:shop-1"},{"ref":format!("code:maint@{head}:src/retry.rs#L1"),"quote":"one key per attempt"},
                    {"ref":format!("code:maint@{head}:src/retry.rs"),"quote":"two keys"},{"ref":"code:elsewhere@abc:x.rs","quote":"anything"}]}});
    let out = r.ok("drafts.stage", input);
    let checked: Vec<&str> = out["evidence"].as_array().unwrap().iter().map(|c| c["verification"].as_str().unwrap()).collect();
    assert_eq!(checked, vec!["none", "verified", "quote_not_found", "unverifiable"]);
    let refs = out["entry"]["extra"]["external_refs"].as_array().unwrap();
    assert_eq!(refs[0], "ticket://7", "plain references keep their place");
    assert_eq!(refs.len(), 5);
    let file = r.read(".ai/decisions/retry-with-one-key.md");
    assert!(file.contains("## Evidence"), "{file}");
    assert!(file.contains("\"one key per attempt\" (verified)"), "{file}");
    assert!(file.contains("\"two keys\" (quote not found at that revision)"), "{file}");
    assert!(out["notes"].to_string().contains("not at that revision"));
    let bad = r.api(
        "drafts.stage",
        json!({"operation_id":"bad","entry":{"kind":"learning","title":"T","body":"B","evidence":[{"ref":"someone@example.com"}]}}),
    );
    assert_eq!(bad["error"]["code"], "invalid_input", "{bad}");
    r.commit("evidence");
    assert!(r.kontext(&["brief", "--no-adapters"]).contains("Retry with one key (proposed; 5 sources)"));
    assert!(r.kontext(&["read", "kx:retry-with-one-key", "--level", "1"]).contains("sources (5): ticket://7, task:shop-1"));
}

#[test]
fn the_brief_says_what_changed_since_the_last_one() {
    let r = Repo::new();
    r.decision("2026-01-10-use-cron", "Use cron for exports", "accepted", "");
    r.commit("seed");
    // no cache yet: the last 14 days, which includes the seed commit
    let first = r.kontext(&["brief", "--no-adapters"]);
    assert!(first.contains("## Changed in the last 14 days\n- [2026-01-10-use] Use cron for exports (new)"), "{first}");
    let quiet = r.kontext(&["brief", "--no-adapters"]);
    assert!(!quiet.contains("## Changed"), "{quiet}");
    let out = r.ok("drafts.stage", json!({"operation_id":"q","requested_path":".ai/decisions/2026-02-01-use-a-queue.md","entry":{
        "kind":"decision","status":"accepted","title":"Use a queue for exports","body":"A queue replaces cron.","supersedes":["2026-01-10-use-cron"]}}));
    assert_eq!(out["related"], json!([".ai/decisions/2026-01-10-use-cron.md"]));
    let before = r.git(&["rev-parse", "--short=8", "HEAD"]);
    r.commit("queue");
    let second = r.kontext(&["brief", "--no-adapters"]);
    assert!(
        second.contains(&format!("## Changed since {before}\n- [2026-02-01-use] Use a queue for exports (new, supersedes [2026-01-10-use] Use cron for exports)\n")),
        "{second}"
    );
    assert!(!r.kontext(&["brief", "--no-adapters"]).contains("## Changed"));
    // a full budget drops the section first, and it stays due
    r.decision("2026-03-01-keep-csv", "Keep CSV as the export format", "accepted", "");
    for i in 0..8 {
        r.decision(
            &format!("2026-03-0{}-rule-{i}", i + 2),
            &format!("Rule number {i} about the nightly export pipeline, its retries, its alerting and its storage layout"),
            "accepted",
            "",
        );
    }
    r.commit("csv");
    let tight = r.kontext(&["brief", "--no-adapters", "--budget", "300"]);
    assert!(!tight.contains("## Changed"), "{tight}");
    assert!(r.kontext(&["brief", "--no-adapters"]).contains("Keep CSV as the export format (new)"));
}

#[test]
fn a_challenge_is_resolved_by_retiring_or_superseding_never_in_place() {
    let r = Repo::new();
    r.decision("2026-01-10-use-cron", "Use cron for exports", "accepted", "");
    r.decision("2026-01-10-use-csv", "Use CSV for exports", "accepted", "");
    r.write(".ai/conventions/log-ids.md", "---\nid: log-ids\nkind: convention\ntitle: Log request ids\n---\n\nLog the request id.\n");
    let head = r.commit("seed");
    let said = r.kontext(&["call", "ctx_challenge", r#"{"id":"2026-01-10-use-cron","why":"Exports run on the queue since September."}"#]);
    assert!(said.contains("inbox:"), "{said}");
    let original = r.read(".ai/decisions/2026-01-10-use-cron.md");
    assert!(r.kontext(&["brief", "--no-adapters"]).contains("Use cron for exports (challenged locally)"));
    let offer = r.kontext(&["prepare-commit"]);
    assert!(offer.contains("## Challenges (local)") && offer.contains("retire=[<entry id>]"), "{offer}");
    // a challenge is not a candidate to promote
    let inbox = r.ok("inbox.list", json!({}));
    let cid = inbox["entries"][0]["id"].as_str().unwrap().to_string();
    assert_eq!(inbox["entries"][0]["kind"], "challenge");
    assert_eq!(inbox["entries"][0]["extra"]["against"], "2026-01-10-use-cron");
    assert_eq!(inbox["entries"][0]["extra"]["against_revision"], head.as_str());
    let staged = r.api("drafts.stage", json!({"operation_id":"stage-challenge","candidate_id":cid}));
    assert_eq!(staged["error"]["code"], "invalid_input", "{staged}");
    let o = r.cmd(env!("CARGO_BIN_EXE_kontext")).args(["promote", &cid]).output().unwrap();
    assert!(!o.status.success() && String::from_utf8_lossy(&o.stderr).contains("is a challenge"));
    assert_eq!(r.read(".ai/decisions/2026-01-10-use-cron.md"), original, "nothing changed in place");
    let done = r.kontext(&["prepare-commit", "--retire", "2026-01-10-use-cron"]);
    assert!(done.contains("retired 2026-01-10-use-cron"), "{done}");
    let retired = r.read(".ai/decisions/2026-01-10-use-cron.md");
    assert!(
        retired.contains("status: deprecated") && retired.contains("## Retired") && retired.contains("on the queue since September"),
        "{retired}"
    );
    assert!(r.git(&["diff", "--cached", "--name-only"]).contains("2026-01-10-use-cron.md"));
    assert_eq!(r.ok("inbox.list", json!({}))["entries"], json!([]), "the challenge left with the change");
    r.commit("retire cron");

    // drafts.retire: a receipt, a replay, and the same refusal for a stale revision
    let input = json!({"operation_id":"retire-log","entry_id":"log-ids","entry_revision":head,"reason":"Tracing replaced it."});
    // an unrelated commit after the read does not make the revision stale
    r.write("unrelated.txt", "later commit");
    r.commit("unrelated");
    let first = r.ok("drafts.retire", input.clone());
    assert_eq!(first["entry"]["status"], "deprecated");
    assert_eq!(first["entry"]["publication"]["state"], "staged");
    let again = r.ok("drafts.retire", input);
    assert_eq!(again["replayed"], true);
    let stale = r.api("drafts.retire", json!({"operation_id":"retire-csv","entry_id":"2026-01-10-use-csv","entry_revision":"0123456789012345678901234567890123456789","reason":"x"}));
    assert_eq!(stale["error"]["code"], "operation_conflict");
    assert_eq!(r.api("drafts.retire", json!({"entry_id":"log-ids"}))["error"]["code"], "invalid_input", "operation_id is required");
    r.commit("retire log");
    // the entry's blob (publication.blob_hash) works as entry_revision too
    let blob = r.ok("entries.get", json!({"id":"2026-01-10-use-csv"}))["entry"]["publication"]["blob_hash"].clone();
    assert!(blob.is_string(), "{blob}");
    let by_blob = r.ok(
        "drafts.retire",
        json!({"operation_id":"retire-csv-blob","entry_id":"2026-01-10-use-csv","entry_revision":blob,"reason":"checked by blob"}),
    );
    assert_eq!(by_blob["entry"]["status"], "deprecated");
    r.git(&["reset", "-q", "--hard"]);

    // a replacement that supersedes the entry resolves the challenge as well
    r.kontext(&["challenge", "2026-01-10-use-csv", "Partners want Parquet."]);
    let staged = r.ok("drafts.stage", json!({"operation_id":"parquet","entry":{"kind":"decision","title":"Use Parquet for exports","body":"Partners load Parquet.","supersedes":["2026-01-10-use-csv"]}}));
    assert_eq!(staged["resolved_challenges"].as_array().unwrap().len(), 1, "{staged}");
    assert!(r.read(".ai/decisions/2026-01-10-use-csv.md").contains("status: superseded"));
}

/// A command adapter standing in for a model: answers with `.git/answer.json` and counts its calls.
fn fake_model(r: &Repo, answer: Value) {
    r.write(".git/answer.json", &answer.to_string());
    let root = r.0.display();
    r.write(
        ".git/kontext/config.toml",
        &format!(
            "[init]\nllm = \"fake\"\n[adapters.fake]\ndriver = \"command\"\n[adapters.fake.ops.llm]\ncommand = [\"sh\", \"-c\", \"cat >/dev/null; echo x >> {root}/.git/calls; cat {root}/.git/answer.json\"]\nstdin = \"{{{{prompt}}}}\"\nformat = \"text\"\n"
        ),
    );
}

fn calls(r: &Repo) -> usize {
    std::fs::read_to_string(r.0.join(".git/calls")).map(|s| s.lines().count()).unwrap_or(0)
}

#[test]
fn related_entries_ask_the_model_once_and_trust_only_shown_entries() {
    let r = Repo::new();
    r.decision("2026-01-10-round-half-even", "Round invoice lines half-even", "accepted", "");
    r.decision("2026-01-10-round-half-up", "Round receipt totals half-up", "accepted", "");
    let head = r.commit("seed");
    let text = "Invoices round only the grand total half-up";
    let plain = r.ok("entries.related", json!({"text":text,"paths":["src/lib.rs"]}));
    assert!(plain.get("relations").is_none(), "no model unless asked: {plain}");
    assert_eq!(plain["candidates"].as_array().unwrap().len(), 2);
    assert_eq!(plain["candidates"][0]["label"], "related, check");
    fake_model(
        &r,
        json!({"relations":[{"relation":"conflicts","id":"2026-01-10-round-half-even"},{"relation":"same","id":"kx:2026-01-10-round-half-up"},{"relation":"conflicts","id":"not-shown"}]}),
    );
    let asked = r.ok("entries.related", json!({"text":text,"paths":["src/lib.rs"],"llm":true}));
    assert_eq!(
        asked["relations"],
        json!([{"relation":"conflicts","id":"2026-01-10-round-half-even","label":"possible conflict, check","title":"Round invoice lines half-even"},
               {"relation":"same","id":"2026-01-10-round-half-up","label":"possibly the same, check","title":"Round receipt totals half-up"}])
    );
    assert_eq!(asked["model"]["asked"], true);
    // capture, formalize and a second question reuse the answer: one model call in all
    let again = r.ok("entries.related", json!({"text":text,"paths":["src/lib.rs"],"llm":true}));
    assert_eq!(again["model"]["cached"], true);
    let captured = r.kontext(&[
        "call",
        "ctx_capture",
        &json!({"kind":"decision","title":"Invoices round only the grand total half-up","body":"","summary":"x","paths":["src/lib.rs"]})
            .to_string(),
    ]);
    assert!(captured.contains("possible conflict, check: Round invoice lines half-even <kx:2026-01-10-round-half-even>"), "{captured}");
    assert_eq!(calls(&r), 1);
    // an answer about entries it was not shown is not trusted
    fake_model(&r, json!({"relations":[{"relation":"conflicts","id":"elsewhere"}]}));
    let other = r.ok("entries.related", json!({"text":"Round receipts with banker's rounding","llm":true}));
    assert_eq!(other["relations"], json!([{"relation":"uncertain","label":"uncertain, check"}]));

    // revise: a draft that supersedes the entry, never the old body back
    fake_model(
        &r,
        json!({"title":"Round invoice totals half-up","body":"Invoices round the grand total half-up, like receipts, since accounting sums unrounded lines."}),
    );
    let draft = r.ok(
        "drafts.revise",
        json!({"entry_id":"2026-01-10-round-half-even","entry_revision":head,"objections":["Accounting now sums unrounded lines."]}),
    );
    assert_eq!(draft["entry"]["supersedes"], json!(["2026-01-10-round-half-even"]));
    assert_eq!(draft["entry"]["status"], "proposed");
    assert_eq!(draft["entry"]["kind"], "decision");
    assert_eq!(draft["entry"]["paths"], json!(["src/**"]));
    assert!(r.git(&["status", "--porcelain"]).is_empty(), "revise writes nothing");
    fake_model(&r, json!({"title":"Round invoice lines half-even","body":"Round invoice lines half-even, for the record."}));
    let same = r.api("drafts.revise", json!({"entry_id":"2026-01-10-round-half-even","entry_revision":head,"objections":["x"]}));
    assert_eq!(same["error"]["code"], "invalid_model_output", "{same}");
    for bad in [json!([]), json!(["x".repeat(301)]), json!(vec!["x"; 21])] {
        let out = r.api("drafts.revise", json!({"entry_id":"2026-01-10-round-half-even","entry_revision":head,"objections":bad}));
        assert_eq!(out["error"]["code"], "invalid_input");
    }
    r.write(".git/kontext/config.toml", "");
    let none = r.api("drafts.revise", json!({"entry_id":"2026-01-10-round-half-even","entry_revision":head,"objections":["x"]}));
    assert_eq!(none["error"]["code"], "model_unavailable", "{none}");
    let status = r.ok("status", json!({}));
    for op in ["entries.related", "drafts.retire", "drafts.revise"] {
        assert!(status["capabilities"].as_array().unwrap().iter().any(|c| c == op), "{op}");
    }
}

/// Trust pins and the verify runner: a cloned repository's commands and adapters run only after a
/// person at a terminal pinned them, and the runner keeps secrets and time bounded.
mod trust_and_verify {
    use super::Repo;
    use serde_json::{Value, json};
    use std::io::{BufRead, BufReader, Write};
    use std::process::{Output, Stdio};
    use std::time::{Duration, Instant};

    /// A remote with credentials in it: gives the repository a known slug and tests that templates never see them.
    fn remote() -> String {
        format!("https://x-access-token:{}@example.invalid/acme/maint.git", "ghp_remotesecret123")
    }
    const SLUG: &str = "example.invalid-acme-maint";

    fn repo() -> Repo {
        let r = Repo::new();
        r.git(&["remote", "add", "origin", remote().as_str()]);
        r
    }

    fn sha256_hex(text: &str) -> String {
        use sha2::{Digest, Sha256};
        Sha256::digest(text.as_bytes()).iter().map(|b| format!("{b:02x}")).collect()
    }

    fn trust_file(r: &Repo) -> std::path::PathBuf {
        r.0.join(".git/user-config/trust.toml")
    }

    /// Pin a verify command the way `kontext trust --verify` records it.
    fn pin(r: &Repo, id: &str, command: &str) {
        let rel = format!(".ai/decisions/{id}.md");
        let key = sha256_hex(&format!("{rel}\0{command}"));
        let mut text = std::fs::read_to_string(trust_file(r)).unwrap_or_else(|_| "pin_version = 2\n".into());
        text.push_str(&format!(
            "\n[repos.\"{SLUG}\".verify.{key}]\nid = \"{id}\"\npath = \"{rel}\"\ncommand = {}\npinned_at = \"2026-10-29T00:00:00Z\"\n",
            toml_str(command)
        ));
        r.write(".git/user-config/trust.toml", &text);
    }

    /// A TOML basic string (a JSON string is one).
    fn toml_str(s: &str) -> String {
        serde_json::to_string(s).unwrap()
    }

    fn yaml_str(s: &str) -> String {
        format!("'{}'", s.replace('\'', "''"))
    }

    fn entry_with(r: &Repo, id: &str, command: &str) {
        r.decision(id, "Checked by a command", "accepted", &format!("verify: {}\n", yaml_str(command)));
    }

    fn kontext_raw(r: &Repo, args: &[&str]) -> Output {
        r.cmd(env!("CARGO_BIN_EXE_kontext")).args(args).stdin(Stdio::null()).output().unwrap()
    }

    /// Run kontext under a pseudo-terminal (`script`), as a person at a terminal would.
    fn kontext_tty(r: &Repo, args: &[&str]) -> Output {
        let bin = env!("CARGO_BIN_EXE_kontext");
        let mut c = r.cmd("script");
        if cfg!(target_os = "macos") {
            c.args(["-q", "/dev/null", bin]).args(args);
        } else {
            let line = std::iter::once(bin).chain(args.iter().copied()).map(|a| format!("'{a}'")).collect::<Vec<_>>().join(" ");
            c.args(["-qec", &line, "/dev/null"]);
        }
        c.stdin(Stdio::null()).output().unwrap()
    }

    fn mcp_verify(r: &Repo, id: &str) -> Value {
        let mut child = r
            .cmd(env!("CARGO_BIN_EXE_kontext"))
            .arg("mcp")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let mut stdin = child.stdin.take().unwrap();
        let call = json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"ctx_verify","arguments":{"id":id}}});
        for line in [
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"test","version":"0"}}}"#.to_string(),
            r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#.to_string(),
            call.to_string(),
        ] {
            writeln!(stdin, "{line}").unwrap();
        }
        stdin.flush().unwrap();
        let mut reader = BufReader::new(child.stdout.take().unwrap());
        let mut reply = Value::Null;
        loop {
            let mut line = String::new();
            if reader.read_line(&mut line).unwrap() == 0 {
                break;
            }
            let v: Value = serde_json::from_str(&line).unwrap();
            if v["id"] == 2 {
                reply = v;
                break;
            }
        }
        drop(stdin);
        let _ = child.wait();
        reply
    }

    #[test]
    fn verify_refuses_unpinned_command() {
        let r = repo();
        let command = "touch .git/pwned";
        entry_with(&r, "2026-01-10-checked", command);
        r.commit("seed");
        let marker = r.0.join(".git/pwned");

        // an agent over MCP gets the command back, nothing runs
        let reply = mcp_verify(&r, "2026-01-10-checked");
        assert!(reply.to_string().contains(command), "the command is shown: {reply}");
        assert!(!marker.exists(), "ctx_verify ran an unpinned command");

        // an application gets untrusted_verify with what a person needs to decide
        let out = r.api("drafts.verify", json!({"id":"2026-01-10-checked"}));
        assert_eq!(out["ok"], false, "{out}");
        assert_eq!(out["error"]["code"], "untrusted_verify", "{out}");
        assert_eq!(out["error"]["id"], "2026-01-10-checked");
        assert_eq!(out["error"]["command"], command);
        assert_eq!(out["error"]["command_sha256"], sha256_hex(&format!(".ai/decisions/2026-01-10-checked.md\0{command}")));
        assert_eq!(out["error"]["hint"], "kontext trust --verify 2026-01-10-checked");
        assert!(!marker.exists(), "drafts.verify ran an unpinned command");

        // the terminal command without a terminal refuses too
        let cli = kontext_raw(&r, &["verify", "2026-01-10-checked"]);
        assert!(!cli.status.success());
        let err = String::from_utf8_lossy(&cli.stderr);
        assert!(err.contains("kontext trust --verify 2026-01-10-checked") && err.contains(command), "{err}");
        assert!(!marker.exists(), "kontext verify ran an unpinned command");
        assert!(!r.0.join(".ai/confirmations.log").exists(), "nothing was recorded");

        // pinned by a person: it runs
        pin(&r, "2026-01-10-checked", command);
        let ran = r.ok("drafts.verify", json!({"id":"2026-01-10-checked"}));
        assert_eq!((ran["ran"].clone(), ran["passed"].clone()), (json!(true), json!(true)), "{ran}");
        assert!(marker.exists());
        let trust = std::fs::read_to_string(trust_file(&r)).unwrap();
        assert!(trust.contains("pin_version = 2"), "{trust}");
    }

    #[test]
    fn verify_pin_revoked_when_command_changes() {
        let r = repo();
        entry_with(&r, "2026-01-10-checked", "touch .git/first");
        r.commit("seed");
        pin(&r, "2026-01-10-checked", "touch .git/first");
        assert_eq!(r.ok("drafts.verify", json!({"id":"2026-01-10-checked"}))["passed"], true);
        entry_with(&r, "2026-01-10-checked", "touch .git/second");
        r.commit("change the command");
        let out = r.api("drafts.verify", json!({"id":"2026-01-10-checked"}));
        assert_eq!(out["error"]["code"], "untrusted_verify", "an edited command needs a new pin: {out}");
        assert_eq!(out["error"]["command"], "touch .git/second");
        assert!(!r.0.join(".git/second").exists());
    }

    #[test]
    fn verify_scrubs_environment() {
        let r = repo();
        let command = r#"env | grep -c TOKEN; echo "pass=$KX_PASS""#;
        entry_with(&r, "2026-01-10-env", command);
        // a repository cannot widen what the command sees; the user can
        r.write(".ai/kontext.toml", "[project]\nname = \"maint\"\n\n[verify]\npass_env = [\"GITHUB_TOKEN\"]\n");
        r.write(".git/user-config/config.toml", "[verify]\npass_env = [\"KX_PASS\"]\n");
        r.commit("seed");
        pin(&r, "2026-01-10-env", command);
        let o = r
            .cmd(env!("CARGO_BIN_EXE_kontext"))
            .args(["api", "drafts.verify", r#"{"id":"2026-01-10-env"}"#])
            .env("GITHUB_TOKEN", "ghp_not_for_the_command")
            .env("MY_TOKEN", "nope")
            .env("KX_PASS", "yes")
            .output()
            .unwrap();
        let v: Value = serde_json::from_slice(&o.stdout).unwrap();
        assert_eq!(v["result"]["output_tail"], "0\npass=yes\n", "{v}");
    }

    #[test]
    fn verify_returns_when_background_child_holds_stdout() {
        let r = repo();
        let command = "(sleep 300 &); echo ok";
        entry_with(&r, "2026-01-10-bg", command);
        r.commit("seed");
        pin(&r, "2026-01-10-bg", command);
        let started = Instant::now();
        let o = r
            .cmd(env!("CARGO_BIN_EXE_kontext"))
            .args(["api", "drafts.verify", r#"{"id":"2026-01-10-bg"}"#])
            .env("KONTEXT_VERIFY_TIMEOUT_SECS", "5")
            .output()
            .unwrap();
        assert!(started.elapsed() < Duration::from_secs(8), "held by the background child: {:?}", started.elapsed());
        let v: Value = serde_json::from_slice(&o.stdout).unwrap();
        assert_eq!(v["result"]["passed"], true, "{v}");
        assert!(v["result"]["output_tail"].as_str().unwrap().starts_with("ok\n"), "{v}");
    }

    #[test]
    fn verify_output_tail_is_redacted() {
        let r = repo();
        let command = "echo token ghp_$(echo a1B2c3D4e5F6g7H8i9J0k1L2m3N4o5P6q7R8)";
        entry_with(&r, "2026-01-10-leaky", command);
        r.commit("seed");
        pin(&r, "2026-01-10-leaky", command);
        let out = r.ok("drafts.verify", json!({"id":"2026-01-10-leaky"}));
        let tail = out["output_tail"].as_str().unwrap();
        assert!(tail.starts_with("token "), "{tail}");
        assert!(!tail.contains("a1B2c3D4e5F6g7H8i9J0k1L2m3N4o5P6q7R8"), "{tail}");
    }

    #[test]
    fn verify_on_dirty_governed_path_is_marked() {
        let r = repo();
        let command = "grep -q MAGIC src/lib.rs";
        entry_with(&r, "2026-01-10-magic", command);
        let head = r.commit("seed");
        pin(&r, "2026-01-10-magic", command);
        r.write("src/lib.rs", "pub fn f() {} // MAGIC\n");
        let out = r.ok("drafts.verify", json!({"id":"2026-01-10-magic"}));
        assert_eq!(out["passed"], true);
        assert_eq!(out["commit"], head.as_str());
        assert_eq!(out["dirty"], true, "{out}");
        assert_eq!(out["dirty_paths"], json!(["src/lib.rs"]));
        let ledger = r.read(".ai/confirmations.log");
        assert!(ledger.contains("[dirty: src/lib.rs]"), "{ledger}");
        // over MCP a dirty tree is the agent's to check: the command comes back, nothing is recorded
        r.git(&["rm", "-q", "--cached", ".ai/confirmations.log"]);
        std::fs::remove_file(r.0.join(".ai/confirmations.log")).unwrap();
        let reply = mcp_verify(&r, "2026-01-10-magic");
        assert!(reply.to_string().contains(command), "{reply}");
        assert!(!r.0.join(".ai/confirmations.log").exists(), "ctx_verify ran on a dirty tree: {reply}");
    }

    const ADAPTER: &str = r#"[project]
name = "maint"

[adapters.shared]
driver = "command"

[adapters.shared.ops.search]
command = ["sh", "-c", "echo WORD >> ROOT/.git/shared-ran"]
format = "text"
"#;

    fn shared_adapter(r: &Repo, word: &str) {
        r.write(".ai/kontext.toml", &ADAPTER.replace("ROOT", &r.0.to_string_lossy()).replace("WORD", word));
    }

    fn search(r: &Repo) {
        let _ = kontext_raw(r, &["search", "zzqqx", "--adapters"]);
    }

    #[test]
    fn trust_gate_skips_untrusted_and_revokes_on_edit() {
        let r = repo();
        let marker = r.0.join(".git/shared-ran");
        shared_adapter(&r, "one");
        r.commit("seed");
        search(&r);
        assert!(!marker.exists(), "an untrusted shared adapter ran");
        let status = String::from_utf8_lossy(&kontext_raw(&r, &["status"]).stdout).to_string();
        assert!(status.contains("not trusted yet") && status.contains("kontext trust"), "{status}");

        let trusted = kontext_tty(&r, &["trust", "--yes"]);
        let text = String::from_utf8_lossy(&trusted.stdout).to_string();
        assert!(trusted.status.success(), "{text}");
        assert!(text.contains("[adapters.shared") && text.contains("sha256:"), "it prints everything it pins: {text}");
        let pins = std::fs::read_to_string(trust_file(&r)).unwrap();
        assert!(pins.contains("adapters = \"sha256:") && pins.contains("pin_version = 2"), "{pins}");
        search(&r);
        assert!(marker.exists(), "a trusted adapter runs");

        std::fs::remove_file(&marker).unwrap();
        shared_adapter(&r, "two");
        r.commit("edit the adapter");
        search(&r);
        assert!(!marker.exists(), "an edited adapter table needs a new trust");
        // vars and the project name are part of the pin too
        let table = r.read(".ai/kontext.toml");
        kontext_tty(&r, &["trust", "--yes"]);
        search(&r);
        assert!(marker.exists());
        std::fs::remove_file(&marker).unwrap();
        r.write(".ai/kontext.toml", &format!("{table}\n[vars]\nendpoint = \"https://elsewhere.example\"\n"));
        search(&r);
        assert!(!marker.exists(), "new shared vars revoke the pin");
    }

    #[test]
    fn legacy_fnv_pin_is_refused_with_warning() {
        let r = repo();
        let marker = r.0.join(".git/shared-ran");
        shared_adapter(&r, "one");
        r.commit("seed");
        r.write(".git/user-config/trust.toml", &format!("[repos.\"{SLUG}\"]\nadapters = \"0123456789ab\"\n"));
        search(&r);
        assert!(!marker.exists(), "an old FNV pin no longer trusts anything");
        let status = String::from_utf8_lossy(&kontext_raw(&r, &["status"]).stdout).to_string();
        assert!(status.contains("old format") && status.contains("kontext trust"), "{status}");
    }

    const USER_ADAPTER: &str = r#"[vars]
memory_url = "http://127.0.0.1:1933"

[adapters.mine]
driver = "command"

[adapters.mine.ops.search]
command = ["sh", "-c", "echo 'url={{vars.memory_url}} remote={{repo.remote}} project={{project.name}}' >> ROOT/.git/seen"]
format = "text"
"#;

    fn seen_after_search(r: &Repo) -> String {
        r.write(".git/user-config/config.toml", &USER_ADAPTER.replace("ROOT", &r.0.to_string_lossy()));
        search(r);
        std::fs::read_to_string(r.0.join(".git/seen")).unwrap_or_default()
    }

    #[test]
    fn shared_vars_do_not_reach_user_adapter() {
        let r = repo();
        r.write(".ai/kontext.toml", "[project]\nname = \"../other-team\"\n\n[vars]\nmemory_url = \"https://attacker.example\"\n");
        r.commit("seed");
        let seen = seen_after_search(&r);
        assert!(seen.contains("url=http://127.0.0.1:1933"), "{seen}");
        assert!(!seen.contains("attacker.example"), "{seen}");
        assert!(seen.contains("project=maint") && !seen.contains("other-team"), "{seen}");
        let status = String::from_utf8_lossy(&kontext_raw(&r, &["status"]).stdout).to_string();
        assert!(status.contains("not a slug"), "{status}");
    }

    #[test]
    fn template_remote_url_has_no_userinfo() {
        let r = repo();
        r.commit("seed");
        let seen = seen_after_search(&r);
        assert!(seen.contains("remote=https://example.invalid/acme/maint.git"), "{seen}");
        assert!(!seen.contains("ghp_remotesecret123") && !seen.contains("x-access-token"), "{seen}");
    }

    #[test]
    fn trust_refuses_without_tty() {
        let r = repo();
        shared_adapter(&r, "one");
        entry_with(&r, "2026-01-10-checked", "touch .git/pwned");
        r.commit("seed");
        for args in [&["trust"][..], &["trust", "--yes"], &["trust", "--verify", "--all", "--yes"]] {
            let o = kontext_raw(&r, args);
            assert!(!o.status.success(), "{args:?}");
            assert!(String::from_utf8_lossy(&o.stderr).contains("interactive terminal"), "{args:?}");
        }
        assert!(!trust_file(&r).exists(), "nothing was pinned");
        // a person at a terminal pins every listed command
        let o = kontext_tty(&r, &["trust", "--verify", "--all", "--yes"]);
        let text = String::from_utf8_lossy(&o.stdout).to_string();
        assert!(o.status.success() && text.contains("touch .git/pwned"), "{text}");
        assert_eq!(r.ok("drafts.verify", json!({"id":"2026-01-10-checked"}))["passed"], true);
    }
}
