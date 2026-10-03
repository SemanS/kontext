//! Core operations shared by the CLI and the MCP server.
use crate::app::App;
use crate::events::{self, Outbox};
use crate::glob::{path_matches, path_overlaps};
use crate::inbox::{Inbox, Origin};
use crate::index::SearchOpts;
use crate::model::{self, Hit, HitGroup};
use crate::secrets::{self, Finding};
use crate::store::{Entry, Issue, Level, Store};
use crate::util;
use anyhow::{Context, Result, anyhow, bail};
use serde::Serialize;
use std::collections::{BTreeMap, HashSet};
use std::fmt::Write as _;
use std::time::Duration;

pub fn normalize_kind(app: &App, raw: &str) -> Option<(String, Option<&'static str>)> {
    let k = raw.trim().to_lowercase();
    let known = |k: &str| app.cfg().store.kinds.contains_key(k);
    if known(&k) {
        return Some((k, None));
    }
    let alias: Option<(&str, Option<&'static str>)> = match k.as_str() {
        "adr" | "adrs" | "decisions" => Some(("decision", None)),
        "pitfall" | "pitfalls" | "gotcha" | "gotchas" => Some(("learning", Some("pitfall"))),
        "note" | "notes" | "learnings" | "lesson" | "lessons" => Some(("learning", None)),
        "conventions" | "rule" | "rules" | "guideline" => Some(("convention", None)),
        "incidents" | "postmortem" | "outage" => Some(("incident", None)),
        "module" | "modules" => Some(("architecture", None)),
        _ => None,
    };
    if let Some((a, tag)) = alias
        && known(a)
    {
        return Some((a.to_string(), tag));
    }
    let singular = k.trim_end_matches('s');
    if known(singular) {
        return Some((singular.to_string(), None));
    }
    None
}

fn kick_outbox(app: &App) {
    if cfg!(test) {
        return;
    }
    if app.cfg().adapters.values().any(|a| !a.on.is_empty()) {
        events::spawn_background_flush(&app.repo);
    }
}

// ------------------------------------------------------------------------------------ search

#[derive(Debug, Clone, Default)]
pub struct SearchReq {
    pub query: String,
    pub kinds: Vec<String>,
    pub sources: Vec<String>,
    pub limit: usize,
}

pub fn search(app: &App, req: &SearchReq) -> Result<(Vec<Hit>, Vec<String>)> {
    let limit = req.limit.clamp(1, 50);
    let mut notes = Vec::new();
    let mut groups = Vec::new();
    let want_local = req.sources.is_empty() || req.sources.iter().any(|s| s == "local");
    if want_local {
        let mut kinds = Vec::new();
        for k in &req.kinds {
            match k.to_lowercase().as_str() {
                "doc" | "docs" => kinds.push("doc".to_string()),
                "commit" | "commits" | "history" => kinds.push("commit".to_string()),
                other => match normalize_kind(app, other) {
                    Some((kk, _)) => kinds.push(kk),
                    None => notes.push(format!("unknown kind '{other}' ignored")),
                },
            }
        }
        let opts = SearchOpts { limit, kinds: kinds.clone(), sources: Vec::new() };
        let hits = app.with_index(|idx| idx.search(&req.query, &opts))?;
        groups.push(HitGroup { source: "local".into(), weight: 1.0, hits });
        let mut local_notes = inbox_hits(app, &req.query, &kinds, limit);
        // a submodule's team knowledge belongs to the work done here too (camp-bot's `extractor/`),
        // and so do the notes captured into its inbox from here
        for n in crate::route::nested_stores(app).into_iter().take(3) {
            let label = format!("{}/", n.rel);
            match n.app.with_index(|idx| idx.search(&req.query, &opts)) {
                Ok(mut hits) => {
                    for h in &mut hits {
                        h.uri = crate::route::prefix_uri(&n.rel, &h.uri);
                        h.source = label.clone();
                    }
                    groups.push(HitGroup { source: label.clone(), weight: 0.95, hits });
                }
                Err(e) => notes.push(format!("{label}: {}", util::truncate_chars(&format!("{e:#}"), 200))),
            }
            for mut h in inbox_hits(&n.app, &req.query, &kinds, limit) {
                h.uri = crate::route::prefix_uri(&n.rel, &h.uri);
                h.status = h.status.map(|st| format!("{st} in {label}"));
                local_notes.push(h);
            }
        }
        if !local_notes.is_empty() {
            local_notes.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
            groups.push(HitGroup { source: "inbox".into(), weight: 0.9, hits: local_notes });
        }
    }
    let external: Vec<String> = req.sources.iter().filter(|s| *s != "local").cloned().collect();
    if req.sources.is_empty() || !external.is_empty() {
        let q = req.query.clone();
        let reg = app.registry();
        let results = reg.fanout("search", &external, Duration::from_secs(12), move |a| a.search(&q, limit));
        for (name, r) in results {
            match r {
                Ok(hits) => {
                    let w = reg.get(&name).map(|a| a.weight()).unwrap_or(0.9);
                    groups.push(HitGroup { source: name, weight: w, hits });
                }
                Err(e) => notes.push(format!("{name}: {}", util::truncate_chars(&format!("{e:#}"), 200))),
            }
        }
    }
    Ok((model::merge(groups, limit), notes))
}

/// Inbox notes that match a query. The inbox is a handful of files outside the repository, so it
/// is scanned rather than indexed. Without this, a private capture in a repository without a store
/// was written once and never found again.
fn inbox_hits(app: &App, query: &str, kinds: &[String], limit: usize) -> Vec<Hit> {
    let terms: Vec<String> = format!("{query} {}", crate::index::expand_identifiers(query))
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.len() > 2)
        .map(str::to_string)
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    if terms.is_empty() {
        return Vec::new();
    }
    let mut hits: Vec<Hit> = Vec::new();
    for e in Inbox::open(&app.repo).list() {
        if !kinds.is_empty() && !kinds.contains(&e.kind) {
            continue;
        }
        let title = e.title.to_lowercase();
        let meta = format!("{} {} {}", e.summary.as_deref().unwrap_or(""), e.tags.join(" "), e.paths.join(" ")).to_lowercase();
        let body = e.body.to_lowercase();
        let (mut score, mut matched) = (0.0f32, 0usize);
        for t in &terms {
            let s = [(title.contains(t.as_str()), 3.0), (meta.contains(t.as_str()), 2.0), (body.contains(t.as_str()), 1.0)]
                .iter()
                .filter(|(hit, _)| *hit)
                .map(|(_, w)| w)
                .sum::<f32>();
            if s > 0.0 {
                matched += 1;
                score += s;
            }
        }
        // most of the query, not one common word
        if matched * 2 < terms.len() || score < 2.0 {
            continue;
        }
        let private = e.visibility.as_deref() == Some("private");
        hits.push(Hit {
            source: "inbox".into(),
            uri: format!("inbox:{}", e.id),
            title: e.title.clone(),
            snippet: e.summary_text(200),
            kind: Some(e.kind.clone()),
            score,
            date: e.date.clone(),
            status: Some(if private { "local note, private".into() } else { "local note".into() }),
        });
    }
    hits.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    hits.truncate(limit);
    hits
}

// -------------------------------------------------------------------------------------- read

fn show_commit(app: &App, sha: &str, level: u8) -> Result<String> {
    Ok(match level {
        0 => app.repo.git(&["show", "-s", "--format=%h %ad %an: %s", "--date=short", sha])?,
        1 => app.repo.git(&["show", "--stat", "--format=commit %H%nAuthor: %an%nDate: %ad%n%n%B", "--date=short", sha])?,
        _ => {
            util::truncate_chars(&app.repo.git(&["show", "--format=commit %H%nAuthor: %an%nDate: %ad%n%n%B", "--date=short", sha])?, 60_000)
        }
    })
}

pub fn read(app: &App, uri: &str, level: u8) -> Result<String> {
    let uri = uri.trim();
    // a nested repository's entry, commit or note, as search and the brief name them: kx:<nested>/<id>
    for scheme in ["kx:", "git:", "inbox:"] {
        if let Some(rest) = uri.strip_prefix(scheme)
            && rest.contains('/')
            && let Some((root, inner, n)) = crate::route::nested_of(app, rest)
            && !inner.is_empty()
        {
            return read(&n, &format!("{scheme}{inner}"), level).map(|t| nested_refs(&t, &root));
        }
    }
    if let Some(id) = uri.strip_prefix("inbox:") {
        let e = Inbox::open(&app.repo).get(id).ok_or_else(|| anyhow!("no inbox entry '{id}'"))?;
        return Ok(e.markdown());
    }
    if let Some(sha) = uri.strip_prefix("git:") {
        let sha = sha.trim();
        if !sha.chars().all(|c| c.is_ascii_hexdigit()) || sha.len() < 4 {
            bail!("'{sha}' is not a commit id");
        }
        return match show_commit(app, sha, level) {
            Ok(text) => Ok(text),
            // a commit of a submodule, named as `ctx_why` lists it there
            Err(e) => crate::route::nested_repos(app)
                .iter()
                .find_map(|n| show_commit(&n.app, sha, level).ok().map(|t| format!("(commit in {}/)\n{t}", n.rel)))
                .ok_or(e),
        };
    }
    if uri.contains("://") {
        let reg = app.registry();
        for a in &reg.adapters {
            if a.owns(uri) {
                return a.read(uri, level);
            }
        }
        bail!("no adapter can read '{uri}' (configure a `read` op with `owns = [\"{}\"]`)", uri.split("://").next().unwrap_or(""));
    }
    let key = uri.strip_prefix("kx:").unwrap_or(uri);
    if !uri.starts_with("file:") {
        let (entries, _) = app.entries();
        if let Some(e) = Store::find(&entries, key) {
            return Ok(render_entry(e, level));
        }
        if uri.starts_with("kx:") {
            // an entry of a nested repository named without its path
            let nested = crate::route::nested_stores(app);
            let mut several: Vec<String> = entries.iter().filter(|e| e.id.starts_with(key)).map(|e| e.id.clone()).collect();
            for n in &nested {
                let (theirs, _) = n.app.entries();
                if let Some(e) = Store::find(&theirs, key) {
                    return Ok(format!("(entry of {}/)\n{}", n.rel, nested_refs(&render_entry(e, level), &n.rel)));
                }
                several.extend(theirs.iter().filter(|e| e.id.starts_with(key)).map(|e| format!("{}/{}", n.rel, e.id)));
            }
            if several.len() > 1 {
                several.truncate(6);
                bail!("'{key}' names several entries: {}", several.join(", "));
            }
            bail!("no entry '{key}' (`ctx_log` lists them, `ctx_search` finds them)");
        }
    }
    let raw = uri.strip_prefix("file:").unwrap_or(uri);
    let (path, anchor) = match raw.split_once('#') {
        Some((p, a)) => (p, Some(a)),
        None => (raw, None),
    };
    read_file(app, path, anchor, level)
}

