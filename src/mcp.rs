//! MCP server over stdio (newline-delimited JSON-RPC 2.0). Requests are handled concurrently.
//! The repository context is (re)opened lazily: the server starts even outside a repository and
//! picks up config edits (e.g. a freshly bootstrapped `.ai/kontext.toml`) and branch switches
//! without a restart.
//!
//! Two protocol eras are served side by side. Legacy clients `initialize` a session (2024-11-05
//! to 2025-11-25). Clients of 2026-07-28 call `server/discover` and then send any request with
//! `_meta["io.modelcontextprotocol/protocolVersion"]` (plus `clientInfo` and `clientCapabilities`,
//! accepted and not needed) without initializing; their results carry `resultType: "complete"` and
//! `_meta["io.modelcontextprotocol/serverInfo"]`.
use crate::adapters;
use crate::app::{self, App, lock};
use crate::ops;
use crate::repo::Repo;
use crate::store::Store;
use crate::tools;
use anyhow::Result;
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};

/// Protocol versions this server speaks, oldest first.
pub const SUPPORTED: &[&str] = &["2024-11-05", "2025-03-26", "2025-06-18", "2025-11-25", "2026-07-28"];
/// What a legacy `initialize` gets when it asks for a version this server does not know.
const LEGACY_FALLBACK: &str = "2025-11-25";
pub const META_PROTOCOL_VERSION: &str = "io.modelcontextprotocol/protocolVersion";
pub const META_SERVER_INFO: &str = "io.modelcontextprotocol/serverInfo";
/// JSON-RPC error for a protocol version the server does not support (data lists the supported ones).
pub const UNSUPPORTED_VERSION: i64 = -32022;
/// Entries per `resources/list` page.
const RESOURCES_PAGE: usize = 100;
/// How long a client may cache a list or a read (`ttlMs`, with `cacheScope: "private"`).
const LIST_TTL_MS: u64 = 30_000;
const READ_TTL_MS: u64 = 10_000;

type RpcError = (i64, String, Option<Value>);

fn rpc_err(code: i64, message: impl Into<String>) -> RpcError {
    (code, message.into(), None)
}

/// Public tool name → (adapter, original tool name).
type ToolRoutes = HashMap<String, (String, String)>;

/// Adapter tools re-exposed through this server, as last listed.
#[derive(Default)]
struct AdapterTools {
    /// definitions (sorted by name) and public name → (adapter, original tool name)
    loaded: Option<(Vec<Value>, ToolRoutes)>,
    loading: bool,
    /// adapter tool names the client last saw in a tools/list (sorted)
    announced: Option<Vec<String>>,
}

struct State {
    dir: PathBuf,
    app: RwLock<Option<Arc<App>>>,
    /// A config edit that does not parse: (stamp it was seen at, message, shown in a tool result).
    /// Cached by stamp, so a broken file is not re-parsed on every request.
    config_error: Mutex<Option<(String, String, bool)>>,
    out: Mutex<std::io::Stdout>,
    tools: Mutex<AdapterTools>,
    in_flight: AtomicUsize,
    /// Requests running a tool: JSON-RPC id (as text) → tool name.
    running: Mutex<HashMap<String, String>>,
    /// Requests the client cancelled: their replies are not sent.
    cancelled: Mutex<HashSet<String>>,
}

impl State {
    fn current(&self) -> Option<Arc<App>> {
        self.app.read().unwrap_or_else(|e| e.into_inner()).clone()
    }

    fn app(&self) -> Result<Arc<App>> {
        let current = self.current();
        if let Some(a) = &current {
            if !a.stale() {
                return Ok(a.clone());
            }
            let stamp = a.stamp_now();
            if lock(&self.config_error).as_ref().is_some_and(|(s, _, _)| *s == stamp) {
                return Ok(a.clone());
            }
            return match App::open(&self.dir) {
                Ok(fresh) => Ok(self.replace(fresh)),
                Err(e) => {
                    let msg = crate::util::truncate_chars(&crate::util::one_line(&format!("{e:#}")), 300);
                    *lock(&self.config_error) = Some((stamp, msg, false));
                    Ok(a.clone())
                }
            };
        }
        App::open(&self.dir).map(|fresh| self.replace(fresh))
    }

