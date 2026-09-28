//! `{{ path | filter }}` templating over a JSON context.
//!
//! * `{{repo.name}}`, `{{vars.user}}`, `{{env.HOME}}`, `{{query}}` …
//! * filters: `urlencode`, `json`, `lower`, `upper`, `slug`, `trim`, `first_line`, `join:<sep>`,
//!   `truncate:<n>`, `default:<literal>`
//! * A JSON string that is exactly one `{{expr}}` is replaced by the typed value (number, list, …).
//!   Object members that render to `null` are dropped, so optional arguments disappear.
use serde_json::{Map, Value};

pub fn render_str(tpl: &str, ctx: &Value) -> String {
    let mut out = String::with_capacity(tpl.len());
    let mut rest = tpl;
    while let Some(start) = rest.find("{{") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        match after.find("}}") {
            Some(end) => {
                let v = eval(&after[..end], ctx);
                out.push_str(&to_text(&v));
                rest = &after[end + 2..];
            }
            None => {
                out.push_str(&rest[start..]);
                rest = "";
            }
        }
    }
    out.push_str(rest);
    out
}

pub fn render_value(v: &Value, ctx: &Value) -> Value {
    match v {
        Value::String(s) => {
            let t = s.trim();
            if t.starts_with("{{") && t.ends_with("}}") && t[2..t.len() - 2].find("{{").is_none() && t.matches("}}").count() == 1 {
                return eval(&t[2..t.len() - 2], ctx);
            }
            Value::String(render_str(s, ctx))
        }
        Value::Array(a) => Value::Array(a.iter().map(|x| render_value(x, ctx)).filter(|x| !x.is_null()).collect()),
        Value::Object(o) => {
            let mut m = Map::new();
            for (k, x) in o {
                let r = render_value(x, ctx);
                if !r.is_null() {
                    m.insert(render_str(k, ctx), r);
                }
            }
            Value::Object(m)
        }
        other => other.clone(),
    }
}

