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
