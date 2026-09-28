//! Transport drivers. None of them knows any particular product: they execute an `OpCfg`
//! (rendered with the template context) and return the raw payload as JSON.
use super::mcp_client::{McpClient, ToolInfo, result_text};
use crate::config::{AdapterCfg, OpCfg};
use crate::template::{render_str, render_value, to_text};
use anyhow::{Context, Result, anyhow, bail};
use serde_json::{Map, Value, json};
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub trait Driver: Send + Sync {
    /// Execute an op. `role` is the capability name (search, read, store, …) used for argument auto-binding.
    fn call(&self, op: &OpCfg, role: &str, ctx: &Value, timeout: Duration) -> Result<Value>;
    fn tools(&self, _timeout: Duration) -> Result<Vec<ToolInfo>> {
        Ok(Vec::new())
    }
    fn call_tool(&self, _name: &str, _args: &Value, _timeout: Duration) -> Result<Value> {
        bail!("this adapter does not speak MCP")
    }
}

// ------------------------------------------------------------------------------------------ MCP

pub struct McpDriver {
    name: String,
    command: Option<Vec<String>>,
    url: Option<String>,
    headers: Vec<(String, String)>,
    cwd: PathBuf,
    env: BTreeMap<String, String>,
    log: PathBuf,
    root: PathBuf,
    client: Mutex<Option<Arc<McpClient>>>,
}

impl McpDriver {
    pub fn new(name: &str, cfg: &AdapterCfg, ctx: &Value, root: PathBuf, log: PathBuf) -> Result<McpDriver> {
        let command = cfg.command.as_ref().map(|c| c.iter().map(|a| render_str(a, ctx)).collect::<Vec<_>>());
        let url = cfg.url.as_ref().map(|u| render_str(u, ctx));
        if command.is_none() && url.is_none() {
            bail!("adapter '{name}': driver \"mcp\" needs `command` (stdio) or `url` (streamable HTTP)");
        }
        let cwd = cfg.cwd.as_ref().map(|c| PathBuf::from(render_str(c, ctx))).unwrap_or_else(|| root.clone());
        let env = cfg.env.iter().map(|(k, v)| (k.clone(), render_str(v, ctx))).collect();
        let headers = cfg.headers.iter().map(|(k, v)| (k.clone(), render_str(v, ctx))).collect();
        Ok(McpDriver { name: name.to_string(), command, url, headers, cwd, env, log, root, client: Mutex::new(None) })
    }

    fn client(&self, timeout: Duration) -> Result<Arc<McpClient>> {
        let mut guard = self.client.lock().unwrap();
        if let Some(c) = guard.as_ref()
            && c.alive()
        {
            return Ok(c.clone());
        }
        let init_timeout = timeout.max(Duration::from_secs(45));
        let c = match (&self.url, &self.command) {
            (Some(url), _) => McpClient::connect_http(&self.name, url, self.headers.clone(), init_timeout)?,
            (None, Some(cmd)) => McpClient::spawn(&self.name, cmd, &self.cwd, &self.env, Some(self.log.clone()), &self.root, init_timeout)?,
            _ => unreachable!(),
        };
        let c = Arc::new(c);
        *guard = Some(c.clone());
        Ok(c)
    }
}

impl Driver for McpDriver {
    fn call(&self, op: &OpCfg, role: &str, ctx: &Value, timeout: Duration) -> Result<Value> {
        let tool = op.tool.as_ref().map(|t| render_str(t, ctx)).ok_or_else(|| anyhow!("op '{role}' needs `tool`"))?;
        let client = self.client(timeout)?;
        let args = match &op.args {
            Some(a) => render_value(a, ctx),
            None => {
                let tools = client.list_tools(timeout)?;
                let schema = tools.iter().find(|t| t.name == tool).map(|t| t.input_schema.clone()).ok_or_else(|| {
                    anyhow!(
                        "adapter '{}' has no tool '{tool}' (available: {})",
                        self.name,
                        tools.iter().map(|t| t.name.as_str()).collect::<Vec<_>>().join(", ")
                    )
                })?;
                autobind(&schema, ctx, role)
            }
        };
        let res = client.call_tool(&tool, &args, timeout)?;
        if res.get("isError").and_then(|b| b.as_bool()) == Some(true) {
            bail!("{}", crate::util::truncate_chars(&result_text(&res), 500));
        }
        Ok(res)
    }

    fn tools(&self, timeout: Duration) -> Result<Vec<ToolInfo>> {
        self.client(timeout)?.list_tools(timeout)
    }

    fn call_tool(&self, name: &str, args: &Value, timeout: Duration) -> Result<Value> {
        self.client(timeout)?.call_tool(name, args, timeout)
    }
}

