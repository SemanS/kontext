//! A small MCP client: stdio (newline-delimited JSON-RPC to a child process) and streamable HTTP.
use anyhow::{Context, Result, anyhow, bail};
use serde::Serialize;
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashMap};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;

pub const PROTOCOL_VERSION: &str = "2025-06-18";

#[derive(Debug, Clone, Serialize)]
pub struct ToolInfo {
    pub name: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub description: String,
    #[serde(rename = "inputSchema")]
    pub input_schema: Value,
}

type Pending = Arc<Mutex<HashMap<u64, mpsc::Sender<Value>>>>;

struct Stdio_ {
    child: Mutex<Child>,
    stdin: Arc<Mutex<ChildStdin>>,
    pending: Pending,
    alive: Arc<AtomicBool>,
}

struct Http_ {
    url: String,
    headers: Vec<(String, String)>,
    agent: ureq::Agent,
    session: Mutex<Option<String>>,
    protocol: Mutex<Option<String>>,
}

enum Transport {
    Stdio(Stdio_),
    Http(Http_),
}

pub struct McpClient {
    pub name: String,
    transport: Transport,
    next_id: AtomicU64,
    pub server_info: Mutex<Value>,
    tools: Mutex<Option<Vec<ToolInfo>>>,
}

fn id_of(v: &Value) -> Option<u64> {
    match v.get("id")? {
        Value::Number(n) => n.as_u64(),
        Value::String(s) => s.parse().ok(),
        _ => None,
    }
}

impl McpClient {
    pub fn spawn(
        name: &str,
        command: &[String],
        cwd: &Path,
        env: &BTreeMap<String, String>,
        log: Option<PathBuf>,
        root: &Path,
        init_timeout: Duration,
    ) -> Result<McpClient> {
        let program = command.first().ok_or_else(|| anyhow!("adapter '{name}': empty command"))?;
        let mut cmd = Command::new(program);
        cmd.args(&command[1..]).current_dir(cwd).envs(env).stdin(Stdio::piped()).stdout(Stdio::piped());
        match log.as_ref().and_then(|p| {
            if let Some(dir) = p.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            std::fs::OpenOptions::new().create(true).append(true).open(p).ok()
        }) {
            Some(f) => cmd.stderr(f),
            None => cmd.stderr(Stdio::null()),
        };
        let mut child = cmd.spawn().with_context(|| format!("adapter '{name}': cannot start `{}`", command.join(" ")))?;
        let stdout = child.stdout.take().expect("piped stdout");
        let stdin = Arc::new(Mutex::new(child.stdin.take().expect("piped stdin")));
        let pending: Pending = Arc::default();
        let alive = Arc::new(AtomicBool::new(true));
        {
            let pending = pending.clone();
            let alive = alive.clone();
            let stdin = stdin.clone();
            let root_uri = format!("file://{}", root.display());
            let root_name = root.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
            std::thread::spawn(move || {
                let reader = BufReader::new(stdout);
                for line in reader.lines() {
                    let Ok(line) = line else { break };
                    let Ok(msg) = serde_json::from_str::<Value>(line.trim()) else { continue };
                    let is_response = msg.get("method").is_none() && (msg.get("result").is_some() || msg.get("error").is_some());
                    if is_response {
                        if let Some(id) = id_of(&msg)
                            && let Some(tx) = pending.lock().unwrap().remove(&id)
                        {
                            let _ = tx.send(msg);
                        }
                        continue;
                    }
                    // A request from the server to us (ping, roots/list, …)
                    if let (Some(method), Some(id)) = (msg.get("method").and_then(|m| m.as_str()), msg.get("id")) {
                        let reply = match method {
                            "ping" => json!({"jsonrpc": "2.0", "id": id, "result": {}}),
                            "roots/list" => {
                                json!({"jsonrpc": "2.0", "id": id, "result": {"roots": [{"uri": root_uri, "name": root_name}]}})
                            }
                            _ => {
                                json!({"jsonrpc": "2.0", "id": id, "error": {"code": -32601, "message": "method not supported by kontext"}})
                            }
                        };
                        if let Ok(mut w) = stdin.lock() {
                            let _ = writeln!(w, "{reply}");
                            let _ = w.flush();
                        }
                    }
                }
                alive.store(false, Ordering::SeqCst);
                for (_, tx) in pending.lock().unwrap().drain() {
                    let _ = tx.send(json!({"error": {"code": -32000, "message": "MCP server exited"}}));
                }
            });
        }
        let client = McpClient {
            name: name.to_string(),
            transport: Transport::Stdio(Stdio_ { child: Mutex::new(child), stdin, pending, alive }),
            next_id: AtomicU64::new(1),
            server_info: Mutex::new(Value::Null),
            tools: Mutex::new(None),
        };
        client.initialize(init_timeout)?;
        Ok(client)
    }

