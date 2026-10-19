//! Machine-readable knowledge queries and exports pinned to an explicit review ref.
use crate::app::App;
use crate::store::{Entry, FmValue, Store};
use anyhow::Result;
use serde_json::{Value, json};

const MAX_EXPORT_BYTES: usize = 32 * 1024 * 1024;
const MAX_BLOB_BYTES: usize = 1024 * 1024;

pub fn call(app: &App, operation: &str, input: &Value) -> Result<Value> {
    let result = match operation {
        "status" => status(app),
        "entries.list" => list(app, input),
        "entries.get" => {
            let id = string(input, "id")?;
            let entries = entries(app)?;
            let entry = Store::find(&entries, id).ok_or_else(|| crate::api::error("not_found", format!("no entry '{id}'")))?;
            Ok(json!({"entry":entry_json(entry)}))
        }
        "entries.search" => search(app, input),
        "entries.governing" => governing(app, input),
        "entries.export" => export(app, input),
        _ => Err(crate::api::error("unsupported_operation", format!("unknown read operation '{operation}'"))),
    }?;
    if operation == "status" || operation == "entries.export" { Ok(result) } else { working_result(app, result) }
}

pub fn entry_json(entry: &Entry) -> Value {
    let extra: serde_json::Map<String, Value> = entry
        .extra
        .iter()
        .map(|(k, v)| {
            let value = match v {
                FmValue::Str(s) => json!(s),
                FmValue::List(items) => json!(items),
            };
            (k.clone(), value)
        })
        .collect();
    let mut out = json!({
        "id":entry.id,"kind":entry.kind,"title":entry.title,"body":entry.body,
        "summary":entry.summary_text(usize::MAX),"status":entry.status,"date":entry.date,"review_by":entry.review_by,
        "paths":entry.paths,"tags":entry.tags,"file":entry.rel_path,"markdown":entry.markdown(),
        "style":entry.style,"supersedes":entry.supersedes,"superseded_by":entry.superseded_by,
        "commits":entry.commits,"author":entry.author,"visibility":entry.visibility,"origin":entry.origin,
        "extra":extra
    });
    for (key, value) in extra {
        out.as_object_mut().expect("object").entry(key).or_insert(value);
    }
    out
}

struct WorkingView {
    revision: Option<String>,
    changes: Vec<(String, String)>,
}

impl WorkingView {
    fn new(app: &App) -> Result<Self> {
        let dirs: Vec<String> = app.cfg().kind_names().iter().map(|kind| app.cfg().kind_path(kind)).collect();
        let mut args = vec![
            "--literal-pathspecs",
            "status",
            "--porcelain=v1",
            "-z",
            "--no-renames",
            "--untracked-files=all",
            "--ignored=matching",
            "--",
        ];
        args.extend(dirs.iter().map(String::as_str));
        let data = app.repo.git_bytes(&args)?;
        let changes = data
            .split(|b| *b == 0)
            .filter_map(|record| {
                if record.len() < 4 {
                    return None;
                }
                Some((String::from_utf8_lossy(&record[3..]).to_string(), String::from_utf8_lossy(&record[..2]).to_string()))
            })
            .collect();
        Ok(Self { revision: app.repo.head(), changes })
    }
    fn annotate(&self, entry: &mut Value) {
        let Some(file) = entry.get("file").and_then(Value::as_str) else { return };
        let change = self
            .changes
            .iter()
            .find(|(path, _)| path == file || (path.ends_with('/') && file.starts_with(path)))
            .map(|(_, status)| status.as_str())
            .unwrap_or("  ");
        let untracked = matches!(change, "??" | "!!");
        let staged = !untracked && change.as_bytes()[0] != b' ';
        let modified = !untracked && change.as_bytes()[1] != b' ';
        let state = if untracked {
            "draft"
        } else if modified {
            "modified"
        } else if staged {
            "staged"
        } else {
            "committed"
        };
        entry["publication"] = json!({"state":state,"revision":self.revision,"staged":staged,"modified":modified});
    }
}

/// Knowledge files that differ from HEAD in the working tree or the index: drafts, edits and staged
/// changes nobody has approved yet.
pub(crate) fn unapproved_files(app: &App) -> Vec<String> {
    WorkingView::new(app).map(|v| v.changes.into_iter().map(|(path, _)| path).collect()).unwrap_or_default()
}

pub(crate) fn is_unapproved(files: &[String], file: &str) -> bool {
    files.iter().any(|path| path == file || (path.ends_with('/') && file.starts_with(path.as_str())))
}

pub fn entry_json_in(app: &App, entry: &Entry) -> Result<Value> {
    let mut value = entry_json(entry);
    WorkingView::new(app)?.annotate(&mut value);
    Ok(value)
}

