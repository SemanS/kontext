//! Evaluation set: real situations an agent meets, run against the real binary on a fixture
//! repository (`tests/eval/fixtures/shop`), scored per scenario as found / missed / wrongly
//! suppressed, plus noise, tokens and milliseconds. Scenarios live in `tests/eval/scenarios.toml`.
//!
//! A scenario marked `pending = "<item>"` describes behaviour that is not built yet: it is reported,
//! not asserted. Every other scenario must pass. The table is printed (`cargo test --test eval --
//! --nocapture`) and written to `target/tmp/eval-report.md`.
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

static NEXT: AtomicUsize = AtomicUsize::new(0);

const FIXTURE_DATE: &str = "2026-05-01T10:00:00+00:00";

struct Sandbox {
    root: PathBuf,
    worktree: PathBuf,
}

impl Sandbox {
    fn new(fixture: &str) -> Sandbox {
        let base = std::env::temp_dir().join(format!("kontext-eval-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
        let _ = std::fs::remove_dir_all(&base);
        let root = base.join("repo");
        copy_dir(&Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/eval/fixtures").join(fixture), &root);
        let sandbox = Sandbox { worktree: base.join("other-worktree"), root };
        sandbox.git(&["init", "-q", "-b", "main"]);
        sandbox.git(&["add", "-A"]);
        sandbox.git_dated(&["commit", "-qm", "fixture"], FIXTURE_DATE);
        sandbox
    }

    fn command(&self, program: &str, date: Option<&str>) -> Command {
        let mut cmd = Command::new(program);
        cmd.current_dir(&self.root)
            .env("KONTEXT_CONFIG_DIR", self.root.join(".git/kontext-user-config"))
            .env("GIT_AUTHOR_NAME", "Eval")
            .env("GIT_AUTHOR_EMAIL", "eval@example.invalid")
            .env("GIT_COMMITTER_NAME", "Eval")
            .env("GIT_COMMITTER_EMAIL", "eval@example.invalid")
            .env("KONTEXT_SKIP", "1");
        if let Some(d) = date {
            cmd.env("GIT_AUTHOR_DATE", d).env("GIT_COMMITTER_DATE", d);
        }
        cmd
    }

    fn git(&self, args: &[&str]) -> String {
        self.git_dated(args, "")
    }

    fn git_dated(&self, args: &[&str], date: &str) -> String {
        let out = self.command("git", (!date.is_empty()).then_some(date)).args(args).output().unwrap();
        assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    }

    fn expand(&self, s: &str) -> String {
        let head = if s.contains("{head}") { self.git(&["rev-parse", "HEAD"]) } else { String::new() };
        s.replace("{root}", &self.root.to_string_lossy()).replace("{wt}", &self.worktree.to_string_lossy()).replace("{head}", &head)
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        if let Some(base) = self.root.parent() {
            let _ = std::fs::remove_dir_all(base);
        }
    }
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap().flatten() {
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), target).unwrap();
        }
    }
}

#[derive(Default, Clone)]
struct Score {
    expected: usize,
    found: usize,
    suppressed: usize,
    noise: usize,
    rank_misses: usize,
    over_budget: usize,
    tokens: usize,
    ms: u128,
    failures: Vec<String>,
}

impl Score {
    fn passed(&self) -> bool {
        self.failures.is_empty()
    }
}

fn strings(v: Option<&toml::Value>) -> Vec<String> {
    v.and_then(|v| v.as_array()).map(|a| a.iter().filter_map(|s| s.as_str().map(String::from)).collect()).unwrap_or_default()
}

/// Rank of the first numbered hit line (`3. Title … <kx:id>`) that contains `needle`.
fn rank_of(output: &str, needle: &str) -> Option<usize> {
    output
        .lines()
        .filter(|l| l.split_once(". ").is_some_and(|(n, _)| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit())))
        .position(|l| l.contains(needle))
        .map(|i| i + 1)
}

