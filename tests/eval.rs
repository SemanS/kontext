//! Evaluation set: real situations an agent meets, run against the real binary on a fixture
//! repository (`tests/eval/fixtures/shop`), scored per scenario as found / missed / wrongly
//! suppressed, plus noise, bad exit statuses, tokens and milliseconds. Scenarios live in
//! `tests/eval/scenarios.toml`.
//!
//! A scenario marked `pending = "<item>"` describes behaviour that is not built yet: it is reported,
//! not asserted, and flagged `pass (remove pending)` once it passes. Every other scenario must pass.
//! The table is printed (`cargo test --test eval -- --nocapture`) and written to
//! `target/tmp/eval-report.md`.
//!
//! When `tests/eval/qrels.tsv` and `tests/eval/queries.toml` exist, `retrieval_quality` also scores
//! `kontext search --json` per query (nDCG@5, recall@10, MRR, per slice) into
//! `target/tmp/eval-retrieval.md`, and holds them to `tests/eval/qrels-baseline.toml` when that exists.
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::Path;
use std::time::Instant;

mod common;
use common::Sandbox;

trait Expand {
    fn expand(&self, s: &str) -> String;
}

impl Expand for Sandbox {
    /// `{root}` the repository, `{wt}` a free path for another worktree, `{head}` HEAD's commit.
    fn expand(&self, s: &str) -> String {
        let head = if s.contains("{head}") { self.git_dated(&["rev-parse", "HEAD"], "") } else { String::new() };
        s.replace("{root}", &self.repo.to_string_lossy()).replace("{wt}", &self.worktree.to_string_lossy()).replace("{head}", &head)
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
    bad_exits: usize,
    tokens: usize,
    ms: u128,
    failures: Vec<String>,
}

impl Score {
    fn passed(&self) -> bool {
        self.failures.is_empty()
    }

    /// Expectations not met: absent `expect` strings, ranked hits below their rank, outputs over
    /// their token limit. Wrongly suppressed `keep` strings are counted on their own.
    fn missed(&self) -> usize {
        self.expected - self.found - self.suppressed
    }

    fn add(&mut self, s: &Score) {
        self.expected += s.expected;
        self.found += s.found;
        self.suppressed += s.suppressed;
        self.noise += s.noise;
        self.rank_misses += s.rank_misses;
        self.over_budget += s.over_budget;
        self.bad_exits += s.bad_exits;
        self.tokens += s.tokens;
        self.ms += s.ms;
    }
}

fn strings(v: Option<&toml::Value>) -> Vec<String> {
    v.and_then(|v| v.as_array()).map(|a| a.iter().filter_map(|s| s.as_str().map(String::from)).collect()).unwrap_or_default()
}

/// Whether a line is a numbered search hit: `3. Title … [meta] <uri>`. A numbered list inside a
/// snippet never starts a line of its own (snippets are one line) and does not end in `<uri>`.
fn is_hit_line(line: &str) -> bool {
    let line = line.trim_end();
    line.split_once(". ").is_some_and(|(n, _)| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()))
        && line.ends_with('>')
        && line.contains(" <")
}

/// Rank of the first numbered hit line that contains `needle`.
fn rank_of(output: &str, needle: &str) -> Option<usize> {
    output.lines().filter(|l| is_hit_line(l)).position(|l| l.contains(needle)).map(|i| i + 1)
}