fn working_result(app: &App, mut result: Value) -> Result<Value> {
    let view = WorkingView::new(app)?;
    if let Some(entry) = result.get_mut("entry") {
        view.annotate(entry);
    }
    if let Some(entries) = result.get_mut("entries").and_then(Value::as_array_mut) {
        for entry in entries {
            view.annotate(entry);
        }
    }
    if let Some(matches) = result.get_mut("matches").and_then(Value::as_array_mut) {
        for item in matches {
            if let Some(entry) = item.get_mut("entry") {
                view.annotate(entry);
            }
        }
    }
    if let Some(base) = result.get("snapshot") {
        result["snapshot"] = json!(format!("kx1-{}", crate::util::short_hash(&json!([base, view.changes]).to_string())));
    }
    result["view"] = json!("working_tree");
    result["revision"] = json!(view.revision);
    Ok(result)
}

fn string<'a>(input: &'a Value, key: &str) -> Result<&'a str> {
    input
        .get(key)
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty() && s.len() <= 4096)
        .ok_or_else(|| crate::api::error("invalid_input", format!("{key} must be a non-empty string of at most 4096 bytes")))
}

fn strings(input: &Value, key: &str) -> Result<Vec<String>> {
    let Some(value) = input.get(key) else { return Ok(Vec::new()) };
    let array = value
        .as_array()
        .filter(|a| a.len() <= 500)
        .ok_or_else(|| crate::api::error("invalid_input", format!("{key} must be an array of at most 500 strings")))?;
    array
        .iter()
        .map(|v| {
            v.as_str()
                .filter(|s| !s.is_empty() && s.len() <= 4096)
                .map(str::to_string)
                .ok_or_else(|| crate::api::error("invalid_input", format!("{key} must contain non-empty strings of at most 4096 bytes")))
        })
        .collect()
}

fn number(input: &Value, key: &str, default: usize, max: usize, min: usize) -> Result<usize> {
    match input.get(key) {
        None => Ok(default),
        Some(value) => value
            .as_u64()
            .and_then(|n| usize::try_from(n).ok())
            .filter(|n| *n >= min && *n <= max)
            .ok_or_else(|| crate::api::error("invalid_input", format!("{key} must be an integer between {min} and {max}"))),
    }
}

fn active_only(input: &Value) -> Result<bool> {
    match input.get("active_only") {
        None => Ok(true),
        Some(v) => v.as_bool().ok_or_else(|| crate::api::error("invalid_input", "active_only must be a boolean")),
    }
}

fn entries(app: &App) -> Result<Vec<Entry>> {
    let (mut entries, errors) = app.entries();
    if !errors.is_empty() {
        return Err(crate::api::error("store_read_failed", errors.join("; ")));
    }
    entries.sort_by(|a, b| a.rel_path.cmp(&b.rel_path).then(a.id.cmp(&b.id)));
    Ok(entries)
}

fn snapshot(app: &App, entries: &[Entry]) -> String {
    let values: Vec<Value> = entries.iter().map(entry_json).collect();
    let canonical = json!({"revision":app.repo.head(),"store":app.cfg().store,"entries":values});
    format!("kx1-{}", crate::util::short_hash(&canonical.to_string()))
}

fn status(app: &App) -> Result<Value> {
    let mut adapters: Vec<String> = app.registry().with_op("llm").iter().map(|a| a.name.clone()).collect();
    adapters.sort();
    let configured = app.cfg().init.llm.as_ref();
    let selected = match configured {
        Some(name) if adapters.contains(name) => Some(name.clone()),
        Some(_) => None,
        None if adapters.len() == 1 => adapters.first().cloned(),
        None => None,
    };
    let available = selected.is_some();
    Ok(json!({"api_version":1,"version":env!("CARGO_PKG_VERSION"),"root":app.repo.root,
        "store":{"configured":app.is_set_up() || app.cfg().kind_names().iter().any(|kind|app.repo.abs(&app.cfg().kind_path(kind)).is_dir()),"dir":app.store().dir()},"capabilities":crate::api::OPERATIONS,
        "llm":{"available":available,"adapters":adapters,"selected":selected,"configured":configured,"requires_selection":configured.is_none() && adapters.len()>1},"revision":app.repo.head()}))
}

fn list(app: &App, input: &Value) -> Result<Value> {
    let limit = number(input, "limit", 100, 500, 1)?;
    let offset = number(input, "offset", 0, usize::MAX, 0)?;
    let active = active_only(input)?;
    let kinds = strings(input, "kinds")?;
    let tags = strings(input, "tags")?;
    let all = entries(app)?;
    let snapshot = snapshot(app, &all);
    let selected: Vec<&Entry> = all
        .iter()
        .filter(|e| {
            (!active || e.is_active())
                && (kinds.is_empty() || kinds.contains(&e.kind))
                && (tags.is_empty() || tags.iter().any(|t| e.tags.contains(t)))
        })
        .collect();
    let total = selected.len();
    let entries: Vec<Value> = selected.into_iter().skip(offset).take(limit).map(entry_json).collect();
    let next = offset.saturating_add(entries.len());
    Ok(json!({"entries":entries,"total":total,"next_offset":(next<total).then_some(next),"snapshot":snapshot}))
}

