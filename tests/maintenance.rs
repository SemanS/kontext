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
