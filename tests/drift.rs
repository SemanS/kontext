//! Knowledge drift end to end: confirmations, the entries a range of commits (or the working tree)
//! left behind, the CI report and prepare-commit without a staged change, against the real binary in
//! throwaway repositories.
use serde_json::{Value, json};
use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT: AtomicUsize = AtomicUsize::new(0);

struct Repo(PathBuf);

impl Repo {
    fn new() -> Repo {
        let root = std::env::temp_dir().join(format!("kontext-drift-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let repo = Repo(root);
        repo.write(".ai/kontext.toml", "[project]\nname = \"drift\"\n");
        repo.write("src/billing/invoice.rs", "pub fn total() {}\n");
        repo.write("src/api/routes.rs", "pub fn routes() {}\n");
        repo.write("docs/notes.md", "Notes.\n");
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

    fn run(&self, args: &[&str]) -> Output {
        self.cmd(env!("CARGO_BIN_EXE_kontext")).args(args).output().unwrap()
    }

    fn kontext(&self, args: &[&str]) -> String {
        let o = self.run(args);
        assert!(o.status.success(), "kontext {args:?}: {}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr));
        String::from_utf8_lossy(&o.stdout).to_string()
    }

    fn api(&self, operation: &str, input: Value) -> Value {
        let o = self.run(&["api", operation, &input.to_string()]);
        serde_json::from_slice(&o.stdout).unwrap_or_else(|e| panic!("{e}: {}", String::from_utf8_lossy(&o.stdout)))
    }

    fn ok(&self, operation: &str, input: Value) -> Value {
        let v = self.api(operation, input);
        assert_eq!(v["ok"], true, "{v}");
        v["result"].clone()
    }

    fn entry(&self, dir: &str, id: &str, kind: &str, paths: &str, extra: &str) {
        self.write(
            &format!(".ai/{dir}/{id}.md"),
            &format!("---\nid: {id}\nkind: {kind}\ntitle: Rule {id}\nstatus: accepted\ndate: 2026-01-10\npaths: [{paths}]\n{extra}---\n\nRule {id}, for the record.\n"),
        );
    }
}

impl Drop for Repo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn confirm_sets_checked_stages_it_and_refuses_what_it_cannot_confirm() {
    let r = Repo::new();
    r.entry("conventions", "money-in-cents", "convention", "src/billing/**", "");
    r.entry("decisions", "old-rule", "decision", "src/**", "");
    r.write(".ai/decisions/old-rule.md", &r.read(".ai/decisions/old-rule.md").replace("accepted", "deprecated"));
    r.commit("seed");
    let before = r.read(".ai/conventions/money-in-cents.md");
    let out = r.kontext(&["confirm", "money-in-cents"]);
    let today = chrono_today();
    assert!(out.contains(&format!("confirmed money-in-cents — checked {today} in .ai/conventions/money-in-cents.md (staged)")), "{out}");
    let after = r.read(".ai/conventions/money-in-cents.md");
    assert_eq!(after, before.replace("date: 2026-01-10\n", &format!("date: 2026-01-10\nchecked: {today}\n")), "one line and nothing else");
    assert_eq!(r.git(&["diff", "--cached", "--name-only"]), ".ai/conventions/money-in-cents.md");
    assert_eq!(r.ok("entries.get", json!({"id":"money-in-cents"}))["entry"]["checked"], today.as_str());
    // a second confirmation the same day changes nothing
    let again = r.ok("drafts.confirm", json!({"id":"money-in-cents"}));
    assert_eq!(
        again,
        json!({"id":"money-in-cents","path":".ai/conventions/money-in-cents.md","checked":today,"repository":again["repository"]})
    );
    assert_eq!(r.read(".ai/conventions/money-in-cents.md"), after);
    assert!(r.kontext(&["check"]).contains("0 error(s)"));

    assert_eq!(r.api("drafts.confirm", json!({"id":"nope"}))["error"]["code"], "not_found");
    assert_eq!(r.api("drafts.confirm", json!({"id":"old-rule"}))["error"]["code"], "operation_conflict");
    assert_eq!(r.api("drafts.confirm", json!({}))["error"]["code"], "invalid_input");
    r.write(".ai/conventions/money-in-cents.md", &format!("{after}Unstaged edit.\n"));
    assert_eq!(
        r.api("drafts.confirm", json!({"id":"money-in-cents"}))["error"]["code"],
        "operation_conflict",
        "unstaged edits stay unstaged"
    );
    let o = r.run(&["confirm", "nope"]);
    assert_eq!(o.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&o.stderr).contains("no entry 'nope'"));
    let tool = r.run(&["call", "ctx_confirm", r#"{"id":"nope"}"#]);
    assert!(String::from_utf8_lossy(&tool.stderr).contains("no entry 'nope'"));
}

/// Today in UTC, as a confirmation writes it.
fn chrono_today() -> String {
    chrono::Utc::now().format("%Y-%m-%d").to_string()
}

#[test]
fn drift_lists_what_a_range_left_behind_and_a_look_takes_it_off() {
    let r = Repo::new();
    r.entry("conventions", "money-in-cents", "convention", "src/billing/**", "");
    r.entry("learnings", "routes-are-flat", "learning", "src/api/**", "checked: 2026-02-01\n");
    r.entry("decisions", "notes-in-docs", "decision", "docs/**", "");
    let base = r.commit("seed");
    r.write("src/billing/invoice.rs", "pub fn total() -> u64 { 0 }\n");
    let billing = r.commit("feat(billing): totals in cents");
    r.write("src/api/routes.rs", "pub fn routes() -> u8 { 1 }\n");
    r.write(".ai/learnings/routes-are-flat.md", &r.read(".ai/learnings/routes-are-flat.md").replace("for the record", "still flat"));
    let head = r.commit("feat(api): one more route, rule edited with it");

    // committed range: the edited entry looked at its change, the convention did not
    let out = r.ok("entries.drift", json!({"base":base,"head":"HEAD"}));
    assert_eq!(
        out["entries"],
        json!([{"id":"money-in-cents","kind":"convention","title":"Rule money-in-cents","path":".ai/conventions/money-in-cents.md",
            "date":"2026-01-10","changed":["src/billing/invoice.rs"],"commits":[{"sha":billing,"subject":"feat(billing): totals in cents"}]}])
    );
    assert_eq!(out["base"], base.as_str());
    assert_eq!(out["head"], head.as_str());
    let text = r.kontext(&["drift", "--base", &base, "--head", "HEAD"]);
    assert!(text.contains("- [convention] Rule money-in-cents <kx:money-in-cents> — .ai/conventions/money-in-cents.md"), "{text}");
    assert!(
        text.contains("  changed: src/billing/invoice.rs") && text.contains(&format!("  commits: {} feat(billing)", &billing[..8])),
        "{text}"
    );
    assert!(text.contains("`kontext confirm <id>`"), "{text}");
    assert_eq!(r.run(&["drift", "--base", &base, "--head", "HEAD", "--fail"]).status.code(), Some(2));
    let json: Value = serde_json::from_str(&r.kontext(&["drift", "--base", &base, "--head", "HEAD", "--json"])).unwrap();
    assert_eq!(json["entries"][0]["id"], "money-in-cents");
    let tool = r.kontext(&["call", "ctx_drift", &json!({"base":base,"head":"HEAD"}).to_string()]);
    assert!(tool.contains("<kx:money-in-cents>") && tool.contains("ctx_confirm"), "{tool}");

    // the working tree: uncommitted and untracked changes count without a head, with no commits
    let quiet = r.ok("entries.drift", json!({"base":head}));
    assert_eq!(quiet["entries"], json!([]));
    assert_eq!(quiet["head"], "working");
    r.write("docs/new-guide.md", "Untracked.\n");
    let working = r.ok("entries.drift", json!({"base":head}));
    assert_eq!(working["entries"][0]["id"], "notes-in-docs");
    assert_eq!(working["entries"][0]["changed"], json!(["docs/new-guide.md"]));
    assert_eq!(working["entries"][0]["commits"], json!([]));
    assert_eq!(r.run(&["drift", "--base", &head, "--fail"]).status.code(), Some(2));
    // no base: the fork point from origin's default branch (none here, so HEAD), plus the working tree
    assert_eq!(r.ok("entries.drift", json!({}))["base"], head.as_str());

    // a confirmation is a look: it is staged and the entry leaves the list
    r.kontext(&["confirm", "notes-in-docs"]);
    assert!(r.git(&["diff", "--cached", "--name-only"]).contains(".ai/decisions/notes-in-docs.md"));
    assert_eq!(r.ok("entries.drift", json!({"base":head}))["entries"], json!([]));
    assert!(r.kontext(&["drift", "--base", &head]).contains("No knowledge drift"));
    r.commit("docs: guide, rule confirmed");
    assert_eq!(r.ok("entries.drift", json!({"base":head,"head":"HEAD"}))["entries"], json!([]), "committed, it still counts");

    // kinds, and bad input
    let decisions = r.ok("entries.drift", json!({"base":base,"head":"HEAD","kinds":["decision"]}));
    assert_eq!(decisions["entries"], json!([]));
    assert_eq!(r.api("entries.drift", json!({"base":"--output=/tmp/x"}))["error"]["code"], "invalid_input");
    assert_eq!(r.api("entries.drift", json!({"head":"no-such-branch"}))["error"]["code"], "invalid_input");
    assert_eq!(r.api("entries.drift", json!({"kinds":"decision"}))["error"]["code"], "invalid_input");
    let status = r.ok("status", json!({}));
    for op in ["entries.drift", "drafts.confirm"] {
        assert!(status["capabilities"].as_array().unwrap().iter().any(|c| c == op), "{op}");
    }
}

#[test]
fn prepare_commit_reads_the_working_tree_when_nothing_is_staged() {
    let r = Repo::new();
    r.entry("conventions", "money-in-cents", "convention", "src/billing/**", "");
    r.commit("seed");
    // a jj working copy: edits git sees as unstaged, and a new file
    r.write("src/billing/invoice.rs", "pub fn total() -> u64 { 0 }\n");
    r.write("src/billing/tax.rs", "pub const VAT: u64 = 20;\n");
    let report = r.kontext(&["prepare-commit"]);
    assert!(
        report.contains("Nothing staged yet — read the working tree's changes against HEAD (untracked files included): 2 file(s)"),
        "{report}"
    );
    assert!(report.contains("## Recorded knowledge covering this change — confirm it still holds"), "{report}");
    assert!(report.contains("<kx:money-in-cents>") && report.contains("`kontext confirm <id>`"), "{report}");
    let tool = r.kontext(&["call", "ctx_prepare_commit", "{}"]);
    assert!(tool.contains("read the working tree's changes against HEAD") && tool.contains("ctx_confirm"), "{tool}");
}

#[test]
fn ci_checks_every_entry_and_warns_or_fails_on_drift() {
    let r = Repo::new();
    r.entry("conventions", "money-in-cents", "convention", "src/billing/**", "");
    let base = r.commit("seed");
    // the default base is where HEAD left origin's default branch
    r.git(&["update-ref", "refs/remotes/origin/main", &base]);
    r.write("src/billing/invoice.rs", "pub fn total() -> u64 { 0 }\n");
    r.commit("feat(billing): totals in cents");
    // uncommitted changes are not CI's business
    r.write("src/api/routes.rs", "changed\n");

    let warn = r.run(&["ci"]);
    let text = String::from_utf8_lossy(&warn.stdout).to_string();
    assert_eq!(warn.status.code(), Some(0), "drift alone is a warning: {text}");
    assert!(text.contains("check: 1 file(s) checked, 0 error(s)") && text.contains("<kx:money-in-cents>"), "{text}");
    assert_eq!(r.run(&["ci", "--strict"]).status.code(), Some(2));
    let md = r.kontext(&["ci", "--format", "markdown", "--base", &base]);
    assert!(md.starts_with("## kontext\n\n**Check:** 1 file(s) checked, 0 error(s), 0 warning(s)\n"), "{md}");
    assert!(md.contains("### Knowledge drift (`") && md.contains("| Rule money-in-cents (`money-in-cents`, [file](.ai/conventions/money-in-cents.md)) | convention | `src/billing/invoice.rs` |"), "{md}");
    assert_eq!(r.run(&["ci", "--format", "html"]).status.code(), Some(1));

    // check errors fail the job whatever the drift says
    r.write(".ai/conventions/bad.md", "---\nid: bad\nkind: convention\ntitle: Bad\nchecked: someday\n---\nBody.\n");
    r.commit("a bad entry");
    let failed = r.run(&["ci"]);
    assert_eq!(failed.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&failed.stdout).contains("checked 'someday' is not a YYYY-MM-DD date"));
    r.git(&["rm", "-q", ".ai/conventions/bad.md"]);
    r.kontext(&["confirm", "money-in-cents"]);
    r.commit("rule confirmed");
    let clean = r.run(&["ci", "--strict"]);
    assert_eq!(clean.status.code(), Some(0), "{}", String::from_utf8_lossy(&clean.stdout));
    assert!(String::from_utf8_lossy(&clean.stdout).contains("No knowledge drift"));
    // a clone without the default branch's history has an empty range, and says so
    r.git(&["update-ref", "-d", "refs/remotes/origin/main"]);
    assert!(r.kontext(&["ci"]).contains("fetch-depth: 0"));
}