/// A nested repository's answer with its references as the outer repository names them.
fn nested_refs(text: &str, root: &str) -> String {
    text.replace("<kx:", &format!("<kx:{root}/")).replace("<git:", &format!("<git:{root}/"))
}

fn render_entry(e: &Entry, level: u8) -> String {
    match level {
        0 => format!("{} [{}{}] <kx:{}>", e.l0(240), e.kind, e.status.as_deref().map(|s| format!(" · {s}")).unwrap_or_default(), e.id),
        1 => {
            let mut s = format!("# {}\n{} · {}", e.title, e.kind, e.rel_path);
            if let Some(st) = &e.status {
                let _ = write!(s, " · {st}");
            }
            if let Some(d) = &e.date {
                let _ = write!(s, " · {d}");
            }
            if !e.paths.is_empty() {
                let _ = write!(s, "\npaths: {}", e.paths.join(", "));
            }
            let _ = write!(s, "\n\n{}", util::truncate_chars(e.body.trim(), 4000));
            s
        }
        _ => e.markdown(),
    }
}

fn read_file(app: &App, path: &str, anchor: Option<&str>, level: u8) -> Result<String> {
    let rel = path.trim_start_matches("./").trim_start_matches('/');
    if rel.split('/').any(|seg| seg == "..") {
        bail!("path escapes the repository");
    }
    if secrets::is_sensitive_path(rel) {
        bail!("refusing to read '{rel}': it looks like a secrets file");
    }
    let abs = app.repo.abs(rel);
    let canon = abs.canonicalize().with_context(|| format!("no such file: {rel}"))?;
    let root = app.repo.root.canonicalize()?;
    if !canon.starts_with(&root) {
        bail!("path escapes the repository");
    }
    if canon.is_dir() {
        let mut names: Vec<String> = std::fs::read_dir(&canon)?.flatten().map(|e| e.file_name().to_string_lossy().to_string()).collect();
        names.sort();
        return Ok(format!("{rel}/ ({} entries)\n{}", names.len(), names.join("\n")));
    }
    let text = std::fs::read_to_string(&canon).with_context(|| format!("{rel} is not a text file"))?;
    let section = anchor.and_then(|a| find_section(&text, a));
    Ok(match level {
        0 => util::first_paragraph(section.as_deref().unwrap_or(&text), 300),
        1 => util::truncate_chars(section.as_deref().unwrap_or(&text), 4000),
        _ => util::truncate_chars(section.as_deref().unwrap_or(&text), 80_000),
    })
}

fn find_section(text: &str, anchor: &str) -> Option<String> {
    let mut out = String::new();
    let mut inside = false;
    let mut depth = 0;
    for line in text.lines() {
        let t = line.trim_start();
        if t.starts_with('#') {
            let d = t.chars().take_while(|c| *c == '#').count();
            let h = t.trim_start_matches('#').trim();
            if inside && d <= depth {
                break;
            }
            if !inside && util::slugify(h, 60) == anchor {
                inside = true;
                depth = d;
            }
        }
        if inside {
            out.push_str(line);
            out.push('\n');
        }
    }
    (!out.is_empty()).then_some(out)
}

// ------------------------------------------------------------------------- capture & promote

