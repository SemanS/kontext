//! Local BM25 index (Tantivy) over store entries, repository docs and commit history.
//! Derived data only: it lives in `<git-common-dir>/kontext/worktrees/<wt>/index` and is rebuilt
//! incrementally from the sources of truth (files + git) whenever it is queried.
//!
//! `title` and `body` keep the exact words. Their shadows `title_f` and `body_f` fold case and
//! diacritics and stem English, so `adapter` finds `adapters` and `namietka` finds `námietka`; they
//! are queried alongside at a lower boost, so an exact wording still ranks first.
use crate::config::Config;
use crate::glob::GlobSet;
use crate::model::Hit;
use crate::repo::Repo;
use crate::store::{Entry, Store};
use crate::util;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tantivy::collector::TopDocs;
use tantivy::query::{BooleanQuery, Occur, Query, QueryParser, TermQuery};
use tantivy::schema::{Field, IndexRecordOption, STORED, STRING, Schema, TEXT, TextFieldIndexing, TextOptions, Value};
use tantivy::snippet::SnippetGenerator;
use tantivy::tokenizer::{AsciiFoldingFilter, Language, LowerCaser, RemoveLongFilter, SimpleTokenizer, Stemmer, TextAnalyzer};
use tantivy::{Index, IndexReader, IndexWriter, ReloadPolicy, TantivyDocument, Term};

/// Bumped whenever the schema or an analyzer changes: an index of another version is rebuilt.
/// 6: folded and stemmed shadow fields `title_f`/`body_f`, and a stored `digest` of the whole text.
pub const SCHEMA_VERSION: u32 = 6;

/// The analyzer of the shadow fields: lower case, ASCII folding, English stemming.
const FOLD: &str = "kx_fold";
/// How much a match on a folded/stemmed shadow field counts against the same match on the exact field.
const SHADOW_BOOST: f32 = 0.6;
const TITLE_BOOST: f32 = 2.5;
const AUX_BOOST: f32 = 1.5;

#[derive(Clone, Copy)]
struct Fields {
    uri: Field,
    file: Field,
    source: Field,
    kind: Field,
    title: Field,
    body: Field,
    aux: Field,
    title_f: Field,
    body_f: Field,
    status: Field,
    date: Field,
    summary: Field,
    digest: Field,
}

fn schema() -> (Schema, Fields) {
    let mut b = Schema::builder();
    let folded = TextOptions::default()
        .set_indexing_options(TextFieldIndexing::default().set_tokenizer(FOLD).set_index_option(IndexRecordOption::WithFreqsAndPositions));
    let f = Fields {
        uri: b.add_text_field("uri", STRING | STORED),
        file: b.add_text_field("file", STRING | STORED),
        source: b.add_text_field("source", STRING | STORED),
        kind: b.add_text_field("kind", STRING | STORED),
        title: b.add_text_field("title", TEXT | STORED),
        body: b.add_text_field("body", TEXT | STORED),
        aux: b.add_text_field("aux", TEXT),
        title_f: b.add_text_field("title_f", folded.clone()),
        body_f: b.add_text_field("body_f", folded),
        status: b.add_text_field("status", STRING | STORED),
        date: b.add_text_field("date", STRING | STORED),
        summary: b.add_text_field("summary", STORED),
        digest: b.add_text_field("digest", STORED),
    };
    (b.build(), f)
}