    fn replace(&self, fresh: App) -> Arc<App> {
        let a = Arc::new(fresh);
        *self.app.write().unwrap_or_else(|e| e.into_inner()) = Some(a.clone());
        *lock(&self.config_error) = None;
        lock(&self.tools).loaded = None;
        a
    }

    /// One line about a config edit that does not parse: once in the next tool result, and in
    /// every brief while it lasts.
    fn config_warning(&self, brief: bool) -> Option<String> {
        let mut g = lock(&self.config_error);
        let (_, msg, shown) = g.as_mut()?;
        if *shown && !brief {
            return None;
        }
        *shown = true;
        Some(format!("warning: a config edit does not parse, kontext keeps the previous config: {msg}"))
    }

    /// Whether any adapter re-exposes tools: then the tool list can change while the server runs.
    fn exposes_tools(&self) -> bool {
        self.app().is_ok_and(|a| a.cfg().adapters.values().any(adapters::cfg_exposes_any))
    }
}

pub fn serve(dir: PathBuf) -> Result<()> {
    let state = Arc::new(State {
        dir,
        app: RwLock::new(None),
        config_error: Mutex::new(None),
        out: Mutex::new(std::io::stdout()),
        tools: Mutex::new(AdapterTools::default()),
        in_flight: AtomicUsize::new(0),
        running: Mutex::new(HashMap::new()),
        cancelled: Mutex::new(HashSet::new()),
    });
    if let Err(e) = state.app() {
        eprintln!("kontext: starting without a repository context: {e:#}");
    }
    let stdin = std::io::stdin();
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        let msg: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(e) => {
                write(&state, &json!({"jsonrpc": "2.0", "id": null, "error": {"code": -32700, "message": format!("parse error: {e}")}}));
                continue;
            }
        };
        // notifications are handled in order, before anything they refer to can finish
        if let Some(method) = msg.get("method").and_then(Value::as_str)
            && method.starts_with("notifications/")
        {
            notification(&state, method, msg.get("params").unwrap_or(&Value::Null));
            continue;
        }
        let st = state.clone();
        st.in_flight.fetch_add(1, Ordering::SeqCst);
        std::thread::spawn(move || {
            // the counter drops even when the request panics
            let _flight = InFlight(st.clone());
            let reply = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| process(&st, &msg))).unwrap_or_else(|_| panic_reply(&msg));
            if let Some(r) = reply {
                let cancelled = r.get("id").map(Value::to_string).is_some_and(|id| lock(&st.cancelled).remove(&id));
                if !cancelled {
                    write(&st, &r);
                }
            }
        });
    }
    // the client is gone: a verify it started must not outlive the session
    kill_running_verifies();
    let deadline = Instant::now() + Duration::from_secs(5);
    while state.in_flight.load(Ordering::SeqCst) > 0 && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(20));
    }
    Ok(())
}

struct InFlight(Arc<State>);

impl Drop for InFlight {
    fn drop(&mut self) {
        self.0.in_flight.fetch_sub(1, Ordering::SeqCst);
    }
}

/// Stop every running `verify` process group (a cancelled request, a closed session).
fn kill_running_verifies() {
    crate::verification::kill_running_verifies();
}

fn process(st: &Arc<State>, msg: &Value) -> Option<Value> {
    match msg {
        Value::Array(batch) => {
            let replies: Vec<Value> = batch.iter().filter_map(|m| handle(st, m)).collect();
            (!replies.is_empty()).then_some(Value::Array(replies))
        }
        m => handle(st, m),
    }
}

/// A request whose handler panicked still gets an answer: -32603, and the server goes on.
fn panic_reply(msg: &Value) -> Option<Value> {
    let err = |id: &Value| json!({"jsonrpc": "2.0", "id": id, "error": {"code": -32603, "message": "internal error: kontext failed on this request; other requests are not affected"}});
    match msg {
        Value::Array(batch) => {
            let v: Vec<Value> = batch.iter().filter_map(|m| m.get("id").map(err)).collect();
            (!v.is_empty()).then_some(Value::Array(v))
        }
        m => m.get("id").map(err),
    }
}

fn write(st: &State, v: &Value) {
    let mut out = lock(&st.out);
    let _ = writeln!(out, "{v}");
    let _ = out.flush();
}