#[derive(Debug, Clone, Default)]
pub struct CaptureReq {
    pub kind: String,
    pub title: String,
    pub summary: Option<String>,
    pub body: String,
    pub paths: Vec<String>,
    pub tags: Vec<String>,
    pub visibility: Option<String>,
    pub supersedes: Vec<String>,
    pub status: Option<String>,
    pub promote: bool,
    pub origin: String,
    /// Where it was found (e.g. `claude:4f1c2a9b`); kept in the inbox, dropped on promotion.
    pub source: Option<String>,
    /// Commits it came from.
    pub commits: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct CaptureOut {
    pub id: String,
    pub location: String,
    pub promoted: bool,
    pub visibility: String,
    pub redactions: usize,
    pub notes: Vec<String>,
}

pub fn capture(app: &App, mut req: CaptureReq) -> Result<CaptureOut> {
    // `--paths a,` or an agent's [""] must not become an entry's paths, tags or supersedes
    for list in [&mut req.paths, &mut req.tags, &mut req.supersedes] {
        list.iter_mut().for_each(|v| *v = v.trim().to_string());
        list.retain(|v| !v.is_empty());
    }
    let (kind, implied_tag) = normalize_kind(app, &req.kind)
        .ok_or_else(|| anyhow!("unknown kind '{}' (use one of: {})", req.kind, app.cfg().kind_names().join(", ")))?;
    if req.title.trim().is_empty() {
        bail!("title is required");
    }
    if req.body.trim().is_empty() && req.summary.as_deref().is_none_or(|s| s.trim().is_empty()) {
        bail!("body (or summary) is required — say what was decided or learned and why");
    }
    let visibility = req.visibility.clone().unwrap_or_else(|| app.cfg().capture.default_visibility.clone()).to_lowercase();
    if visibility != "team" && visibility != "private" {
        bail!("visibility must be 'team' or 'private'");
    }
    if req.promote && visibility == "private" {
        bail!("private entries stay in the local inbox; use visibility=team to write into the shared store");
    }
    let mut redactions = 0;
    let mut clean = |s: &str| -> String {
        if app.cfg().capture.redact {
            let (r, n) = secrets::redact(s);
            redactions += n;
            r
        } else {
            s.to_string()
        }
    };
    let title = clean(req.title.trim());
    let body = clean(req.body.trim());
    let summary = req.summary.as_deref().map(|s| clean(s.trim())).filter(|s| !s.is_empty());
    let mut notes = Vec::new();
    if redactions > 0 {
        notes.push(format!("{redactions} secret-looking value(s) were redacted"));
    }
    let mut tags = req.tags.clone();
    if let Some(t) = implied_tag
        && !tags.iter().any(|x| x == t)
    {
        tags.push(t.to_string());
    }
    let status =
        req.status.clone().map(|s| crate::store::normalize_status(&s)).or_else(|| (kind == "decision").then(|| "accepted".to_string()));
    let body = if body.is_empty() { String::new() } else { format!("{body}\n") };
    let mut entry = Entry {
        kind: kind.clone(),
        title,
        status,
        date: Some(util::today()),
        summary,
        tags,
        paths: req.paths.iter().map(|p| p.trim().trim_start_matches("./").to_string()).filter(|p| !p.is_empty()).collect(),
        supersedes: req.supersedes.clone(),
        author: app.repo.user_name(),
        visibility: Some(visibility.clone()),
        origin: Some(req.origin.clone()),
        commits: req.commits.iter().map(|c| c.trim().chars().take(12).collect::<String>()).filter(|c| !c.is_empty()).collect(),
        body,
        ..Default::default()
    };
    if let Some(src) = req.source.as_deref().filter(|s| !s.is_empty()) {
        entry.set_extra("source", crate::store::FmValue::Str(src.to_string()));
    }
    // similar entries already in the store?
    if let Ok(hits) =
        app.with_index(|idx| idx.search(&entry.title, &SearchOpts { limit: 3, kinds: vec![kind.clone()], sources: vec!["entry".into()] }))
    {
        for h in hits {
            if title_similarity(&h.title, &entry.title) >= 0.6 {
                notes.push(format!("similar existing entry: {} <{}> — update it instead if this is the same thing", h.title, h.uri));
            }
        }
    }
    let _lock = app.write_lock.lock().unwrap();
    let outbox = Outbox::open(&app.repo);
    if req.promote {
        entry.visibility = None;
        let store = app.store();
        let rel = store.write_new(&mut entry)?;
        apply_supersedes(app, &entry, false)?;
        outbox.push("promote", events::entry_payload(&app.repo, &entry, "team"))?;
        kick_outbox(app);
        return Ok(CaptureOut { id: entry.id.clone(), location: rel, promoted: true, visibility, redactions, notes });
    }
    let saved = Inbox::open(&app.repo).add(&app.repo, entry)?;
    outbox.push("capture", events::entry_payload(&app.repo, &saved, &visibility))?;
    kick_outbox(app);
    Ok(CaptureOut { id: saved.id.clone(), location: format!("inbox:{}", saved.id), promoted: false, visibility, redactions, notes })
}

pub(crate) fn title_similarity(a: &str, b: &str) -> f32 {
    let words = |s: &str| -> HashSet<String> {
        s.to_lowercase().split(|c: char| !c.is_alphanumeric()).filter(|w| w.len() > 2).map(str::to_string).collect()
    };
    let (x, y) = (words(a), words(b));
    if x.is_empty() || y.is_empty() {
        return 0.0;
    }
    x.intersection(&y).count() as f32 / x.union(&y).count() as f32
}

fn apply_supersedes(app: &App, entry: &Entry, stage: bool) -> Result<Vec<String>> {
    let mut touched = Vec::new();
    if entry.supersedes.is_empty() {
        return Ok(touched);
    }
    let store = app.store();
    let (entries, _) = store.load_all();
    for s in &entry.supersedes {
        match Store::find(&entries, s) {
            Some(old) if old.id != entry.id => {
                store.mark_superseded(old, &entry.id)?;
                touched.push(old.rel_path.clone());
            }
            _ => eprintln!("kontext: supersedes '{s}': no such entry"),
        }
    }
    if stage && !touched.is_empty() {
        let mut args = vec!["add", "--"];
        args.extend(touched.iter().map(String::as_str));
        app.repo.git(&args)?;
    }
    Ok(touched)
}

/// Promote inbox candidates into the store (working tree), optionally staging them.
pub fn promote(app: &App, ids: &[String], stage: bool) -> Result<Vec<(String, String)>> {
    let inbox = Inbox::open(&app.repo);
    let store = app.store();
    let outbox = Outbox::open(&app.repo);
    let _lock = app.write_lock.lock().unwrap();
    let mut done = Vec::new();
    for id in ids {
        let cand = inbox.get(id).ok_or_else(|| anyhow!("no inbox entry '{id}' (see `kontext inbox`)"))?;
        if cand.visibility.as_deref() == Some("private") {
            bail!("'{}' is private; recapture it with visibility=team if the team should see it", cand.id);
        }
        let mut e = cand.clone();
        e.extra.retain(|(k, _)| !matches!(k.as_str(), "captured" | "branch" | "worktree" | "source"));
        e.visibility = None;
        e.id.clear();
        e.rel_path.clear();
        // a decision captured without paths governs the change it ships with (conventions are
        // usually repository-wide and keep none)
        if stage && e.paths.is_empty() && e.kind != "convention" {
            e.paths = change_paths(app);
        }
        let rel = store.write_new(&mut e)?;
        let superseded = apply_supersedes(app, &e, stage)?;
        inbox.mark_promoted(&cand.id, &rel)?;
        if stage {
            app.repo.git(&["add", "--", &rel])?;
        }
        outbox.push("promote", events::entry_payload(&app.repo, &e, "team"))?;
        let _ = superseded;
        done.push((cand.id.clone(), rel));
    }
    kick_outbox(app);
    Ok(done)
}

/// The code a commit in progress changes: what is staged, else the working tree (up to 8 paths).
fn change_paths(app: &App) -> Vec<String> {
    let store = app.store();
    let code = |list: Vec<(char, String)>| -> Vec<String> {
        list.into_iter().filter(|(st, p)| *st != 'D' && !store.is_store_path(p)).map(|(_, p)| p).collect()
    };
    let staged = code(app.repo.staged_paths().unwrap_or_default());
    let mut paths = if staged.is_empty() { code(app.repo.changed_paths().unwrap_or_default()) } else { staged };
    paths.truncate(8);
    paths
}

// ------------------------------------------------------------------------------------- brief

pub fn module_docs(entries: &[Entry]) -> Vec<&Entry> {
    entries.iter().filter(|e| e.kind == "architecture" && e.id.starts_with("mod-")).collect()
}

pub fn init_progress(entries: &[Entry]) -> (usize, usize) {
    let mods = module_docs(entries);
    let done = mods.iter().filter(|e| e.get_extra("deepened").is_some()).count();
    (done, mods.len())
}

struct Focus {
    paths: Vec<String>,
    words: Vec<String>,
}

fn parse_focus(app: &App, focus: &[String]) -> Focus {
    let mut f = Focus { paths: Vec::new(), words: Vec::new() };
    for item in focus {
        let t = item.trim().trim_start_matches("./");
        if t.is_empty() {
            continue;
        }
        let as_path = t.split(':').next().unwrap_or(t);
        if app.repo.abs(as_path).exists() {
            f.paths.push(as_path.trim_end_matches('/').to_string());
        } else {
            f.words.extend(t.to_lowercase().split(|c: char| !c.is_alphanumeric()).filter(|w| w.len() > 2).map(str::to_string));
        }
    }
    f
}

fn relevance(e: &Entry, f: &Focus) -> f32 {
    let mut s = 0.0;
    for p in &f.paths {
        if e.paths.iter().any(|ep| path_overlaps(ep, p) || path_matches(ep, p)) {
            s += 10.0;
        }
    }
    if !f.words.is_empty() {
        let hay = format!("{} {} {}", e.title, e.summary_text(300), e.tags.join(" ")).to_lowercase();
        s += f.words.iter().filter(|w| hay.contains(w.as_str())).count() as f32 * 3.0;
    }
    s
}

/// How a brief refers to entries: an ADR keeps its number, any other id the shortest prefix, cut
/// before a `-`, that no other id starts with, which is what `Store::find` resolves alone (a dated
/// id keeps the date and a word, a slug two words). Ids sharing a prefix sit next to each other
/// once sorted, so only an id's neighbours are compared.
pub struct Refs {
    sorted: Vec<String>,
}

impl Refs {
    pub fn new<'a>(ids: impl IntoIterator<Item = &'a str>) -> Refs {
        // duplicates stay: an id two entries share has no unique prefix and keeps its full form
        let mut sorted: Vec<String> = ids.into_iter().map(str::to_string).collect();
        sorted.sort();
        Refs { sorted }
    }

    pub fn of(&self, e: &Entry) -> String {
        let short = e.short_id();
        if short != e.id { short } else { self.prefix(&e.id) }
    }

    pub fn prefix(&self, id: &str) -> String {
        let Ok(i) = self.sorted.binary_search_by(|x| x.as_str().cmp(id)) else { return id.to_string() };
        let prev = i.checked_sub(1).map(|j| self.sorted[j].as_str());
        let next = self.sorted.get(i + 1).map(String::as_str);
        let dated = id.get(..10).is_some_and(|d| chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d").is_ok());
        // a slug keeps two words: `batch-concurrency` reads, `batch` does not
        for (k, _) in id.match_indices('-').skip(if dated { 3 } else { 1 }) {
            let cand = &id[..k];
            if !prev.is_some_and(|p| p.starts_with(cand)) && !next.is_some_and(|n| n.starts_with(cand)) {
                return cand.to_string();
            }
        }
        id.to_string()
    }
}

/// Which part of a brief is written: a whole brief, or a nested repository's section of an outer
/// one. `prefix` (`extractor/`) goes before its references so they resolve from the outer repository.
#[derive(Clone, Copy)]
struct Part<'a> {
    prefix: &'a str,
    section: bool,
}

type Elsewhere = (std::sync::Arc<App>, Option<String>, Vec<String>);

pub fn brief(app: &App, focus: &[String], budget: usize, with_adapters: bool) -> String {
    let split = crate::route::split_focus(app, focus);
    // every focus path lies in one other repository (another worktree, a sibling project, or a
    // submodule of a repository without team knowledge of its own): that repository's brief answers
    if split.here_paths == 0
        && let [(other, nested, paths)] = split.elsewhere.as_slice()
        && (nested.is_none() || !app.is_set_up())
    {
        let mut f = paths.clone();
        f.extend(split.words.iter().cloned());
        let prefix = nested.as_ref().map(|n| format!("{n}/")).unwrap_or_default();
        let inner = brief_in(other, &f, budget, with_adapters, Part { prefix: &prefix, section: false }, &[]);
        // the outer repository's own agent rules still apply to work inside its submodule
        let rules: Vec<&str> = if nested.is_some() {
            ["AGENTS.md", "CLAUDE.md"].into_iter().filter(|r| app.repo.abs(r).is_file()).collect()
        } else {
            Vec::new()
        };
        let also =
            if rules.is_empty() { String::new() } else { format!("\nRules of {} itself: read {}", app.project_name(), rules.join(", ")) };
        return format!("{}{also}\n{inner}", crate::route::banner(app, other));
    }
    brief_in(app, &split.here, budget, with_adapters, Part { prefix: "", section: false }, &split.elsewhere)
}

/// One line about a nested repository's own team knowledge and how to reach it.
fn nested_pointer(a: &App, rel: &str) -> String {
    let (entries, _) = a.entries();
    let what: Vec<String> = ["decision", "convention", "learning", "incident"]
        .iter()
        .filter_map(|k| {
            let n = entries.iter().filter(|e| e.kind == *k && e.is_active()).count();
            (n > 0).then(|| format!("{n} {k}{}", if n == 1 { "" } else { "s" }))
        })
        .collect();
    format!(
        "- {rel}/ keeps its own team knowledge ({}): ctx_brief with focus paths under {rel}/, the other tools with `dir: \"{rel}\"`",
        if what.is_empty() { "a module map".to_string() } else { what.join(", ") }
    )
}

