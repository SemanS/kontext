//! `api.describe`: the JSON Schema of each operation's input, written by hand next to the
//! validators it describes. Within API v1 operations and optional inputs may be added; a client
//! ignores what it does not know.
use super::{OPERATIONS, error};
use anyhow::Result;
use serde_json::{Map, Value, json};

fn string(description: &str) -> Value {
    json!({"type": "string", "description": description})
}
fn strings(description: &str) -> Value {
    json!({"type": "array", "items": {"type": "string"}, "description": description})
}
fn integer(description: &str, min: u64, max: u64) -> Value {
    json!({"type": "integer", "minimum": min, "maximum": max, "description": description})
}
fn boolean(description: &str) -> Value {
    json!({"type": "boolean", "description": description})
}
fn llm() -> Value {
    json!({"type": ["boolean", "string"], "description": "false: no model; true: the configured model; a string: that llm adapter. No model is `model_unavailable`."})
}
fn revision(what: &str) -> Value {
    string(&format!(
        "{what}: the entry's blob id (`revision`, `publication.blob_hash`) or a full commit id it was read at. Never a ref name. Stale is `stale_entry`."
    ))
}

/// The `entry` object of a capture or stage.
fn entry() -> Value {
    json!({"type": "object", "description": "A finding: one line per metadata value; body is Markdown.", "properties": {
        "kind": string("decision, convention, learning, incident or a configured kind"),
        "title": string("A short statement"),
        "body": string("Markdown: what, why, consequences"),
        "summary": string("One line; else the body's first sentence"),
        "paths": strings("Repository-relative paths or globs it governs"),
        "tags": strings("Tags"),
        "status": string("proposed, accepted, …; a decision defaults to proposed"),
        "visibility": {"type": "string", "enum": ["team", "private"]},
        "supersedes": strings("Full ids of entries it replaces; unknown or ambiguous is not_found"),
        "extends": strings("Ids of entries it builds on; unknown is unknown_entry"),
        "commits": strings("Commits it came from"),
        "review_by": string("YYYY-MM-DD"),
        "verify": string("One shell command that proves it, run from the repository root"),
        "evidence": {"type": "array", "items": {"type": "object", "properties": {"ref": {"type": "string"}, "quote": {"type": "string"}}, "required": ["ref"]}},
        "id": string("The client's own id, kept as extra.external_id"),
        "extra": {"type": "object", "description": "Further metadata: strings or string arrays; reserved keys are refused", "additionalProperties": {"type": ["string", "array"]}}
    }})
}

fn object(properties: Value, required: &[&str]) -> Value {
    let mut properties = properties.as_object().cloned().unwrap_or_default();
    properties.insert("dir".into(), string("Answer from the repository or worktree containing this path"));
    json!({"type": "object", "properties": Value::Object(properties), "required": required})
}