fn notification(st: &State, method: &str, params: &Value) {
    if method == "notifications/cancelled"
        && let Some(id) = params.get("requestId")
    {
        let key = id.to_string();
        let tool = lock(&st.running).get(&key).cloned();
        if tool.is_some() {
            lock(&st.cancelled).insert(key);
        }
        if tool.as_deref() == Some("ctx_verify") {
            kill_running_verifies();
        }
    }
}

fn server_info() -> Value {
    json!({"name": "kontext", "title": "kontext — team context bridge", "version": env!("CARGO_PKG_VERSION")})
}

fn capabilities(st: &State) -> Value {
    json!({
        "tools": {"listChanged": st.exposes_tools()},
        "prompts": {"listChanged": false},
        "resources": {"listChanged": false, "subscribe": false}
    })
}

fn instructions(st: &State) -> String {
    match st.app() {
        Ok(a) => tools::instructions(&a),
        Err(_) if Repo::discover(&st.dir).is_err() => tools::NO_REPO_INSTRUCTIONS.to_string(),
        Err(_) => tools::NOT_SET_UP_INSTRUCTIONS.to_string(),
    }
}

fn unsupported(version: &str) -> RpcError {
    (UNSUPPORTED_VERSION, format!("unsupported protocol version '{version}'"), Some(json!({"supported": SUPPORTED, "requested": version})))
}

/// The version a legacy `initialize` settles on: the one asked for when known; an older or
/// malformed unknown one falls back to 2025-11-25; one newer than every supported version is an
/// error that lists them.
fn negotiate(asked: Option<&str>) -> Result<&'static str, RpcError> {
    match asked {
        Some(v) => match SUPPORTED.iter().find(|s| **s == v) {
            Some(s) => Ok(s),
            None if v > SUPPORTED[SUPPORTED.len() - 1] => Err(unsupported(v)),
            None => Ok(LEGACY_FALLBACK),
        },
        None => Ok(LEGACY_FALLBACK),
    }
}

fn with_cache(mut v: Value, ttl_ms: u64) -> Value {
    v["ttlMs"] = json!(ttl_ms);
    v["cacheScope"] = json!("private");
    v
}

fn handle(st: &Arc<State>, msg: &Value) -> Option<Value> {
    let method = msg.get("method").and_then(|m| m.as_str())?;
    let id = msg.get("id").cloned();
    if method.starts_with("notifications/") {
        notification(st, method, msg.get("params").unwrap_or(&Value::Null));
        return None;
    }
    let params = msg.get("params").cloned().unwrap_or_else(|| json!({}));
    let asked = params.get("_meta").and_then(|m| m.get(META_PROTOCOL_VERSION));
    let modern = method == "server/discover" || asked.is_some();
    let result = match asked {
        Some(v) if !v.as_str().is_some_and(|v| SUPPORTED.contains(&v)) => Err(unsupported(v.as_str().unwrap_or(""))),
        _ => dispatch(st, method, &params, modern, id.as_ref()),
    };
    let id = id?;
    Some(match result {
        Ok(mut r) => {
            if modern && r.is_object() {
                r["resultType"] = json!("complete");
                if !r["_meta"].is_object() {
                    r["_meta"] = json!({});
                }
                r["_meta"][META_SERVER_INFO] = server_info();
            }
            json!({"jsonrpc": "2.0", "id": id, "result": r})
        }
        Err((code, message, data)) => {
            let mut error = json!({"code": code, "message": message});
            if let Some(d) = data {
                error["data"] = d;
            }
            json!({"jsonrpc": "2.0", "id": id, "error": error})
        }
    })
}