/// This clone's inbox notes that bear on the focus (without one, the newest): captured, not yet
/// reviewed or shared, and in a repository without a store the only knowledge there is.
fn local_notes(app: &App, focus: &Focus, h2: &str, prefix: &str) -> String {
    let notes = Inbox::open(&app.repo).list();
    let focused = !focus.paths.is_empty() || !focus.words.is_empty();
    let mut ranked: Vec<(&Entry, f32)> = notes.iter().map(|e| (e, relevance(e, focus))).filter(|(_, r)| !focused || *r > 0.0).collect();
    if ranked.is_empty() {
        return String::new();
    }
    // stable: the newest first among equals
    ranked.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    let refs = Refs::new(notes.iter().map(|e| e.id.as_str()));
    let max = if focused { 4 } else { 3 };
    let mut s = format!("\n{h2} Local notes (this clone's inbox: not reviewed, not shared)\n");
    for (e, _) in ranked.iter().take(max) {
        let private = if e.visibility.as_deref() == Some("private") { ", private" } else { "" };
        let _ = writeln!(s, "- [{}{private}] {} <inbox:{prefix}{}>", e.kind, e.l0(140), refs.prefix(&e.id));
    }
    if ranked.len() > max {
        let _ = writeln!(s, "- … {} more: `ctx_search` finds them", ranked.len() - max);
    }
    s
}

fn brief_in(app: &App, focus: &[String], budget: usize, with_adapters: bool, part: Part, elsewhere: &[Elsewhere]) -> String {
    let cfg = app.cfg();
    let (entries, errors) = app.entries();
    let focus = parse_focus(app, focus);
    let focused = !focus.paths.is_empty() || !focus.words.is_empty();
    let h2 = if part.section { "###" } else { "##" };
    let mut budget_left = budget.max(300) as isize;
    let mut out = String::new();
    let add = |out: &mut String, text: &str, budget_left: &mut isize| -> bool {
        let cost = util::est_tokens(text) as isize;
        if cost > *budget_left {
            return false;
        }
        *budget_left -= cost;
        out.push_str(text);
        true
    };
    let refs = Refs::new(entries.iter().map(|e| e.id.as_str()));
    let reference = |e: &Entry| format!("{}{}", part.prefix, refs.of(e));

    // the state comes last but must not be crowded out (a stale decision or missing hooks matter
    // most when knowledge fills the brief): its lines are written now and their budget kept aside
    let mut state = String::new();
    if !part.section {
        let inbox = Inbox::open(&app.repo).list();
        let (ours, elsewhere_wt): (Vec<&Entry>, Vec<&Entry>) = inbox.iter().partition(|e| Origin::of(&app.repo, e).is_ours());
        if !ours.is_empty() {
            let team = ours.iter().filter(|e| e.visibility.as_deref() != Some("private")).count();
            let _ = writeln!(
                state,
                "- Local inbox: {team} team candidate(s), {} private — not shared until promoted and committed (`ctx_prepare_commit`).{}",
                ours.len() - team,
                if elsewhere_wt.is_empty() { String::new() } else { format!(" ({} more belong to other worktrees.)", elsewhere_wt.len()) }
            );
        }
        let (done, total) = init_progress(&entries);
        if total == 0 && entries.is_empty() {
            let _ = writeln!(state, "- {}", no_knowledge_hint(app));
        } else if done < total {
            let _ = writeln!(state, "- Knowledge bootstrap: {done}/{total} module summaries written — continue with `ctx_init`.");
        }
        let stale = crate::freshness::stale_decisions(app, &entries);
        if !stale.is_empty() {
            let named = |s: &crate::freshness::Stale| entries.iter().find(|e| e.id == s.id).map(reference).unwrap_or_else(|| s.id.clone());
            let _ = writeln!(
                state,
                "- May need a refresh ({}+ commits on their paths since they were made): {} — check they still hold; supersede what no longer does.",
                cfg.freshness.threshold_commits,
                crate::freshness::render_list(&stale, 3, &named)
            );
        }
        // a fresh clone (a Superset project, a CI checkout) runs none of the commit-time checks
        if app.is_set_up() && !crate::hooks::opted_out(&app.repo) && !crate::hooks::installed(app, "pre-commit") {
            let _ = writeln!(
                state,
                "- Git hooks are not installed in this clone, so commits get no knowledge checks, secret scan or trailers: `kontext hooks install`."
            );
        }
        if !errors.is_empty() {
            let _ = writeln!(state, "- {} store file(s) could not be read.", errors.len());
        }
    }
    let reserved = if state.is_empty() { 0 } else { util::est_tokens(&format!("\n## State\n{state}")) as isize };
    budget_left -= reserved;

    let overview = entries.iter().find(|e| e.kind == "architecture" && e.id == "overview");
    let mut head = if part.section {
        format!("\n## {} — a nested repository with its own team knowledge\n{}", part.prefix, app.repo.id)
    } else {
        format!("# {} — team context\n{}", app.project_name(), app.repo.id)
    };
    if let Some(b) = &app.repo.branch {
        let _ = write!(head, " · branch {b}");
    }
    if part.section {
        let _ = write!(head, " · its tools: `dir: \"{}\"`", part.prefix.trim_end_matches('/'));
    }
    head.push('\n');
    if let Some(o) = overview {
        let about = o.summary_text(260);
        if !about.is_empty() {
            let _ = writeln!(head, "About: {about}");
        }
        if let Some(stack) = o.get_extra("stack") {
            let _ = writeln!(head, "Stack: {stack}");
        }
    }
    add(&mut out, &head, &mut budget_left);

    let mut rules: Vec<&str> = Vec::new();
    for f in ["AGENTS.md", "CLAUDE.md", "Claude.md", ".cursorrules", ".github/copilot-instructions.md"] {
        // case-insensitive file systems: CLAUDE.md and Claude.md may be one file
        if app.repo.abs(f).is_file() && !rules.iter().any(|r| r.eq_ignore_ascii_case(f)) {
            rules.push(f);
        }
    }
    // every AGENTS.md / CLAUDE.md between the root and a focused path applies, outermost first
    let mut nested_rules: Vec<String> = Vec::new();
    for p in &focus.paths {
        let p = p.trim_end_matches('/');
        let mut dirs: Vec<String> = Vec::new();
        if app.repo.abs(p).is_dir() {
            dirs.push(p.to_string());
        }
        let mut cur = std::path::Path::new(p);
        while let Some(parent) = cur.parent().filter(|d| !d.as_os_str().is_empty()) {
            dirs.push(parent.to_string_lossy().to_string());
            cur = parent;
        }
        for d in dirs.iter().rev() {
            for name in ["AGENTS.md", "CLAUDE.md"] {
                let rel = format!("{d}/{name}");
                if app.repo.abs(&rel).is_file() && !nested_rules.contains(&rel) {
                    nested_rules.push(rel);
                }
            }
        }
    }
    nested_rules.truncate(8);
    if !rules.is_empty() || !nested_rules.is_empty() {
        let mut all: Vec<String> = rules.iter().map(|s| format!("{}{s}", part.prefix)).collect();
        all.extend(nested_rules.iter().map(|s| format!("{}{s}", part.prefix)));
        add(&mut out, &format!("Rules for agents: read {}\n", all.join(", ")), &mut budget_left);
    }

    // the repositories this work reaches into: a submodule's team knowledge gets its own section,
    // anything further a pointer
    let own = app.is_set_up() && !entries.is_empty();
    let mut nested_parts: Vec<(std::sync::Arc<App>, String, Vec<String>)> = Vec::new();
    let mut pointers: Vec<String> = Vec::new();
    if !part.section {
        for (other, nested, paths) in elsewhere {
            match nested {
                Some(rel) => {
                    let mut f = paths.clone();
                    f.extend(focus.words.iter().cloned());
                    nested_parts.push((other.clone(), rel.clone(), f));
                }
                None => pointers.push(format!(
                    "- {} at {}: the focus paths there — ctx_brief with `dir: \"{}\"`",
                    other.project_name(),
                    other.repo.root.display(),
                    other.repo.root.display()
                )),
            }
        }
        for n in crate::route::nested_stores(app) {
            if nested_parts.iter().any(|(a, _, _)| a.repo.root == n.app.repo.root) {
                continue;
            }
            if own {
                pointers.push(nested_pointer(&n.app, &n.rel));
            } else {
                // nothing of its own to say: its submodules' team knowledge is what applies here
                nested_parts.push((n.app.clone(), n.rel.clone(), focus.words.clone()));
            }
        }
        while nested_parts.len() > 2 {
            let (a, rel, _) = nested_parts.pop().unwrap();
            pointers.push(nested_pointer(&a, &rel));
        }
    }

    // submodules' team knowledge first: focus paths in a submodule ask for it, and a repository
    // without knowledge of its own has nothing else to say
    if !nested_parts.is_empty() {
        let pct = if own { 55 } else { 85 };
        let share = ((budget_left.max(0) as usize) * pct / 100 / nested_parts.len()).max(300);
        for (a, rel, f) in &nested_parts {
            let prefix = format!("{rel}/");
            let text = brief_in(a, f, share, false, Part { prefix: &prefix, section: true }, &[]);
            if !add(&mut out, &text, &mut budget_left) {
                pointers.push(nested_pointer(a, rel));
            }
        }
    }

    // decisions
    let mut decisions: Vec<(&Entry, f32)> =
        entries.iter().filter(|e| e.kind == "decision" && e.is_active()).map(|e| (e, relevance(e, &focus))).collect();
    decisions.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal).then(b.0.date.cmp(&a.0.date)));
    if !decisions.is_empty() {
        let total = decisions.len();
        let mut sec = format!("\n{h2} Decisions ({total} active{})\n", if focused { ", most relevant first" } else { ", newest first" });
        let mut shown = 0;
        for (e, _) in decisions.iter().take(cfg.brief.max_decisions) {
            let r = reference(e);
            // a dated reference already says when
            let date = e
                .date
                .as_deref()
                .filter(|d| !r.trim_start_matches(part.prefix).starts_with(*d))
                .map(|d| format!(" ({d})"))
                .unwrap_or_default();
            let line = format!("- [{r}] {} — {}{date}\n", e.title, e.summary_text(150));
            if util::est_tokens(&(sec.clone() + &line)) as isize > budget_left * 6 / 10 {
                break;
            }
            sec.push_str(&line);
            shown += 1;
        }
        if shown < total {
            let how = if part.prefix.is_empty() {
                "`ctx_log` or `ctx_search kinds=[decision]`".to_string()
            } else {
                format!("`ctx_search kinds=[decision]`, or `ctx_log` with `dir: \"{}\"`", part.prefix.trim_end_matches('/'))
            };
            let _ = writeln!(sec, "- … {} more: {how}", total - shown);
        }
        add(&mut out, &sec, &mut budget_left);
    }

    // conventions, learnings, incidents
    for (kind, heading, max) in [("convention", "Conventions", 8), ("learning", "Learnings & pitfalls", 6), ("incident", "Incidents", 3)] {
        let mut list: Vec<(&Entry, f32)> =
            entries.iter().filter(|e| e.kind == kind && e.is_active()).map(|e| (e, relevance(e, &focus))).collect();
        if list.is_empty() {
            continue;
        }
        list.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal).then(b.0.date.cmp(&a.0.date)));
        let mut sec = format!("\n{h2} {heading}\n");
        for (e, _) in list.iter().take(max) {
            let _ = writeln!(sec, "- {} <kx:{}>", e.l0(140), reference(e));
        }
        add(&mut out, &sec, &mut budget_left);
    }

    // module map
    let mods = module_docs(&entries);
    if !mods.is_empty() {
        let mut ranked: Vec<(&Entry, f32)> = mods.iter().map(|e| (*e, relevance(e, &focus))).collect();
        ranked.sort_by(|a, b| {
            b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal).then_with(|| {
                a.0.get_extra("rank")
                    .unwrap_or_default()
                    .parse::<u32>()
                    .unwrap_or(999)
                    .cmp(&b.0.get_extra("rank").unwrap_or_default().parse::<u32>().unwrap_or(999))
            })
        });
        let mut sec = format!("\n{h2} Modules\n");
        // a focused module without a doc of its own (outside init.max_module_docs) still gets its facts
        let documented: Vec<&str> = mods.iter().filter_map(|e| e.paths.first()).map(|p| p.trim_end_matches("/**")).collect();
        let uncovered: Vec<&String> =
            focus.paths.iter().filter(|p| !documented.iter().any(|d| p.as_str() == *d || p.starts_with(&format!("{d}/")))).collect();
        if !uncovered.is_empty()
            && let Some(inv) = crate::init::cached_inventory(app)
        {
            let mut seen = HashSet::new();
            for p in uncovered {
                let Some(m) = inv
                    .modules
                    .iter()
                    .filter(|m| p.as_str() == m.path || p.starts_with(&format!("{}/", m.path)))
                    .max_by_key(|m| m.path.len())
                else {
                    continue;
                };
                if !seen.insert(m.path.clone()) {
                    continue;
                }
                let what =
                    m.description.as_deref().map(|d| util::truncate_chars(d, 110)).unwrap_or_else(|| format!("{} {}", m.language, m.kind));
                let docs = if m.docs.is_empty() { String::new() } else { format!("; read {}", m.docs.join(", ")) };
                let _ = writeln!(sec, "- {}{} — {what} ({} files, no module doc yet{docs})", part.prefix, m.path, m.files);
            }
        }
        let mut bare: Vec<String> = Vec::new();
        let mut shown = 0;
        for (e, rel) in ranked.iter() {
            let path = e.paths.first().map(|p| format!("{}{}", part.prefix, p.trim_end_matches("/**"))).unwrap_or_else(|| e.title.clone());
            let summary = e.summary_text(110);
            if summary.is_empty() {
                bare.push(path);
                continue;
            }
            if shown >= cfg.brief.max_modules {
                bare.push(path);
                continue;
            }
            shown += 1;
            let _ = writeln!(sec, "- {path} — {summary}");
            if *rel >= 10.0
                && let Some(pos) = e.body.find("## Overview")
            {
                let ov = util::first_paragraph(&e.body[pos + 11..], 500);
                if !ov.is_empty() && !ov.starts_with("_Pending") {
                    let _ = writeln!(sec, "  {ov}");
                }
            }
        }
        if !bare.is_empty() {
            let listed: Vec<String> = bare.iter().take(16).cloned().collect();
            let more = if bare.len() > 16 { format!(" (+{} more)", bare.len() - 16) } else { String::new() };
            let _ = writeln!(sec, "- {}: {}{more}", if shown == 0 { "Map" } else { "Also" }, listed.join(", "));
        }
        add(&mut out, &sec, &mut budget_left);
    }

    if !pointers.is_empty() {
        add(&mut out, &format!("\n{h2} Elsewhere\n{}\n", pointers.join("\n")), &mut budget_left);
    }

    let notes = local_notes(app, &focus, h2, part.prefix);
    if !notes.is_empty() {
        add(&mut out, &notes, &mut budget_left);
    }

    // without team knowledge, what the docs and the history say about the focus
    // (only over an index a search already built: a brief does not build one, nor write it here)
    let warm = app.repo.worktree_state_dir().join("index").join("manifest.json").is_file();
    if !part.section && !app.is_set_up() && nested_parts.is_empty() && focused && warm {
        let mut q = focus.words.clone();
        for p in &focus.paths {
            q.extend(p.split(['/', '.', '-', '_']).filter(|s| s.len() > 2).map(str::to_string));
        }
        let req = SearchReq { query: q.join(" "), kinds: vec!["doc".into(), "commit".into()], sources: vec!["local".into()], limit: 5 };
        let room = ((budget_left.max(0) as usize) / 2).min(450);
        if room >= 80
            && let Ok((hits, _)) = search(app, &req)
            && !hits.is_empty()
        {
            add(&mut out, &format!("\n## Related docs and history\n{}", model::render(&hits, room)), &mut budget_left);
        }
    }

    if part.section {
        return out;
    }

    // external adapters that can brief
    if with_adapters {
        let q = focus.words.join(" ") + " " + &focus.paths.join(" ");
        let reg = app.registry();
        let q = q.trim().to_string();
        let per = ((budget_left.max(0) as usize) / 3).max(80);
        for (name, r) in reg.fanout("brief", &[], Duration::from_secs(8), move |a| a.brief(&q)) {
            if let Ok(text) = r {
                let text = text.trim();
                if !text.is_empty() {
                    let clipped = util::truncate_chars(text, per * 4);
                    add(&mut out, &format!("\n## From {name}\n{clipped}\n"), &mut budget_left);
                }
            }
        }
    }

    // the state, with the reservation given back; adapter notes only exist once the registry ran
    budget_left += reserved;
    let mut full = state.clone();
    for w in app.warnings().iter().take(3) {
        let _ = writeln!(full, "- note: {w}");
    }
    if !full.is_empty() && !add(&mut out, &format!("\n## State\n{full}"), &mut budget_left) && !state.is_empty() {
        add(&mut out, &format!("\n## State\n{state}"), &mut budget_left);
    }
    out.push_str("\nMore: `ctx_search` (decisions, docs, history, adapters) · `ctx_read kx:<id>` (L0/L1/L2) · `ctx_why <path>` · record with `ctx_capture` · before committing `ctx_prepare_commit`.\n");
    out
}

