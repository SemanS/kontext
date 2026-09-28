//! Adapters: external systems plugged in purely through configuration.
//!
//! The core only knows *capabilities* (ops): `search`, `read`, `store`, `history`, `code`, `brief`,
//! `llm`, `health`; *events* (`capture`, `promote`, `sync`) that adapters can subscribe to; and
//! *tools* that adapters may contribute to kontext's own MCP server (federated or declared).
//! How an op is executed is up to a generic driver (`mcp`, `http`, `command`).
pub mod drivers;
pub mod mcp_client;

use crate::config::{AdapterCfg, Config, OnCfg, OpCfg};
use crate::jpath;
use crate::model::Hit;
use crate::repo::Repo;
use crate::template::{render_str, to_text};
use crate::util;
use anyhow::{Result, anyhow, bail};
use drivers::{CommandDriver, Driver, HttpDriver, McpDriver};
use mcp_client::{ToolInfo, result_text};
use regex::Regex;
use serde_json::{Map, Value, json};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

pub const OPS: &[&str] = &["search", "read", "store", "history", "code", "brief", "llm", "health"];

pub struct Adapter {
    pub name: String,
    pub cfg: AdapterCfg,
    driver: Box<dyn Driver>,
    base: Value,
    broken_until: Mutex<Option<(Instant, String)>>,
}

impl Adapter {
    pub fn describe(&self) -> String {
        let ops: Vec<&str> = OPS.iter().copied().filter(|o| self.has_op(o)).collect();
        let events: Vec<String> = self.cfg.on.iter().map(|o| format!("{}→{}", o.event, o.op)).collect();
        let mut parts = vec![format!("driver={}", self.cfg.driver)];
        if !ops.is_empty() {
            parts.push(format!("ops={}", ops.join(",")));
        }
        if !events.is_empty() {
            parts.push(format!("on={}", events.join(",")));
        }
        if self.exposes_any() {
            parts.push("exposes tools".into());
        }
        if !self.cfg.tools.is_empty() {
            parts.push(format!("tools={}", self.cfg.tools.iter().map(|t| t.name.as_str()).collect::<Vec<_>>().join(",")));
        }
        parts.join("  ")
    }

    pub fn has_op(&self, op: &str) -> bool {
        self.cfg.ops.contains_key(op)
    }

    pub fn weight(&self) -> f32 {
        self.cfg.weight.unwrap_or(0.9)
    }

    fn timeout(&self, op: Option<&OpCfg>) -> Duration {
        Duration::from_millis(op.and_then(|o| o.timeout_ms).or(self.cfg.timeout_ms).unwrap_or(10_000))
    }

    fn ctx(&self, extra: Value) -> Value {
        let mut ctx = self.base.clone();
        if let (Some(obj), Value::Object(ex)) = (ctx.as_object_mut(), extra) {
            for (k, v) in ex {
                obj.insert(k, v);
            }
        }
        ctx
    }

    fn check_breaker(&self) -> Result<()> {
        let mut g = self.broken_until.lock().unwrap();
        if let Some((until, why)) = g.as_ref() {
            if Instant::now() < *until {
                bail!("adapter '{}' is cooling down after an error: {why}", self.name);
            }
            *g = None;
        }
        Ok(())
    }

    fn trip(&self, err: &anyhow::Error) {
        *self.broken_until.lock().unwrap() = Some((Instant::now() + Duration::from_secs(30), util::truncate_chars(&err.to_string(), 160)));
    }

    /// Execute an op and return the unwrapped payload (MCP results become JSON or text).
    fn run_op_cfg(&self, op: &OpCfg, role: &str, extra: Value) -> Result<Value> {
        self.check_breaker()?;
        let ctx = self.ctx(extra);
        match self.driver.call(op, role, &ctx, self.timeout(Some(op))) {
            Ok(v) => Ok(unwrap_payload(v)),
            Err(e) => {
                self.trip(&e);
                Err(e)
            }
        }
    }

