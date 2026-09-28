//! MCP server over stdio (newline-delimited JSON-RPC 2.0). Requests are handled concurrently.
//! The repository context is (re)opened lazily: the server starts even outside a repository and
//! picks up config edits (e.g. a freshly bootstrapped `.ai/kontext.toml`) without a restart.
use crate::app::App;
use crate::ops;
use crate::store::Store;
use crate::tools;
use anyhow::Result;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::io::{BufRead, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};

const SUPPORTED: &[&str] = &["2024-11-05", "2025-03-26", "2025-06-18", "2025-11-25"];
const DEFAULT_VERSION: &str = "2025-06-18";

struct State {
    dir: PathBuf,
    app: RwLock<Option<Arc<App>>>,
    out: Mutex<std::io::Stdout>,
    /// public tool name → (adapter, original tool name)
    adapter_tools: Mutex<Option<HashMap<String, (String, String)>>>,
    in_flight: AtomicUsize,
}

impl State {
    fn app(&self) -> Result<Arc<App>> {
        let current = self.app.read().unwrap().clone();
        if let Some(a) = &current
            && !a.config_changed()
        {
            return Ok(a.clone());
        }
        match App::open(&self.dir) {
            Ok(a) => {
                let a = Arc::new(a);
                *self.app.write().unwrap() = Some(a.clone());
                *self.adapter_tools.lock().unwrap() = None;
                Ok(a)
            }
            Err(e) => current.ok_or(e),
        }
    }
}

pub fn serve(dir: PathBuf) -> Result<()> {
    let state = Arc::new(State {
        dir,
        app: RwLock::new(None),
        out: Mutex::new(std::io::stdout()),
        adapter_tools: Mutex::new(None),
        in_flight: AtomicUsize::new(0),
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
        let st = state.clone();
        st.in_flight.fetch_add(1, Ordering::SeqCst);
        std::thread::spawn(move || {
            let reply = match &msg {
                Value::Array(batch) => {
                    let replies: Vec<Value> = batch.iter().filter_map(|m| handle(&st, m)).collect();
                    (!replies.is_empty()).then_some(Value::Array(replies))
                }
                m => handle(&st, m),
            };
            if let Some(r) = reply {
                write(&st, &r);
            }
            st.in_flight.fetch_sub(1, Ordering::SeqCst);
        });
    }
    let deadline = Instant::now() + Duration::from_secs(5);
    while state.in_flight.load(Ordering::SeqCst) > 0 && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(20));
    }
    Ok(())
}

fn write(st: &State, v: &Value) {
    if let Ok(mut out) = st.out.lock() {
        let _ = writeln!(out, "{v}");
        let _ = out.flush();
    }
}

fn handle(st: &State, msg: &Value) -> Option<Value> {
    let method = msg.get("method").and_then(|m| m.as_str())?;
    let id = msg.get("id").cloned();
    let params = msg.get("params").cloned().unwrap_or_else(|| json!({}));
    let result: Result<Value, (i64, String)> = match method {
        "initialize" => {
            let asked = params.get("protocolVersion").and_then(|v| v.as_str()).unwrap_or(DEFAULT_VERSION);
            let version = if SUPPORTED.contains(&asked) { asked } else { DEFAULT_VERSION };
            Ok(json!({
                "protocolVersion": version,
                "capabilities": {"tools": {"listChanged": false}, "prompts": {"listChanged": false}, "resources": {"listChanged": false, "subscribe": false}, "logging": {}},
                "serverInfo": {"name": "kontext", "title": "kontext — team context bridge", "version": env!("CARGO_PKG_VERSION")},
                "instructions": if st.app().is_ok_and(|a| a.is_set_up()) { tools::INSTRUCTIONS } else { tools::NOT_SET_UP_INSTRUCTIONS }
            }))
        }
        m if m.starts_with("notifications/") => return None,
        "ping" => Ok(json!({})),
        "logging/setLevel" => Ok(json!({})),
        "tools/list" => Ok(json!({"tools": list_tools(st)})),
        "tools/call" => Ok(call_tool(st, &params)),
        "prompts/list" => Ok(json!({"prompts": tools::prompts()})),
        "prompts/get" => {
            let name = params.get("name").and_then(|n| n.as_str()).unwrap_or("");
            let args = params.get("arguments").cloned().unwrap_or_else(|| json!({}));
            match tools::prompt_text(name, &args) {
                Ok((desc, text)) => {
                    Ok(json!({"description": desc, "messages": [{"role": "user", "content": {"type": "text", "text": text}}]}))
                }
                Err(e) => Err((-32602, format!("{e:#}"))),
            }
        }
        "resources/list" => Ok(json!({"resources": list_resources(st)})),
        "resources/templates/list" => Ok(
            json!({"resourceTemplates": [{"uriTemplate": "kontext://entry/{id}", "name": "Knowledge entry", "mimeType": "text/markdown"}]}),
        ),
        "resources/read" => {
            let uri = params.get("uri").and_then(|u| u.as_str()).unwrap_or("");
            match read_resource(st, uri) {
                Ok(text) => Ok(json!({"contents": [{"uri": uri, "mimeType": "text/markdown", "text": text}]})),
                Err(e) => Err((-32002, format!("{e:#}"))),
            }
        }
        other => Err((-32601, format!("method not found: {other}"))),
    };
    let id = id?;
    Some(match result {
        Ok(r) => json!({"jsonrpc": "2.0", "id": id, "result": r}),
        Err((code, message)) => json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}}),
    })
}