fn dispatch(st: &Arc<State>, method: &str, params: &Value, modern: bool, id: Option<&Value>) -> Result<Value, RpcError> {
    match method {
        "initialize" => {
            let version = negotiate(params.get("protocolVersion").and_then(|v| v.as_str()))?;
            Ok(json!({
                "protocolVersion": version,
                "capabilities": capabilities(st),
                "serverInfo": server_info(),
                "instructions": instructions(st)
            }))
        }
        "server/discover" => Ok(json!({
            "supportedVersions": SUPPORTED,
            "capabilities": capabilities(st),
            "serverInfo": server_info(),
            "instructions": instructions(st)
        })),
        "ping" => Ok(json!({})),
        // no logging capability is declared; legacy clients that set a level anyway get an empty answer
        "logging/setLevel" if !modern => Ok(json!({})),
        "tools/list" => Ok(with_cache(json!({"tools": list_tools(st)}), LIST_TTL_MS)),
        "tools/call" => call_tool(st, params, id),
        "prompts/list" => Ok(with_cache(json!({"prompts": tools::prompts()}), LIST_TTL_MS)),
        "prompts/get" => {
            let name = params.get("name").and_then(|n| n.as_str()).unwrap_or("");
            let args = params.get("arguments").cloned().unwrap_or_else(|| json!({}));
            match tools::prompt_text(name, &args) {
                Ok((desc, text)) => {
                    Ok(json!({"description": desc, "messages": [{"role": "user", "content": {"type": "text", "text": text}}]}))
                }
                Err(e) => Err(rpc_err(-32602, format!("{e:#}"))),
            }
        }
        "resources/list" => list_resources(st, params.get("cursor").and_then(Value::as_str)).map(|v| with_cache(v, LIST_TTL_MS)),
        "resources/templates/list" => Ok(with_cache(
            json!({"resourceTemplates": [{"uriTemplate": "kontext://entry/{id}", "name": "Knowledge entry", "mimeType": "text/markdown"}]}),
            LIST_TTL_MS,
        )),
        "resources/read" => {
            let uri = params.get("uri").and_then(|u| u.as_str()).unwrap_or("");
            match read_resource(st, uri) {
                Ok(text) => Ok(with_cache(json!({"contents": [{"uri": uri, "mimeType": "text/markdown", "text": text}]}), READ_TTL_MS)),
                // a resource that does not exist: invalid params on the modern path, -32002 for legacy clients
                Err(e) => Err(rpc_err(if modern { -32602 } else { -32002 }, format!("{e:#}"))),
            }
        }
        // lets tests prove that a panicking request still gets an answer
        "kontext/test_panic" if std::env::var("KONTEXT_TEST_PANIC").as_deref() == Ok("1") => panic!("KONTEXT_TEST_PANIC"),
        other => Err(rpc_err(-32601, format!("method not found: {other}"))),
    }
}

/// Re-exposed adapter tools (and tools declared in config) of `app`, sorted by name.
fn adapter_tool_map(app: &App) -> (Vec<Value>, HashMap<String, (String, String)>) {
    let mut found: Vec<(String, Value, String, String)> = Vec::new();
    let mut seen = HashSet::new();
    for a in &app.registry().adapters {
        for (original, info) in a.tool_list() {
            if info.name.starts_with("ctx_") || !seen.insert(info.name.clone()) {
                continue;
            }
            found.push((info.name.clone(), serde_json::to_value(&info).unwrap_or(Value::Null), a.name.clone(), original));
        }
    }
    found.sort_by(|a, b| a.0.cmp(&b.0));
    let map = found.iter().map(|(n, _, a, o)| (n.clone(), (a.clone(), o.clone()))).collect();
    (found.into_iter().map(|(_, d, _, _)| d).collect(), map)
}

fn sorted_names(map: &HashMap<String, (String, String)>) -> Vec<String> {
    let mut v: Vec<String> = map.keys().cloned().collect();
    v.sort();
    v
}

/// kontext's own tools at once, adapter tools when they are loaded: an exposing adapter that is
/// slow to start never holds up the list. When its tools arrive (or change), the client gets
/// `notifications/tools/list_changed`.
fn list_tools(st: &Arc<State>) -> Vec<Value> {
    let mut list = tools::definitions();
    let Ok(app) = st.app() else { return list };
    if !app.cfg().adapters.values().any(adapters::cfg_exposes_any) {
        // only declared tools: no adapter process is contacted, listing them is cheap
        let (defs, map) = adapter_tool_map(&app);
        list.extend(defs.iter().cloned());
        let mut g = lock(&st.tools);
        g.announced = Some(sorted_names(&map));
        g.loaded = Some((defs, map));
        return list;
    }
    let mut g = lock(&st.tools);
    let loaded = g.loaded.as_ref().map(|(defs, map)| (defs.clone(), sorted_names(map)));
    if let Some((defs, names)) = loaded {
        list.extend(defs);
        g.announced = Some(names);
        return list;
    }
    if g.announced.is_none() {
        g.announced = Some(Vec::new());
    }
    if !g.loading {
        g.loading = true;
        let st = st.clone();
        std::thread::spawn(move || warm_up(st, app));
    }
    list
}