    pub fn connect_http(name: &str, url: &str, headers: Vec<(String, String)>, init_timeout: Duration) -> Result<McpClient> {
        let agent: ureq::Agent =
            ureq::Agent::config_builder().timeout_global(Some(Duration::from_secs(120))).http_status_as_error(false).build().into();
        let client = McpClient {
            name: name.to_string(),
            transport: Transport::Http(Http_ {
                url: url.to_string(),
                headers,
                agent,
                session: Mutex::new(None),
                protocol: Mutex::new(None),
            }),
            next_id: AtomicU64::new(1),
            server_info: Mutex::new(Value::Null),
            tools: Mutex::new(None),
        };
        client.initialize(init_timeout)?;
        Ok(client)
    }

    fn initialize(&self, timeout: Duration) -> Result<()> {
        let res = self
            .request(
                "initialize",
                json!({
                    "protocolVersion": PROTOCOL_VERSION,
                    "capabilities": {"roots": {"listChanged": false}},
                    "clientInfo": {"name": "kontext", "version": env!("CARGO_PKG_VERSION")}
                }),
                timeout,
            )
            .with_context(|| format!("adapter '{}': MCP initialize failed", self.name))?;
        if let Transport::Http(h) = &self.transport
            && let Some(v) = res.get("protocolVersion").and_then(|v| v.as_str())
        {
            *h.protocol.lock().unwrap() = Some(v.to_string());
        }
        *self.server_info.lock().unwrap() = res;
        self.notify("notifications/initialized", json!({}))?;
        Ok(())
    }

    pub fn alive(&self) -> bool {
        match &self.transport {
            Transport::Stdio(s) => s.alive.load(Ordering::SeqCst),
            Transport::Http(_) => true,
        }
    }

    fn notify(&self, method: &str, params: Value) -> Result<()> {
        let msg = json!({"jsonrpc": "2.0", "method": method, "params": params});
        match &self.transport {
            Transport::Stdio(s) => {
                let mut w = s.stdin.lock().unwrap();
                writeln!(w, "{msg}")?;
                w.flush()?;
                Ok(())
            }
            Transport::Http(h) => {
                self.http_post(h, &msg, None, Duration::from_secs(30))?;
                Ok(())
            }
        }
    }

