//! Local BM25 index (Tantivy) over store entries, repository docs and commit history.
//! Derived data only: it lives in `<git-common-dir>/kontext/worktrees/<wt>/index` and is rebuilt
//! incrementally from the sources of truth (files + git) whenever it is queried.
use crate::config::Config;
use crate::glob::GlobSet;
use crate::model::Hit;
use crate::repo::Repo;
use crate::store::{Entry, Store};
use crate::util;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};
use std::path::PathBuf;
use tantivy::collector::TopDocs;
use tantivy::directory::MmapDirectory;
use tantivy::query::{BooleanQuery, Occur, Query, QueryParser, TermQuery};
use tantivy::schema::{Field, IndexRecordOption, STORED, STRING, Schema, TEXT, Value};
use tantivy::snippet::SnippetGenerator;
use tantivy::{Index, IndexReader, IndexWriter, ReloadPolicy, TantivyDocument, Term};

const SCHEMA_VERSION: u32 = 5;

#[derive(Clone, Copy)]
struct Fields {
    uri: Field,
    file: Field,
    source: Field,
    kind: Field,
    title: Field,
    body: Field,
    aux: Field,
    status: Field,
    date: Field,
    summary: Field,
}

fn schema() -> (Schema, Fields) {
    let mut b = Schema::builder();
    let f = Fields {
        uri: b.add_text_field("uri", STRING | STORED),
        file: b.add_text_field("file", STRING | STORED),
        source: b.add_text_field("source", STRING | STORED),
        kind: b.add_text_field("kind", STRING | STORED),
        title: b.add_text_field("title", TEXT | STORED),
        body: b.add_text_field("body", TEXT | STORED),
        aux: b.add_text_field("aux", TEXT),
        status: b.add_text_field("status", STRING | STORED),
        date: b.add_text_field("date", STRING | STORED),
        summary: b.add_text_field("summary", STORED),
    };
    (b.build(), f)
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

impl LocalIndex {
    pub fn open(repo: &Repo) -> Result<LocalIndex> {
        let dir = repo.worktree_state_dir().join("index");
        let manifest_path = dir.join("manifest.json");
        let manifest: Option<Manifest> = std::fs::read_to_string(&manifest_path).ok().and_then(|t| serde_json::from_str(&t).ok());
        if dir.exists() && manifest.as_ref().map(|m| m.version) != Some(SCHEMA_VERSION) {
            let _ = std::fs::remove_dir_all(&dir);
        }
        std::fs::create_dir_all(&dir).with_context(|| format!("create {}", dir.display()))?;
        let (schema, f) = schema();
        let mmap = MmapDirectory::open(&dir).context("open index directory")?;
        let index = Index::open_or_create(mmap, schema).context("open search index")?;
        let reader = index.reader_builder().reload_policy(ReloadPolicy::Manual).try_into()?;
        Ok(LocalIndex { index, reader, f, dir })
    }

    fn manifest_path(&self) -> PathBuf {
        self.dir.join("manifest.json")
    }

    fn load_manifest(&self) -> Manifest {
        std::fs::read_to_string(self.manifest_path())
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or(Manifest { version: SCHEMA_VERSION, ..Default::default() })
    }

    pub fn num_docs(&self) -> u64 {
        self.reader.searcher().num_docs()
    }

    /// Bring the index up to date with the store, doc sources and history. Cheap when nothing changed.
    pub fn refresh(&self, repo: &Repo, cfg: &Config, entries: &[Entry]) -> Result<RefreshStats> {
        let mut stats = RefreshStats::default();
        let mut manifest = self.load_manifest();
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

        let mut writer: IndexWriter = match self.index.writer_with_num_threads(1, 40_000_000) {
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
        writer.commit()?;
        drop(writer);
        self.reader.reload()?;
        manifest.version = SCHEMA_VERSION;
        manifest.files = current;
        util::write_atomic(&self.manifest_path(), &serde_json::to_string(&manifest)?)?;
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
        doc.add_text(self.f.status, &d.status);
        doc.add_text(self.f.date, &d.date);
        doc.add_text(self.f.summary, &d.summary);
        doc
    }

    pub fn search(&self, query: &str, o: &SearchOpts) -> Result<Vec<Hit>> {
        let searcher = self.reader.searcher();
        let mut qp = QueryParser::for_index(&self.index, vec![self.f.title, self.f.body, self.f.aux]);
        qp.set_field_boost(self.f.title, 2.5);
        qp.set_field_boost(self.f.aux, 1.5);
        let (user_q, _errors) = qp.parse_query_lenient(&sanitize_query(query));
        let mut clauses: Vec<(Occur, Box<dyn Query>)> = vec![(Occur::Must, user_q)];
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
            hits.push(Hit {
                source: "local".into(),
                uri: get(self.f.uri),
                title: get(self.f.title),
                snippet,
                kind: opt(get(self.f.kind)),
                score,
                date: opt(get(self.f.date)),
                status: opt(get(self.f.status)),
            });
        }
        Ok(hits)
    }
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

fn sanitize_query(q: &str) -> String {
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
    let words: Vec<String> = cleaned
        .split_whitespace()
        .map(|w| w.trim_start_matches(['-', '+']).to_string())
        .filter(|w| !w.is_empty() && !matches!(w.as_str(), "AND" | "OR" | "NOT" | "IN"))
        .collect();
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
}