    pub fn search(&self, query: &str, limit: usize) -> Result<Vec<Hit>> {
        self.hits_op("search", json!({"query": query, "limit": limit}))
    }

    pub fn code(&self, query: &str, limit: usize) -> Result<Vec<Hit>> {
        self.hits_op("code", json!({"query": query, "limit": limit}))
    }

    pub fn history(&self, target: &str, limit: usize) -> Result<Vec<Hit>> {
        self.hits_op("history", json!({"target": target, "query": target, "limit": limit}))
    }

    fn hits_op(&self, op_name: &str, extra: Value) -> Result<Vec<Hit>> {
        let op = self.cfg.ops.get(op_name).ok_or_else(|| anyhow!("no '{op_name}' op"))?;
        let payload = self.run_op_cfg(op, op_name, extra)?;
        Ok(map_hits(&payload, op, &self.name))
    }

    pub fn owns(&self, uri: &str) -> bool {
        self.cfg.ops.get("read").is_some_and(|op| op.owns.iter().any(|p| uri.starts_with(p.as_str())))
    }

    pub fn read(&self, uri: &str, level: u8) -> Result<String> {
        let op = self.cfg.ops.get("read").ok_or_else(|| anyhow!("no read op"))?;
        let payload = self.run_op_cfg(op, "read", json!({"uri": uri, "level": level}))?;
        Ok(payload_text(&payload, op))
    }

    pub fn brief(&self, focus: &str) -> Result<String> {
        let op = self.cfg.ops.get("brief").ok_or_else(|| anyhow!("no brief op"))?;
        let payload = self.run_op_cfg(op, "brief", json!({"query": focus, "focus": focus}))?;
        Ok(payload_text(&payload, op))
    }

    pub fn llm(&self, prompt: &str) -> Result<String> {
        let op = self.cfg.ops.get("llm").ok_or_else(|| anyhow!("adapter '{}' has no llm op", self.name))?;
        let payload = self.run_op_cfg(op, "llm", json!({"prompt": prompt}))?;
        Ok(payload_text(&payload, op))
    }

    pub fn health(&self) -> Result<String> {
        if let Some(op) = self.cfg.ops.get("health") {
            let payload = self.run_op_cfg(op, "health", json!({}))?;
            return Ok(util::truncate_chars(&payload_text(&payload, op), 200));
        }
        if self.cfg.driver == "mcp" {
            let tools = self.driver.tools(self.timeout(None))?;
            return Ok(format!("MCP ok, {} tools", tools.len()));
        }
        Ok("no health op configured".into())
    }

    /// Run an op for an event payload (store receipts are returned as text).
    pub fn deliver(&self, op_name: &str, payload: &Value) -> Result<String> {
        let op = self.cfg.ops.get(op_name).ok_or_else(|| anyhow!("adapter '{}' has no '{op_name}' op", self.name))?;
        let v = self.run_op_cfg(op, op_name, payload.clone())?;
        Ok(util::truncate_chars(&payload_text(&v, op), 200))
    }

    pub fn subscriptions(&self, event: &str, visibility: Option<&str>, kind: Option<&str>) -> Vec<&OnCfg> {
        self.cfg
            .on
            .iter()
            .filter(|o| o.event == event)
            .filter(|o| o.visibility.as_deref().is_none_or(|v| Some(v) == visibility))
            .filter(|o| o.kinds.is_empty() || kind.is_some_and(|k| o.kinds.iter().any(|x| x == k)))
            .collect()
    }

    // ------------------------------------------------------------------ tools through kontext

    fn exposes_any(&self) -> bool {
        match &self.cfg.expose {
            Some(Value::String(s)) => s == "all",
            Some(Value::Array(a)) => !a.is_empty(),
            Some(Value::Bool(b)) => *b,
            _ => false,
        }
    }