// --------------------------------------------------------------------------------------- why

struct CommitLine {
    short: String,
    date: String,
    author: String,
    subject: String,
    trailers: String,
    score: f32,
}

fn commit_lines(app: &App, extra: &[&str]) -> Vec<CommitLine> {
    let mut args = vec!["log", "--date=short", "--format=%x1e%h%x1f%ad%x1f%an%x1f%s%x1f%b%x1f%(trailers:only,unfold)"];
    args.extend_from_slice(extra);
    let out = app.repo.git(&args).unwrap_or_default();
    out.split('\x1e')
        .filter_map(|rec| {
            let p: Vec<&str> = rec.splitn(6, '\x1f').collect();
            if p.len() < 6 {
                return None;
            }
            let sig = crate::signals::decision_score(p[3], p[4]);
            let trailers = knowledge_trailers(app, p[5]);
            let score = sig.score + if trailers.is_empty() { 0.0 } else { 4.0 };
            Some(CommitLine { short: p[0].trim().into(), date: p[1].into(), author: p[2].into(), subject: p[3].into(), trailers, score })
        })
        .collect()
}

/// Only the trailers kontext writes (Decision:, Convention:, …) — not Co-authored-by & friends.
fn knowledge_trailers(app: &App, raw: &str) -> String {
    let keys: Vec<String> = app.cfg().kind_names().iter().filter_map(|k| app.cfg().trailer_for(k)).map(|k| k.to_lowercase()).collect();
    raw.lines()
        .map(str::trim)
        .filter(|l| l.split_once(':').is_some_and(|(k, _)| keys.contains(&k.trim().to_lowercase())))
        .collect::<Vec<_>>()
        .join("; ")
}