fn run_step(sb: &Sandbox, step: &toml::Value, score: &mut Score) {
    let t = step.as_table().expect("step is a table");
    if let Some(path) = t.get("write").and_then(|v| v.as_str()) {
        let p = sb.root.join(sb.expand(path));
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, sb.expand(t.get("content").and_then(|v| v.as_str()).unwrap_or(""))).unwrap();
        return;
    }
    if let Some(path) = t.get("remove").and_then(|v| v.as_str()) {
        let p = sb.root.join(sb.expand(path));
        let _ = std::fs::remove_file(&p).or_else(|_| std::fs::remove_dir_all(&p));
        return;
    }
    if let Some(path) = t.get("touch").and_then(|v| v.as_str()) {
        let times = t.get("times").and_then(|v| v.as_integer()).unwrap_or(1);
        for i in 0..times {
            let p = sb.root.join(path);
            let mut text = std::fs::read_to_string(&p).unwrap_or_default();
            text.push_str(&format!("// change {i}\n"));
            std::fs::write(&p, text).unwrap();
            sb.git(&["add", path]);
            sb.git_dated(&["commit", "-qm", &format!("change {path} {i}")], &format!("2026-06-{:02}T10:00:00+00:00", i + 1));
        }
        return;
    }
    if let Some(args) = t.get("git") {
        let args: Vec<String> = strings(Some(args)).iter().map(|a| sb.expand(a)).collect();
        let refs: Vec<&str> = args.iter().map(String::as_str).collect();
        let date = t.get("date").and_then(|v| v.as_str()).unwrap_or("");
        sb.git_dated(&refs, date);
        return;
    }
    let args: Vec<String> = strings(t.get("run")).iter().map(|a| sb.expand(a)).collect();
    assert!(!args.is_empty(), "a step needs write, remove, touch, git or run");
    let started = Instant::now();
    let out = sb.command(env!("CARGO_BIN_EXE_kontext"), None).args(&args).output().unwrap();
    let elapsed = started.elapsed().as_millis();
    let text = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    let expect = strings(t.get("expect"));
    let keep = strings(t.get("keep"));
    let forbid = strings(t.get("forbid"));
    let measured = !expect.is_empty() || !keep.is_empty() || !forbid.is_empty() || t.contains_key("rank") || t.contains_key("max_tokens");
    if !measured {
        return;
    }
    let tokens = text.chars().count().div_ceil(4);
    score.tokens += tokens;
    score.ms += elapsed;
    let shown = |s: &str| text.contains(&sb.expand(s));
    for e in &expect {
        score.expected += 1;
        if shown(e) {
            score.found += 1;
        } else {
            score.failures.push(format!("missed {e:?}"));
        }
    }
    for k in &keep {
        score.expected += 1;
        if shown(k) {
            score.found += 1;
        } else {
            score.suppressed += 1;
            score.failures.push(format!("wrongly suppressed {k:?}"));
        }
    }
    for f in &forbid {
        if shown(f) {
            score.noise += 1;
            score.failures.push(format!("noise {f:?}"));
        }
    }
    if let Some(ranks) = t.get("rank").and_then(|v| v.as_table()) {
        for (needle, max) in ranks {
            let max = max.as_integer().unwrap_or(1) as usize;
            match rank_of(&text, needle) {
                Some(r) if r <= max => {}
                r => {
                    score.rank_misses += 1;
                    score.failures.push(format!("{needle} at rank {r:?}, wanted ≤ {max}"));
                }
            }
        }
    }
    if let Some(max) = t.get("max_tokens").and_then(|v| v.as_integer())
        && tokens > max as usize
    {
        score.over_budget += 1;
        score.failures.push(format!("{tokens} tokens > {max}"));
    }
    if score.failures.iter().any(|f| f.starts_with("missed") || f.starts_with("wrongly")) && std::env::var("KONTEXT_EVAL_VERBOSE").is_ok() {
        eprintln!("--- output of {args:?}\n{text}---");
    }
}

#[test]
fn evaluation_set() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/eval/scenarios.toml");
    let doc: toml::Value = toml::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let only = std::env::var("KONTEXT_EVAL_ONLY").ok();
    let mut rows = Vec::new();
    let mut failed = Vec::new();
    for sc in doc.get("scenario").and_then(|v| v.as_array()).expect("[[scenario]] list") {
        let name = sc.get("name").and_then(|v| v.as_str()).expect("scenario name");
        if only.as_deref().is_some_and(|o| !name.contains(o)) {
            continue;
        }
        let item = sc.get("item").and_then(|v| v.as_str()).unwrap_or("");
        let pending = sc.get("pending").and_then(|v| v.as_str());
        let sb = Sandbox::new(sc.get("fixture").and_then(|v| v.as_str()).unwrap_or("shop"));
        let mut score = Score::default();
        for step in sc.get("step").and_then(|v| v.as_array()).map(Vec::as_slice).unwrap_or_default() {
            run_step(&sb, step, &mut score);
        }
        let state = match (score.passed(), pending) {
            (true, _) => "pass".to_string(),
            (false, Some(p)) => format!("pending {p}"),
            (false, None) => {
                failed.push(format!("{name}: {}", score.failures.join("; ")));
                "FAIL".to_string()
            }
        };
        rows.push((name.to_string(), item.to_string(), state, score));
    }
    let mut report = String::from(
        "| scenario | item | result | found | missed | wrongly suppressed | noise | tokens | ms |\n|---|---|---|---|---|---|---|---|---|\n",
    );
    let mut total = Score::default();
    for (name, item, state, s) in &rows {
        let missed = s.expected - s.found - s.suppressed + s.rank_misses + s.over_budget;
        report.push_str(&format!(
            "| {name} | {item} | {state} | {}/{} | {missed} | {} | {} | {} | {} |\n",
            s.found, s.expected, s.suppressed, s.noise, s.tokens, s.ms
        ));
        total.expected += s.expected;
        total.found += s.found;
        total.suppressed += s.suppressed;
        total.noise += s.noise;
        total.tokens += s.tokens;
        total.ms += s.ms;
    }
    let passing = rows.iter().filter(|r| r.2 == "pass").count();
    report.push_str(&format!(
        "\n{passing}/{} scenarios pass · found {}/{} · wrongly suppressed {} · noise {} · {} tokens · {} ms\n",
        rows.len(),
        total.found,
        total.expected,
        total.suppressed,
        total.noise,
        total.tokens,
        total.ms
    ));
    println!("{report}");
    let out = Path::new(env!("CARGO_TARGET_TMPDIR")).join("eval-report.md");
    let _ = std::fs::write(&out, &report);
    if let Ok(json) = std::env::var("KONTEXT_EVAL_JSON") {
        let rows: Vec<Value> = rows
            .iter()
            .map(|(n, i, st, s)| serde_json::json!({"name":n,"item":i,"result":st,"found":s.found,"expected":s.expected,"suppressed":s.suppressed,"noise":s.noise,"tokens":s.tokens,"ms":s.ms,"failures":s.failures}))
            .collect();
        let _ = std::fs::write(json, serde_json::to_string_pretty(&rows).unwrap());
    }
    assert!(failed.is_empty(), "evaluation scenarios failed:\n{}", failed.join("\n"));
}
