//! `kontext init` end to end: skipped and covered tasks stay done, re-renders keep human edits,
//! decision clusters keep their ids, bots and recorded commits are not mined, non-English history works.
use std::path::PathBuf;
use std::process::{Command, Output};

struct Sandbox {
    root: PathBuf,
    repo: PathBuf,
    config: PathBuf,
}

impl Sandbox {
    fn new(name: &str) -> Sandbox {
        let root = std::env::temp_dir().join(format!("kontext-init-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let repo = root.join("repo");
        let config = root.join("config");
        std::fs::create_dir_all(&repo).unwrap();
        std::fs::create_dir_all(&config).unwrap();
        let s = Sandbox { root, repo, config };
        s.ok_git(&["init", "-q", "-b", "main"]);
        s
    }

    fn cmd(&self, program: &str) -> Command {
        let mut c = Command::new(program);
        let bin_dir = PathBuf::from(env!("CARGO_BIN_EXE_kontext")).parent().unwrap().to_path_buf();
        c.current_dir(&self.repo)
            .env("PATH", format!("{}:{}", bin_dir.display(), std::env::var("PATH").unwrap_or_default()))
            .env("KONTEXT_CONFIG_DIR", &self.config)
            .env("GIT_AUTHOR_NAME", "Test")
            .env("GIT_AUTHOR_EMAIL", "test@example.com")
            .env("GIT_COMMITTER_NAME", "Test")
            .env("GIT_COMMITTER_EMAIL", "test@example.com")
            .env_remove("KONTEXT_SKIP");
        c
    }

    fn ok_git(&self, args: &[&str]) -> String {
        let o = self.cmd("git").args(args).output().unwrap();
        assert!(o.status.success(), "git {args:?} failed: {}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr));
        String::from_utf8_lossy(&o.stdout).to_string()
    }

    fn run(&self, args: &[&str]) -> Output {
        self.cmd(env!("CARGO_BIN_EXE_kontext")).args(args).output().unwrap()
    }

    fn kontext(&self, args: &[&str]) -> String {
        let o = self.run(args);
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

    fn read(&self, rel: &str) -> String {
        std::fs::read_to_string(self.repo.join(rel)).unwrap_or_default()
    }

    /// Commit everything with `subject` and `body` (and, when given, as `author`).
    fn commit(&self, subject: &str, body: &str, author: Option<&str>) {
        self.ok_git(&["add", "-A"]);
        let mut args = vec!["commit", "-q", "-m", subject];
        if !body.is_empty() {
            args.extend(["-m", body]);
        }
        let a;
        if let Some(who) = author {
            a = format!("--author={who}");
            args.push(&a);
        }
        self.ok_git(&args);
    }

    /// Ids of the tasks `kontext init next` lists, in order.
    fn task_ids(&self) -> Vec<String> {
        let out = self.kontext(&["init", "next", "-n", "50"]);
        out.lines().filter_map(|l| l.strip_prefix("## Task `")).filter_map(|l| l.split('`').next()).map(str::to_string).collect()
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// A small Python service: `api/` (with a README) and `worker/` are modules.
fn seed(s: &Sandbox) {
    s.write("README.md", "# Shop\n\nA tiny shop backend.\n");
    s.write("api/README.md", "# API\n\nThe HTTP API of the shop.\n");
    s.write("api/app.py", "def main():\n    return 0\n");
    s.write("api/routes.py", "def routes():\n    return []\n");
    s.write("api/models.py", "class Order:\n    pass\n");
    s.write("worker/jobs.py", "def nightly():\n    return 0\n");
    s.write("worker/queue.py", "class Queue:\n    pass\n");
    s.write("worker/retry.py", "def retry():\n    return 1\n");
    s.commit("feat: first version of the shop", "", None);
}

/// One decision-shaped commit touching `api/<file>`.
fn decision(s: &Sandbox, file: &str, subject: &str, body: &str) {
    s.write(&format!("api/{file}"), &format!("# {subject}\nVALUE = 1\n"));
    s.commit(subject, body, None);
}

#[test]
fn init_skip_module_completes_queue() {
    let s = Sandbox::new("skip");
    seed(&s);
    s.kontext(&["init", "--no-hooks"]);
    let before = s.task_ids();
    assert!(before.iter().any(|t| t == "mod:api"), "{before:?}");
    let skipped = s.kontext(&["init", "skip", "mod:api", "--reason", "vendored"]);
    assert!(skipped.contains("Skipped `mod:api`"), "{skipped}");
    let after = s.task_ids();
    assert!(!after.iter().any(|t| t == "mod:api"), "a skipped module task is done: {after:?}");
    assert_eq!(after.len() + 1, before.len(), "{before:?} → {after:?}");
    // a typo is refused instead of recorded as a phantom completion
    let typo = s.run(&["init", "skip", "dec:no-such-task"]);
    assert!(!typo.status.success(), "{}", String::from_utf8_lossy(&typo.stdout));
    assert!(String::from_utf8_lossy(&typo.stderr).contains("unknown task id"), "{}", String::from_utf8_lossy(&typo.stderr));
}

#[test]
fn init_rerender_keeps_human_edits() {
    let s = Sandbox::new("rerender");
    seed(&s);
    s.kontext(&["init", "--no-hooks"]);
    let rel = ".ai/architecture/modules/api.md";
    let doc = s.read(rel);
    assert!(doc.contains("The HTTP API of the shop."), "{doc}");
    let edited: Vec<String> = doc
        .lines()
        .map(|l| {
            if l.starts_with("summary:") {
                "summary: Orders and carts over HTTP, owned by the web team.".to_string()
            } else if l.starts_with("tags:") {
                "tags: [module, team-owned]".to_string()
            } else if l.starts_with("paths:") {
                "paths: [api/**, \"!api/generated/**\", docs/api.md]".to_string()
            } else {
                l.to_string()
            }
        })
        .collect();
    s.write(rel, &format!("{}\n", edited.join("\n")));
    s.write("api/extra.py", "def extra():\n    return 2\n");
    s.commit("feat: extra endpoint", "", None);
    s.kontext(&["init", "--no-hooks"]);
    let doc = s.read(rel);
    assert!(doc.contains("Orders and carts over HTTP, owned by the web team."), "{doc}");
    assert!(doc.contains("team-owned") && doc.contains("!api/generated/**") && doc.contains("docs/api.md"), "{doc}");
    let tags = doc.lines().find(|l| l.starts_with("tags:")).unwrap_or_default().to_string();
    assert_eq!(tags.matches("module").count(), 1, "{tags}");
    // the facts were refreshed and say which commit they describe
    assert!(doc.contains("<!-- kontext:facts:start head="), "{doc}");
    assert!(doc.contains("4 files"), "{doc}");
}

#[test]
fn cluster_ids_stable_across_runs() {
    let s = Sandbox::new("clusters");
    seed(&s);
    decision(&s, "db.py", "refactor(api): replace MySQL with Postgres", "Because we need JSONB and transactional DDL.");
    decision(&s, "cache.py", "refactor(api): switch to Redis instead of memcached", "Because we need persistence for sessions.");
    decision(&s, "auth.py", "feat(api): adopt OAuth instead of API keys", "Keys leaked twice, so tokens must expire.");
    let mut runs = Vec::new();
    for _ in 0..3 {
        s.kontext(&["init", "--no-hooks"]);
        let ids: Vec<String> = s.task_ids().into_iter().filter(|t| t.starts_with("dec:")).collect();
        runs.push(ids);
    }
    assert!(!runs[0].is_empty(), "{runs:?}");
    assert!(runs.iter().all(|r| r == &runs[0]), "{runs:?}");
    for id in &runs[0] {
        let sha = id.rsplit('-').next().unwrap_or("");
        assert!(sha.len() == 7 && sha.chars().all(|c| c.is_ascii_hexdigit()), "named by the oldest commit: {id}");
    }
    // newer history joins the stretch; the id stays
    decision(&s, "queue.py", "refactor(api): move jobs to a queue instead of cron", "Because cron runs overlapped.");
    s.kontext(&["init", "--no-hooks"]);
    let later = s.task_ids();
    assert!(runs[0].iter().all(|id| later.contains(id)), "{:?} → {later:?}", runs[0]);
}

#[test]
fn agent_bootstrap_keeps_hooks() {
    let s = Sandbox::new("bootstrap");
    seed(&s);
    let out = s.kontext(&["call", "ctx_init", r#"{"bootstrap":true}"#]);
    assert!(out.contains("Bootstrapped"), "{out}");
    assert!(!s.repo.join(".git/kontext/no-hooks").exists(), "an agent's bootstrap declines nothing");
    let init = s.kontext(&["init"]);
    assert!(!init.contains("chose --no-hooks"), "{init}");
    assert!(s.repo.join(".git/hooks/pre-commit").exists(), "{init}");
}

#[test]
fn trailer_commits_not_replanned() {
    let s = Sandbox::new("trailers");
    seed(&s);
    s.kontext(&["init", "--no-hooks"]);
    s.ok_git(&["add", "-A"]);
    s.ok_git(&["commit", "-q", "-m", "docs: bootstrap team knowledge"]);
    // knowledge promoted with the code: the commit names the decision in a trailer
    s.write(
        ".ai/decisions/2026-01-01-integer-cents.md",
        "---\nid: 2026-01-01-integer-cents\nkind: decision\ntitle: Prices are integer cents\ndate: 2026-01-01\npaths: [api/**]\n---\n\nInteger cents everywhere.\n",
    );
    decision(
        &s,
        "money.py",
        "refactor(api): replace float prices with integer cents",
        "Because float rounding broke VAT totals.\n\nDecision: 2026-01-01-integer-cents",
    );
    decision(&s, "db.py", "refactor(api): replace MySQL with Postgres", "Because we need JSONB and transactional DDL.");
    s.kontext(&["init", "--no-hooks"]);
    let next = s.kontext(&["init", "next", "-n", "50"]);
    assert!(next.contains("replace MySQL with Postgres"), "{next}");
    assert!(!next.contains("integer cents"), "a commit whose decision is recorded is not mined again: {next}");
}

#[test]
fn bot_commits_not_decisions() {
    let s = Sandbox::new("bots");
    seed(&s);
    s.write("package.json", "{\"name\": \"shop\", \"devDependencies\": {\"vite\": \"^4.5.0\"}}\n");
    s.commit("chore: add vite", "", None);
    s.write("package.json", "{\"name\": \"shop\", \"devDependencies\": {\"vite\": \"^5.0.0\"}}\n");
    let notes = format!(
        "Bumps vite from 4.5.0 to 5.0.0.\n<details>\n<summary>Release notes</summary>\n{}</details>",
        "Breaking: migrate the config; deprecated options were removed because the API changed. ".repeat(30)
    );
    s.commit("chore(deps-dev): bump vite from 4.5.0 to 5.0.0", &notes, Some("dependabot[bot] <support@github.com>"));
    decision(&s, "db.py", "refactor(api): replace MySQL with Postgres", "Because we need JSONB and transactional DDL.");
    let init = s.kontext(&["init", "--no-hooks"]);
    let next = s.kontext(&["init", "next", "-n", "50"]);
    assert!(next.contains("replace MySQL with Postgres"), "{next}");
    assert!(!next.contains("bump vite"), "{init}\n{next}");
}

#[test]
fn sk_history_yields_decision_task() {
    let s = Sandbox::new("sk-history");
    seed(&s);
    let commits = [
        ("db.py", "Prechod z MySQL na Postgres", "Prechádzame na Postgres, lebo potrebujeme JSONB a transakčné DDL."),
        ("cache.py", "Redis namiesto memcached", "Sessions musia prežiť reštart, preto volíme Redis."),
        ("auth.py", "Nahradenie API kľúčov tokenmi OAuth", "Kľúče dvakrát unikli, takže tokeny musia expirovať."),
        ("queue.py", "Úlohy cez frontu namiesto cronu", "Cron sa prekrýval, lebo úlohy trvali dlhšie ako interval."),
        ("money.py", "Ceny v celých centoch namiesto float", "Zaokrúhľovanie floatov kazilo DPH, preto centy."),
        (
            "search.py",
            "Rozhodli sme sa pre Tantivy v prospech Elasticsearch",
            "Elasticsearch je na náš objem drahý, pretože vyžaduje cluster.",
        ),
    ];
    for (file, subject, body) in commits {
        decision(&s, file, subject, body);
    }
    let init = s.kontext(&["init", "--no-hooks"]);
    assert!(init.contains("sk decision words"), "{init}");
    let ids = s.task_ids();
    assert!(ids.iter().any(|t| t.starts_with("dec:")), "{init}\n{ids:?}");
}