pub fn why(app: &App, target: &str, limit: usize) -> Result<String> {
    let target = target.trim();
    if target.is_empty() {
        bail!("target is required (a path, path:line, commit or symbol)");
    }
    let limit = limit.clamp(3, 40);
    // a path in a submodule or another worktree: its knowledge and its history live there
    let (path_part, line_part) = match target.rsplit_once(':') {
        Some((p, l)) if !l.is_empty() && l.chars().all(|c| c.is_ascii_digit() || c == '-') => (p, Some(l)),
        _ => (target, None),
    };
    if let crate::route::Place::Other { app: other, rel, .. } = crate::route::locate(app, path_part) {
        let inner = match (rel.is_empty(), line_part) {
            (true, _) => ".".to_string(),
            (false, Some(l)) => format!("{rel}:{l}"),
            (false, None) => rel,
        };
        return Ok(format!("{}\n{}", crate::route::banner(app, &other), why(&other, &inner, limit)?));
    }
    let (entries, _) = app.entries();
    let mut out = String::new();

    // commit?
    if target.len() >= 7
        && target.chars().all(|c| c.is_ascii_hexdigit())
        && app.repo.git_opt(&["cat-file", "-t", target]).as_deref() == Some("commit")
    {
        let show = app.repo.git(&["show", "-s", "--date=short", "--format=%h %ad %an%n%s%n%n%b", target])?;
        let _ = writeln!(out, "# Commit {target}\n\n{}", show.trim());
        let files = app.repo.git(&["show", "--name-only", "--format=", target]).unwrap_or_default();
        let files: Vec<&str> = files.lines().filter(|l| !l.is_empty()).collect();
        let covering: Vec<&Entry> = entries
            .iter()
            .filter(|e| {
                e.commits.iter().any(|c| target.starts_with(c.as_str()) || c.starts_with(target))
                    || files.iter().any(|f| e.paths.iter().any(|p| path_matches(p, f)))
            })
            .collect();
        if !covering.is_empty() {
            let _ = writeln!(out, "\n## Related knowledge");
            for e in covering.iter().take(limit) {
                let _ = writeln!(out, "- [{}] {} <kx:{}>", e.kind, e.l0(150), e.id);
            }
        }
        let _ = writeln!(out, "\nFiles ({}): {}", files.len(), files.iter().take(25).copied().collect::<Vec<_>>().join(", "));
        return Ok(out);
    }

    // path[:line[-end]]
    let (path_part, lines) = match target.rsplit_once(':') {
        Some((p, l)) if !l.is_empty() && l.chars().all(|c| c.is_ascii_digit() || c == '-') => (p, Some(l)),
        _ => (target, None),
    };
    let rel = path_part.trim_start_matches("./").trim_end_matches('/');
    if app.repo.abs(rel).exists() {
        let _ = writeln!(out, "# Why: {target}");
        let covering: Vec<&Entry> = entries
            .iter()
            .filter(|e| e.kind != "architecture" && e.paths.iter().any(|p| path_matches(p, rel) || path_overlaps(p, rel)))
            .collect();
        if !covering.is_empty() {
            let _ = writeln!(out, "\n## Knowledge covering this path");
            for e in covering.iter().take(limit) {
                let st = e.status.as_deref().map(|s| format!(" · {s}")).unwrap_or_default();
                let _ = writeln!(out, "- [{}{st}] {} <kx:{}>", e.kind, e.l0(170), e.id);
            }
        }
        let module = module_docs(&entries)
            .into_iter()
            .filter(|m| m.paths.iter().any(|p| path_matches(p, rel)))
            .max_by_key(|m| m.paths.first().map(|p| p.len()).unwrap_or(0));
        if let Some(m) = module {
            let s = m.summary_text(200);
            let _ =
                writeln!(out, "\n## Module\n- {}{} <kx:{}>", m.title, if s.is_empty() { String::new() } else { format!(" — {s}") }, m.id);
        }
        if let Some(range) = lines {
            let (a, b) = match range.split_once('-') {
                Some((a, b)) => (a.to_string(), b.to_string()),
                None => (range.to_string(), range.to_string()),
            };
            let blame = app.repo.git(&["blame", "--porcelain", "-L", &format!("{a},{b}"), "--", rel]).unwrap_or_default();
            let mut shas: Vec<String> = Vec::new();
            for l in blame.lines() {
                let first = l.split(' ').next().unwrap_or("");
                if first.len() == 40
                    && first.chars().all(|c| c.is_ascii_hexdigit())
                    && !shas.contains(&first.to_string())
                    && !first.starts_with("0000000")
                {
                    shas.push(first.to_string());
                }
            }
            if !shas.is_empty() {
                let _ = writeln!(out, "\n## Lines {range} last changed in");
                let mut args = vec!["show", "-s", "--date=short", "--format=- %h %ad %an: %s"];
                let refs: Vec<&str> = shas.iter().take(8).map(String::as_str).collect();
                args.extend(refs);
                out.push_str(&app.repo.git(&args).unwrap_or_default());
            }
        }
        let is_file = app.repo.abs(rel).is_file();
        let mut extra: Vec<&str> = vec!["-n", "60"];
        if is_file {
            extra.push("--follow");
        }
        extra.push("--");
        extra.push(rel);
        let mut commits = commit_lines(app, &extra);
        let total = commits.len();
        if total > 0 {
            let recent: Vec<String> = commits.iter().take(3).map(|c| c.short.clone()).collect();
            commits.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
            let mut chosen: Vec<&CommitLine> = commits.iter().filter(|c| c.score >= 2.0).take(limit).collect();
            for c in commits.iter().filter(|c| recent.contains(&c.short)) {
                if !chosen.iter().any(|x| x.short == c.short) && chosen.len() < limit + 3 {
                    chosen.push(c);
                }
            }
            chosen.sort_by(|a, b| b.date.cmp(&a.date));
            let _ = writeln!(out, "\n## History ({} of the last {total} commits; decision-shaped first, plus the latest)", chosen.len());
            for c in chosen {
                let tr = if c.trailers.is_empty() { String::new() } else { format!("  [{}]", util::one_line(&c.trailers)) };
                let _ = writeln!(out, "- {} {} {}: {}{tr}  <git:{}>", c.date, c.short, c.author, c.subject, c.short);
            }
        }
        adapter_history(app, target, limit, &mut out);
        if covering.is_empty() {
            let _ = writeln!(
                out,
                "\nNo recorded decision covers this path yet. If you learn why it is this way, `ctx_capture` it with paths=[\"{rel}\"]."
            );
        }
        return Ok(out);
    }

    // symbol or topic
    let _ = writeln!(out, "# Why: {target}");
    let reg = app.registry();
    let q = target.to_string();
    let code = reg.fanout("code", &[], Duration::from_secs(12), move |a| a.code(&q, 5));
    for (name, r) in code {
        if let Ok(hits) = r
            && !hits.is_empty()
        {
            let _ = writeln!(out, "\n## Code ({name})");
            for h in hits.iter().take(5) {
                let _ = writeln!(out, "- {} {}", h.title, if h.uri.is_empty() { h.snippet.clone() } else { h.uri.clone() });
            }
        }
    }
    let (hits, _) = search(app, &SearchReq { query: target.into(), kinds: Vec::new(), sources: vec!["local".into()], limit })?;
    if !hits.is_empty() {
        let _ = writeln!(out, "\n## Knowledge and history\n{}", model::render(&hits, 900));
    }
    adapter_history(app, target, limit, &mut out);
    Ok(out)
}

fn adapter_history(app: &App, target: &str, limit: usize, out: &mut String) {
    let reg = app.registry();
    let t = target.to_string();
    for (name, r) in reg.fanout("history", &[], Duration::from_secs(12), move |a| a.history(&t, limit)) {
        match r {
            Ok(hits) if !hits.is_empty() => {
                let _ = writeln!(out, "\n## Sessions & memory ({name})");
                out.push_str(&model::render(&hits, 600));
            }
            Ok(_) => {}
            Err(e) => {
                let _ = writeln!(out, "\n({name} unavailable: {})", util::truncate_chars(&format!("{e:#}"), 160));
            }
        }
    }
}

// --------------------------------------------------------------------------------------- log

#[derive(Debug, Serialize)]
pub struct LogRow {
    pub date: String,
    pub id: String,
    pub title: String,
    pub status: String,
    pub summary: String,
    pub commit: Option<String>,
    pub superseded_by: Vec<String>,
    pub path: String,
}

pub fn log_rows(app: &App, kind: &str, all: bool, limit: usize) -> Vec<LogRow> {
    let (entries, _) = app.entries();
    let path = app.cfg().kind_path(kind);
    let added = first_commits(app, &path);
    let mut rows: Vec<LogRow> = entries
        .iter()
        .filter(|e| e.kind == kind && (all || e.is_active()))
        .map(|e| LogRow {
            date: e.date.clone().or_else(|| added.get(&e.rel_path).map(|(_, d)| d.clone())).unwrap_or_default(),
            id: e.short_id(),
            title: e.title.clone(),
            status: e.status.clone().unwrap_or_default(),
            summary: e.summary_text(160),
            commit: added.get(&e.rel_path).map(|(c, _)| c.clone()),
            superseded_by: e.superseded_by.clone(),
            path: e.rel_path.clone(),
        })
        .collect();
    rows.sort_by(|a, b| b.date.cmp(&a.date).then(b.id.cmp(&a.id)));
    rows.truncate(limit.max(1));
    rows
}

fn first_commits(app: &App, dir: &str) -> BTreeMap<String, (String, String)> {
    let mut map = BTreeMap::new();
    let out =
        app.repo.git(&["log", "--diff-filter=A", "--date=short", "--name-only", "--format=%x1e%h %ad", "--", dir]).unwrap_or_default();
    for rec in out.split('\x1e') {
        let mut lines = rec.lines();
        let Some(head) = lines.next() else { continue };
        let Some((sha, date)) = head.split_once(' ') else { continue };
        for f in lines.filter(|l| !l.trim().is_empty()) {
            map.insert(f.trim().to_string(), (sha.to_string(), date.to_string()));
        }
    }
    map
}