    fn exposes(&self, tool: &str) -> bool {
        match &self.cfg.expose {
            Some(Value::String(s)) => s == "all",
            Some(Value::Bool(b)) => *b,
            Some(Value::Array(a)) => a.iter().any(|x| x.as_str() == Some(tool)),
            _ => false,
        }
    }

    pub fn exposed_name(&self, tool: &str) -> String {
        let prefix = self.cfg.prefix.clone().unwrap_or_else(|| format!("{}_", util::slugify(&self.name, 30).replace('-', "_")));
        if prefix.is_empty() || tool.starts_with(&prefix) || tool.starts_with(&format!("{}_", self.name)) {
            tool.to_string()
        } else {
            format!("{prefix}{tool}")
        }
    }

    /// Tools re-exposed from an MCP adapter plus tools declared in config.
    pub fn tool_list(&self) -> Vec<(String, ToolInfo)> {
        let mut out = Vec::new();
        if self.exposes_any() {
            match self.driver.tools(self.timeout(None)) {
                Ok(tools) => {
                    for t in tools.into_iter().filter(|t| self.exposes(&t.name)) {
                        let public = self.exposed_name(&t.name);
                        let mut info = t.clone();
                        info.description = format!("[{}] {}", self.name, t.description);
                        info.name = public.clone();
                        out.push((t.name, info));
                    }
                }
                Err(e) => eprintln!("kontext: adapter '{}' tools unavailable: {e:#}", self.name),
            }
        }
        for t in &self.cfg.tools {
            out.push((
                format!("declared:{}", t.name),
                ToolInfo {
                    name: t.name.clone(),
                    description: format!("[{}] {}", self.name, t.description),
                    input_schema: simple_schema(t.params.as_ref()),
                },
            ));
        }
        out
    }

    /// Call a tool listed by `tool_list` (by its original name). Returns an MCP tool result.
    pub fn call_listed(&self, original: &str, args: &Value) -> Result<Value> {
        if let Some(decl) = original.strip_prefix("declared:") {
            let t = self.cfg.tools.iter().find(|t| t.name == decl).ok_or_else(|| anyhow!("unknown tool {decl}"))?;
            let (op, role): (OpCfg, String) = match &t.op {
                Some(Value::String(name)) => {
                    (self.cfg.ops.get(name).cloned().ok_or_else(|| anyhow!("tool {decl}: no op '{name}'"))?, name.clone())
                }
                Some(v @ Value::Object(_)) => (serde_json::from_value(v.clone())?, "tool".into()),
                _ => bail!("tool {decl}: missing `op`"),
            };
            let mut extra = Map::new();
            extra.insert("args".into(), args.clone());
            if let Value::Object(a) = args {
                for (k, v) in a {
                    extra.insert(k.clone(), v.clone());
                }
            }
            let payload = self.run_op_cfg(&op, &role, Value::Object(extra))?;
            let text = payload_text(&payload, &op);
            return Ok(json!({"content": [{"type": "text", "text": text}]}));
        }
        self.check_breaker()?;
        self.driver.call_tool(original, args, Duration::from_millis(self.cfg.timeout_ms.unwrap_or(60_000).max(30_000)))
    }

    pub fn raw_tools(&self) -> Result<Vec<ToolInfo>> {
        self.driver.tools(self.timeout(None).max(Duration::from_secs(30)))
    }
}

fn simple_schema(params: Option<&Value>) -> Value {
    let Some(p) = params else { return json!({"type": "object", "properties": {}}) };
    if p.get("type").is_some() && p.get("properties").is_some() {
        return p.clone();
    }
    let mut props = Map::new();
    let mut required = Vec::new();
    if let Some(obj) = p.as_object() {
        for (k, v) in obj {
            let spec = v.as_str().unwrap_or("string");
            let (ty, req) = match spec.strip_suffix('!') {
                Some(t) => (t, true),
                None => (spec, false),
            };
            props.insert(k.clone(), json!({"type": ty}));
            if req {
                required.push(Value::String(k.clone()));
            }
        }
    }
    json!({"type": "object", "properties": props, "required": required})
}