fn run_step(sb: &Sandbox, step: &toml::Value, score: &mut Score) {
    let t = step.as_table().expect("step is a table");
    if let Some(path) = t.get("write").and_then(|v| v.as_str()) {
        let p = sb.repo.join(sb.expand(path));
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, sb.expand(t.get("content").and_then(|v| v.as_str()).unwrap_or(""))).unwrap();
        return;
    }
    if let Some(path) = t.get("remove").and_then(|v| v.as_str()) {
        let p = sb.repo.join(sb.expand(path));
        let _ = std::fs::remove_file(&p).or_else(|_| std::fs::remove_dir_all(&p));
        return;
    }
    if let Some(path) = t.get("touch").and_then(|v| v.as_str()) {
        let path = sb.expand(path);
        let times = t.get("times").and_then(|v| v.as_integer()).unwrap_or(1);
        for i in 0..times {
            let p = sb.repo.join(&path);
            let mut text = std::fs::read_to_string(&p).unwrap_or_default();
            text.push_str(&format!("// change {i}\n"));
            std::fs::write(&p, text).unwrap();
            sb.git_dated(&["add", &path], "");
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
    let out = sb.kontext_cmd().args(&args).output().unwrap();
    let elapsed = started.elapsed().as_millis();
    let text = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    // every run step ends with its expected exit status, measured or not: an absence check on a
    // command that crashed measures nothing, and a failing setup step spoils the steps after it
    let want_exit = t.get("expect_exit").and_then(|v| v.as_integer()).unwrap_or(0);
    let exit_ok = out.status.code().map(i64::from) == Some(want_exit);
    if !exit_ok {
        score.bad_exits += 1;
        score.failures.push(format!("{args:?} exited {:?}, wanted {want_exit}", out.status.code()));
    }
    let expect = strings(t.get("expect"));
    let keep = strings(t.get("keep"));
    let forbid = strings(t.get("forbid"));
    let measured = !expect.is_empty() || !keep.is_empty() || !forbid.is_empty() || t.contains_key("rank") || t.contains_key("max_tokens");
    if !measured {
        if !exit_ok && std::env::var("KONTEXT_EVAL_VERBOSE").is_ok() {
            eprintln!("--- output of {args:?}\n{text}---");
        }
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
            score.expected += 1;
            match rank_of(&text, needle) {
                Some(r) if r <= max => score.found += 1,
                r => {
                    score.rank_misses += 1;
                    score.failures.push(format!("{needle} at rank {r:?}, wanted ≤ {max}"));
                }
            }
        }
    }
    if let Some(max) = t.get("max_tokens").and_then(|v| v.as_integer()) {
        score.expected += 1;
        if tokens > max as usize {
            score.over_budget += 1;
            score.failures.push(format!("{tokens} tokens > {max}"));
        } else {
            score.found += 1;
        }
    }
    if !score.passed() && std::env::var("KONTEXT_EVAL_VERBOSE").is_ok() {
        eprintln!("--- output of {args:?}\n{text}---");
    }
}

/// The state shown for a scenario, and whether it fails the run.
fn verdict(passed: bool, pending: Option<&str>) -> (String, bool) {
    match (passed, pending) {
        (true, None) => ("pass".to_string(), false),
        (true, Some(_)) => ("pass (remove pending)".to_string(), false),
        (false, Some(p)) => (format!("pending {p}"), false),
        (false, None) => ("FAIL".to_string(), true),
    }
}

#[test]
fn evaluation_set() {
    let path = common::manifest_dir().join("tests/eval/scenarios.toml");
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
        let sb = Sandbox::fixture(sc.get("fixture").and_then(|v| v.as_str()).unwrap_or("shop"));
        let mut score = Score::default();
        for step in sc.get("step").and_then(|v| v.as_array()).map(Vec::as_slice).unwrap_or_default() {
            run_step(&sb, step, &mut score);
        }
        let (state, fails) = verdict(score.passed(), pending);
        if fails {
            failed.push(format!("{name}: {}", score.failures.join("; ")));
        }
        rows.push((name.to_string(), item.to_string(), state, score));
    }
    let mut report = String::from(
        "| scenario | item | result | found | missed | rank | over budget | wrongly suppressed | noise | bad exit | tokens | ms |\n|---|---|---|---|---|---|---|---|---|---|---|---|\n",
    );
    let mut total = Score::default();
    for (name, item, state, s) in &rows {
        report.push_str(&format!(
            "| {name} | {item} | {state} | {}/{} | {} | {} | {} | {} | {} | {} | {} | {} |\n",
            s.found,
            s.expected,
            s.missed(),
            s.rank_misses,
            s.over_budget,
            s.suppressed,
            s.noise,
            s.bad_exits,
            s.tokens,
            s.ms
        ));
        total.add(s);
    }
    let passing = rows.iter().filter(|r| r.2.starts_with("pass")).count();
    let stale_pending = rows.iter().filter(|r| r.2 == "pass (remove pending)").count();
    report.push_str(&format!(
        "\n{passing}/{} scenarios pass · found {}/{} · missed {} (rank {}, over budget {}) · wrongly suppressed {} · noise {} · bad exit {} · {} tokens · {} ms\n",
        rows.len(),
        total.found,
        total.expected,
        total.missed(),
        total.rank_misses,
        total.over_budget,
        total.suppressed,
        total.noise,
        total.bad_exits,
        total.tokens,
        total.ms
    ));
    if stale_pending > 0 {
        report.push_str(&format!("\n{stale_pending} pending scenario(s) pass now: remove their `pending` marker.\n"));
    }
    println!("{report}");
    let out = Path::new(env!("CARGO_TARGET_TMPDIR")).join("eval-report.md");
    let _ = std::fs::write(&out, &report);
    if let Ok(json) = std::env::var("KONTEXT_EVAL_JSON") {
        let rows: Vec<Value> = rows
            .iter()
            .map(|(n, i, st, s)| serde_json::json!({"name":n,"item":i,"result":st,"found":s.found,"expected":s.expected,"missed":s.missed(),"rank_misses":s.rank_misses,"over_budget":s.over_budget,"suppressed":s.suppressed,"noise":s.noise,"bad_exits":s.bad_exits,"tokens":s.tokens,"ms":s.ms,"failures":s.failures}))
            .collect();
        let _ = std::fs::write(json, serde_json::to_string_pretty(&rows).unwrap());
    }
    assert!(failed.is_empty(), "evaluation scenarios failed:\n{}", failed.join("\n"));
}