const QUERY_NAMES: &[&str] = &[
    "query",
    "q",
    "search",
    "search_query",
    "text",
    "pattern",
    "substring_pattern",
    "keyword",
    "keywords",
    "term",
    "question",
    "topic",
    "name_path_pattern",
    "name_path",
    "symbol",
    "symbol_name",
    "name",
    "input",
    "prompt",
];
const TARGET_NAMES: &[&str] = &["target", "path", "file", "file_path", "filepath", "relative_path", "commit", "query", "q", "text"];
const URI_NAMES: &[&str] = &["uri", "url", "path", "id", "resource", "name"];
const CONTENT_NAMES: &[&str] = &["content", "text", "memory", "body", "markdown", "message", "data", "note"];
const TITLE_NAMES: &[&str] = &["title", "subject", "name", "key"];
const TAG_NAMES: &[&str] = &["tags", "labels", "categories"];
const LIMIT_NAMES: &[&str] = &["limit", "max_results", "top_k", "k", "n", "count", "max", "num_results", "size", "maxFiles"];
const PROJECT_NAMES: &[&str] = &["projectPath", "project_path", "project", "repo", "repository", "cwd", "root", "workspace", "directory"];
const PROMPT_NAMES: &[&str] = &["prompt", "input", "message", "text", "query"];

/// Fill tool arguments from the context using the tool's JSON schema and well-known parameter names.
pub fn autobind(schema: &Value, ctx: &Value, role: &str) -> Value {
    let empty = Map::new();
    let props = schema.get("properties").and_then(|p| p.as_object()).unwrap_or(&empty);
    let required: Vec<String> = schema
        .get("required")
        .and_then(|r| r.as_array())
        .map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect())
        .unwrap_or_default();
    let plan: Vec<(&[&str], &str)> = match role {
        "history" => vec![(TARGET_NAMES, "target"), (LIMIT_NAMES, "limit"), (PROJECT_NAMES, "repo.root")],
        "read" => vec![(URI_NAMES, "uri")],
        "store" => vec![(CONTENT_NAMES, "markdown"), (TITLE_NAMES, "title"), (TAG_NAMES, "tags")],
        "llm" => vec![(PROMPT_NAMES, "prompt")],
        _ => vec![(QUERY_NAMES, "query"), (LIMIT_NAMES, "limit"), (PROJECT_NAMES, "repo.root")],
    };
    let mut args = Map::new();
    let find_prop = |name: &str| props.keys().find(|k| k.eq_ignore_ascii_case(name)).cloned();
    for (names, var) in plan {
        let value = crate::template::lookup(var, ctx);
        if value.is_null() {
            continue;
        }
        for n in names {
            if let Some(key) = find_prop(n) {
                if args.contains_key(&key) {
                    continue;
                }
                let typed = coerce(&value, props.get(&key));
                args.insert(key, typed);
                break;
            }
        }
    }
    // A required string parameter we could not name-match gets the primary value.
    let primary = match role {
        "history" => "target",
        "read" => "uri",
        "store" => "markdown",
        "llm" => "prompt",
        _ => "query",
    };
    let primary_value = crate::template::lookup(primary, ctx);
    for r in required {
        if args.contains_key(&r) || primary_value.is_null() {
            continue;
        }
        let ty = props.get(&r).and_then(|p| p.get("type")).and_then(|t| t.as_str()).unwrap_or("string");
        if ty == "string" {
            args.insert(r, Value::String(to_text(&primary_value)));
        }
    }
    Value::Object(args)
}

fn coerce(v: &Value, prop: Option<&Value>) -> Value {
    let ty = prop.and_then(|p| p.get("type")).and_then(|t| t.as_str()).unwrap_or("");
    match (ty, v) {
        ("string", Value::String(_)) => v.clone(),
        ("string", Value::Array(a)) => Value::String(a.iter().map(to_text).collect::<Vec<_>>().join(", ")),
        ("string", _) => Value::String(to_text(v)),
        ("integer" | "number", Value::String(s)) => s.trim().parse::<i64>().map(Value::from).unwrap_or_else(|_| v.clone()),
        ("array", Value::String(s)) => json!([s]),
        _ => v.clone(),
    }
}

// ----------------------------------------------------------------------------------------- HTTP

pub struct HttpDriver {
    name: String,
    base_url: Option<String>,
    headers: Vec<(String, String)>,
    agent: ureq::Agent,
}

impl HttpDriver {
    pub fn new(name: &str, cfg: &AdapterCfg, ctx: &Value) -> HttpDriver {
        let agent: ureq::Agent = ureq::Agent::config_builder().http_status_as_error(false).build().into();
        HttpDriver {
            name: name.to_string(),
            base_url: cfg.base_url.as_ref().or(cfg.url.as_ref()).map(|u| render_str(u, ctx).trim_end_matches('/').to_string()),
            headers: cfg.headers.iter().map(|(k, v)| (k.clone(), render_str(v, ctx))).collect(),
            agent,
        }
    }
}