/// Use the search library's Unicode folding implementation for literal metadata searches. The raw
/// tokenizer keeps paths/identifier punctuation; removing marks also handles decomposed accents.
fn fold(text: &str) -> String {
    use tantivy::tokenizer::{AsciiFoldingFilter, LowerCaser, RawTokenizer, TextAnalyzer, TokenStream};
    static MARKS: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| regex::Regex::new(r"\p{M}").expect("Unicode mark class"));
    let mut analyzer = TextAnalyzer::builder(RawTokenizer::default()).filter(LowerCaser).filter(AsciiFoldingFilter).build();
    let mut stream = analyzer.token_stream(text);
    if stream.advance() { MARKS.replace_all(&stream.token().text, "").into_owned() } else { String::new() }
}

fn search(app: &App, input: &Value) -> Result<Value> {
    let query = string(input, "query")?;
    let limit = number(input, "limit", 20, 100, 1)?;
    let active = active_only(input)?;
    let all = entries(app)?;
    let eligible: Vec<Entry> = all.iter().filter(|e| !active || e.is_active()).cloned().collect();
    if eligible.is_empty() {
        return Ok(json!({"entries":[],"total":0}));
    }
    if let Some(entry) = Store::find(&eligible, query) {
        return Ok(json!({"entries":[entry_json(entry)],"total":1}));
    }
    let wanted = fold(query.trim());
    let folded: Vec<(&Entry, String)> = eligible.iter().map(|e| (e, fold(&e.id))).collect();
    let exact: Vec<&Entry> = folded.iter().filter(|(_, id)| *id == wanted).map(|(e, _)| *e).collect();
    let prefixed: Vec<&Entry> = folded.iter().filter(|(_, id)| id.starts_with(&wanted)).map(|(e, _)| *e).collect();
    let selected = if !exact.is_empty() {
        exact
    } else if prefixed.len() == 1 {
        prefixed
    } else {
        Vec::new()
    };
    if !selected.is_empty() {
        let total = selected.len();
        let entries: Vec<Value> = selected.into_iter().take(limit).map(entry_json).collect();
        return Ok(json!({"entries":entries,"total":total}));
    }
    let words: Vec<&str> = wanted.split_whitespace().collect();
    let mut selected: Vec<&Entry> = if words.is_empty() {
        Vec::new()
    } else {
        eligible
            .iter()
            .filter(|entry| {
                let metadata = fold(&format!(
                    "{} {} {} {} {} {}",
                    entry.id,
                    entry.kind,
                    entry.title,
                    entry.summary_text(usize::MAX),
                    entry.tags.join(" "),
                    entry.paths.join(" ")
                ));
                words.iter().all(|word| metadata.contains(word))
            })
            .collect()
    };
    selected.sort_by(|a, b| b.date.cmp(&a.date).then(a.rel_path.cmp(&b.rel_path)));
    let hits = app.with_index(|index| {
        // The application helper throttles refreshes; machine queries promise current entries.
        index.refresh(&app.repo, app.cfg(), &all)?;
        index.search(query, &crate::index::SearchOpts { limit: all.len(), sources: vec!["entry".into()], ..Default::default() })
    })?;
    let mut seen: std::collections::HashSet<&str> = selected.iter().map(|entry| entry.id.as_str()).collect();
    for hit in &hits {
        if let Some(entry) = hit.uri.strip_prefix("kx:").and_then(|id| eligible.iter().find(|e| e.id == id))
            && seen.insert(entry.id.as_str())
        {
            selected.push(entry);
        }
    }
    let total = selected.len();
    let entries: Vec<Value> = selected.into_iter().take(limit).map(entry_json).collect();
    Ok(json!({"entries":entries,"total":total}))
}

fn validate_path(path: &str) -> Result<String> {
    let path = path.trim_start_matches("./");
    if path.is_empty()
        || path.starts_with('/')
        || path.contains(['\\', ':', '\0', '\n', '\r'])
        || path.split('/').any(|part| matches!(part, ".." | "." | ""))
    {
        return Err(crate::api::error("invalid_input", format!("expected a repository-relative path: {path}")));
    }
    Ok(path.to_string())
}