// ------------------------------------------------------------------------------ retrieval quality

/// One query of `queries.toml`: `[[query]] id = "q1"`, `text = "…"`, `slice = "…"` (default
/// `all`), `fixture = "…"` (default `shop`).
struct Query {
    id: String,
    text: String,
    slice: String,
    fixture: String,
}

fn load_queries(text: &str) -> Vec<Query> {
    let doc: toml::Value = toml::from_str(text).expect("queries.toml parses");
    let list = doc.get("query").or_else(|| doc.get("queries")).and_then(|v| v.as_array()).cloned().unwrap_or_default();
    list.iter()
        .map(|q| {
            let get = |k: &str| q.get(k).and_then(|v| v.as_str()).map(String::from);
            Query {
                id: get("id").expect("a query has an id"),
                text: get("text").or_else(|| get("query")).expect("a query has a text"),
                slice: get("slice").unwrap_or_else(|| "all".into()),
                fixture: get("fixture").unwrap_or_else(|| "shop".into()),
            }
        })
        .collect()
}

/// An entry id or a hit URI, compared without the `kx:` scheme.
fn doc_key(s: &str) -> String {
    let s = s.trim();
    let s = s.strip_prefix('<').and_then(|s| s.strip_suffix('>')).unwrap_or(s);
    s.strip_prefix("kx:").unwrap_or(s).to_string()
}

/// `qrels.tsv`: `query_id <tab> doc_id <tab> relevance`, or the TREC form
/// `query_id <tab> 0 <tab> doc_id <tab> relevance`. Blank lines, `#` comments and a header whose
/// relevance is not a number are skipped. A doc id is an entry id or a hit URI.
fn load_qrels(text: &str) -> BTreeMap<String, BTreeMap<String, f64>> {
    let mut qrels: BTreeMap<String, BTreeMap<String, f64>> = BTreeMap::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let cols: Vec<&str> =
            if line.contains('\t') { line.split('\t').map(str::trim).collect() } else { line.split_whitespace().collect() };
        let (q, d, r) = match cols.as_slice() {
            [q, _, d, r] => (q, d, r),
            [q, d, r] => (q, d, r),
            _ => panic!("qrels line needs 3 or 4 columns: {line}"),
        };
        let Ok(rel) = r.parse::<f64>() else { continue };
        qrels.entry(q.to_string()).or_default().insert(doc_key(d), rel);
    }
    qrels
}

#[derive(Default, Clone, Copy)]
struct Metrics {
    ndcg5: f64,
    recall10: f64,
    mrr: f64,
}