impl Driver for HttpDriver {
    fn call(&self, op: &OpCfg, role: &str, ctx: &Value, timeout: Duration) -> Result<Value> {
        let mut url = match (&op.url, &op.path, &self.base_url) {
            (Some(u), _, _) => render_str(u, ctx),
            (None, Some(p), Some(base)) => format!("{base}{}", render_str(p, ctx)),
            (None, None, Some(base)) => base.clone(),
            _ => bail!("adapter '{}': op '{role}' needs `url`, or `path` with a `base_url`", self.name),
        };
        if !op.query.is_empty() {
            let mut first = !url.contains('?');
            for (k, v) in &op.query {
                let val = render_str(v, ctx);
                if val.is_empty() {
                    continue;
                }
                url.push(if first { '?' } else { '&' });
                first = false;
                url.push_str(&format!("{}={}", crate::template::urlencode(k), crate::template::urlencode(&val)));
            }
        }
        let method = op.method.clone().unwrap_or_else(|| if op.body.is_some() { "POST".into() } else { "GET".into() }).to_uppercase();
        let body = op.body.as_ref().map(|b| render_value(b, ctx));
        let mut headers: Vec<(String, String)> = self.headers.clone();
        headers.extend(op.headers.iter().map(|(k, v)| (k.clone(), render_str(v, ctx))));

        macro_rules! finish {
            ($req:expr) => {{
                let mut req = $req.config().timeout_global(Some(timeout)).build();
                for (k, v) in &headers {
                    req = req.header(k.as_str(), v.as_str());
                }
                req
            }};
        }
        let resp = match method.as_str() {
            "GET" | "DELETE" | "HEAD" => {
                let req = match method.as_str() {
                    "GET" => self.agent.get(&url),
                    "HEAD" => self.agent.head(&url),
                    _ => self.agent.delete(&url),
                };
                finish!(req).call()
            }
            "POST" | "PUT" | "PATCH" => {
                let req = match method.as_str() {
                    "POST" => self.agent.post(&url),
                    "PUT" => self.agent.put(&url),
                    _ => self.agent.patch(&url),
                };
                let req = finish!(req);
                match &body {
                    Some(Value::String(s)) => req.send(s.as_str()),
                    Some(b) => req.header("Content-Type", "application/json").send(b.to_string()),
                    None => req.send_empty(),
                }
            }
            other => bail!("adapter '{}': unsupported HTTP method {other}", self.name),
        };
        let mut resp = resp.with_context(|| format!("adapter '{}': {method} {url}", self.name))?;
        let status = resp.status().as_u16();
        let text = resp.body_mut().read_to_string().unwrap_or_default();
        if status >= 400 {
            bail!("adapter '{}': HTTP {status} from {url}: {}", self.name, crate::util::truncate_chars(&text, 300));
        }
        Ok(parse_output(&text, op.format.as_deref()))
    }
}

// -------------------------------------------------------------------------------------- command

pub struct CommandDriver {
    name: String,
    base_command: Option<Vec<String>>,
    cwd: PathBuf,
    env: BTreeMap<String, String>,
}

impl CommandDriver {
    pub fn new(name: &str, cfg: &AdapterCfg, ctx: &Value, root: PathBuf) -> CommandDriver {
        CommandDriver {
            name: name.to_string(),
            base_command: cfg.command.clone(),
            cwd: cfg.cwd.as_ref().map(|c| PathBuf::from(render_str(c, ctx))).unwrap_or(root),
            env: cfg.env.iter().map(|(k, v)| (k.clone(), render_str(v, ctx))).collect(),
        }
    }
}