pub fn to_text(v: &Value) -> String {
    match v {
        Value::Null => String::new(),
        Value::String(s) => s.clone(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        Value::Array(a) => a.iter().map(to_text).collect::<Vec<_>>().join(","),
        other => other.to_string(),
    }
}

fn eval(expr: &str, ctx: &Value) -> Value {
    let mut parts = split_filters(expr);
    let head = parts.remove(0);
    let mut v = lookup(head.trim(), ctx);
    for f in parts {
        v = apply_filter(f.trim_start(), v);
    }
    v
}

fn split_filters(expr: &str) -> Vec<&str> {
    // `|` separates filters; `default:a|b` is not supported on purpose (keep it simple)
    expr.split('|').collect()
}

pub fn lookup(path: &str, ctx: &Value) -> Value {
    if path.is_empty() {
        return Value::Null;
    }
    if let Some(name) = path.strip_prefix("env.") {
        return std::env::var(name).map(Value::String).unwrap_or(Value::Null);
    }
    if (path.starts_with('"') && path.ends_with('"') && path.len() >= 2)
        || (path.starts_with('\'') && path.ends_with('\'') && path.len() >= 2)
    {
        return Value::String(path[1..path.len() - 1].to_string());
    }
    let mut cur = ctx;
    for seg in path.split('.') {
        cur = match cur {
            Value::Object(m) => match m.get(seg) {
                Some(v) => v,
                None => return Value::Null,
            },
            Value::Array(a) => match seg.parse::<usize>().ok().and_then(|i| a.get(i)) {
                Some(v) => v,
                None => return Value::Null,
            },
            _ => return Value::Null,
        };
    }
    cur.clone()
}

fn apply_filter(f: &str, v: Value) -> Value {
    // an all-whitespace argument is meaningful (`join: `); otherwise trailing blanks are layout
    let (name, arg) = match f.split_once(':') {
        Some((n, a)) => (n.trim(), Some(if a.trim().is_empty() { a } else { a.trim_end() })),
        None => (f.trim(), None),
    };
    match name {
        "urlencode" => Value::String(urlencode(&to_text(&v))),
        "json" => Value::String(v.to_string()),
        "lower" => Value::String(to_text(&v).to_lowercase()),
        "upper" => Value::String(to_text(&v).to_uppercase()),
        "slug" => Value::String(crate::util::slugify(&to_text(&v), 80)),
        "trim" => Value::String(to_text(&v).trim().to_string()),
        // `after:://` → what follows the first separator (the whole value when absent); `before:` likewise
        "after" => {
            let t = to_text(&v);
            let sep = arg.unwrap_or("");
            Value::String(t.split_once(sep).map(|(_, r)| r.to_string()).unwrap_or(t))
        }
        "before" => {
            let t = to_text(&v);
            let sep = arg.unwrap_or("");
            Value::String(t.split_once(sep).map(|(l, _)| l.to_string()).unwrap_or(t))
        }
        "first_line" => Value::String(to_text(&v).lines().next().unwrap_or("").to_string()),
        "join" => {
            let sep = arg.unwrap_or(",");
            match v {
                Value::Array(a) => Value::String(a.iter().map(to_text).collect::<Vec<_>>().join(sep)),
                other => Value::String(to_text(&other)),
            }
        }
        "truncate" => {
            let n = arg.and_then(|a| a.trim().parse().ok()).unwrap_or(200);
            Value::String(crate::util::truncate_chars(&to_text(&v), n))
        }
        "default" => {
            let empty = match &v {
                Value::Null => true,
                Value::String(s) => s.is_empty(),
                Value::Array(a) => a.is_empty(),
                _ => false,
            };
            if empty {
                let a = arg.unwrap_or("").trim();
                if let Ok(n) = a.parse::<i64>() {
                    Value::from(n)
                } else if a == "true" || a == "false" {
                    Value::Bool(a == "true")
                } else {
                    Value::String(a.to_string())
                }
            } else {
                v
            }
        }
        "int" => match &v {
            Value::String(s) => s.trim().parse::<i64>().map(Value::from).unwrap_or(Value::Null),
            _ => v,
        },
        _ => v,
    }
}

pub fn urlencode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn renders_strings_and_types() {
        let ctx = json!({"query": "a b", "limit": 5, "repo": {"name": "x"}, "tags": ["t1", "t2"], "empty": ""});
        assert_eq!(render_str("q={{query|urlencode}}&n={{ limit }}", &ctx), "q=a%20b&n=5");
        assert_eq!(render_str("{{repo.name}}/{{missing}}!", &ctx), "x/!");
        assert_eq!(render_str("{{tags|join: }}", &ctx), "t1 t2");
        assert_eq!(render_str("{{empty|default:fallback}}", &ctx), "fallback");
        let body = json!({"query": "{{query}}", "limit": "{{limit}}", "opt": "{{nope}}", "s": "n={{limit}}", "arr": ["{{tags}}"]});
        let r = render_value(&body, &ctx);
        assert_eq!(r, json!({"query": "a b", "limit": 5, "s": "n=5", "arr": [["t1", "t2"]]}));
    }

    #[test]
    fn after_and_before() {
        let ctx = json!({"uri": "teamdocs://page-7#intro"});
        assert_eq!(render_str("{{uri|after:teamdocs://}}", &ctx), "page-7#intro");
        assert_eq!(render_str("{{uri|after:://|before:#}}", &ctx), "page-7");
        assert_eq!(render_str("{{uri|after:nope}}", &ctx), "teamdocs://page-7#intro");
    }

    #[test]
    fn env_and_literals() {
        // SAFETY: test-only, single-threaded use of a unique variable name
        unsafe { std::env::set_var("KONTEXT_TPL_TEST", "yes") };
        assert_eq!(render_str("{{env.KONTEXT_TPL_TEST}}", &json!({})), "yes");
        assert_eq!(render_str("{{'lit'|upper}}", &json!({})), "LIT");
    }
}