/// nDCG@5 with gain 2^rel − 1, recall@10 and the reciprocal rank of the first relevant hit.
fn metrics(ranked: &[String], relevant: &BTreeMap<String, f64>) -> Metrics {
    let gain = |rel: f64| 2f64.powf(rel) - 1.0;
    let discount = |i: usize| (i as f64 + 2.0).log2();
    let dcg: f64 = ranked.iter().take(5).enumerate().map(|(i, d)| gain(*relevant.get(d).unwrap_or(&0.0)) / discount(i)).sum();
    let mut ideal: Vec<f64> = relevant.values().copied().filter(|r| *r > 0.0).collect();
    ideal.sort_by(|a, b| b.partial_cmp(a).unwrap());
    let idcg: f64 = ideal.iter().take(5).enumerate().map(|(i, r)| gain(*r) / discount(i)).sum();
    let total = relevant.values().filter(|r| **r > 0.0).count();
    let found = ranked.iter().take(10).filter(|d| relevant.get(*d).is_some_and(|r| *r > 0.0)).count();
    let first = ranked.iter().position(|d| relevant.get(d).is_some_and(|r| *r > 0.0));
    Metrics {
        ndcg5: if idcg > 0.0 { dcg / idcg } else { 0.0 },
        recall10: if total > 0 { found as f64 / total as f64 } else { 0.0 },
        mrr: first.map(|i| 1.0 / (i as f64 + 1.0)).unwrap_or(0.0),
    }
}

/// How far a slice may fall below its baseline before the run fails: exact identifiers are
/// near-deterministic, the other slices move with ranking changes.
fn tolerance(slice: &str) -> f64 {
    if slice == "exact-identifier" { 0.01 } else { 0.02 }
}

fn search_ranked(sb: &Sandbox, text: &str) -> Vec<String> {
    let out = sb.kontext_cmd().args(["search", "--json", "-n", "10", "--", text]).output().unwrap();
    assert!(out.status.success(), "search {text:?} exited {:?}: {}", out.status.code(), String::from_utf8_lossy(&out.stderr));
    let v: Value = serde_json::from_slice(&out.stdout).unwrap_or_else(|e| panic!("{e}: {}", String::from_utf8_lossy(&out.stdout)));
    v["hits"].as_array().map(|hits| hits.iter().filter_map(|h| h["uri"].as_str()).map(doc_key).collect()).unwrap_or_default()
}

