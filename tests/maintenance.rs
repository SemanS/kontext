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

// ------------------------------------------------- verification by ancestry, outbox, distill

/// A ledger line as kontext writes it, appended by hand (a check run elsewhere): dated
/// `offset_secs` from now, naming HEAD.
fn f_ledger_line(r: &Repo, id: &str, kind: &str, passed: &str, offset_secs: i64) -> String {
    let at = (chrono::Utc::now() + chrono::Duration::seconds(offset_secs)).format("%Y-%m-%dT%H:%M:%SZ").to_string();
    let head = r.git(&["rev-parse", "HEAD"]);
    let path = r.0.join(".ai/confirmations.log");
    let mut text = std::fs::read_to_string(&path).unwrap_or_default();
    text.push_str(&format!("{at}\t{id}\t{kind}\t{head}\tTest\t{passed}\tran it\n"));
    std::fs::write(path, text).unwrap();
    at
}

/// Commit everything with author and committer dates `secs_ago` in the past.
fn f_commit_dated(r: &Repo, message: &str, secs_ago: i64) -> String {
    let date = format!("@{} +0000", chrono::Utc::now().timestamp() - secs_ago);
    r.git(&["add", "-A"]);
    let o = r.cmd("git").env("GIT_AUTHOR_DATE", &date).env("GIT_COMMITTER_DATE", &date).args(["commit", "-qm", message]).output().unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    r.git(&["rev-parse", "HEAD"])
}

fn f_entry(r: &Repo, id: &str) -> Value {
    r.ok("entries.get", json!({"id": id}))["entry"].clone()
}

/// A feature branch with an older commit on governed code, merged with a merge commit after the
/// main line looked at the entry.
fn f_merge_older_branch(r: &Repo, on_main: &dyn Fn()) -> String {
    r.git(&["checkout", "-q", "-b", "feature"]);
    r.write("src/feature.rs", "pub fn x() {}\n");
    let feature = f_commit_dated(r, "feat: x", 3600);
    r.git(&["checkout", "-q", "main"]);
    on_main();
    r.git(&["merge", "-q", "--no-ff", "--no-edit", "feature"]);
    feature
}

#[test]
fn since_counts_branch_merged_after_the_look() {
    let r = Repo::new();
    r.decision("rule", "Rule", "accepted", "");
    r.commit("seed");
    let landed = std::cell::RefCell::new(String::new());
    let feature = f_merge_older_branch(&r, &|| {
        r.ok("drafts.confirm", json!({"id":"rule","kind":"read"}));
        *landed.borrow_mut() = r.commit("docs: rule holds");
    });
    let e = f_entry(&r, "rule");
    assert_eq!(e["verification"]["kind"], "read");
    assert_eq!(e["verification"]["landed"], landed.borrow().as_str());
    assert_eq!(e["since"]["commits"], 1, "the branch was merged after the look: {e}");
    assert_eq!(e["since"]["recent"][0]["sha"], feature.as_str());
    assert_eq!(e["last_verified"]["kind"], "read");
    assert_eq!(e["since_verified"]["commits"], 1);
}

#[test]
fn freshness_counts_branch_merged_after_an_edit() {
    let r = Repo::new();
    r.write(".ai/kontext.toml", "[project]\nname = \"maint\"\n[freshness]\nthreshold_commits = 1\n");
    r.decision("rule", "Rule", "accepted", "");
    r.commit("seed");
    let path = ".ai/decisions/rule.md";
    f_merge_older_branch(&r, &|| {
        r.write(path, &r.read(path).replace("for the record", "for the record, reworded"));
        r.commit("docs: reword the rule");
        assert!(!r.kontext(&["status"]).contains("may need a refresh"), "nothing on main since the edit");
    });
    let status = r.kontext(&["status"]);
    assert!(status.contains("freshness 1 entry may need a refresh") && status.contains("[rule]"), "{status}");
}

#[test]
fn failed_verify_does_not_reset_since() {
    let r = Repo::new();
    r.decision("rule", "Rule", "accepted", "");
    r.commit("seed");
    r.ok("drafts.confirm", json!({"id":"rule","kind":"read"}));
    r.commit("docs: rule holds");
    r.write("src/lib.rs", "pub fn f() -> u8 { 1 }\n");
    let change = r.commit("feat: f returns one");
    f_ledger_line(&r, "rule", "verify", "0", 2);
    r.commit("chore: the check failed");
    let e = f_entry(&r, "rule");
    assert_eq!((e["verification"]["kind"].as_str(), e["verification"]["verified"].as_bool()), (Some("verify"), Some(false)));
    assert_eq!(e["failing"], true);
    assert_eq!(e["since"]["commits"], 1, "a failed verify is shown, never a look: {e}");
    assert_eq!(e["since"]["recent"][0]["sha"], change.as_str());
}