/// A compact notice policy: wildcard patterns score their literal directory prefix; an exact
/// file scores 2 at the root and 3 below it. A score of 3 preserves deeply scoped notices.
fn specificity(pattern: &str) -> usize {
    let pattern = pattern.trim().trim_start_matches("./").trim_start_matches('/').trim_end_matches('/');
    let segments: Vec<&str> = pattern.split('/').collect();
    if !pattern.contains(['*', '?', '[', '{']) && segments.last().is_some_and(|s| s.contains('.')) {
        return if segments.len() > 1 { 3 } else { 2 };
    }
    segments.iter().take_while(|s| !s.contains(['*', '?', '[', '{'])).count().min(3)
}

fn governing(app: &App, input: &Value) -> Result<Value> {
    if !input.get("paths").is_some_and(Value::is_array) {
        return Err(crate::api::error("invalid_input", "paths must be an array"));
    }
    let paths: Vec<String> = strings(input, "paths")?.iter().map(|p| validate_path(p)).collect::<Result<_>>()?;
    let excluded = strings(input, "exclude_ids")?;
    let limit = number(input, "limit", 500, 5000, 1)?;
    let min = number(input, "min_specificity", 0, usize::MAX, 0)?;
    let all = entries(app)?;
    let snapshot = snapshot(app, &all);
    let mut matches = Vec::new();
    for entry in all.iter().filter(|e| e.is_active() && !excluded.contains(&e.id)) {
        for pattern in &entry.paths {
            let specific = specificity(pattern);
            if specific < min {
                continue;
            }
            for path in &paths {
                if crate::glob::path_matches(pattern, path) {
                    matches.push((specific, entry, pattern, path));
                }
            }
        }
    }
    matches.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.rel_path.cmp(&b.1.rel_path)).then(a.3.cmp(b.3)).then(a.2.cmp(b.2)));
    let matches:Vec<Value> = matches.into_iter().take(limit).map(|(specificity,entry,pattern,path)|
        json!({"entry":entry_json(entry),"pattern":pattern,"path":path,"specificity":specificity})).collect();
    Ok(json!({"matches":matches,"snapshot":snapshot}))
}

