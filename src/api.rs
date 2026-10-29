//! Versioned application boundary shared by CLI and MCP clients.
use crate::app::App;
use anyhow::Result;
use serde_json::{Value, json};
use std::fmt;

pub const OPERATIONS: &[&str] = &[
    "status",
    "setup",
    "entries.list",
    "entries.get",
    "entries.search",
    "entries.governing",
    "entries.export",
    "entries.related",
    "entries.drift",
    "entries.changed",
    "inbox.list",
    "inbox.drop",
    "inbox.verify",
    "drafts.capture",
    "drafts.stage",
    "drafts.extract",
    "drafts.formalize",
    "drafts.retire",
    "drafts.confirm",
    "drafts.verify",
    "drafts.revise",
    "drafts.extend",
    "guidance.propose",
    "operations.get",
];

/// Inputs an operation gained after it shipped, named `<operation>.<input>`, so a client can tell
/// an older binary that would ignore or refuse them. `status.capabilities` lists them after the
/// operations.
pub const FEATURES: &[&str] = &["drafts.stage.amends", "drafts.stage.extends"];

/// The operations and the later inputs this binary supports.
pub fn capabilities() -> Vec<&'static str> {
    OPERATIONS.iter().chain(FEATURES).copied().collect()
}

#[derive(Debug)]
struct ApiError {
    code: String,
    message: String,
}

impl fmt::Display for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}
impl std::error::Error for ApiError {}

/// The machine code of an error raised with [`error`], if it was.
pub fn error_code(err: &anyhow::Error) -> Option<&str> {
    err.downcast_ref::<ApiError>().map(|e| e.code.as_str())
}

pub fn error(code: &str, message: impl Into<String>) -> anyhow::Error {
    ApiError { code: code.into(), message: message.into() }.into()
}

pub fn failure(err: anyhow::Error) -> Value {
    let code = err.downcast_ref::<ApiError>().map(|e| e.code.as_str()).unwrap_or("operation_failed");
    json!({"schema_version": 1, "ok": false, "error": {"code": code, "message": format!("{err:#}")}})
}

fn dispatch(app: &App, operation: &str, input: &Value) -> Result<Value> {
    if !input.is_object() {
        return Err(error("invalid_input", "input must be a JSON object"));
    }
    if !OPERATIONS.contains(&operation) {
        return Err(error("unsupported_operation", format!("unknown operation '{operation}'")));
    }
    // Keep repository selection outside rendering: machine results never contain a text banner.
    if let Some(dir) = input.get("dir") {
        let dir = dir.as_str().ok_or_else(|| error("invalid_input", "dir must be a string"))?;
        let target = crate::route::open_dir(app, dir)?;
        let mut rest = input.clone();
        rest.as_object_mut().unwrap().remove("dir");
        return dispatch(&target, operation, &rest);
    }
    if matches!(operation, "drafts.capture" | "drafts.stage") && input.pointer("/entry/paths").is_some() {
        let paths = input
            .pointer("/entry/paths")
            .and_then(Value::as_array)
            .ok_or_else(|| error("invalid_input", "entry.paths must be an array of strings"))?;
        let paths: Vec<String> = paths
            .iter()
            .map(|path| path.as_str().map(String::from).ok_or_else(|| error("invalid_input", "entry.paths must contain strings")))
            .collect::<Result<_>>()?;
        let routed = crate::route::owner_of(app, &paths);
        if routed.ambiguous {
            return Err(error("repository_conflict", routed.notes.join("; ")));
        }
        let mut input = input.clone();
        input["entry"]["paths"] = json!(routed.paths);
        let mut result = call_in(routed.owner.as_deref().unwrap_or(app), operation, &input)?;
        if !routed.notes.is_empty() {
            result["routing_notes"] = json!(routed.notes);
        }
        return Ok(result);
    }
    call_in(app, operation, input)
}

fn call_in(app: &App, operation: &str, input: &Value) -> Result<Value> {
    let mut result = if operation == "status" || operation.starts_with("entries.") {
        crate::api_read::call(app, operation, input)?
    } else {
        crate::api_write::call(app, operation, input)?
    };
    result["repository"] = json!({"root":app.repo.root,"worktree":app.repo.worktree_key,
        "branch":app.repo.git_opt(&["symbolic-ref","--quiet","--short","HEAD"]),"revision":app.repo.head()});
    Ok(result)
}

pub fn execute(app: &App, operation: &str, input: &Value) -> Value {
    match dispatch(app, operation, input) {
        Ok(result) => json!({"schema_version": 1, "ok": true, "result": result}),
        Err(err) => failure(err),
    }
}

pub fn output_schema() -> Value {
    json!({"type": "object", "properties": {
        "schema_version": {"type": "integer", "const": 1},
        "ok": {"type": "boolean"},
        "result": {"type": "object"},
        "error": {"type": "object", "properties": {"code": {"type": "string"}, "message": {"type": "string"}}, "required": ["code", "message"]}
    }, "required": ["schema_version", "ok"], "oneOf": [
        {"properties": {"ok": {"const": true}}, "required": ["result"], "not": {"required": ["error"]}},
        {"properties": {"ok": {"const": false}}, "required": ["error"], "not": {"required": ["result"]}}
    ]})
}