pub fn render_log(rows: &[LogRow]) -> String {
    if rows.is_empty() {
        return "No entries yet. Record one with `kontext capture --kind decision --title … --body …` (or `ctx_capture`).".into();
    }
    // date-prefixed ids repeat the date column: show them without it
    let shown: Vec<LogRow> = rows
        .iter()
        .map(|r| LogRow {
            id: r.id.strip_prefix(&format!("{}-", r.date)).map(str::to_string).unwrap_or_else(|| r.id.clone()),
            date: r.date.clone(),
            title: r.title.clone(),
            status: r.status.clone(),
            summary: r.summary.clone(),
            commit: r.commit.clone(),
            superseded_by: r.superseded_by.clone(),
            path: r.path.clone(),
        })
        .collect();
    let rows = &shown;
    let idw = rows.iter().map(|r| r.id.chars().count()).max().unwrap_or(4).min(32);
    let mut out = String::new();
    for r in rows {
        let status = if r.superseded_by.is_empty() { r.status.clone() } else { format!("{} → {}", r.status, r.superseded_by.join(", ")) };
        let commit = r.commit.as_deref().map(|c| format!("  {c}")).unwrap_or_default();
        // learnings and conventions have no status
        let status = if status.is_empty() { String::new() } else { format!("  [{status}]") };
        let _ = writeln!(out, "{:<10}  {:<idw$}  {}{status}{commit}", r.date, util::truncate_chars(&r.id, idw), r.title, idw = idw);
        let _ = &r.path;
        if !r.summary.is_empty() {
            let _ = writeln!(out, "{:<10}  {:<idw$}  ↳ {}", "", "", util::truncate_chars(&r.summary, 140), idw = idw);
        }
    }
    out
}

// ------------------------------------------------------------------------------- check/commit

#[derive(Debug, Default)]
pub struct CheckReport {
    pub issues: Vec<Issue>,
    pub secrets: Vec<(String, Finding)>,
    pub checked: usize,
}

impl CheckReport {
    pub fn errors(&self) -> usize {
        self.issues.iter().filter(|i| i.level == Level::Error).count()
            + self.secrets.iter().filter(|(_, f)| f.severity == secrets::Severity::High).count()
    }
}

fn allow_patterns(app: &App) -> Vec<regex::Regex> {
    app.cfg().secrets.allow.iter().filter_map(|p| regex::Regex::new(p).ok()).collect()
}

/// Validate and secret-scan what is staged (the exact blobs that would be committed).
pub fn check_staged(app: &App) -> Result<CheckReport> {
    let store = app.store();
    let mut rep = CheckReport::default();
    let staged = app.repo.staged_paths()?;
    let (entries, _) = store.load_all();
    let mut ids: HashSet<String> = entries.iter().map(|e| e.id.clone()).collect();
    let tracked: Vec<String> = app.repo.tracked_files().unwrap_or_default().into_iter().map(|t| t.path).collect();
    let allow = allow_patterns(app);
    let scan_all = app.cfg().secrets.scan == "staged";
    let scan_off = app.cfg().secrets.scan == "off";
    let mut seen_ids: BTreeMap<String, String> = BTreeMap::new();
    let to_read: Vec<String> =
        staged.into_iter().filter(|(st, p)| *st != 'D' && (scan_all || store.is_store_path(p))).map(|(_, p)| p).collect();
    let contents = app.repo.staged_contents(&to_read)?;
    for path in to_read {
        let is_store = store.is_store_path(&path);
        let Some(bytes) = contents.get(&path) else { continue };
        if bytes.len() > 2_000_000 || bytes.contains(&0) {
            continue;
        }
        let text = String::from_utf8_lossy(bytes);
        rep.checked += 1;
        if !scan_off {
            for f in secrets::scan(&text, &allow) {
                rep.secrets.push((path.clone(), f));
            }
        }
        if is_store && store.is_entry_path(&path) && app.cfg().hooks.validate {
            let kind = store.kind_for_path(&path).unwrap_or_default();
            let e = Entry::parse(&path, &text, &kind);
            ids.insert(e.id.clone());
            if let Some(other) = seen_ids.insert(e.id.clone(), path.clone()) {
                rep.issues.push(Issue { level: Level::Error, path: path.clone(), msg: format!("duplicate id '{}' (also {other})", e.id) });
            }
            rep.issues.extend(store.validate(&e, &ids, Some(&tracked)));
        }
    }
    Ok(rep)
}

/// Validate every entry in the working tree.
pub fn check_all(app: &App) -> CheckReport {
    let store = app.store();
    let mut rep = CheckReport::default();
    let (entries, errors) = store.load_all();
    for e in errors {
        rep.issues.push(Issue { level: Level::Error, path: String::new(), msg: e });
    }
    let ids: HashSet<String> = entries.iter().map(|e| e.id.clone()).collect();
    let tracked: Vec<String> = app.repo.tracked_files().unwrap_or_default().into_iter().map(|t| t.path).collect();
    let allow = allow_patterns(app);
    let mut seen: BTreeMap<String, String> = BTreeMap::new();
    for e in &entries {
        rep.checked += 1;
        if let Some(other) = seen.insert(e.id.clone(), e.rel_path.clone()) {
            rep.issues.push(Issue {
                level: Level::Error,
                path: e.rel_path.clone(),
                msg: format!("duplicate id '{}' (also {other})", e.id),
            });
        }
        rep.issues.extend(store.validate(e, &ids, Some(&tracked)));
        if let Ok(text) = std::fs::read_to_string(app.repo.abs(&e.rel_path)) {
            for f in secrets::scan(&text, &allow) {
                rep.secrets.push((e.rel_path.clone(), f));
            }
        }
    }
    rep
}

pub fn render_check(rep: &CheckReport) -> String {
    let mut out = String::new();
    for (p, f) in &rep.secrets {
        let lvl = if f.severity == secrets::Severity::High { "error" } else { "warn" };
        let _ = writeln!(out, "{lvl}: {p}:{}: possible secret ({}) {}", f.line, f.rule, f.excerpt);
    }
    for i in &rep.issues {
        let _ = writeln!(out, "{i}");
    }
    out
}

#[derive(Debug, Default)]
pub struct PrepareReq {
    pub promote: Vec<String>,
    pub drop: Vec<String>,
}

/// Another worktree of this clone that has the team knowledge checked out (on a branch or detached).
fn worktree_with_store(app: &App, markers: &[&str]) -> Option<String> {
    app.repo
        .worktree_roots()
        .into_iter()
        .filter(|w| std::path::Path::new(w) != app.repo.root)
        .find(|w| markers.iter().any(|f| std::path::Path::new(w).join(f).is_file()))
}

/// What to say when this branch has no team knowledge: bootstrap it — unless another branch already
/// has it (a bootstrap waiting for review), where a second bootstrap would only conflict with it.
pub fn no_knowledge_hint(app: &App) -> String {
    let overview = format!("{}/overview.md", app.cfg().kind_path("architecture"));
    // a store kept in an ADR directory may have no overview: its shared config marks it too, but
    // only where this worktree has none (here it is this branch's own config)
    let config = format!("{}/kontext.toml", app.cfg().store.dir.trim_end_matches('/'));
    let markers: Vec<&str> = if app.is_set_up() { vec![overview.as_str()] } else { vec![overview.as_str(), config.as_str()] };
    // this branch is not "another branch"; its upstream may well be (pushed, fetched, not pulled)
    let elsewhere: Vec<String> = app.repo.refs_with_any(&markers).into_iter().filter(|r| Some(r) != app.repo.branch.as_ref()).collect();
    match elsewhere.first() {
        Some(r) => format!(
            "Team knowledge exists on `{r}`{} but not on this branch yet — merge or rebase to get it; do not bootstrap it again here.{}",
            if elsewhere.len() > 1 { format!(" (and {} more branch(es))", elsewhere.len() - 1) } else { String::new() },
            // an agent that works in a worktree that has it reads it from there
            worktree_with_store(app, &markers).map(|w| format!(" A worktree that has it: {w} (tools take `dir: \"{w}\"`).")).unwrap_or_default()
        ),
        None => "No team knowledge here yet: `ctx_search` and `ctx_why <path>` answer from the docs and history, and private notes (`ctx_capture` with visibility private) come back in this brief and in search. `kontext init` sets it up when the user asks for it.".to_string(),
    }
}

