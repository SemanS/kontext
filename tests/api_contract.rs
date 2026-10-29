//! The shape of every API v1 operation, held to a snapshot: each operation `status.capabilities`
//! lists runs on the eval fixture (`tests/eval/fixtures/shop`) and its response is reduced to key
//! paths and JSON types (`result.entries[].id: string`). Values are not compared, only the shape a
//! client parses. A removed or retyped field fails; a new field fails too, so an additive change is
//! a reviewed snapshot update rather than a silent one.
//!
//! `tests/api_contract.snap` is created on the first run when it is missing. After an intended
//! change: `KONTEXT_UPDATE_CONTRACT=1 cargo test --test api_contract`, then review the diff.
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;
use std::process::Stdio;

mod common;
use common::Sandbox;

const DECISION: &str = "2026-01-10-orders-live-in-postgresql";

/// The input an operation runs with; `{head}` is HEAD's commit, `{candidate}` an inbox candidate.
fn input_for(operation: &str) -> Value {
    let entry = json!({"kind":"learning","title":"Contract probe","body":"A probe of the response shape.","paths":["src/orders/**"]});
    match operation {
        "status" | "setup" | "inbox.list" => json!({}),
        "entries.list" => json!({"limit": 3}),
        "entries.get" => json!({"id": DECISION}),
        "entries.search" => json!({"query": "orders postgresql", "limit": 3}),
        "entries.governing" => json!({"paths": ["src/orders/mod.rs"]}),
        "entries.export" => json!({"revision": "{head}", "review_ref": "{head}", "paths": ["src/orders/**"]}),
        "entries.related" => json!({"text": "Orders are stored in PostgreSQL", "paths": ["src/orders/**"]}),
        "entries.drift" => json!({"base": "{head}"}),
        "entries.changed" => json!({"base": "{head}", "head": "{head}"}),
        "inbox.drop" => json!({"id": "{candidate}", "reason": "contract probe"}),
        "inbox.verify" => json!({"id": "{candidate}", "verify": "true"}),
        "drafts.capture" => json!({"entry": entry}),
        "drafts.stage" => json!({"entry": entry, "operation_id": "contract-stage"}),
        "drafts.extract" => json!({"text": "We decided to keep orders in PostgreSQL."}),
        "drafts.formalize" => json!({"entry": entry}),
        "drafts.retire" => {
            json!({"operation_id": "contract-retire", "entry_id": DECISION, "entry_revision": "{head}", "reason": "contract probe"})
        }
        "drafts.confirm" => json!({"id": DECISION}),
        "drafts.verify" => json!({"id": DECISION}),
        "drafts.revise" => json!({"entry_id": DECISION, "entry_revision": "{head}", "objections": ["Orders moved."]}),
        "drafts.extend" => json!({"entry_id": DECISION, "entry_revision": "{head}", "text": "Reads go to a replica."}),
        "guidance.propose" => json!({"kept": [], "dismissed": [], "missed": []}),
        "operations.get" => json!({"operation_id": "contract-stage"}),
        _ => json!({}),
    }
}