impl Driver for CommandDriver {
    fn call(&self, op: &OpCfg, role: &str, ctx: &Value, timeout: Duration) -> Result<Value> {
        let tmp = std::env::temp_dir().join(format!(
            "kontext-{}-{}-{}.out",
            self.name,
            std::process::id(),
            crate::util::short_hash(&format!("{:?}", Instant::now()))
        ));
        let mut ctx = ctx.clone();
        if op.output_file {
            ctx["output_file"] = Value::String(tmp.to_string_lossy().to_string());
        }
        let argv: Vec<String> = op
            .command
            .as_ref()
            .or(self.base_command.as_ref())
            .ok_or_else(|| anyhow!("adapter '{}': op '{role}' needs `command`", self.name))?
            .iter()
            .map(|a| render_str(a, &ctx))
            .collect();
        let program = argv.first().ok_or_else(|| anyhow!("empty command"))?;
        let cwd = op.cwd.as_ref().map(|c| PathBuf::from(render_str(c, &ctx))).unwrap_or_else(|| self.cwd.clone());
        let mut cmd = Command::new(program);
        cmd.args(&argv[1..]).current_dir(&cwd).envs(&self.env);
        for (k, v) in &op.env {
            cmd.env(k, render_str(v, &ctx));
        }
        let stdin_text = op.stdin.as_ref().map(|s| render_str(s, &ctx));
        cmd.stdin(if stdin_text.is_some() { Stdio::piped() } else { Stdio::null() }).stdout(Stdio::piped()).stderr(Stdio::piped());
        let mut child = cmd.spawn().with_context(|| format!("adapter '{}': cannot run `{program}`", self.name))?;
        if let (Some(text), Some(mut stdin)) = (stdin_text, child.stdin.take()) {
            std::thread::spawn(move || {
                let _ = stdin.write_all(text.as_bytes());
            });
        }
        let mut out = child.stdout.take().expect("stdout");
        let mut err = child.stderr.take().expect("stderr");
        let out_t = std::thread::spawn(move || {
            let mut s = Vec::new();
            let _ = out.read_to_end(&mut s);
            s
        });
        let err_t = std::thread::spawn(move || {
            let mut s = Vec::new();
            let _ = err.read_to_end(&mut s);
            s
        });
        let deadline = Instant::now() + timeout;
        let status = loop {
            if let Some(st) = child.try_wait()? {
                break st;
            }
            if Instant::now() > deadline {
                let _ = child.kill();
                let _ = child.wait();
                bail!("adapter '{}': `{program}` timed out after {}s", self.name, timeout.as_secs());
            }
            std::thread::sleep(Duration::from_millis(15));
        };
        let stdout = String::from_utf8_lossy(&out_t.join().unwrap_or_default()).to_string();
        let stderr = String::from_utf8_lossy(&err_t.join().unwrap_or_default()).to_string();
        if !status.success() {
            let msg = if stderr.trim().is_empty() { stdout.clone() } else { stderr };
            bail!("adapter '{}': `{program}` exited with {status}: {}", self.name, crate::util::truncate_chars(msg.trim(), 400));
        }
        let text = if op.output_file {
            let t = std::fs::read_to_string(&tmp).unwrap_or(stdout);
            let _ = std::fs::remove_file(&tmp);
            t
        } else {
            stdout
        };
        Ok(parse_output(&text, op.format.as_deref()))
    }
}

/// Parse a textual answer: JSON (tolerating leading log lines), JSON lines, plain lines or text.
pub fn parse_output(text: &str, format: Option<&str>) -> Value {
    match format.unwrap_or("auto") {
        "text" => Value::String(text.to_string()),
        "lines" => Value::Array(text.lines().filter(|l| !l.trim().is_empty()).map(|l| Value::String(l.to_string())).collect()),
        "jsonl" => Value::Array(text.lines().filter_map(|l| serde_json::from_str(l.trim()).ok()).collect()),
        _ => {
            let t = text.trim();
            if let Ok(v) = serde_json::from_str::<Value>(t) {
                return v;
            }
            // tolerate leading non-JSON lines (progress output, "cmd: …" echoes)
            for (i, _) in t.match_indices(['{', '[']) {
                if (i == 0 || t.as_bytes()[i - 1] == b'\n')
                    && let Ok(v) = serde_json::from_str::<Value>(&t[i..])
                {
                    return v;
                }
            }
            if format == Some("json") {
                return Value::Null;
            }
            Value::String(text.to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn autobinds_by_schema() {
        let schema = json!({"type": "object", "properties": {"query": {"type": "string"}, "limit": {"type": "number"}, "projectPath": {"type": "string"}}, "required": ["query"]});
        let ctx = json!({"query": "parser", "limit": 5, "repo": {"root": "/r"}});
        assert_eq!(autobind(&schema, &ctx, "search"), json!({"query": "parser", "limit": 5, "projectPath": "/r"}));
        let serena = json!({"properties": {"name_path_pattern": {"type": "string"}, "relative_path": {"type": "string"}}, "required": ["name_path_pattern"]});
        assert_eq!(autobind(&serena, &ctx, "code"), json!({"name_path_pattern": "parser"}));
        let why = json!({"properties": {"target": {"type": "string"}}, "required": ["target"]});
        assert_eq!(autobind(&why, &json!({"target": "src/a.rs"}), "history"), json!({"target": "src/a.rs"}));
        let odd = json!({"properties": {"needle": {"type": "string"}}, "required": ["needle"]});
        assert_eq!(autobind(&odd, &ctx, "search"), json!({"needle": "parser"}));
    }

    #[test]
    fn tolerant_output() {
        assert_eq!(parse_output("cmd: ov find x\n{\"ok\":true}", None), json!({"ok": true}));
        assert_eq!(parse_output("plain", None), json!("plain"));
        assert_eq!(parse_output("{\"a\":1}\n{\"a\":2}\n", Some("jsonl")), json!([{"a": 1}, {"a": 2}]));
    }
}