#[test]
fn retrieval_quality() {
    let dir = common::manifest_dir().join("tests/eval");
    let (Ok(qrels), Ok(queries)) = (std::fs::read_to_string(dir.join("qrels.tsv")), std::fs::read_to_string(dir.join("queries.toml")))
    else {
        println!("retrieval quality: no tests/eval/qrels.tsv and queries.toml, skipped");
        return;
    };
    let qrels = load_qrels(&qrels);
    let queries = load_queries(&queries);
    let mut sandboxes: BTreeMap<String, Sandbox> = BTreeMap::new();
    let mut per_slice: BTreeMap<String, Vec<Metrics>> = BTreeMap::new();
    let mut rows = String::from("| query | slice | nDCG@5 | recall@10 | MRR |\n|---|---|---|---|---|\n");
    for q in &queries {
        let Some(relevant) = qrels.get(&q.id) else {
            panic!("query {} has no judgments in qrels.tsv", q.id);
        };
        let sb = sandboxes.entry(q.fixture.clone()).or_insert_with(|| Sandbox::fixture(&q.fixture));
        let m = metrics(&search_ranked(sb, &q.text), relevant);
        rows.push_str(&format!("| {} | {} | {:.3} | {:.3} | {:.3} |\n", q.id, q.slice, m.ndcg5, m.recall10, m.mrr));
        per_slice.entry(q.slice.clone()).or_default().push(m);
        if q.slice != "all" {
            per_slice.entry("all".into()).or_default().push(m);
        }
    }
    let mean = |ms: &[Metrics]| {
        let n = ms.len().max(1) as f64;
        Metrics {
            ndcg5: ms.iter().map(|m| m.ndcg5).sum::<f64>() / n,
            recall10: ms.iter().map(|m| m.recall10).sum::<f64>() / n,
            mrr: ms.iter().map(|m| m.mrr).sum::<f64>() / n,
        }
    };
    let slices: BTreeMap<String, (usize, Metrics)> = per_slice.iter().map(|(s, ms)| (s.clone(), (ms.len(), mean(ms)))).collect();

    let baseline_path = dir.join("qrels-baseline.toml");
    let baseline: Option<toml::Value> =
        std::fs::read_to_string(&baseline_path).ok().map(|t| toml::from_str(&t).expect("qrels-baseline.toml parses"));
    let mut report =
        String::from("| slice | queries | nDCG@5 | recall@10 | MRR | baseline (nDCG@5 / recall@10 / MRR) |\n|---|---|---|---|---|---|\n");
    let mut regressions = Vec::new();
    let mut improvements = Vec::new();
    for (slice, (n, m)) in &slices {
        let base = baseline.as_ref().and_then(|b| b.get("slice")).and_then(|s| s.get(slice));
        let shown = match base {
            Some(b) => {
                let tol = tolerance(slice);
                let mut cells = Vec::new();
                for (name, now) in [("ndcg5", m.ndcg5), ("recall10", m.recall10), ("mrr", m.mrr)] {
                    let Some(then) = b.get(name).and_then(|v| v.as_float().or_else(|| v.as_integer().map(|i| i as f64))) else {
                        cells.push("–".to_string());
                        continue;
                    };
                    cells.push(format!("{then:.3}"));
                    if now < then - tol {
                        regressions.push(format!("{slice} {name} {now:.3} < baseline {then:.3} − {tol}"));
                    } else if now > then + tol {
                        improvements.push(format!("{slice} {name} {now:.3} > baseline {then:.3}"));
                    }
                }
                cells.join(" / ")
            }
            None => "none".to_string(),
        };
        report.push_str(&format!("| {slice} | {n} | {:.3} | {:.3} | {:.3} | {shown} |\n", m.ndcg5, m.recall10, m.mrr));
    }
    report.push('\n');
    report.push_str(&rows);
    if !improvements.is_empty() {
        report.push_str(&format!("\nAbove the baseline (update tests/eval/qrels-baseline.toml): {}\n", improvements.join("; ")));
    }
    println!("{report}");
    let _ = std::fs::write(Path::new(env!("CARGO_TARGET_TMPDIR")).join("eval-retrieval.md"), &report);

    if std::env::var("KONTEXT_UPDATE_QRELS_BASELINE").is_ok() {
        let mut text =
            String::from("# Retrieval baseline for tests/eval.rs (retrieval_quality); regenerate with KONTEXT_UPDATE_QRELS_BASELINE=1.\n");
        for (slice, (_, m)) in &slices {
            text.push_str(&format!("\n[slice.\"{slice}\"]\nndcg5 = {:.4}\nrecall10 = {:.4}\nmrr = {:.4}\n", m.ndcg5, m.recall10, m.mrr));
        }
        std::fs::write(&baseline_path, text).unwrap();
        println!("wrote {}", baseline_path.display());
        return;
    }
    assert!(regressions.is_empty(), "retrieval quality fell below tests/eval/qrels-baseline.toml:\n{}", regressions.join("\n"));
}

#[cfg(test)]
mod scoring {
    use super::*;

    #[test]
    fn hit_lines_are_numbered_and_end_in_a_uri() {
        let out = "1. Orders live in PostgreSQL — one db [decision] <kx:orders>\n2. Steps: 1. do this [learning] <kx:steps>\n   ↳ similar, differs: x [] <kx:y>\n3. not a hit\n";
        assert_eq!(rank_of(out, "<kx:steps>"), Some(2));
        assert_eq!(rank_of(out, "<kx:y>"), None);
        assert_eq!(rank_of(out, "not a hit"), None);
    }

    #[test]
    fn pending_scenarios_that_pass_say_so() {
        assert_eq!(verdict(true, Some("4.1")).0, "pass (remove pending)");
        assert_eq!(verdict(false, Some("4.1")), ("pending 4.1".to_string(), false));
        assert!(verdict(false, None).1);
    }

    #[test]
    fn retrieval_metrics() {
        let rel: BTreeMap<String, f64> = [("a".to_string(), 2.0), ("b".to_string(), 1.0)].into();
        let perfect = metrics(&["a".into(), "b".into()], &rel);
        assert!((perfect.ndcg5 - 1.0).abs() < 1e-9 && perfect.recall10 == 1.0 && perfect.mrr == 1.0);
        let late = metrics(&["x".into(), "b".into()], &rel);
        assert!(late.mrr == 0.5 && late.recall10 == 0.5 && late.ndcg5 < 0.5);
        let qrels = load_qrels("# q doc rel\nquery\tdoc\trelevance\nq1\t0\tkx:a\t2\nq1 b 1\n");
        assert_eq!(qrels["q1"]["a"], 2.0);
        assert_eq!(qrels["q1"]["b"], 1.0);
        assert_eq!(doc_key("<kx:a>"), "a");
    }
}