    pub fn request(&self, method: &str, params: Value, timeout: Duration) -> Result<Value> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let msg = json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params});
        let reply = match &self.transport {
            Transport::Stdio(s) => {
                if !s.alive.load(Ordering::SeqCst) {
                    bail!("adapter '{}': MCP server is not running", self.name);
                }
                let (tx, rx) = mpsc::channel();
                s.pending.lock().unwrap().insert(id, tx);
                {
                    let mut w = s.stdin.lock().unwrap();
                    writeln!(w, "{msg}")?;
                    w.flush()?;
                }
                match rx.recv_timeout(timeout) {
                    Ok(v) => v,
                    Err(_) => {
                        s.pending.lock().unwrap().remove(&id);
                        let _ = self.notify("notifications/cancelled", json!({"requestId": id, "reason": "timeout"}));
                        bail!("adapter '{}': {method} timed out after {}s", self.name, timeout.as_secs());
                    }
                }
            }
            Transport::Http(h) => {
                self.http_post(h, &msg, Some(id), timeout)?.ok_or_else(|| anyhow!("adapter '{}': no response to {method}", self.name))?
            }
        };
        if let Some(err) = reply.get("error") {
            let m = err.get("message").and_then(|m| m.as_str()).unwrap_or("error");
            bail!("adapter '{}': {method}: {m}", self.name);
        }
        Ok(reply.get("result").cloned().unwrap_or(Value::Null))
    }

    fn http_post(&self, h: &Http_, msg: &Value, want: Option<u64>, timeout: Duration) -> Result<Option<Value>> {
        let mut req = h
            .agent
            .post(&h.url)
            .config()
            .timeout_global(Some(timeout))
            .build()
            .header("Content-Type", "application/json")
            .header("Accept", "application/json, text/event-stream");
        for (k, v) in &h.headers {
            req = req.header(k, v);
        }
        if let Some(s) = h.session.lock().unwrap().clone() {
            req = req.header("Mcp-Session-Id", &s);
        }
        if let Some(p) = h.protocol.lock().unwrap().clone() {
            req = req.header("MCP-Protocol-Version", &p);
        }
        let mut resp = req.send(msg.to_string()).with_context(|| format!("adapter '{}': POST {}", self.name, h.url))?;
        if let Some(s) = resp.headers().get("mcp-session-id").and_then(|v| v.to_str().ok()) {
            *h.session.lock().unwrap() = Some(s.to_string());
        }
        let status = resp.status().as_u16();
        if status == 202 || status == 204 {
            return Ok(None);
        }
        if status >= 400 {
            let body = resp.body_mut().read_to_string().unwrap_or_default();
            bail!("adapter '{}': HTTP {status}: {}", self.name, crate::util::truncate_chars(&body, 300));
        }
        let ctype = resp.headers().get("content-type").and_then(|v| v.to_str().ok()).unwrap_or("").to_lowercase();
        if ctype.contains("text/event-stream") {
            let mut reader = BufReader::new(resp.into_body().into_reader());
            let mut data = String::new();
            let mut line = String::new();
            loop {
                line.clear();
                let n = reader.read_line(&mut line)?;
                if n == 0 {
                    break;
                }
                let t = line.trim_end_matches(['\r', '\n']);
                if let Some(d) = t.strip_prefix("data:") {
                    data.push_str(d.trim_start());
                    data.push('\n');
                } else if t.is_empty() && !data.is_empty() {
                    if let Ok(v) = serde_json::from_str::<Value>(data.trim())
                        && (want.is_none() || id_of(&v) == want)
                    {
                        return Ok(Some(v));
                    }
                    data.clear();
                }
            }
            if !data.is_empty()
                && let Ok(v) = serde_json::from_str::<Value>(data.trim())
            {
                return Ok(Some(v));
            }
            return Ok(None);
        }
        let mut body = String::new();
        resp.into_body().into_reader().read_to_string(&mut body)?;
        if body.trim().is_empty() {
            return Ok(None);
        }
        let v: Value = serde_json::from_str(&body).with_context(|| format!("adapter '{}': invalid JSON-RPC reply", self.name))?;
        if let Value::Array(items) = v {
            return Ok(items.into_iter().find(|x| want.is_none() || id_of(x) == want));
        }
        Ok(Some(v))
    }

    pub fn list_tools(&self, timeout: Duration) -> Result<Vec<ToolInfo>> {
        if let Some(t) = self.tools.lock().unwrap().clone() {
            return Ok(t);
        }
        let mut tools = Vec::new();
        let mut cursor: Option<String> = None;
        for _ in 0..20 {
            let params = match &cursor {
                Some(c) => json!({"cursor": c}),
                None => json!({}),
            };
            let res = self.request("tools/list", params, timeout)?;
            for t in res.get("tools").and_then(|t| t.as_array()).cloned().unwrap_or_default() {
                tools.push(ToolInfo {
                    name: t.get("name").and_then(|n| n.as_str()).unwrap_or_default().to_string(),
                    description: t.get("description").and_then(|n| n.as_str()).unwrap_or_default().to_string(),
                    input_schema: t.get("inputSchema").cloned().unwrap_or_else(|| json!({"type": "object"})),
                });
            }
            cursor = res.get("nextCursor").and_then(|c| c.as_str()).map(str::to_string);
            if cursor.is_none() {
                break;
            }
        }
        *self.tools.lock().unwrap() = Some(tools.clone());
        Ok(tools)
    }

    pub fn call_tool(&self, name: &str, args: &Value, timeout: Duration) -> Result<Value> {
        self.request("tools/call", json!({"name": name, "arguments": args}), timeout)
    }
}

impl Drop for McpClient {
    fn drop(&mut self) {
        if let Transport::Stdio(s) = &self.transport
            && let Ok(mut c) = s.child.lock()
        {
            let _ = c.kill();
            let _ = c.wait();
        }
    }
}

/// Text of an MCP tool result (`content[].text` joined).
pub fn result_text(res: &Value) -> String {
    res.get("content")
        .and_then(|c| c.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|i| match i.get("type").and_then(|t| t.as_str()) {
                    Some("text") => i.get("text").and_then(|t| t.as_str()).map(str::to_string),
                    Some("resource") => i.pointer("/resource/text").and_then(|t| t.as_str()).map(str::to_string),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_default()
}