pub fn prepare_commit(app: &App, req: &PrepareReq) -> Result<String> {
    let mut out = String::from("# Prepare commit\n");
    let inbox = Inbox::open(&app.repo);
    for id in &req.drop {
        match inbox.get(id) {
            Some(e) => {
                inbox.remove(&e.id)?;
                let _ = writeln!(out, "- dropped inbox entry {}", e.id);
            }
            None => {
                let _ = writeln!(out, "- (no inbox entry '{id}' to drop)");
            }
        }
    }
    if !req.promote.is_empty() {
        for (id, rel) in promote(app, &req.promote, true)? {
            let _ = writeln!(out, "- promoted {id} → {rel} (staged)");
        }
    }
    let store = app.store();
    let mut staged = app.repo.staged_paths()?;
    let using_worktree = staged.is_empty();
    if using_worktree {
        staged = app.repo.changed_paths()?;
    }
    if staged.is_empty() {
        out.push_str("\nNothing staged or changed.\n");
        return Ok(out);
    }
    let code: Vec<&(char, String)> = staged.iter().filter(|(_, p)| !store.is_store_path(p)).collect();
    let knowledge: Vec<&(char, String)> = staged.iter().filter(|(_, p)| store.is_entry_path(p)).collect();
    let _ = writeln!(
        out,
        "\n{} {} file(s): {} code/docs, {} knowledge entr{}.",
        if using_worktree { "Nothing staged yet — looking at the working tree:" } else { "Staged:" },
        staged.len(),
        code.len(),
        knowledge.len(),
        if knowledge.len() == 1 { "y" } else { "ies" }
    );

    let (entries, _) = store.load_all();
    // decisions/conventions touching the changed code
    let mut relevant: Vec<(&Entry, usize)> = entries
        .iter()
        .filter(|e| e.kind != "architecture" && e.is_active() && !e.paths.is_empty())
        .map(|e| (e, code.iter().filter(|(_, p)| e.paths.iter().any(|pat| path_matches(pat, p))).count()))
        .filter(|(_, n)| *n > 0)
        .collect();
    relevant.sort_by_key(|x| std::cmp::Reverse(x.1));
    if !relevant.is_empty() {
        let _ = writeln!(out, "\n## Recorded knowledge covering this change — confirm it still holds");
        for (e, n) in relevant.iter().take(10) {
            let _ = writeln!(out, "- [{}] {} ({n} file(s)) <kx:{}>", e.kind, e.l0(140), e.id);
        }
    }

    // inbox candidates — the inbox is shared by all worktrees; offer only this worktree's own
    let cands = inbox.list();
    let team: Vec<&Entry> = cands.iter().filter(|e| e.visibility.as_deref() != Some("private")).collect();
    let touches = |e: &Entry| code.iter().filter(|(_, p)| e.paths.iter().any(|pat| path_matches(pat, p) || path_overlaps(pat, p))).count();
    let mut ours: Vec<(&Entry, usize, Origin)> = Vec::new();
    let mut theirs: Vec<(&Entry, usize, Origin)> = Vec::new();
    for e in &team {
        let origin = Origin::of(&app.repo, e);
        let n = touches(e);
        if origin.is_ours() { ours.push((*e, n, origin)) } else { theirs.push((*e, n, origin)) }
    }
    ours.sort_by_key(|x| std::cmp::Reverse(x.1));
    if !ours.is_empty() {
        let _ = writeln!(out, "\n## Inbox candidates (local, not shared yet)");
        for (e, n, origin) in &ours {
            let rel = if *n > 0 { format!(" — touches {n} changed file(s)") } else { String::new() };
            let _ = writeln!(out, "- `{}` [{}] {}{rel}{}", e.id, e.kind, e.l0(140), origin.note());
        }
        let _ = writeln!(
            out,
            "Promote the ones that belong to this change: `ctx_prepare_commit` with promote=[ids] (or `kontext promote <id>`); drop stale ones with drop=[ids]."
        );
    }
    let related: Vec<&(&Entry, usize, Origin)> = theirs.iter().filter(|x| x.1 > 0).collect();
    if !related.is_empty() {
        let _ = writeln!(out, "\n## Captured in other worktrees — not for this commit");
        for (e, n, origin) in &related {
            let _ = writeln!(out, "- `{}` [{}] {} — touches {n} changed file(s){}", e.id, e.kind, e.l0(120), origin.note());
        }
        let _ = writeln!(out, "Another session is working on these; leave them to it unless this change really carries one.");
    }
    if theirs.len() > related.len() {
        let _ = writeln!(out, "\n({} more candidate(s) from other worktrees, unrelated to this change.)", theirs.len() - related.len());
    }
    let private = cands.len() - team.len();
    if private > 0 {
        let _ = writeln!(out, "\n({private} private inbox note(s) stay local.)");
    }

    // validation + secrets on staged knowledge
    let rep = check_staged(app)?;
    if !rep.issues.is_empty() || !rep.secrets.is_empty() {
        let _ = writeln!(out, "\n## Checks\n{}", render_check(&rep));
        if rep.errors() > 0 {
            let _ = writeln!(out, "The pre-commit hook will block this commit until the errors are fixed.");
        }
    } else if !knowledge.is_empty() {
        let _ = writeln!(out, "\n## Checks\n- {} knowledge file(s) valid, no secrets found.", knowledge.len());
    }

    // module docs possibly stale
    let mods = module_docs(&entries);
    let mut stale: Vec<(String, usize, usize)> = Vec::new();
    for m in mods {
        let Some(p) = m.paths.first() else { continue };
        let touched: Vec<&&(char, String)> = code.iter().filter(|(_, f)| path_matches(p, f)).collect();
        let structural = touched.iter().filter(|(s, _)| *s == 'A' || *s == 'D').count();
        if touched.len() >= 5 || structural >= 2 {
            stale.push((p.trim_end_matches("/**").to_string(), touched.len(), structural));
        }
    }
    if !stale.is_empty() {
        let _ = writeln!(out, "\n## Module docs that may need a refresh");
        for (p, n, s) in &stale {
            let _ = writeln!(out, "- {p}: {n} changed file(s), {s} added/removed");
        }
        let _ = writeln!(out, "Update the module's `## Overview` if its purpose or structure changed (`ctx_init` offers refresh tasks).");
    }

    // trailers
    let mut trailers = Vec::new();
    for (st, p) in &knowledge {
        if *st == 'D' {
            continue;
        }
        if let Some(e) = entries.iter().find(|e| &e.rel_path == p)
            && let Some(key) = app.cfg().trailer_for(&e.kind)
        {
            trailers.push(format!("{key}: {}", e.id));
        }
    }
    // a team that turned trailers off gets none; a fresh clone has no hooks, and an agent told
    // "added automatically" would ship the commit without them
    if !trailers.is_empty() && app.cfg().hooks.trailers {
        let how = if crate::hooks::installed(app, "prepare-commit-msg") {
            "added automatically by the prepare-commit-msg hook"
        } else {
            "add them to the commit message: the prepare-commit-msg hook is not installed in this clone (`kontext hooks install` adds them from then on)"
        };
        let _ =
            writeln!(out, "\n## Commit trailers ({how})\n{}", trailers.iter().map(|t| format!("    {t}")).collect::<Vec<_>>().join("\n"));
    }

    // nudges
    let big = code.len() >= 8;
    if knowledge.is_empty() && team.is_empty() && big {
        let _ = writeln!(
            out,
            "\n## Worth recording?\nThis change touches {} files but carries no decision or learning. If a design choice or a non-obvious lesson is behind it, record it now with `ctx_capture` (kind=decision|learning, paths=[…]) and promote it into this commit.",
            code.len()
        );
    }
    if using_worktree {
        let _ = writeln!(out, "\nStage your changes (`git add …`) and run this again for an exact report.");
    }
    Ok(out)
}

// ---------------------------------------------------------------------------- misc helpers

/// Trailers for the knowledge entries the index adds or changes against `base` (default HEAD).
pub fn staged_trailers_since(app: &App, base: Option<&str>) -> Result<Vec<String>> {
    let store = app.store();
    let paths: Vec<String> =
        app.repo.staged_paths_since(base)?.into_iter().filter(|(st, p)| *st != 'D' && store.is_entry_path(p)).map(|(_, p)| p).collect();
    let contents = app.repo.staged_contents(&paths)?;
    let mut out = Vec::new();
    for p in &paths {
        let Some(bytes) = contents.get(p) else { continue };
        let kind = store.kind_for_path(p).unwrap_or_default();
        let e = Entry::parse(p, &String::from_utf8_lossy(bytes), &kind);
        // entries mined by init came from older commits (listed in `commits`), not from this one
        if e.origin.as_deref() == Some("init") {
            continue;
        }
        if let Some(key) = app.cfg().trailer_for(&e.kind) {
            out.push(format!("{key}: {}", e.id));
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_references_resolve_to_one_entry() {
        let ids = [
            "2026-09-28-adapters-are-configuration-the-core-never-names-a-product",
            "2026-09-28-captures-become-shared-only-by-being-promoted-into-a-commit",
            "2026-09-28-commits-take-only-the-candidates-their-own-worktree-captured",
            "2026-09-30-keep-reviewed-labeling-datasets",
            "2026-09-30-keep-project-integration-outside",
            "batch-concurrency-is-not-capped",
            "batch-size-follows-the-provider",
            "overview",
        ];
        let refs = Refs::new(ids);
        assert_eq!(refs.prefix(ids[0]), "2026-09-28-adapters");
        assert_eq!(refs.prefix(ids[1]), "2026-09-28-captures");
        assert_eq!(refs.prefix(ids[3]), "2026-09-30-keep-reviewed", "the date and one word are not enough here");
        assert_eq!(refs.prefix(ids[5]), "batch-concurrency");
        assert_eq!(refs.prefix("overview"), "overview");
        let entries: Vec<Entry> =
            ids.iter().map(|id| Entry { id: id.to_string(), kind: "decision".into(), ..Default::default() }).collect();
        for e in &entries {
            let r = refs.of(e);
            assert_eq!(Store::find(&entries, &r).map(|x| x.id.as_str()), Some(e.id.as_str()), "{r}");
        }
        let adr = Entry { id: "0010-pay-by-link-reads-the-deposit-summary".into(), ..Default::default() };
        assert_eq!(Refs::new([adr.id.as_str()]).of(&adr), "0010", "ADR numbers stay as they are");
        // an id two entries share (a decision and a learning) has no unique prefix: it stays whole
        let twice = Refs::new(["2026-09-28-use-cents", "2026-09-28-use-cents", "2026-09-28-ship-weekly"]);
        assert_eq!(twice.prefix("2026-09-28-use-cents"), "2026-09-28-use-cents");
    }
}