/// MCP tool results → structured content, parsed JSON text, or plain text.
pub fn unwrap_payload(v: Value) -> Value {
    let is_mcp = v.get("content").is_some_and(|c| c.is_array())
        && v.as_object().is_some_and(|o| o.keys().all(|k| matches!(k.as_str(), "content" | "isError" | "structuredContent" | "_meta")));
    if !is_mcp {
        return v;
    }
    if let Some(s) = v.get("structuredContent").filter(|s| !s.is_null()) {
        return s.clone();
    }
    let text = result_text(&v);
    let parsed = drivers::parse_output(&text, None);
    match parsed {
        Value::Object(_) | Value::Array(_) => parsed,
        _ => Value::String(text),
    }
}

pub fn payload_text(v: &Value, op: &OpCfg) -> String {
    if let Some(p) = &op.text {
        return jpath::select(v, p).into_iter().map(jpath::as_text).collect::<Vec<_>>().join("\n");
    }
    match v {
        Value::String(s) => s.clone(),
        Value::Object(o) => {
            for k in ["text", "content", "result", "response", "output", "markdown", "message", "answer"] {
                if let Some(Value::String(s)) = o.get(k) {
                    return s.clone();
                }
            }
            serde_json::to_string_pretty(v).unwrap_or_default()
        }
        other => serde_json::to_string_pretty(other).unwrap_or_default(),
    }
}

fn map_field(item: &Value, spec: Option<&String>, defaults: &[&str]) -> Option<String> {
    let text_of = |item: &Value| -> String {
        match item {
            Value::String(s) => s.clone(),
            Value::Object(o) => o.get("raw").or_else(|| o.get("text")).map(to_text).unwrap_or_else(|| item.to_string()),
            other => other.to_string(),
        }
    };
    if let Some(spec) = spec {
        if let Some(lit) = spec.strip_prefix('=') {
            return Some(lit.to_string());
        }
        // a template rendered against the item, e.g. `tpl:teamdocs://{{id}}`
        if let Some(tpl) = spec.strip_prefix("tpl:") {
            return Some(render_str(tpl, item)).filter(|s| !s.is_empty());
        }
        if let Some(re) = spec.strip_prefix("re:") {
            let re = Regex::new(re).ok()?;
            let t = text_of(item);
            let c = re.captures(&t)?;
            return Some(c.get(1).or_else(|| c.get(0))?.as_str().to_string());
        }
        return jpath::first(item, spec).map(jpath::as_text).filter(|s| !s.is_empty());
    }
    if let Value::Object(o) = item {
        for d in defaults {
            if let Some(v) = o.get(*d).filter(|v| !v.is_null()) {
                let s = jpath::as_text(v);
                if !s.is_empty() {
                    return Some(s);
                }
            }
        }
    }
    None
}