/// Analyzers are not stored with the index: every handle registers them before reading or writing.
fn register_analyzers(index: &Index) {
    let fold = TextAnalyzer::builder(SimpleTokenizer::default())
        .filter(RemoveLongFilter::limit(40))
        .filter(LowerCaser)
        .filter(AsciiFoldingFilter)
        .filter(Stemmer::new(Language::English))
        .build();
    index.tokenizers().register(FOLD, fold);
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct Manifest {
    version: u32,
    files: BTreeMap<String, String>,
    commits: Vec<String>,
}

#[derive(Debug, Default, Serialize)]
pub struct RefreshStats {
    pub entries: usize,
    pub docs: usize,
    pub commits: usize,
    pub removed: usize,
    pub skipped_locked: bool,
}

pub struct LocalIndex {
    index: Index,
    reader: IndexReader,
    f: Fields,
    dir: PathBuf,
    /// A hash of the manifest this handle last read or wrote. Another process (a CLI search, a
    /// second agent session) that refreshes the same index changes the manifest; the reader is
    /// reloaded then, or this handle would answer from old segments for as long as it lives.
    seen: Mutex<Option<u64>>,
}

#[derive(Debug, Clone, Default)]
pub struct SearchOpts {
    pub limit: usize,
    pub kinds: Vec<String>,
    pub sources: Vec<String>,
}

struct DocIn {
    uri: String,
    file: String,
    source: &'static str,
    kind: String,
    title: String,
    body: String,
    aux: String,
    status: String,
    date: String,
    summary: String,
}

/// A digest of the whole normalized text (title and body): equal only for copies of the same text.
pub fn text_digest(title: &str, body: &str) -> String {
    let norm: String = format!("{title} {body}").to_lowercase().chars().filter(|c| c.is_alphanumeric()).collect();
    format!("{:016x}", util::fnv1a64(norm.as_bytes()))
}

fn read_manifest(dir: &Path) -> (Option<Manifest>, Option<u64>) {
    match std::fs::read_to_string(dir.join("manifest.json")) {
        Ok(text) => (serde_json::from_str(&text).ok(), Some(util::fnv1a64(text.as_bytes()))),
        Err(_) => (None, None),
    }
}

fn unique_suffix() -> String {
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    format!("{}-{nanos}", std::process::id())
}

/// Build an empty index of this schema in a sibling directory and rename it into place. The old
/// directory is moved aside first, never deleted where it stands: a process still using it keeps
/// its files, and nobody else knows the moved name. When another process installs its own first,
/// that one is kept.
fn install_fresh(dir: &Path) -> Result<()> {
    let parent = dir.parent().context("index directory has no parent")?;
    std::fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    let name = dir.file_name().and_then(|n| n.to_str()).unwrap_or("index");
    let suffix = unique_suffix();
    let tmp = parent.join(format!("{name}.build-{suffix}"));
    std::fs::create_dir_all(&tmp).with_context(|| format!("create {}", tmp.display()))?;
    let (schema, _) = schema();
    let built = Index::create_in_dir(&tmp, schema).context("create search index").and_then(|_| {
        let manifest = Manifest { version: SCHEMA_VERSION, ..Default::default() };
        util::write_atomic(&tmp.join("manifest.json"), &serde_json::to_string(&manifest)?)
    });
    if let Err(e) = built {
        let _ = std::fs::remove_dir_all(&tmp);
        return Err(e);
    }
    let old = parent.join(format!("{name}.old-{suffix}"));
    // moved aside only while still stale: another process may have installed a fresh one meanwhile
    let current = read_manifest(dir).0.is_some_and(|m| m.version == SCHEMA_VERSION) && open_existing(dir).is_ok();
    let moved = dir.exists() && !current && std::fs::rename(dir, &old).is_ok();
    if std::fs::rename(&tmp, dir).is_err() {
        // another process put its fresh index there first
        let _ = std::fs::remove_dir_all(&tmp);
    }
    if moved {
        let _ = std::fs::remove_dir_all(&old);
    }
    Ok(())
}

fn open_existing(dir: &Path) -> Result<Index> {
    let index = Index::open_in_dir(dir).context("open search index")?;
    let (schema, _) = schema();
    if index.schema() != schema {
        anyhow::bail!("search index has another schema");
    }
    Ok(index)
}

impl LocalIndex {
    pub fn open(repo: &Repo) -> Result<LocalIndex> {
        Self::open_at(repo.worktree_state_dir().join("index"))
    }

    /// Open the index in `dir`, creating it, or rebuilding it beside and swapping it in when it has
    /// another schema version. A directory without a manifest may be another process's build in
    /// progress: it is opened as it is, and replaced only when it cannot be opened.
    pub fn open_at(dir: PathBuf) -> Result<LocalIndex> {
        let (manifest, _) = read_manifest(&dir);
        let version = manifest.as_ref().map(|m| m.version);
        if !dir.exists() || version.is_some_and(|v| v != SCHEMA_VERSION) {
            install_fresh(&dir)?;
        }
        let index = match open_existing(&dir) {
            Ok(index) => index,
            Err(_) => {
                install_fresh(&dir)?;
                open_existing(&dir)?
            }
        };
        register_analyzers(&index);
        let (_, f) = schema();
        let reader = index.reader_builder().reload_policy(ReloadPolicy::Manual).try_into()?;
        let (_, seen) = read_manifest(&dir);
        Ok(LocalIndex { index, reader, f, dir, seen: Mutex::new(seen) })
    }

    fn manifest_path(&self) -> PathBuf {
        self.dir.join("manifest.json")
    }

    /// Reload the reader when the manifest on disk is not the one this handle last read or wrote:
    /// another process has committed to the index. Returns whether it reloaded.
    pub fn reload_if_changed(&self) -> Result<bool> {
        let (_, now) = read_manifest(&self.dir);
        self.observe(now)
    }

    fn observe(&self, now: Option<u64>) -> Result<bool> {
        let mut seen = self.seen.lock().unwrap_or_else(|p| p.into_inner());
        if *seen == now {
            return Ok(false);
        }
        self.reader.reload()?;
        *seen = now;
        Ok(true)
    }

    fn load_manifest(&self) -> Result<Manifest> {
        let (manifest, hash) = read_manifest(&self.dir);
        self.observe(hash)?;
        Ok(manifest.unwrap_or(Manifest { version: SCHEMA_VERSION, ..Default::default() }))
    }

    /// Commit the writer, reload, and record the manifest as the one this handle has seen.
    fn commit(&self, mut writer: IndexWriter, manifest: &mut Manifest) -> Result<()> {
        writer.commit()?;
        drop(writer);
        self.reader.reload()?;
        manifest.version = SCHEMA_VERSION;
        let text = serde_json::to_string(manifest)?;
        util::write_atomic(&self.manifest_path(), &text)?;
        *self.seen.lock().unwrap_or_else(|p| p.into_inner()) = Some(util::fnv1a64(text.as_bytes()));
        Ok(())
    }

    pub fn num_docs(&self) -> u64 {
        let _ = self.reload_if_changed();
        self.reader.searcher().num_docs()
    }

    /// Bring the index up to date with the store, doc sources and history. Cheap when nothing changed.
    pub fn refresh(&self, repo: &Repo, cfg: &Config, entries: &[Entry]) -> Result<RefreshStats> {
        let mut stats = RefreshStats::default();
        // before the early return below: another process may have brought the index up to date
        let mut manifest = self.load_manifest()?;
        let store = Store::new(repo, cfg);

        let mut current: BTreeMap<String, String> = BTreeMap::new();
        let mut entry_by_path: BTreeMap<&str, &Entry> = BTreeMap::new();
        for e in entries {
            if let Some(st) = stamp(repo, &e.rel_path) {
                current.insert(e.rel_path.clone(), st);
                entry_by_path.insert(e.rel_path.as_str(), e);
            }
        }
        let include = GlobSet::new(&cfg.sources.include);
        let exclude = GlobSet::new(&cfg.sources.exclude);
        let tracked = repo.tracked_files().unwrap_or_default();
        for t in &tracked {
            let p = &t.path;
            if t.submodule || current.contains_key(p) || store.is_entry_path(p) || p == &store.index_path() {
                continue;
            }
            let textual = p.ends_with(".md") || p.ends_with(".mdx") || p.ends_with(".txt") || p.ends_with(".rst");
            if !textual || !include.matches(p) || exclude.matches(p) || crate::secrets::is_sensitive_path(p) {
                continue;
            }
            if let Some(st) = stamp(repo, p) {
                current.insert(p.clone(), st);
            }
        }

        let changed: Vec<String> = current.iter().filter(|(p, s)| manifest.files.get(*p) != Some(*s)).map(|(p, _)| p.clone()).collect();
        let removed: Vec<String> = manifest.files.keys().filter(|p| !current.contains_key(*p)).cloned().collect();

        let known: HashSet<String> = manifest.commits.iter().cloned().collect();
        let mut missing: Vec<String> = Vec::new();
        let mut in_history: HashSet<String> = HashSet::new();
        if cfg.index.commits && repo.head().is_some() {
            let revs = repo.git(&["rev-list", &format!("--max-count={}", cfg.index.max_commits.max(1)), "HEAD"]).unwrap_or_default();
            in_history = revs.lines().filter(|s| !s.is_empty()).map(str::to_string).collect();
            missing = revs.lines().map(str::to_string).filter(|s| !s.is_empty() && !known.contains(s)).collect();
        }
        // commits that left HEAD's history: rewritten by an amend or rebase, or on a branch left behind
        let stale: Vec<String> = manifest.commits.iter().filter(|s| !in_history.contains(*s)).cloned().collect();
        if changed.is_empty() && removed.is_empty() && missing.is_empty() && stale.is_empty() {
            return Ok(stats);
        }

        let writer: IndexWriter = match self.index.writer_with_num_threads(1, 40_000_000) {
            Ok(w) => w,
            Err(tantivy::TantivyError::LockFailure(..)) => {
                stats.skipped_locked = true;
                return Ok(stats);
            }
            Err(e) => return Err(e.into()),
        };
        for p in removed.iter().chain(changed.iter()) {
            writer.delete_term(Term::from_field_text(self.f.file, p));
        }
        stats.removed = removed.len();
        for p in &changed {
            let docs = match entry_by_path.get(p.as_str()) {
                Some(e) => {
                    stats.entries += 1;
                    vec![entry_doc(e)]
                }
                None => {
                    let Ok(text) = std::fs::read_to_string(repo.abs(p)) else { continue };
                    if text.len() as u64 > cfg.sources.max_bytes {
                        continue;
                    }
                    stats.docs += 1;
                    doc_chunks(p, &text)
                }
            };
            for d in docs {
                writer.add_document(self.to_doc(&d))?;
            }
        }
        for sha in &stale {
            writer.delete_term(Term::from_field_text(self.f.file, &format!("commit:{sha}")));
        }
        stats.removed += stale.len();
        if !stale.is_empty() {
            let gone: HashSet<&String> = stale.iter().collect();
            manifest.commits.retain(|s| !gone.contains(s));
        }
        if !missing.is_empty() {
            let commits = read_commits(repo, &missing)?;
            stats.commits = commits.len();
            for d in commits {
                writer.delete_term(Term::from_field_text(self.f.uri, &d.uri));
                writer.add_document(self.to_doc(&d))?;
            }
            manifest.commits.extend(missing);
        }
        manifest.files = current;
        self.commit(writer, &mut manifest)?;
        Ok(stats)
    }

    fn to_doc(&self, d: &DocIn) -> TantivyDocument {
        let mut doc = TantivyDocument::default();
        doc.add_text(self.f.uri, &d.uri);
        doc.add_text(self.f.file, &d.file);
        doc.add_text(self.f.source, d.source);
        doc.add_text(self.f.kind, &d.kind);
        doc.add_text(self.f.title, &d.title);
        doc.add_text(self.f.body, &d.body);
        doc.add_text(self.f.aux, &d.aux);
        doc.add_text(self.f.title_f, &d.title);
        doc.add_text(self.f.body_f, &d.body);
        doc.add_text(self.f.status, &d.status);
        doc.add_text(self.f.date, &d.date);
        doc.add_text(self.f.summary, &d.summary);
        doc.add_text(self.f.digest, text_digest(&d.title, &d.body));
        doc
    }

    pub fn search(&self, query: &str, o: &SearchOpts) -> Result<Vec<Hit>> {
        Ok(self.search_with_digests(query, o)?.into_iter().map(|(h, _)| h).collect())
    }

    /// Like `search`, with each hit's stored `digest` of its whole text (see `text_digest`), so a
    /// merge can tell copies of one text from two texts that only open alike.
    pub fn search_with_digests(&self, query: &str, o: &SearchOpts) -> Result<Vec<(Hit, String)>> {
        let _ = self.reload_if_changed();
        let searcher = self.reader.searcher();
        let mut qp = QueryParser::for_index(&self.index, vec![self.f.title, self.f.body, self.f.aux, self.f.title_f, self.f.body_f]);
        qp.set_field_boost(self.f.title, TITLE_BOOST);
        qp.set_field_boost(self.f.aux, AUX_BOOST);
        qp.set_field_boost(self.f.title_f, TITLE_BOOST * SHADOW_BOOST);
        qp.set_field_boost(self.f.body_f, SHADOW_BOOST);
        let cleaned = sanitize_query(query);
        let (user_q, _errors) = qp.parse_query_lenient(&cleaned);
        let mut clauses: Vec<(Occur, Box<dyn Query>)> = vec![(Occur::Must, user_q)];
        // the whole query as a phrase ranks an exact wording first; it never adds a hit of its own
        if let Some(phrase) = phrase_text(query) {
            let (pq, _) = qp.parse_query_lenient(&format!("\"{phrase}\""));
            clauses.push((Occur::Should, Box::new(tantivy::query::BoostQuery::new(pq, 2.0))));
        }
        let terms = |field: Field, values: &[String]| -> Box<dyn Query> {
            Box::new(BooleanQuery::new(
                values
                    .iter()
                    .map(|v| {
                        let q: Box<dyn Query> = Box::new(TermQuery::new(Term::from_field_text(field, v), IndexRecordOption::Basic));
                        (Occur::Should, q)
                    })
                    .collect(),
            ))
        };
        if !o.kinds.is_empty() {
            clauses.push((Occur::Must, terms(self.f.kind, &o.kinds)));
        }
        if !o.sources.is_empty() {
            clauses.push((Occur::Must, terms(self.f.source, &o.sources)));
        }
        let q = BooleanQuery::new(clauses);
        let limit = o.limit.max(1);
        let top = searcher.search(&q, &TopDocs::with_limit(limit).order_by_score())?;
        let mut sg = SnippetGenerator::create(&searcher, &q, self.f.body)?;
        sg.set_max_num_chars(220);
        let mut hits = Vec::new();
        for (score, addr) in top {
            let doc: TantivyDocument = searcher.doc(addr)?;
            let get = |f: Field| doc.get_first(f).and_then(|v| v.as_str()).unwrap_or("").to_string();
            let snippet = sg.snippet_from_doc(&doc);
            let frag = util::strip_md(snippet.fragment());
            let summary = get(self.f.summary);
            let source = get(self.f.source);
            let snippet = if (source == "entry" && !summary.is_empty()) || frag.is_empty() { summary } else { frag };
            let opt = |s: String| if s.is_empty() { None } else { Some(s) };
            let digest = get(self.f.digest);
            hits.push((
                Hit {
                    source: "local".into(),
                    uri: get(self.f.uri),
                    title: get(self.f.title),
                    snippet,
                    kind: opt(get(self.f.kind)),
                    score,
                    date: opt(get(self.f.date)),
                    status: opt(get(self.f.status)),
                    similar_to: None,
                },
                digest,
            ));
        }
        Ok(hits)
    }
}

/// Digests by URI, for `model::merge_with`.
pub fn digests_by_uri(hits: &[(Hit, String)]) -> HashMap<String, String> {
    hits.iter().filter(|(_, d)| !d.is_empty()).map(|(h, d)| (h.uri.clone(), d.clone())).collect()
}

fn stamp(repo: &Repo, rel: &str) -> Option<String> {
    let md = std::fs::metadata(repo.abs(rel)).ok()?;
    let mtime = md.modified().ok()?.duration_since(std::time::UNIX_EPOCH).ok()?.as_nanos();
    Some(format!("{mtime}:{}", md.len()))
}

/// Split camelCase / snake_case / path identifiers into words so that `snapshotUpsert` finds `snapshot upsert`.
pub fn expand_identifiers(text: &str) -> String {
    let mut out = String::new();
    for token in text.split(|c: char| !(c.is_alphanumeric() || c == '_')) {
        if token.len() < 4 {
            continue;
        }
        let has_upper_inner = token.chars().skip(1).any(|c| c.is_uppercase()) && token.chars().any(|c| c.is_lowercase());
        if !(has_upper_inner || token.contains('_')) {
            continue;
        }
        let mut word = String::new();
        let mut prev_lower = false;
        for c in token.chars() {
            if c == '_' {
                if !word.is_empty() {
                    out.push_str(&word);
                    out.push(' ');
                    word.clear();
                }
                prev_lower = false;
                continue;
            }
            if c.is_uppercase() && prev_lower && !word.is_empty() {
                out.push_str(&word);
                out.push(' ');
                word.clear();
            }
            prev_lower = c.is_lowercase() || c.is_ascii_digit();
            word.push(c);
        }
        if !word.is_empty() {
            out.push_str(&word);
            out.push(' ');
        }
    }
    out
}

/// The query's words with query syntax removed: no operators, quotes or field prefixes.
fn clean_words(q: &str) -> Vec<String> {
    let cleaned: String = q
        .chars()
        .map(|c| {
            if matches!(c, ':' | '^' | '~' | '(' | ')' | '[' | ']' | '{' | '}' | '!' | '"' | '\\' | '*' | '?' | '\'' | '`') {
                ' '
            } else {
                c
            }
        })
        .collect();
    cleaned
        .split_whitespace()
        .map(|w| w.trim_start_matches(['-', '+']).to_string())
        .filter(|w| !w.is_empty() && !matches!(w.as_str(), "AND" | "OR" | "NOT" | "IN"))
        .collect()
}

/// The text of the phrase bonus: the cleaned words as typed, without identifier expansions (an
/// expansion appended to the phrase is a word sequence no document contains). None for one word.
fn phrase_text(q: &str) -> Option<String> {
    let words = clean_words(q);
    if words.len() < 2 {
        return None;
    }
    Some(words.into_iter().take(12).collect::<Vec<_>>().join(" "))
}

fn sanitize_query(q: &str) -> String {
    let words = clean_words(q);
    let mut out = words.join(" ");
    // camelCase / snake_case identifiers also match as their word sequence (a phrase, so that
    // `RunContext` does not degrade into every document containing "run")
    for w in &words {
        let parts = expand_identifiers(w);
        let parts = parts.trim();
        if parts.contains(' ') {
            out.push_str(&format!(" \"{parts}\""));
        }
    }
    out
}

fn entry_doc(e: &Entry) -> DocIn {
    let aux = format!(
        "{} {} {} {}",
        e.tags.join(" "),
        e.paths.join(" ").replace(['/', '.', '-', '*'], " "),
        expand_identifiers(&format!("{} {}", e.title, e.body)),
        e.id.replace('-', " ")
    );
    DocIn {
        uri: format!("kx:{}", e.id),
        file: e.rel_path.clone(),
        source: "entry",
        kind: e.kind.clone(),
        title: e.title.clone(),
        body: e.body.clone(),
        aux,
        status: e.status.clone().unwrap_or_default(),
        date: e.date.clone().unwrap_or_default(),
        summary: e.summary_text(220),
    }
}

/// Split a Markdown document into heading-sized chunks so hits point at the relevant section.
fn doc_chunks(rel: &str, text: &str) -> Vec<DocIn> {
    let text = crate::store::split_frontmatter(text).map(|(_, b)| b).unwrap_or(text);
    let doc_title = text.lines().find_map(|l| l.trim().strip_prefix("# ").map(|t| t.trim().to_string())).unwrap_or_else(|| rel.to_string());
    let mut sections: Vec<(String, String)> = vec![(String::new(), String::new())];
    let mut in_code = false;
    for line in text.lines() {
        let t = line.trim_start();
        if t.starts_with("```") {
            in_code = !in_code;
        }
        if !in_code && (t.starts_with("## ") || (t.starts_with("# ") && !sections.last().unwrap().1.trim().is_empty())) {
            let heading = t.trim_start_matches('#').trim().to_string();
            sections.push((heading, String::new()));
            continue;
        }
        let last = sections.last_mut().unwrap();
        last.1.push_str(line);
        last.1.push('\n');
    }
    // merge tiny sections into the previous one
    let mut merged: Vec<(String, String)> = Vec::new();
    for (h, body) in sections {
        if body.trim().is_empty() && h.is_empty() {
            continue;
        }
        match merged.last_mut() {
            Some(prev) if body.trim().len() < 280 && prev.1.len() < 3000 => {
                if !h.is_empty() {
                    prev.1.push_str(&format!("\n{h}\n"));
                }
                prev.1.push_str(&body);
            }
            _ => merged.push((h, body)),
        }
    }
    let mut used_anchors = HashSet::new();
    let mut docs = Vec::new();
    for (i, (h, body)) in merged.into_iter().enumerate() {
        let body = util::truncate_chars(&body, 12000);
        let (uri, title) = if i == 0 || h.is_empty() {
            (format!("file:{rel}"), doc_title.clone())
        } else {
            let mut anchor = util::slugify(&h, 60);
            let base = anchor.clone();
            let mut n = 2;
            while !used_anchors.insert(anchor.clone()) {
                anchor = format!("{base}-{n}");
                n += 1;
            }
            (format!("file:{rel}#{anchor}"), format!("{doc_title} › {h}"))
        };
        if docs.iter().any(|d: &DocIn| d.uri == uri) {
            continue;
        }
        docs.push(DocIn {
            summary: util::first_paragraph(&body, 220),
            uri,
            file: rel.to_string(),
            source: "doc",
            kind: "doc".into(),
            title,
            aux: format!("{} {}", rel.replace(['/', '.', '-', '_'], " "), expand_identifiers(&body)),
            body,
            status: String::new(),
            date: String::new(),
        });
    }
    docs
}

/// Read commit metadata for `shas` in one git call.
fn read_commits(repo: &Repo, shas: &[String]) -> Result<Vec<DocIn>> {
    let mut docs = Vec::new();
    for chunk in shas.chunks(2000) {
        let input = chunk.join("\n") + "\n";
        let out = repo.git_with_stdin(
            &[
                "log",
                "--no-walk=unsorted",
                "--stdin",
                "--date=short",
                "--name-only",
                "--format=%x1e%H%x1f%h%x1f%an%x1f%ad%x1f%s%x1f%b%x1f%(trailers:only,unfold)%x1f",
            ],
            &input,
        )?;
        for rec in out.split('\x1e') {
            let parts: Vec<&str> = rec.splitn(8, '\x1f').collect();
            if parts.len() < 8 {
                continue;
            }
            let (sha, short, author, date, subject, body, trailers, files) =
                (parts[0].trim(), parts[1], parts[2], parts[3], parts[4], parts[5].trim(), parts[6].trim(), parts[7]);
            // "Merge branch 'main' into …" says nothing; PR merges ("Merge pull request #…") stay
            if subject.starts_with("Merge branch ") || subject.starts_with("Merge remote-tracking branch ") {
                continue;
            }
            let files: Vec<&str> = files.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
            let is_noise = |l: &str| {
                let l = l.trim().to_lowercase();
                l.starts_with("co-authored-by:") || l.starts_with("signed-off-by:") || l.starts_with("🤖 generated with")
            };
            let body: String = body.lines().filter(|l| !is_noise(l)).collect::<Vec<_>>().join("\n");
            let body = body.trim();
            let trailers: String = trailers.lines().filter(|l| !is_noise(l)).collect::<Vec<_>>().join("\n");
            let mut text = String::new();
            if !body.is_empty() {
                text.push_str(&util::truncate_chars(body, 4000));
                text.push('\n');
            }
            if !trailers.trim().is_empty() && !body.contains(trailers.trim()) {
                text.push_str(trailers.trim());
                text.push('\n');
            }
            // file names are searchable (aux) but kept out of the body so snippets show the message
            let paths_words =
                files.iter().take(120).map(|f| format!("{f} {}", f.replace(['/', '.', '-', '_'], " "))).collect::<Vec<_>>().join(" ");
            docs.push(DocIn {
                uri: format!("git:{short}"),
                // the full sha is the key that deletes the commit once it leaves the history
                file: format!("commit:{sha}"),
                source: "commit",
                kind: "commit".into(),
                title: subject.to_string(),
                summary: if body.is_empty() {
                    format!("{author}, {} file(s): {}", files.len(), files.iter().take(4).copied().collect::<Vec<_>>().join(", "))
                } else {
                    util::first_sentence(body, 200)
                },
                body: text,
                aux: format!("{paths_words} {} {sha}", expand_identifiers(subject)),
                status: String::new(),
                date: date.to_string(),
            });
        }
    }
    Ok(docs)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("kx-index-{name}-{}", unique_suffix()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn doc(uri: &str, title: &str, body: &str) -> DocIn {
        DocIn {
            uri: uri.into(),
            file: uri.into(),
            source: "doc",
            kind: "doc".into(),
            title: title.into(),
            body: body.into(),
            aux: String::new(),
            status: String::new(),
            date: String::new(),
            summary: String::new(),
        }
    }

    impl LocalIndex {
        /// What a refresh does, without a repository: add `docs`, commit, record the manifest.
        fn add_for_test(&self, docs: &[DocIn]) {
            let mut manifest = self.load_manifest().unwrap();
            let writer: IndexWriter = self.index.writer_with_num_threads(1, 15_000_000).unwrap();
            for d in docs {
                writer.add_document(self.to_doc(d)).unwrap();
                manifest.files.insert(d.file.clone(), "test".into());
            }
            self.commit(writer, &mut manifest).unwrap();
        }

        fn uris(&self, q: &str) -> Vec<String> {
            self.search(q, &SearchOpts { limit: 10, ..Default::default() }).unwrap().into_iter().map(|h| h.uri).collect()
        }
    }

    #[test]
    fn identifiers() {
        assert_eq!(expand_identifiers("orderTotal user_account_balances x"), "order Total user account balances ");
        assert_eq!(expand_identifiers("plain words only"), "");
    }

    #[test]
    fn chunks() {
        let text =
            "# Guide\n\nIntro paragraph that is here.\n\n## Deploy\n\n".to_string() + &"Deploy details. ".repeat(40) + "\n\n## Tiny\n\nx\n";
        let docs = doc_chunks("docs/guide.md", &text);
        assert_eq!(docs[0].uri, "file:docs/guide.md");
        assert!(docs.iter().any(|d| d.uri == "file:docs/guide.md#deploy"));
        assert!(docs.iter().all(|d| !d.uri.ends_with("#tiny")), "tiny sections merge into the previous chunk");
    }

    #[test]
    fn sanitizes() {
        assert_eq!(sanitize_query("why: -foo (bar) NOT baz"), "why foo bar baz");
        assert_eq!(sanitize_query("RunContext"), "RunContext \"Run Context\"");
    }

    #[test]
    fn phrase_clause_with_camel_case_keeps_the_typed_words() {
        assert_eq!(phrase_text("snapshotUpsert retry").as_deref(), Some("snapshotUpsert retry"));
        assert_eq!(phrase_text("fooBar baz").as_deref(), Some("fooBar baz"));
        assert_eq!(phrase_text("\"user_id\" (lookup)").as_deref(), Some("user_id lookup"));
        assert_eq!(phrase_text("single"), None);
        let dir = scratch("phrase");
        let idx = LocalIndex::open_at(dir.join("index")).unwrap();
        idx.add_for_test(&[
            doc("file:a.md", "Notes", "retry later; the snapshotUpsert call is idempotent and safe to repeat"),
            doc("file:b.md", "Notes", "a failed snapshotUpsert retry is safe and idempotent, repeat it later"),
        ]);
        assert_eq!(idx.uris("snapshotUpsert retry").first().map(String::as_str), Some("file:b.md"), "the exact wording ranks first");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn folding_finds_words_typed_without_diacritics() {
        let dir = scratch("fold");
        let idx = LocalIndex::open_at(dir.join("index")).unwrap();
        idx.add_for_test(&[doc("file:docs/sk/namietky.md", "Námietka k rozhodnutiu", "Námietka sa zapíše do schránky.")]);
        assert_eq!(idx.uris("namietka"), vec!["file:docs/sk/namietky.md".to_string()]);
        assert_eq!(idx.uris("NÁMIETKA"), vec!["file:docs/sk/namietky.md".to_string()]);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn stemming_matches_singular_and_plural() {
        let dir = scratch("stem");
        let idx = LocalIndex::open_at(dir.join("index")).unwrap();
        idx.add_for_test(&[
            doc("kx:adapters", "Adapters are configuration", "The core never names a product."),
            doc("kx:retry", "Retry payments", "A retried request never charges twice."),
        ]);
        assert_eq!(idx.uris("adapter"), vec!["kx:adapters".to_string()]);
        assert_eq!(idx.uris("retries"), vec!["kx:retry".to_string()]);
        // the exact form still counts more than its stem
        let exact = idx.search("adapters", &SearchOpts { limit: 1, ..Default::default() }).unwrap()[0].score;
        let stem = idx.search("adapter", &SearchOpts { limit: 1, ..Default::default() }).unwrap()[0].score;
        assert!(exact > stem, "{exact} vs {stem}");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn digest_covers_the_whole_text() {
        assert_eq!(text_digest("Prices", "Integer cents."), text_digest("prices", "integer  cents"));
        assert_ne!(text_digest("Prices", "at most 3 times"), text_digest("Prices", "at most 10 times"));
        let dir = scratch("digest");
        let idx = LocalIndex::open_at(dir.join("index")).unwrap();
        idx.add_for_test(&[doc("kx:a", "Frobnicator retries", "at most 3 times")]);
        let hits = idx.search_with_digests("frobnicator", &SearchOpts { limit: 5, ..Default::default() }).unwrap();
        assert_eq!(hits[0].1, text_digest("Frobnicator retries", "at most 3 times"));
        assert_eq!(digests_by_uri(&hits).get("kx:a"), Some(&hits[0].1));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn two_handles_see_each_others_commits() {
        let dir = scratch("two");
        let a = LocalIndex::open_at(dir.join("index")).unwrap();
        let b = LocalIndex::open_at(dir.join("index")).unwrap();
        assert!(b.uris("zorbquux").is_empty());
        a.add_for_test(&[doc("file:docs/zorbquux.md", "Zorbquux", "zorbquux lives here")]);
        // b never committed anything itself: it must still see a's segments
        assert_eq!(b.uris("zorbquux"), vec!["file:docs/zorbquux.md".to_string()]);
        assert!(!b.reload_if_changed().unwrap(), "nothing new since the last look");
        b.add_for_test(&[doc("file:docs/flimbatz.md", "Flimbatz", "flimbatz too")]);
        assert!(a.reload_if_changed().unwrap());
        assert_eq!(a.uris("flimbatz"), vec!["file:docs/flimbatz.md".to_string()]);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn open_does_not_remove_dir_being_built() {
        let base = scratch("building");
        let dir = base.join("index");
        // a first build in progress: the index exists, its manifest is not written yet
        std::fs::create_dir_all(&dir).unwrap();
        let (schema, _) = schema();
        Index::create_in_dir(&dir, schema).unwrap();
        std::fs::write(dir.join("in-progress"), "x").unwrap();
        let idx = LocalIndex::open_at(dir.clone()).unwrap();
        assert!(dir.join("in-progress").exists(), "the directory another process is building stays");
        drop(idx);

        // an index of another schema version is replaced by a fresh one built beside it
        std::fs::write(dir.join("manifest.json"), r#"{"version":1,"files":{},"commits":[]}"#).unwrap();
        let idx = LocalIndex::open_at(dir.clone()).unwrap();
        assert!(!dir.join("in-progress").exists());
        let (manifest, _) = read_manifest(&dir);
        assert_eq!(manifest.map(|m| m.version), Some(SCHEMA_VERSION));
        let leftovers: Vec<_> = std::fs::read_dir(&base).unwrap().flatten().filter(|e| e.file_name() != "index").collect();
        assert!(leftovers.is_empty(), "no build or old directory is left behind");
        assert_eq!(idx.num_docs(), 0);
        let _ = std::fs::remove_dir_all(base);
    }
}