#[test]
fn failed_verify_keeps_last_verified() {
    let r = Repo::new();
    r.decision("rule", "Rule", "accepted", "");
    r.commit("seed");
    let read = r.ok("drafts.confirm", json!({"id":"rule","kind":"read"}))["ledger"]["at"].as_str().unwrap().to_string();
    let read_landed = r.commit("docs: rule holds");
    r.write("src/lib.rs", "pub fn f() -> u8 { 1 }\n");
    r.commit("feat: f returns one");
    f_ledger_line(&r, "rule", "verify", "0", 2);
    r.commit("chore: the check failed");
    let e = f_entry(&r, "rule");
    assert_eq!(e["last_verified"], json!({"kind":"read","at":read,"commit":e["last_verified"]["commit"],"landed":read_landed}));
    assert_eq!(e["since_verified"]["commits"], 1);
    assert_eq!((e["failing"].as_bool(), e["amended_since"].as_bool()), (Some(true), Some(false)));

    // a pass ends the failure and is the new verification
    f_ledger_line(&r, "rule", "verify", "1", 4);
    let pass = r.commit("chore: the check passes");
    let e = f_entry(&r, "rule");
    assert_eq!(e["failing"], false);
    assert_eq!((e["last_verified"]["kind"].as_str(), e["last_verified"]["landed"].as_str()), (Some("verify"), Some(pass.as_str())));
    assert_eq!((e["since"]["commits"].as_u64(), e["since_verified"]["commits"].as_u64()), (Some(0), Some(0)));

    // a stamp only rewrites `checked`; a changed body after the verification is an amendment
    r.ok("drafts.confirm", json!({"id":"rule"}));
    r.commit("docs: stamped");
    assert_eq!(f_entry(&r, "rule")["amended_since"], false);
    let path = ".ai/decisions/rule.md";
    r.write(path, &r.read(path).replace("for the record", "for the record, and taxes too"));
    r.commit("docs: the rule covers taxes");
    let e = f_entry(&r, "rule");
    assert_eq!(e["amended_since"], true, "{e}");
    assert_eq!(e["last_verified"]["kind"], "verify");
}

#[test]
fn cache_only_checked_is_not_verified() {
    let r = Repo::new();
    r.decision("rule", "Rule", "accepted", "checked: 2026-10-20\n");
    r.commit("seed");
    let e = f_entry(&r, "rule");
    assert_eq!(
        (e["verification"]["kind"].as_str(), e["verification"]["source"].as_str(), e["verification"]["verified"].as_bool()),
        (Some("cache"), Some("frontmatter"), Some(false))
    );
    assert_eq!(e["checked"], "2026-10-20", "the cache is still reported");
    assert_eq!(e["last_verified"], Value::Null);
    assert_eq!(e["since_verified"], Value::Null);
}

#[test]
fn migrate_legacy_writes_bulk_lines() {
    let r = Repo::new();
    r.decision("rule", "Rule", "accepted", "checked: 2026-10-20\n");
    r.decision("plain", "Plain", "accepted", "");
    let seed = r.commit("seed");
    let out = r.kontext(&["confirm", "--migrate-legacy"]);
    assert!(out.contains("migrated rule — bulk stamp dated 2026-10-20T00:00:00Z in .ai/confirmations.log (staged)"), "{out}");
    assert!(!out.contains("plain"), "{out}");
    let ledger = r.read(".ai/confirmations.log");
    let fields: Vec<&str> = ledger.trim_end().split('\t').collect();
    assert_eq!(&fields[..4], &["2026-10-20T00:00:00Z", "rule", "bulk", seed.as_str()], "{ledger}");
    assert!(r.git(&["diff", "--cached", "--name-only"]).contains(".ai/confirmations.log"));
    let e = f_entry(&r, "rule");
    assert_eq!((e["verification"]["kind"].as_str(), e["verification"]["source"].as_str()), (Some("bulk"), Some("ledger")));
    assert!(r.read(".ai/decisions/rule.md").contains("checked: 2026-10-20\n"), "the cache is kept");
    assert!(r.kontext(&["confirm", "--migrate-legacy"]).contains("nothing to migrate"));
}