/// Generic result mapping: find the items, then title/snippet/uri/score/kind/date per item.
pub fn map_hits(payload: &Value, op: &OpCfg, source: &str) -> Vec<Hit> {
    let items: Vec<Value> = if let Some(path) = &op.items {
        jpath::select(payload, path).into_iter().cloned().collect()
    } else if let (Some(prefix), Value::String(text)) = (&op.split, payload) {
        split_text(text, prefix)
    } else {
        match payload {
            Value::Array(a) => a.clone(),
            Value::Object(o) => ["results", "items", "hits", "data", "matches", "entries", "memories", "documents"]
                .iter()
                .find_map(|k| o.get(*k).and_then(|v| v.as_array()).cloned())
                .unwrap_or_else(|| vec![payload.clone()]),
            Value::String(s) if s.trim().is_empty() => Vec::new(),
            Value::String(s) => {
                let (first, rest) = s.trim().split_once('\n').unwrap_or((s.trim(), ""));
                vec![json!({"title": first.trim_start_matches('#').trim(), "text": util::truncate_chars(rest.trim(), 1200), "raw": s})]
            }
            Value::Null => Vec::new(),
            other => vec![json!({"text": other.to_string()})],
        }
    };
    let items: Vec<Value> = items
        .into_iter()
        .filter(|item| op.filter.iter().all(|(path, want)| jpath::first(item, path).map(jpath::as_text).as_deref() == Some(want.as_str())))
        .collect();
    let n = items.len().max(1) as f32;
    items
        .iter()
        .enumerate()
        .filter_map(|(i, item)| {
            let title = map_field(item, op.map.get("title"), &["title", "name", "subject", "heading", "uri", "path", "id"]);
            let snippet = map_field(
                item,
                op.map.get("snippet"),
                &["snippet", "abstract", "summary", "content", "text", "description", "body", "overview"],
            );
            if title.is_none() && snippet.is_none() {
                return None;
            }
            let uri = map_field(item, op.map.get("uri"), &["uri", "url", "path", "file", "id"]).unwrap_or_default();
            let score = map_field(item, op.map.get("score"), &["score", "relevance", "similarity", "rank"])
                .and_then(|s| s.parse::<f32>().ok())
                .unwrap_or(1.0 - i as f32 / (n * 2.0));
            let kind = map_field(item, op.map.get("kind"), &["kind", "type", "context_type", "category"]);
            let date = map_field(item, op.map.get("date"), &["date", "created_at", "updated_at", "time", "timestamp"]);
            let snippet = snippet.unwrap_or_default();
            let snippet = strip_duplicate_title(&snippet, title.as_deref());
            Some(Hit {
                source: source.to_string(),
                uri,
                title: title.unwrap_or_else(|| util::truncate_chars(&util::one_line(&snippet), 80)),
                snippet: util::truncate_chars(&util::one_line(&snippet), 400),
                kind,
                score,
                date,
                status: None,
            })
        })
        .collect()
}

fn strip_duplicate_title(snippet: &str, title: Option<&str>) -> String {
    let s = snippet.trim().trim_start_matches("# Summary").trim();
    match title {
        Some(t) if s.starts_with(&format!("# {t}")) => s[t.len() + 2..].trim().to_string(),
        _ => s.to_string(),
    }
}

fn split_text(text: &str, prefix: &str) -> Vec<Value> {
    let mut items = Vec::new();
    let mut cur: Option<(String, String)> = None;
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix(prefix) {
            if let Some((t, b)) = cur.take() {
                items.push(json!({"title": t, "text": b.trim(), "raw": format!("{t}\n{b}")}));
            }
            cur = Some((rest.trim().to_string(), String::new()));
        } else if let Some((_, b)) = cur.as_mut() {
            b.push_str(line);
            b.push('\n');
        }
    }
    if let Some((t, b)) = cur {
        items.push(json!({"title": t, "text": b.trim(), "raw": format!("{t}\n{b}")}));
    }
    items
}

// ---------------------------------------------------------------------------------------- registry

pub struct Registry {
    pub adapters: Vec<Arc<Adapter>>,
    pub skipped: Vec<(String, String)>,
    pub errors: Vec<String>,
}

fn path_exists_glob(p: &str) -> bool {
    let path = std::path::Path::new(p);
    let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    if !name.contains('*') && !name.contains('?') {
        return path.exists();
    }
    let Some(dir) = path.parent() else { return false };
    let Some(g) = crate::glob::Glob::new(&format!("/{name}")) else { return false };
    std::fs::read_dir(dir).map(|rd| rd.flatten().any(|e| g.matches(&e.file_name().to_string_lossy()))).unwrap_or(false)
}