fn api(sb: &Sandbox, operation: &str, input: &Value) -> Value {
    let mut child =
        sb.kontext_cmd().args(["api", operation, "-"]).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap();
    child.stdin.take().unwrap().write_all(input.to_string().as_bytes()).unwrap();
    let out = child.wait_with_output().unwrap();
    let value: Value = serde_json::from_slice(&out.stdout).unwrap_or_else(|e| {
        panic!("{operation}: {e}: stdout={} stderr={}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr))
    });
    assert_eq!(out.status.success(), value["ok"] == true, "{operation}: exit status and ok disagree: {value}");
    value
}

fn substitute(v: &Value, vars: &BTreeMap<&str, String>) -> Value {
    match v {
        Value::String(s) => {
            let mut s = s.clone();
            for (k, val) in vars {
                s = s.replace(k, val);
            }
            Value::String(s)
        }
        Value::Array(a) => Value::Array(a.iter().map(|x| substitute(x, vars)).collect()),
        Value::Object(o) => Value::Object(o.iter().map(|(k, x)| (k.clone(), substitute(x, vars))).collect()),
        other => other.clone(),
    }
}

fn type_name(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(n) if n.is_f64() => "number",
        Value::Number(_) => "integer",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

/// Every key path with the JSON types seen there; array elements share the path `[]`.
fn shape(v: &Value, path: &str, out: &mut BTreeMap<String, BTreeSet<&'static str>>) {
    out.entry(path.to_string()).or_default().insert(type_name(v));
    match v {
        Value::Object(o) => {
            for (k, x) in o {
                let key = if path.is_empty() { k.clone() } else { format!("{path}.{k}") };
                shape(x, &key, out);
            }
        }
        Value::Array(a) => {
            for x in a {
                shape(x, &format!("{path}[]"), out);
            }
        }
        _ => {}
    }
}

/// An operation, as opposed to a later input of one (`drafts.stage.amends` extends `drafts.stage`).
fn operations(capabilities: &[String]) -> Vec<String> {
    capabilities.iter().filter(|c| !capabilities.iter().any(|o| o != *c && c.starts_with(&format!("{o}.")))).cloned().collect()
}

fn render(op: &str, response: &Value) -> String {
    let mut paths = BTreeMap::new();
    shape(response, "", &mut paths);
    paths.remove("");
    let mut out = format!("## {op}\n");
    for (path, types) in paths {
        out.push_str(&format!("{path}: {}\n", types.iter().copied().collect::<Vec<_>>().join(" | ")));
    }
    out
}

#[test]
fn every_operation_keeps_its_response_shape() {
    let probe = Sandbox::fixture("shop");
    let status = api(&probe, "status", &json!({}));
    let capabilities: Vec<String> = status["result"]["capabilities"]
        .as_array()
        .expect("status.capabilities")
        .iter()
        .filter_map(|c| c.as_str().map(String::from))
        .collect();
    drop(probe);

    let mut snapshot = String::from("# API v1 response shapes on tests/eval/fixtures/shop (tests/api_contract.rs).\n");
    snapshot.push_str(&format!("# capabilities: {}\n", capabilities.join(", ")));
    for op in operations(&capabilities) {
        // a fresh fixture per operation: a write never changes what the next one sees
        let sb = Sandbox::fixture("shop");
        let mut vars = BTreeMap::new();
        vars.insert("{head}", sb.git_dated(&["rev-parse", "HEAD"], ""));
        if op.starts_with("inbox.") && op != "inbox.list" {
            let captured =
                api(&sb, "drafts.capture", &json!({"entry": {"kind":"learning","title":"Candidate","body":"For the contract test."}}));
            let id = captured["result"]["candidate"]["id"].as_str().or_else(|| captured["result"]["id"].as_str()).unwrap_or("").to_string();
            vars.insert("{candidate}", id);
        }
        if op == "operations.get" {
            api(&sb, "drafts.stage", &input_for("drafts.stage"));
        }
        let response = api(&sb, &op, &substitute(&input_for(&op), &vars));
        snapshot.push('\n');
        snapshot.push_str(&render(&op, &response));
    }

    let path = common::manifest_dir().join("tests/api_contract.snap");
    let update = std::env::var("KONTEXT_UPDATE_CONTRACT").is_ok_and(|v| !v.is_empty() && v != "0");
    match std::fs::read_to_string(&path) {
        Ok(stored) if !update => {
            if stored != snapshot {
                let actual = path.with_extension("snap.new");
                let _ = std::fs::write(&actual, &snapshot);
                let old: BTreeSet<&str> = stored.lines().collect();
                let new: BTreeSet<&str> = snapshot.lines().collect();
                let mut diff: Vec<String> = old.difference(&new).map(|l| format!("- {l}")).collect();
                diff.extend(new.difference(&old).map(|l| format!("+ {l}")));
                panic!(
                    "API response shapes changed (written to {}); if intended, run KONTEXT_UPDATE_CONTRACT=1 cargo test --test api_contract:\n{}",
                    actual.display(),
                    diff.join("\n")
                );
            }
        }
        _ => {
            std::fs::write(&path, &snapshot).unwrap();
            println!("wrote {}", path.display());
        }
    }
}

#[test]
fn shapes_are_key_paths_and_types() {
    let mut out = BTreeMap::new();
    shape(&json!({"ok": true, "result": {"entries": [{"id": "a", "n": 1}, {"id": "b", "x": null}]}}), "", &mut out);
    assert_eq!(out["result.entries[].id"].iter().copied().collect::<Vec<_>>(), ["string"]);
    assert!(out.contains_key("result.entries[].n") && out.contains_key("result.entries[].x"));
    let caps = ["drafts.stage".to_string(), "drafts.stage.amends".to_string(), "status".to_string()];
    assert_eq!(operations(&caps), ["drafts.stage", "status"]);
}