#[test]
fn cli_confirm_without_tty_is_agent() {
    let r = Repo::new();
    r.decision("rule", "Rule", "accepted", "");
    r.commit("seed");
    let kind = |r: &Repo| r.read(".ai/confirmations.log").lines().last().unwrap().split('\t').nth(2).unwrap().to_string();
    r.kontext(&["confirm", "rule"]);
    assert_eq!(kind(&r), "agent", "no terminal: an agent's stamp");
    assert_eq!(f_entry(&r, "rule")["verification"]["verified"], false);
    std::thread::sleep(std::time::Duration::from_millis(1100));
    let o = r.cmd(env!("CARGO_BIN_EXE_kontext")).args(["confirm", "rule", "--kind", "read"]).output().unwrap();
    assert!(o.status.success());
    assert!(
        String::from_utf8_lossy(&o.stderr).contains("--kind read without an interactive terminal"),
        "{}",
        String::from_utf8_lossy(&o.stderr)
    );
    assert_eq!(kind(&r), "read");
    assert_eq!(f_entry(&r, "rule")["verification"]["verified"], true);
    let bad = r.cmd(env!("CARGO_BIN_EXE_kontext")).args(["confirm", "rule", "--kind", "bulk"]).output().unwrap();
    assert!(!bad.status.success());
}

fn f_event(id: &str) -> String {
    json!({"id":id,"event":"capture","at":"2026-10-29T10:00:00Z","payload":{"id":id,"kind":"learning","visibility":"team"}}).to_string()
}

#[test]
fn outbox_dead_letters_after_repeated_failures_and_retry() {
    let r = Repo::new();
    r.commit("seed");
    r.write(
        ".git/kontext/config.toml",
        "[adapters.down]\ndriver = \"command\"\non = [{ event = \"capture\", op = \"store\" }]\n[adapters.down.ops.store]\ncommand = [\"sh\", \"-c\", \"exit 3\"]\nformat = \"text\"\n",
    );
    r.write(".git/kontext/outbox.jsonl", &format!("{}\n{}\n", f_event("first"), f_event("second")));
    for _ in 0..8 {
        r.kontext(&["outbox", "flush"]);
    }
    let list = r.kontext(&["outbox"]);
    assert!(list.contains("1 event(s) ran out of attempts") && list.contains(" capture first attempts=8"), "{list}");
    assert!(list.contains(" capture second attempts=0"), "an adapter that failed is not tried again in the same flush: {list}");
    assert!(r.kontext(&["status"]).contains("1 ran out of attempts"));
    let retry = r.kontext(&["outbox", "retry"]);
    assert!(retry.contains("queued 1 dead event(s) again"), "{retry}");
    assert!(!r.0.join(".git/kontext/outbox.dead.jsonl").exists());
    let list = r.kontext(&["outbox"]);
    assert!(!list.contains("ran out of attempts") && list.contains(" capture first attempts="), "{list}");
}

#[test]
fn when_skipped_events_stay_queued() {
    let r = Repo::new();
    r.commit("seed");
    let root = r.0.display();
    r.write(
        ".git/kontext/config.toml",
        &format!(
            "[adapters.sink]\ndriver = \"command\"\non = [{{ event = \"capture\", op = \"store\" }}]\n[adapters.sink.when]\nenv = \"KONTEXT_TEST_SINK_ON\"\n[adapters.sink.ops.store]\ncommand = [\"sh\", \"-c\", \"echo got >> {root}/.git/sink.txt\"]\nformat = \"text\"\n"
        ),
    );
    r.write(".git/kontext/outbox.jsonl", &format!("{}\n", f_event("kept")));
    let out = r.kontext(&["outbox", "flush"]);
    assert!(out.contains("0 delivered, 0 failed, 1 remaining"), "{out}");
    assert!(r.kontext(&["outbox"]).contains(" capture kept attempts=0"));
    let o = r.cmd(env!("CARGO_BIN_EXE_kontext")).env("KONTEXT_TEST_SINK_ON", "1").args(["outbox", "flush"]).output().unwrap();
    let out = String::from_utf8_lossy(&o.stdout);
    assert!(out.contains("1 delivered, 0 failed, 0 remaining"), "{out}");
    assert!(r.0.join(".git/sink.txt").exists());
}

#[test]
fn rename_emits_remove_and_add() {
    let r = Repo::new();
    r.decision("rule", "Rule", "accepted", "");
    r.commit("seed");
    r.write(
        ".git/kontext/config.toml",
        "[adapters.mirror]\ndriver = \"command\"\non = [{ event = \"sync\", op = \"store\" }, { event = \"remove\", op = \"store\" }]\n[adapters.mirror.ops.store]\ncommand = [\"sh\", \"-c\", \"exit 3\"]\nformat = \"text\"\n",
    );
    assert!(r.kontext(&["sync"]).contains("queued 0 sync event(s)"), "the first run records the snapshot");
    r.git(&["mv", ".ai/decisions/rule.md", ".ai/decisions/rule-moved.md"]);
    r.commit("docs: move the rule");
    let out = r.kontext(&["sync"]);
    assert!(out.contains("queued 2 sync event(s)"), "{out}");
    let list = r.kontext(&["outbox"]);
    assert!(list.contains(" remove rule attempts="), "{list}");
    assert!(list.contains(" sync rule attempts="), "{list}");
}