fn when_fails(a: &AdapterCfg, repo: &Repo, ctx: &Value) -> Option<String> {
    let w = &a.when;
    if let Some(pat) = &w.exists {
        let p = render_str(pat, ctx);
        if !path_exists_glob(&p) {
            return Some(format!("{p} does not exist"));
        }
    }
    if let Some(c) = &w.command
        && util::which(c).is_none()
    {
        return Some(format!("`{c}` not on PATH"));
    }
    if let Some(f) = &w.file
        && !repo.root.join(f).exists()
    {
        return Some(format!("{f} not present"));
    }
    if let Some(r) = &w.repo {
        match Regex::new(r) {
            Ok(re) if re.is_match(&repo.id) => {}
            Ok(_) => return Some(format!("repo does not match /{r}/")),
            Err(e) => return Some(format!("bad `when.repo` regex: {e}")),
        }
    }
    if let Some(e) = &w.env
        && std::env::var_os(e).is_none()
    {
        return Some(format!("${e} not set"));
    }
    None
}

impl Registry {
    pub fn empty() -> Registry {
        Registry { adapters: Vec::new(), skipped: Vec::new(), errors: Vec::new() }
    }

    pub fn build(repo: &Repo, cfg: &Config) -> Registry {
        let mut reg = Registry::empty();
        let now = json!({"date": util::today(), "iso": util::now_iso()});
        let repo_ctx = repo.template_ctx();
        let project = json!({"name": cfg.project_name(repo)});
        let global_ctx = json!({"repo": repo_ctx, "now": now, "project": project});
        let global_vars: Map<String, Value> =
            cfg.vars.iter().map(|(k, v)| (k.clone(), Value::String(render_str(v, &global_ctx)))).collect();
        for (name, a) in &cfg.adapters {
            if a.enabled == Some(false) {
                reg.skipped.push((name.clone(), "disabled".into()));
                continue;
            }
            let mut vars = global_vars.clone();
            let var_ctx = json!({"repo": repo.template_ctx(), "now": now, "project": project, "vars": Value::Object(global_vars.clone())});
            for (k, v) in &a.vars {
                vars.insert(k.clone(), Value::String(render_str(v, &var_ctx)));
            }
            let base = json!({"repo": repo.template_ctx(), "now": now, "project": project, "vars": Value::Object(vars), "adapter": {"name": name}});
            if let Some(why) = when_fails(a, repo, &base) {
                reg.skipped.push((name.clone(), why));
                continue;
            }
            let log = repo.state_dir().join("logs").join(format!("{}.log", util::slugify(name, 40)));
            let driver: Result<Box<dyn Driver>> = match a.driver.as_str() {
                "mcp" => McpDriver::new(name, a, &base, repo.root.clone(), log).map(|d| Box::new(d) as Box<dyn Driver>),
                "http" => Ok(Box::new(HttpDriver::new(name, a, &base))),
                "command" => Ok(Box::new(CommandDriver::new(name, a, &base, repo.root.clone()))),
                other => Err(anyhow!("adapter '{name}': unknown driver '{other}' (use mcp, http or command)")),
            };
            match driver {
                Ok(driver) => reg.adapters.push(Arc::new(Adapter {
                    name: name.clone(),
                    cfg: a.clone(),
                    driver,
                    base,
                    broken_until: Mutex::new(None),
                })),
                Err(e) => reg.errors.push(format!("{e:#}")),
            }
        }
        reg
    }

    pub fn get(&self, name: &str) -> Option<Arc<Adapter>> {
        self.adapters.iter().find(|a| a.name == name).cloned()
    }

    pub fn with_op(&self, op: &str) -> Vec<Arc<Adapter>> {
        self.adapters.iter().filter(|a| a.has_op(op)).cloned().collect()
    }