fn warm_up(st: Arc<State>, app: Arc<App>) {
    let listed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| adapter_tool_map(&app)));
    let mut g = lock(&st.tools);
    g.loading = false;
    let Ok((defs, map)) = listed else { return };
    // the context was reopened meanwhile: the next tools/list lists its tools
    if !st.current().is_some_and(|c| Arc::ptr_eq(&c, &app)) {
        return;
    }
    let names = sorted_names(&map);
    let changed = g.announced.as_ref() != Some(&names);
    g.loaded = Some((defs, map));
    drop(g);
    if changed {
        write(&st, &json!({"jsonrpc": "2.0", "method": "notifications/tools/list_changed"}));
    }
}

/// The adapter and original name behind a re-exposed tool (listing them now if needed).
fn adapter_tool(st: &State, app: &App, name: &str) -> Option<(String, String)> {
    if let Some((_, map)) = &lock(&st.tools).loaded {
        return map.get(name).cloned();
    }
    let (defs, map) = adapter_tool_map(app);
    let hit = map.get(name).cloned();
    lock(&st.tools).loaded = Some((defs, map));
    hit
}

fn text_result(r: Result<String>) -> Value {
    match r {
        Ok(t) => json!({"content": [{"type": "text", "text": tools::clip(t)}]}),
        Err(e) => json!({"content": [{"type": "text", "text": format!("Error: {e:#}")}], "isError": true}),
    }
}

/// Removes a request from the running set however it ends.
struct Running<'a>(&'a State, Option<String>);

impl Drop for Running<'_> {
    fn drop(&mut self) {
        if let Some(k) = &self.1 {
            lock(&self.0.running).remove(k);
        }
    }
}

fn call_tool(st: &Arc<State>, params: &Value, id: Option<&Value>) -> Result<Value, RpcError> {
    let name = params.get("name").and_then(|n| n.as_str()).unwrap_or("");
    let mut args = params.get("arguments").cloned().unwrap_or_else(|| json!({}));
    if name.is_empty() {
        return Err(rpc_err(-32602, "tools/call needs a tool name"));
    }
    if name.starts_with("ctx_") && !tools::is_tool(name) {
        return Err(rpc_err(-32602, format!("unknown tool '{name}'")));
    }
    let app = match st.app() {
        Ok(a) => a,
        Err(e) => {
            // outside a repository a call still works when it names one with `dir`
            let dir = args.get("dir").and_then(Value::as_str).filter(|d| !d.trim().is_empty()).map(str::to_string);
            match dir.filter(|_| name.starts_with("ctx_") && name != "ctx_api") {
                Some(d) => {
                    let p = Path::new(d.trim());
                    let abs = if p.is_absolute() { p.to_path_buf() } else { st.dir.join(p) };
                    match app::open_shared(&abs) {
                        Ok(a) => {
                            if let Some(o) = args.as_object_mut() {
                                o.remove("dir");
                            }
                            a
                        }
                        Err(e) => return Ok(text_result(Err(e))),
                    }
                }
                None if name == "ctx_api" => {
                    let value = crate::api::failure(e);
                    return Ok(
                        json!({"content": [{"type": "text", "text": value.to_string()}], "structuredContent": value, "isError": true}),
                    );
                }
                None if !name.starts_with("ctx_") => return Err(rpc_err(-32602, format!("unknown tool '{name}'"))),
                None => {
                    return Ok(text_result(Err(e.context(
                        "kontext found no git repository: pass `dir` (a path inside the project), or start the MCP server from the project directory (or with -C <dir>)",
                    ))));
                }
            }
        }
    };
    if name == "ctx_api" {
        let value = tools::api_call(&app, &args);
        let is_error = value["ok"] != true;
        return Ok(json!({"content": [{"type": "text", "text": value.to_string()}], "structuredContent": value, "isError": is_error}));
    }
    if name.starts_with("ctx_") {
        let key = id.map(Value::to_string);
        if let Some(k) = &key {
            lock(&st.running).insert(k.clone(), name.to_string());
        }
        let _running = Running(st, key);
        let warning = st.config_warning(name == "ctx_brief");
        return Ok(match tools::call_full(&app, name, &args, "agent") {
            Ok(out) => {
                let text = match &warning {
                    Some(w) => format!("{w}\n{}", out.text),
                    None => out.text,
                };
                let mut content = vec![json!({"type": "text", "text": tools::clip(text)})];
                content.extend(out.links);
                let mut r = json!({"content": content});
                if let Some(s) = out.structured {
                    r["structuredContent"] = s;
                }
                r
            }
            Err(e) => {
                let mut r = text_result(Err(e));
                if let Some(w) = warning {
                    r["content"][0]["text"] = json!(format!("{w}\n{}", r["content"][0]["text"].as_str().unwrap_or("")));
                }
                r
            }
        });
    }
    let Some((adapter, original)) = adapter_tool(st, &app, name) else {
        return Err(rpc_err(-32602, format!("unknown tool '{name}'")));
    };
    let Some(a) = app.registry().get(&adapter) else {
        return Ok(text_result(Err(anyhow::anyhow!("adapter '{adapter}' is not available"))));
    };
    Ok(match a.call_listed(&original, &args) {
        Ok(v) if v.get("content").is_some() => v,
        Ok(v) => json!({"content": [{"type": "text", "text": v.to_string()}]}),
        Err(e) => text_result(Err(anyhow::anyhow!("{}", adapters::drivers::strip_url_secrets(&format!("{e:#}"))))),
    })
}