fn git_bounded(app: &App, args: &[&str], max: usize) -> Result<Vec<u8>> {
    use std::io::Read;
    use std::process::{Command, Stdio};
    let mut child = Command::new("git")
        .arg("-C")
        .arg(&app.repo.root)
        .args(args)
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("GIT_NO_REPLACE_OBJECTS", "1")
        .env("GIT_NO_LAZY_FETCH", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;
    let mut data = Vec::new();
    let read = child.stdout.take().expect("piped stdout").take(max as u64 + 1).read_to_end(&mut data);
    if read.is_err() || data.len() > max {
        let _ = child.kill();
        let _ = child.wait();
        read?;
        return Err(crate::api::error("export_too_large", "reviewed export exceeds its size limit"));
    }
    if !child.wait()?.success() {
        return Err(crate::api::error("git_read_failed", "failed to read the requested committed knowledge tree"));
    }
    Ok(data)
}

fn committed_config(app: &App, revision: &str) -> Result<(crate::config::Config, Value)> {
    let candidates = crate::application::KnowledgePolicy::config_candidates(&app.store().dir())
        .map_err(|message| crate::api::error("invalid_config", message))?;
    for candidate in candidates {
        let Ok(path) = validate_path(&candidate) else { continue };
        let listing = git_bounded(app, &["--literal-pathspecs", "ls-tree", "-z", "-l", revision, "--", &path], 8192)?;
        if listing.is_empty() {
            continue;
        }
        let listing = std::str::from_utf8(&listing)?;
        let records: Vec<&str> = listing.split('\0').filter(|record| !record.is_empty()).collect();
        let (metadata, listed) = records
            .first()
            .and_then(|record| record.split_once('\t'))
            .ok_or_else(|| crate::api::error("invalid_config", "invalid committed configuration tree entry"))?;
        let fields: Vec<&str> = metadata.split_whitespace().collect();
        if records.len() != 1
            || listed != path
            || !matches!(fields.first().copied(), Some("100644" | "100755"))
            || fields.get(1) != Some(&"blob")
        {
            return Err(crate::api::error("invalid_config", "committed configuration must be a regular file"));
        }
        let blob = fields.get(2).ok_or_else(|| crate::api::error("invalid_config", "committed configuration has no blob"))?;
        let bytes = git_bounded(app, &["cat-file", "blob", blob], MAX_BLOB_BYTES)?;
        let text = std::str::from_utf8(&bytes)?;
        let cfg = crate::application::KnowledgePolicy::from_config(&app.store().dir(), Some(text))
            .map_err(|message| crate::api::error("invalid_config", message))?
            .cfg;
        return Ok((cfg, json!({"kind":"committed","file":path,"revision":revision})));
    }
    // Locally configured stores are supported, but that boundary is not evidence of review.
    // Its source is explicit so callers can require committed configuration in their policy.
    let mut cfg: crate::config::Config = toml::from_str(crate::config::DEFAULTS)?;
    cfg.store = app.cfg().store.clone();
    Ok((cfg, json!({"kind":"local_configuration"})))
}

fn export(app: &App, input: &Value) -> Result<Value> {
    let revision = string(input, "revision")?;
    let review_ref = string(input, "review_ref")?;
    if !matches!(revision.len(), 40 | 64) || !revision.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(crate::api::error("invalid_input", "revision must be a full Git object id"));
    }
    if !(review_ref.starts_with("refs/heads/") || review_ref.starts_with("refs/remotes/"))
        || review_ref.ends_with("/HEAD")
        || !app.repo.git_status_ok(&["check-ref-format", review_ref])
        || app.repo.git_status_ok(&["symbolic-ref", "-q", review_ref])
    {
        return Err(crate::api::error("invalid_input", "review_ref must explicitly name a local branch or remote-tracking branch"));
    }
    let resolved = app
        .repo
        .git_opt(&["rev-parse", "--verify", &format!("{review_ref}^{{commit}}")])
        .ok_or_else(|| crate::api::error("review_ref_not_found", "review_ref has no locally available commit"))?;
    if !resolved.eq_ignore_ascii_case(revision) {
        return Err(crate::api::error("review_revision_mismatch", "revision is not the current tip of review_ref"));
    }
    let requested: Vec<String> = strings(input, "paths")?.iter().map(|p| validate_path(p)).collect::<Result<_>>()?;
    let (cfg, config_source) = committed_config(app, &resolved)?;
    let policy = crate::application::KnowledgePolicy { cfg: cfg.clone() };
    let store = Store::new(&app.repo, &cfg);
    let mut dirs: Vec<String> = cfg.kind_names().iter().map(|kind| validate_path(&cfg.kind_path(kind))).collect::<Result<_>>()?;
    dirs.sort();
    dirs.dedup();
    if dirs.is_empty() {
        return Ok(json!({"revision":resolved,"review_ref":review_ref,"entries":[],"snapshot":resolved,"config_source":config_source}));
    }
    let mut args = vec!["--literal-pathspecs", "ls-tree", "-r", "-z", "-l", &resolved, "--"];
    args.extend(dirs.iter().map(String::as_str));
    let listing = git_bounded(app, &args, 8 * 1024 * 1024)?;
    let mut exported = Vec::new();
    let mut total_bytes: usize = 0;
    let mut read_blobs = 0;
    for record in listing.split(|b| *b == 0).filter(|b| !b.is_empty()) {
        let text = std::str::from_utf8(record)?;
        let Some((header, path)) = text.split_once('\t') else { continue };
        let mut fields = header.split_whitespace();
        let mode = fields.next().unwrap_or_default();
        let kind = fields.next().unwrap_or_default();
        let blob = fields.next().unwrap_or_default();
        let size = fields.next().and_then(|v| v.parse::<usize>().ok()).unwrap_or(usize::MAX);
        if !matches!(mode, "100644" | "100755")
            || kind != "blob"
            || !store.is_entry_path(path)
            || (!requested.is_empty() && !requested.iter().any(|p| path == p || path.starts_with(&format!("{p}/"))))
        {
            continue;
        }
        validate_path(path)?;
        if size > MAX_BLOB_BYTES || total_bytes.saturating_add(size) > MAX_EXPORT_BYTES || read_blobs >= 5000 {
            return Err(crate::api::error("export_too_large", "reviewed export exceeds its size limit"));
        }
        total_bytes += size;
        read_blobs += 1;
        let markdown = String::from_utf8(git_bounded(app, &["cat-file", "blob", blob], MAX_BLOB_BYTES)?)?;
        let kind = store.kind_for_path(path).expect("entry path has configured kind");
        let entry = Entry::parse(path, &markdown, &kind);
        if policy.inspect(path, &markdown).is_err() {
            continue;
        }
        let mut value = entry_json(&entry);
        value["markdown"] = json!(markdown);
        value["publication"] = json!({"state":"reviewed_ref","revision":resolved,"review_ref":review_ref,"blob_hash":blob});
        exported.push(json!({"entry":value,"revision":resolved,"blob_hash":blob,"markdown":markdown}));
    }
    if app.repo.git_opt(&["rev-parse", "--verify", &format!("{review_ref}^{{commit}}")]).as_deref() != Some(resolved.as_str()) {
        return Err(crate::api::error("review_revision_mismatch", "review_ref changed during export"));
    }
    exported.sort_by(|a, b| a["entry"]["file"].as_str().cmp(&b["entry"]["file"].as_str()));
    let snapshot =
        format!("kx1-{}", crate::util::short_hash(&json!({"revision":resolved,"config":cfg.store,"entries":exported}).to_string()));
    Ok(json!({"revision":resolved,"review_ref":review_ref,"entries":exported,"snapshot":snapshot,"config_source":config_source}))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};

    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let path =
                std::env::temp_dir().join(format!("kontext-api-read-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
            std::fs::create_dir_all(&path).unwrap();
            let f = Self(path);
            f.git(&["init", "-q", "-b", "main"]);
            f.git(&["config", "user.email", "test@example.invalid"]);
            f.git(&["config", "user.name", "Test"]);
            f
        }
        fn git(&self, args: &[&str]) -> String {
            let out = std::process::Command::new("git").arg("-C").arg(&self.0).args(args).output().unwrap();
            assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
            String::from_utf8(out.stdout).unwrap().trim().into()
        }
        fn write(&self, path: &str, text: &str) {
            let p = self.0.join(path);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, text).unwrap();
        }
        fn app(&self) -> App {
            let mut app = App::open(&self.0).unwrap();
            app.loaded.cfg.adapters.clear();
            app.loaded.cfg.init.llm = None;
            app
        }
        fn commit(&self) -> String {
            self.git(&["add", "."]);
            self.git(&["commit", "-qm", "fixture"]);
            self.git(&["rev-parse", "HEAD"])
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn custom_store_adr_pagination_active_and_content_snapshot() {
        let f = Fixture::new();
        f.write(".kontext.toml", "[store]\ndir = 'knowledge'\n[store.kinds.decision]\npath = 'docs/adr'\nstyle = 'fields'\n[store.kinds.policy]\ndir = 'policies'\n");
        f.write(
            "docs/adr/0001-rule.md",
            "# 0001 — Rule\n\nStatus: accepted\nDate: 2026-01-01\nPaths: src/**\n\n## Decision\nDo the thing.\n",
        );
        for (id, status) in [("a", "accepted"), ("b", "deprecated"), ("c", "rejected"), ("d", "superseded")] {
            f.write(
                &format!("knowledge/policies/{id}.md"),
                &format!(
                    "---\nid: {id}\nkind: policy\ntitle: {id}\nstatus: {status}\ntags: [security]\ncustom: [one, two]\n---\nA policy.\n"
                ),
            );
        }
        let app = f.app();
        let page = call(&app, "entries.list", &json!({"limit":1})).unwrap();
        assert_eq!(page["total"], 2);
        assert_eq!(page["next_offset"], 1);
        let next = call(&app, "entries.list", &json!({"limit":1,"offset":1})).unwrap();
        assert_ne!(page["entries"][0]["id"], next["entries"][0]["id"]);
        assert!(next["next_offset"].is_null());
        let all = call(&app, "entries.list", &json!({"active_only":false})).unwrap();
        assert_eq!(all["total"], 5);
        let filtered = call(&app, "entries.list", &json!({"kinds":["policy"],"tags":["security"]})).unwrap();
        assert_eq!(filtered["total"], 1);
        assert_eq!(filtered["entries"][0]["extra"]["custom"], json!(["one", "two"]));
        let adr = call(&app, "entries.get", &json!({"id":"0001"})).unwrap();
        assert_eq!(adr["entry"]["title"], "Rule");
        assert_eq!(adr["entry"]["style"], "fields");
        f.write("knowledge/policies/a.md", "---\nid: a\nkind: policy\ntitle: a\n---\nChanged policy.\n");
        let changed = call(&app, "entries.list", &json!({"limit":1})).unwrap();
        assert_ne!(page["snapshot"], changed["snapshot"]);
        assert!(call(&app, "entries.list", &json!({"limit":0})).is_err());
        assert!(call(&app, "entries.list", &json!({"active_only":"false"})).is_err());
    }

    #[test]
    fn governing_uses_canonical_globs_and_specificity_filter() {
        let f = Fixture::new();
        f.write(
            ".ai/conventions/rule.md",
            "---\nid: rule\nkind: convention\ntitle: scoped\npaths: [apps/*/src/**, src/**, '**/*.{rs,ts}']\n---\nApply here.\n",
        );
        let app = f.app();
        let out = call(&app, "entries.governing", &json!({"paths":["apps/web/src/a.ts"]})).unwrap();
        assert_eq!(out["matches"].as_array().unwrap().len(), 2);
        assert_eq!(out["matches"][0]["pattern"], "apps/*/src/**");
        assert_eq!(out["matches"][0]["specificity"], 1);
        let narrow = call(&app, "entries.governing", &json!({"paths":["apps/web/src/a.ts"],"min_specificity":2})).unwrap();
        assert!(narrow["matches"].as_array().unwrap().is_empty());
        let excluded = call(&app, "entries.governing", &json!({"paths":["apps/web/src/a.ts"],"exclude_ids":["rule"]})).unwrap();
        assert!(excluded["matches"].as_array().unwrap().is_empty());
        assert!(call(&app, "entries.governing", &json!({"paths":["../secret"]})).is_err());
    }

    #[test]
    fn export_requires_review_tip_and_ignores_dirty_files_and_config() {
        let f = Fixture::new();
        f.write(".kontext.toml", "[store]\ndir = 'knowledge'\n[store.kinds.decision]\npath = 'docs/adr'\nstyle = 'fields'\n");
        let markdown = "# 0001 — Reviewed\n\nStatus: accepted\n\n## Decision\nReviewed bytes.\n";
        f.write("docs/adr/0001-reviewed.md", markdown);
        f.write("docs/adr/0002-proposal.md", "# 0002 — Proposal\n\nStatus: proposed\n\nUnreviewed idea.\n");
        f.write("docs/adr/0003-deprecated.md", "# 0003 — Deprecated\n\nStatus: deprecated\n\nOld.\n");
        let revision = f.commit();
        f.write("docs/adr/0001-reviewed.md", "# Dirty replacement\n\nStatus: accepted\n\nDirty bytes.\n");
        f.write(".kontext.toml", "[store.kinds.decision]\npath = 'wrong'\n");
        let app = f.app();
        let input = json!({"revision":revision,"review_ref":"refs/heads/main"});
        let out = call(&app, "entries.export", &input).unwrap();
        assert_eq!(out["entries"].as_array().unwrap().len(), 1);
        assert_eq!(out["entries"][0]["markdown"], markdown);
        assert_eq!(out["entries"][0]["entry"]["title"], "Reviewed");
        let hash = f.git(&["rev-parse", &format!("{revision}:docs/adr/0001-reviewed.md")]);
        assert_eq!(out["entries"][0]["blob_hash"], hash);
        assert_eq!(out["entries"][0]["entry"]["publication"]["state"], "reviewed_ref");
        assert_eq!(out["config_source"]["kind"], "committed");
        assert!(std::fs::read_to_string(f.0.join("docs/adr/0001-reviewed.md")).unwrap().contains("Dirty bytes"));
        assert!(f.git(&["diff", "--cached", "--name-only"]).is_empty());
        assert!(call(&app, "entries.export", &json!({"revision":revision,"review_ref":"HEAD"})).is_err());
        assert!(call(&app, "entries.export", &json!({"revision":&revision[..8],"review_ref":"refs/heads/main"})).is_err());
        f.write("file", "new commit");
        f.commit();
        assert!(call(&app, "entries.export", &input).is_err());
        // No checkout or index mutation occurred while reading the earlier revision.
        assert!(Path::new(&app.repo.root.join("docs/adr/0001-reviewed.md")).exists());
    }

    #[test]
    fn publication_tracks_git_independently_of_content_status() {
        let f = Fixture::new();
        let path = ".ai/decisions/rule.md";
        let original = "---\nid: rule\nkind: decision\nstatus: accepted\n---\nOriginal.\n";
        f.write(path, original);
        let app = f.app();
        let get = || call(&app, "entries.get", &json!({"id":"rule"})).unwrap();
        assert_eq!(get()["entry"]["publication"]["state"], "draft");
        f.git(&["add", path]);
        assert_eq!(get()["entry"]["publication"]["state"], "staged");
        f.git(&["commit", "-qm", "rule"]);
        assert_eq!(get()["entry"]["publication"]["state"], "committed");
        f.write(path, &format!("{original}Changed.\n"));
        assert_eq!(get()["entry"]["publication"]["state"], "modified");
        assert_eq!(get()["view"], "working_tree");
        assert_eq!(get()["entry"]["status"], "accepted");
    }

    #[test]
    fn search_resolves_ids_and_folded_all_word_metadata_before_ranked_body_hits() {
        let f = Fixture::new();
        f.write(".ai/decisions/2026-pricing-rule.md","---\nid: 2026-pricing-rule\nkind: decision\ntitle: Zaokrúhľovanie ceny\ntags: [Platby]\npaths: [src/pricing/**]\nstatus: accepted\n---\nPreserve accurate amounts.\n");
        f.write(
            ".ai/decisions/body.md",
            "---\nid: body\nkind: decision\ntitle: Other rule\nstatus: accepted\n---\nPricing details are here.\n",
        );
        let app = f.app();
        let prefix = call(&app, "entries.search", &json!({"query":"2026-pric"})).unwrap();
        assert_eq!(prefix["total"], 1);
        assert_eq!(prefix["entries"][0]["id"], "2026-pricing-rule");
        for query in ["ZAOKRUHLOVANIE platby", "zaokruhl\u{030c}ovanie platby", "zaokruhlovanie src/pricing"] {
            let out = call(&app, "entries.search", &json!({"query":query})).unwrap();
            assert_eq!(out["entries"][0]["id"], "2026-pricing-rule", "{query}");
        }
        let fallback = call(&app, "entries.search", &json!({"query":"details"})).unwrap();
        assert_eq!(fallback["entries"][0]["id"], "body");
    }

    #[test]
    fn search_uses_canonical_index_and_filters_inactive_entries() {
        let f = Fixture::new();
        for (id, status) in [("active", "accepted"), ("old", "deprecated")] {
            f.write(
                &format!(".ai/decisions/{id}.md"),
                &format!("---\nid: {id}\nkind: decision\ntitle: Zebra routing\nstatus: {status}\n---\nZebra routing details.\n"),
            );
        }
        let app = f.app();
        let active = call(&app, "entries.search", &json!({"query":"zebra"})).unwrap();
        assert_eq!(active["total"], 1);
        assert_eq!(active["entries"][0]["id"], "active");
        let all = call(&app, "entries.search", &json!({"query":"zebra","active_only":false})).unwrap();
        assert_eq!(all["total"], 2);
    }

    #[test]
    fn status_requires_selection_for_multiple_llm_adapters() {
        let f = Fixture::new();
        let mut app = f.app();
        for name in ["first", "second"] {
            let mut adapter = crate::config::AdapterCfg { driver: "command".into(), ..Default::default() };
            adapter.ops.insert("llm".into(), crate::config::OpCfg { command: Some(vec!["echo".into()]), ..Default::default() });
            app.loaded.cfg.adapters.insert(name.into(), adapter);
        }
        let out = call(&app, "status", &json!({})).unwrap();
        assert_eq!(out["llm"]["available"], false);
        assert_eq!(out["llm"]["requires_selection"], true);
        assert!(out["llm"]["selected"].is_null());
    }

    #[cfg(unix)]
    #[test]
    fn export_rejects_symlink_configuration() {
        let f = Fixture::new();
        f.write(".ai/learnings/good.md", "---\nid: good\nkind: learning\ntitle: Good\n---\nA finding.\n");
        std::os::unix::fs::symlink("[store]\ndir='notes'\n", f.0.join(".kontext.toml")).unwrap();
        let revision = f.commit();
        let app = f.app();
        let error = call(&app, "entries.export", &json!({"revision":revision,"review_ref":"refs/heads/main"})).unwrap_err();
        assert_eq!(crate::api::failure(error)["error"]["code"], "invalid_config");
    }

    #[test]
    fn export_uses_same_configured_kind_and_content_policy_as_embedding() {
        let f = Fixture::new();
        for (id, kind, body) in
            [("good", "learning", "A useful finding."), ("unknown", "mystery", "Unknown configured kind."), ("empty", "learning", "")]
        {
            f.write(&format!(".ai/learnings/{id}.md"), &format!("---\nid: {id}\nkind: {kind}\ntitle: {id}\n---\n{body}\n"));
        }
        let revision = f.commit();
        let app = f.app();
        let out = call(&app, "entries.export", &json!({"revision":revision,"review_ref":"refs/heads/main"})).unwrap();
        assert_eq!(out["entries"].as_array().unwrap().len(), 1);
        assert_eq!(out["entries"][0]["entry"]["id"], "good");
    }

    #[test]
    fn export_rejects_oversized_committed_blobs() {
        let f = Fixture::new();
        f.write(".ai/learnings/large.md", &"a".repeat(MAX_BLOB_BYTES + 1));
        let revision = f.commit();
        let app = f.app();
        let err = call(&app, "entries.export", &json!({"revision":revision,"review_ref":"refs/heads/main"})).unwrap_err();
        assert!(err.to_string().contains("size limit"));
    }

    #[test]
    fn export_excludes_private_unknown_status_and_symlinks() {
        let f = Fixture::new();
        f.write(".ai/conventions/team.md", "---\nid: team\nkind: convention\n---\nTeam rule.\n");
        f.write(".ai/conventions/private.md", "---\nid: private\nkind: convention\nvisibility: private\n---\nPrivate rule.\n");
        f.write(".ai/conventions/unknown.md", "---\nid: unknown\nkind: convention\nstatus: awaiting-review\n---\nUnknown status.\n");
        #[cfg(unix)]
        std::os::unix::fs::symlink("team.md", f.0.join(".ai/conventions/link.md")).unwrap();
        let revision = f.commit();
        let app = f.app();
        let out = call(&app, "entries.export", &json!({"revision":revision,"review_ref":"refs/heads/main"})).unwrap();
        assert_eq!(out["entries"].as_array().unwrap().len(), 1);
        assert_eq!(out["entries"][0]["entry"]["id"], "team");
        assert_eq!(out["config_source"]["kind"], "local_configuration");
    }

    #[test]
    fn status_does_not_offer_an_unconfigured_llm_binary() {
        let f = Fixture::new();
        let app = f.app();
        let status = call(&app, "status", &json!({})).unwrap();
        assert_eq!(status["llm"]["available"], false);
        assert!(status["llm"]["adapters"].as_array().unwrap().is_empty());
    }
}