    /// Run `f` on every adapter with `op` in parallel; results that miss the deadline are dropped.
    pub fn fanout<T, F>(&self, op: &str, only: &[String], deadline: Duration, f: F) -> Vec<(String, Result<T>)>
    where
        T: Send + 'static,
        F: Fn(&Adapter) -> Result<T> + Send + Sync + 'static,
    {
        let targets: Vec<Arc<Adapter>> =
            self.with_op(op).into_iter().filter(|a| only.is_empty() || only.iter().any(|o| o == &a.name)).collect();
        if targets.is_empty() {
            return Vec::new();
        }
        let f = Arc::new(f);
        let (tx, rx) = mpsc::channel();
        for a in targets.iter().cloned() {
            let tx = tx.clone();
            let f = f.clone();
            std::thread::spawn(move || {
                let r = f(&a);
                let _ = tx.send((a.name.clone(), r));
            });
        }
        drop(tx);
        let end = Instant::now() + deadline;
        let mut out = Vec::new();
        while out.len() < targets.len() {
            let left = end.saturating_duration_since(Instant::now());
            match rx.recv_timeout(left) {
                Ok(r) => out.push(r),
                Err(_) => break,
            }
        }
        for a in &targets {
            if !out.iter().any(|(n, _)| n == &a.name) {
                out.push((a.name.clone(), Err(anyhow!("timed out after {}s", deadline.as_secs()))));
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_openviking_shape() {
        let payload = json!({"status": "ok", "result": {"memories": [{"uri": "viking://m/1", "abstract": "# Summary\nRotated the signing key", "score": 0.61, "context_type": "memory"}], "resources": [], "total": 1}});
        let op = OpCfg { items: Some("result.*[*]".into()), ..Default::default() };
        let hits = map_hits(&payload, &op, "ov");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].uri, "viking://m/1");
        assert_eq!(hits[0].snippet, "Rotated the signing key");
        assert_eq!(hits[0].kind.as_deref(), Some("memory"));
    }

    #[test]
    fn maps_split_text() {
        let text = "## Search Results (2 found)\n\n### Parser (struct)\ncrates/parser/src/lib.rs:8\n\n### run (method)\ncrates/parser/src/lib.rs:36\n`(&self)`\n";
        let mut op = OpCfg { split: Some("### ".into()), ..Default::default() };
        op.map.insert("uri".into(), r"re:([\w./@-]+\.\w+:\d+)".into());
        let hits = map_hits(&Value::String(text.into()), &op, "cg");
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].title, "Parser (struct)");
        assert_eq!(hits[0].uri, "crates/parser/src/lib.rs:8");
        assert_eq!(hits[1].uri, "crates/parser/src/lib.rs:36");
    }

    #[test]
    fn filters_items() {
        let payload = json!([{"type": "begin", "data": {"path": {"text": "a.md"}}}, {"type": "match", "data": {"path": {"text": "a.md"}, "lines": {"text": "hit"}}}]);
        let mut op = OpCfg::default();
        op.filter.insert("type".into(), "match".into());
        op.map.insert("title".into(), "data.path.text".into());
        op.map.insert("snippet".into(), "data.lines.text".into());
        let hits = map_hits(&payload, &op, "wiki");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].snippet, "hit");
    }

    #[test]
    fn maps_with_templates() {
        let payload = json!({"results": [{"id": "page-7", "title": "Deploy"}]});
        let mut op = OpCfg::default();
        op.map.insert("uri".into(), "tpl:teamdocs://{{id}}".into());
        let hits = map_hits(&payload, &op, "td");
        assert_eq!(hits[0].uri, "teamdocs://page-7");
        assert_eq!(hits[0].title, "Deploy");
    }

    #[test]
    fn unwraps_mcp_results() {
        let v = json!({"content": [{"type": "text", "text": "{\"a\": 1}"}]});
        assert_eq!(unwrap_payload(v), json!({"a": 1}));
        let v = json!({"content": [{"type": "text", "text": "hello"}], "isError": false});
        assert_eq!(unwrap_payload(v), json!("hello"));
        let plain = json!({"content": "not mcp", "other": 1});
        assert_eq!(unwrap_payload(plain.clone()), plain);
    }
}