fn adapter_tool_map(app: &App) -> (Vec<Value>, HashMap<String, (String, String)>) {
    let mut defs = Vec::new();
    let mut map = HashMap::new();
    for a in &app.registry().adapters {
        for (original, info) in a.tool_list() {
            if map.contains_key(&info.name) || info.name.starts_with("ctx_") {
                continue;
            }
            map.insert(info.name.clone(), (a.name.clone(), original));
            defs.push(serde_json::to_value(&info).unwrap_or(Value::Null));
        }
    }
    (defs, map)
}

fn list_tools(st: &State) -> Vec<Value> {
    let mut tools = tools::definitions();
    if let Ok(app) = st.app() {
        let (defs, map) = adapter_tool_map(&app);
        tools.extend(defs);
        *st.adapter_tools.lock().unwrap() = Some(map);
    }
    tools
}

fn text_result(r: Result<String>) -> Value {
    match r {
        Ok(t) => json!({"content": [{"type": "text", "text": tools::clip(t)}]}),
        Err(e) => json!({"content": [{"type": "text", "text": format!("Error: {e:#}")}], "isError": true}),
    }
}

fn call_tool(st: &State, params: &Value) -> Value {
    let name = params.get("name").and_then(|n| n.as_str()).unwrap_or("");
    let args = params.get("arguments").cloned().unwrap_or_else(|| json!({}));
    let app = match st.app() {
        Ok(a) => a,
        Err(e) => {
            return text_result(Err(e.context(
                "kontext needs to run inside a git repository (start the MCP server from the project directory, or pass -C <dir>)",
            )));
        }
    };
    if name.starts_with("ctx_") {
        return text_result(tools::call(&app, name, &args, "agent"));
    }
    let target = {
        let mut guard = st.adapter_tools.lock().unwrap();
        if guard.is_none() {
            *guard = Some(adapter_tool_map(&app).1);
        }
        guard.as_ref().and_then(|m| m.get(name).cloned())
    };
    let Some((adapter, original)) = target else {
        return text_result(Err(anyhow::anyhow!("unknown tool '{name}'")));
    };
    let Some(a) = app.registry().get(&adapter) else {
        return text_result(Err(anyhow::anyhow!("adapter '{adapter}' is not available")));
    };
    match a.call_listed(&original, &args) {
        Ok(v) if v.get("content").is_some() => v,
        Ok(v) => json!({"content": [{"type": "text", "text": v.to_string()}]}),
        Err(e) => text_result(Err(e)),
    }
}

fn list_resources(st: &State) -> Vec<Value> {
    let mut out = vec![
        json!({"uri": "kontext://brief", "name": "Team brief", "description": "Decisions, conventions, learnings and module map for this repository", "mimeType": "text/markdown"}),
    ];
    if let Ok(app) = st.app() {
        let (entries, _) = app.entries();
        for e in entries.iter().take(300) {
            out.push(json!({"uri": format!("kontext://entry/{}", e.id), "name": e.title, "description": crate::util::truncate_chars(&e.l0(200), 200), "mimeType": "text/markdown"}));
        }
    }
    out
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