/// The input schema of `operation`.
pub fn input_schema(operation: &str) -> Option<Value> {
    let operation_id = || string("A stable id the client keeps for this write: a retry replays the stored result");
    let fingerprint = || string("The client's fingerprint of the finding; a resume or lookup must repeat it");
    Some(match operation {
        "status" => object(json!({}), &[]),
        "setup" => object(json!({"hooks": boolean("Also install the git hooks (default false)")}), &[]),
        "entries.list" => object(
            json!({"limit": integer("Page size", 1, 500), "offset": integer("Entries to skip", 0, u64::MAX),
                "active_only": boolean("Only active entries (default true)"), "kinds": strings("Only these kinds"), "tags": strings("Only entries with one of these tags")}),
            &[],
        ),
        "entries.get" => object(json!({"id": string("An entry id, short id or file")}), &["id"]),
        "entries.search" => object(
            json!({"query": string("Words, an id or a path"), "limit": integer("At most", 1, 100), "active_only": boolean("Only active entries (default true)")}),
            &["query"],
        ),
        "entries.governing" => object(
            json!({"paths": strings("Repository-relative paths"), "exclude_ids": strings("Entries to leave out"),
                "limit": integer("At most", 1, 5000), "min_specificity": integer("Skip broader patterns", 0, 3)}),
            &["paths"],
        ),
        "entries.export" => object(
            json!({"revision": string("Full commit id: the current tip of review_ref"), "review_ref": string("refs/heads/… or refs/remotes/…"),
                "paths": strings("Only files under these paths")}),
            &["revision", "review_ref"],
        ),
        "entries.related" => object(
            json!({"text": string("The new text, up to 20000 bytes"), "paths": strings("Paths it governs"), "exclude_ids": strings("Entries to leave out"), "llm": llm()}),
            &["text"],
        ),
        "entries.drift" => object(
            json!({"base": string("Base revision (default: where this branch left the default branch)"), "head": string("Head revision (default: the working tree)"), "kinds": strings("Kinds")}),
            &[],
        ),
        "entries.changed" => object(
            json!({"base": string("Base revision"), "head": string("Head revision"), "kinds": strings("Kinds (default decision, convention, learning, incident)")}),
            &["base", "head"],
        ),
        "inbox.list" => object(json!({"state": {"type": "string", "enum": ["pending", "dismissed", "promoted", "all"]}}), &[]),
        "inbox.drop" => object(json!({"id": string("Inbox candidate id"), "reason": string("Why it is dropped")}), &["id"]),
        "inbox.verify" => object(
            json!({"id": string("Inbox candidate id"), "verify": string("The command; an empty string clears it")}),
            &["id", "verify"],
        ),
        "drafts.capture" => object(
            json!({"operation_id": operation_id(), "fingerprint": fingerprint(), "entry": entry(), "source": string("Where it was found")}),
            &["entry"],
        ),
        "drafts.stage" => object(
            json!({"operation_id": operation_id(), "fingerprint": fingerprint(), "entry": entry(),
                "candidate_id": string("An inbox candidate instead of entry"), "requested_path": string("The file to write, inside a kind directory"),
                "source": string("Where it was found"),
                "amends": {"type": "object", "description": "Amend this entry in place: entry.body must be its current body followed by `## Added <YYYY-MM-DD>` sections, else amend_not_additive",
                    "properties": {"id": string("The entry"), "revision": revision("What was read")}, "required": ["id", "revision"]}}),
            &["operation_id"],
        ),
        "drafts.extract" => object(
            json!({"text": string("Text to extract findings from"), "max": integer("At most", 1, 100), "source": string("Where it came from"), "llm": llm()}),
            &["text"],
        ),
        "drafts.formalize" => object(
            json!({"entry": entry(), "llm": llm(), "sources": strings("Source excerpts"), "context": string("More context")}),
            &["entry"],
        ),
        "drafts.retire" => object(
            json!({"operation_id": operation_id(), "entry_id": string("The entry"), "entry_revision": revision("What was read"), "reason": string("Why it no longer holds")}),
            &["operation_id", "entry_id", "entry_revision", "reason"],
        ),
        "drafts.confirm" => object(
            json!({"id": string("The entry"), "ids": strings("Several entries (more than five is a bulk stamp)"),
                "kind": {"type": "string", "enum": ["read", "agent"]}, "because": string("One line"), "repair": boolean("Repair checked dates behind the ledger"),
                "operation_id": operation_id()}),
            &[],
        ),
        "drafts.verify" => object(json!({"id": string("The entry"), "because": string("One line")}), &["id"]),
        "drafts.revise" => object(
            json!({"entry_id": string("The entry"), "entry_revision": revision("What was read"), "objections": strings("1 to 20 objections"), "llm": llm()}),
            &["entry_id", "entry_revision", "objections"],
        ),
        "drafts.extend" => object(
            json!({"entry_id": string("The entry"), "entry_revision": revision("What was read"), "text": string("The finding, added as written"), "llm": llm()}),
            &["entry_id", "entry_revision", "text"],
        ),
        "guidance.propose" => object(
            json!({"kept": strings("Kept findings"), "dismissed": strings("Dismissed findings"), "missed": strings("Missed findings"),
                "current": string("Current guidance"), "profile_tag": string("Tag for the proposed convention"), "llm": llm()}),
            &[],
        ),
        "operations.get" => {
            object(json!({"operation_id": string("The write's operation_id"), "fingerprint": fingerprint()}), &["operation_id"])
        }
        "api.describe" => object(json!({"operation": string("One operation; else all")}), &[]),
        _ => return None,
    })
}

/// `api.describe`: `{operation, input}` for one operation, else `{operations: {name: {input}}}`.
pub fn describe(input: &Value) -> Result<Value> {
    if let Some(operation) = input.get("operation").filter(|v| !v.is_null()) {
        let operation = operation.as_str().ok_or_else(|| error("invalid_input", "operation must be a string"))?;
        let schema = input_schema(operation).ok_or_else(|| error("unsupported_operation", format!("unknown operation '{operation}'")))?;
        return Ok(json!({"operation": operation, "input": schema}));
    }
    let mut operations = Map::new();
    for operation in OPERATIONS {
        if let Some(schema) = input_schema(operation) {
            operations.insert((*operation).into(), json!({"input": schema}));
        }
    }
    Ok(json!({"operations": operations}))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn every_operation_has_an_input_schema() {
        for operation in OPERATIONS {
            let schema = input_schema(operation).unwrap_or_else(|| panic!("no schema for {operation}"));
            assert_eq!(schema["type"], "object", "{operation}");
            assert!(schema["properties"]["dir"].is_object(), "{operation}");
        }
        assert!(describe(&json!({"operation": "nope"})).is_err());
        assert_eq!(describe(&json!({})).unwrap()["operations"].as_object().unwrap().len(), OPERATIONS.len());
    }
}
