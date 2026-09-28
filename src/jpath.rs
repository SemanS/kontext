//! Tiny JSON path used by adapter result mappings: `result.*[*]`, `items[0].title`, `a|b` fallbacks.
use serde_json::Value;

#[derive(Debug, PartialEq)]
enum Seg {
    Key(String),
    Index(usize),
    Wild,
}

fn parse(path: &str) -> Vec<Seg> {
    let mut segs = Vec::new();
    let path = path.trim().trim_start_matches('$').trim_start_matches('.');
    for part in path.split('.') {
        if part.is_empty() {
            continue;
        }
        let mut rest = part;
        if let Some(b) = rest.find('[') {
            let key = &rest[..b];
            if !key.is_empty() {
                segs.push(if key == "*" { Seg::Wild } else { Seg::Key(key.to_string()) });
            }
            rest = &rest[b..];
            while let Some(stripped) = rest.strip_prefix('[') {
                let Some(end) = stripped.find(']') else { break };
                let inner = stripped[..end].trim();
                if inner == "*" {
                    segs.push(Seg::Wild);
                } else if let Ok(i) = inner.parse() {
                    segs.push(Seg::Index(i));
                } else {
                    segs.push(Seg::Key(inner.trim_matches(['"', '\'']).to_string()));
                }
                rest = &stripped[end + 1..];
            }
        } else if rest == "*" {
            segs.push(Seg::Wild);
        } else {
            segs.push(Seg::Key(rest.to_string()));
        }
    }
    segs
}

pub fn select<'a>(v: &'a Value, path: &str) -> Vec<&'a Value> {
    let mut cur = vec![v];
    for seg in parse(path) {
        let mut next = Vec::new();
        for c in cur {
            match (&seg, c) {
                (Seg::Key(k), Value::Object(m)) => next.extend(m.get(k)),
                (Seg::Index(i), Value::Array(a)) => next.extend(a.get(*i)),
                (Seg::Wild, Value::Array(a)) => next.extend(a.iter()),
                (Seg::Wild, Value::Object(m)) => next.extend(m.values()),
                _ => {}
            }
        }
        cur = next;
    }
    cur
}

/// First non-null match; `a|b|c` tries alternatives in order.
pub fn first<'a>(v: &'a Value, path: &str) -> Option<&'a Value> {
    for alt in path.split('|') {
        if let Some(found) = select(v, alt).into_iter().find(|x| !x.is_null()) {
            return Some(found);
        }
    }
    None
}

pub fn as_text(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn selects() {
        let v = json!({"result": {"memories": [{"uri": "a"}], "resources": [{"uri": "b"}], "total": 2}});
        let uris: Vec<_> = select(&v, "result.*[*]").iter().map(|x| x["uri"].as_str().unwrap().to_string()).collect();
        assert_eq!(uris, vec!["a", "b"]);
        assert_eq!(first(&v, "result.memories[0].uri").unwrap(), "a");
        assert_eq!(first(&v, "nope|result.total").unwrap(), 2);
        assert_eq!(select(&v, "$").len(), 1);
        assert_eq!(select(&json!([1, 2, 3]), "[*]").len(), 3);
    }
}
