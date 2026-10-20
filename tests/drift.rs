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