/// The brief, then every entry (sorted by id, the status of inactive ones in the description),
/// a page at a time: `cursor` is where the previous page stopped.
fn list_resources(st: &State, cursor: Option<&str>) -> Result<Value, RpcError> {
    let mut all = vec![
        json!({"uri": "kontext://brief", "name": "Team brief", "description": "Decisions, conventions, learnings and module map for this repository", "mimeType": "text/markdown"}),
    ];
    if let Ok(app) = st.app() {
        let (mut entries, _) = app.entries();
        entries.sort_by(|a, b| a.id.cmp(&b.id));
        for e in &entries {
            let status =
                e.status.as_deref().filter(|s| !matches!(*s, "accepted" | "active")).map(|s| format!("[{s}] ")).unwrap_or_default();
            all.push(json!({"uri": format!("kontext://entry/{}", e.id), "name": e.title,
                "description": format!("{status}{}", crate::util::truncate_chars(&e.l0(200), 200)), "mimeType": "text/markdown"}));
        }
    }
    let start = match cursor {
        None => 0,
        Some(c) => c.parse::<usize>().ok().filter(|n| *n <= all.len()).ok_or_else(|| rpc_err(-32602, format!("invalid cursor '{c}'")))?,
    };
    let end = (start + RESOURCES_PAGE).min(all.len());
    let mut v = json!({"resources": all[start..end].to_vec()});
    if end < all.len() {
        v["nextCursor"] = json!(end.to_string());
    }
    Ok(v)
}

fn read_resource(st: &State, uri: &str) -> Result<String> {
    let app = st.app()?;
    if uri == "kontext://brief" {
        return Ok(ops::brief(&app, &[], app.cfg().brief.budget_tokens, false));
    }
    if let Some(id) = uri.strip_prefix("kontext://entry/") {
        let (entries, _) = app.entries();
        return Store::find(&entries, id).map(|e| e.markdown()).ok_or_else(|| anyhow::anyhow!("no entry '{id}'"));
    }
    ops::read(&app, uri, 2)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_versions_negotiate_and_newer_ones_are_refused() {
        for v in &SUPPORTED[..4] {
            assert_eq!(negotiate(Some(v)).unwrap(), *v);
        }
        assert_eq!(negotiate(None).unwrap(), "2025-11-25");
        assert_eq!(negotiate(Some("2025-01-01")).unwrap(), "2025-11-25");
        let (code, _, data) = negotiate(Some("2099-01-01")).unwrap_err();
        assert_eq!(code, UNSUPPORTED_VERSION);
        assert_eq!(data.unwrap()["supported"], json!(SUPPORTED));
    }

    #[test]
    fn a_panicking_request_is_answered() {
        let r = panic_reply(&json!({"jsonrpc": "2.0", "id": 7, "method": "tools/call"})).unwrap();
        assert_eq!((r["id"].clone(), r["error"]["code"].clone()), (json!(7), json!(-32603)));
        assert!(panic_reply(&json!({"jsonrpc": "2.0", "method": "notifications/x"})).is_none());
    }
}
