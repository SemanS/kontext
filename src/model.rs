//! Shared result model: search hits from the local index and from adapters, merged and budgeted.
use crate::util;
use serde::Serialize;
use std::collections::HashSet;

#[derive(Debug, Clone, Serialize, Default)]
pub struct Hit {
    pub source: String,
    pub uri: String,
    pub title: String,
    pub snippet: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    pub score: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub date: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
}

pub struct HitGroup {
    pub source: String,
    pub weight: f32,
    pub hits: Vec<Hit>,
}

fn norm_key(s: &str) -> String {
    util::one_line(s).to_lowercase().chars().filter(|c| c.is_alphanumeric()).take(90).collect()
}

/// Merge ranked lists from several sources with weighted reciprocal rank fusion: every source
/// contributes by rank, so no source's score scale (BM25 here, cosine similarity there) crowds the
/// others out, and a single source keeps its own order. Dedupes by URI, near-identical text and
/// adapter copies of local entries (a `sync`ed entry comes back as `…/<id>.md`).
pub fn merge(groups: Vec<HitGroup>, limit: usize) -> Vec<Hit> {
    // small k: the top of each list matters most
    const K: f32 = 8.0;
    let mut all = Vec::new();
    for g in groups {
        for (i, mut h) in g.hits.into_iter().enumerate() {
            if h.source.is_empty() {
                h.source = g.source.clone();
            }
            h.score = g.weight / (K + 1.0 + i as f32);
            all.push(h);
        }
    }
    // stable: on equal scores the earlier group (local) stays first
    all.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    let local_ids: HashSet<String> = all.iter().filter_map(|h| h.uri.strip_prefix("kx:")).map(str::to_string).collect();
    let mut seen_uri = HashSet::new();
    let mut seen_text = HashSet::new();
    let mut out = Vec::new();
    for h in all {
        if !h.uri.is_empty() && !seen_uri.insert(h.uri.clone()) {
            continue;
        }
        if mirrored_id(&h.uri).is_some_and(|id| local_ids.contains(id)) {
            continue;
        }
        let key = norm_key(&format!("{} {}", h.title, h.snippet));
        if !key.is_empty() && !seen_text.insert(key) {
            continue;
        }
        out.push(h);
        if out.len() >= limit {
            break;
        }
    }
    out
}

/// The entry id an adapter URI names by its last segment (`viking://…/decision/<id>.md` → `<id>`).
fn mirrored_id(uri: &str) -> Option<&str> {
    if !uri.contains("://") {
        return None;
    }
    let last = uri.trim_end_matches('/').rsplit('/').next()?;
    Some(last.strip_suffix(".md").unwrap_or(last)).filter(|s| !s.is_empty())
}

pub fn render(hits: &[Hit], budget_tokens: usize) -> String {
    if hits.is_empty() {
        return "No matches.".into();
    }
    let mut out = String::new();
    let mut used = 0;
    for (i, h) in hits.iter().enumerate() {
        let mut meta = Vec::new();
        if let Some(k) = &h.kind {
            meta.push(k.clone());
        }
        if let Some(s) = &h.status {
            meta.push(s.clone());
        }
        if let Some(d) = &h.date {
            meta.push(d.chars().take(10).collect());
        }
        if h.source != "local" {
            meta.push(h.source.clone());
        }
        let snippet = util::truncate_chars(&util::one_line(&h.snippet), 240);
        let title = util::truncate_chars(&util::one_line(&h.title), 140);
        let body = if snippet.is_empty() || snippet == title { String::new() } else { format!(" — {snippet}") };
        let line = format!("{}. {title}{body} [{}] <{}>\n", i + 1, meta.join(" · "), h.uri);
        let cost = util::est_tokens(&line);
        if used + cost > budget_tokens && i > 0 {
            out.push_str(&format!("(+{} more — narrow the query or raise budget_tokens)\n", hits.len() - i));
            break;
        }
        used += cost;
        out.push_str(&line);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hit(src: &str, uri: &str, title: &str, score: f32) -> Hit {
        Hit { source: src.into(), uri: uri.into(), title: title.into(), score, ..Default::default() }
    }

    #[test]
    fn merges_and_dedupes() {
        let a = HitGroup {
            source: "local".into(),
            weight: 1.0,
            hits: vec![hit("local", "kx:a", "Alpha", 10.0), hit("local", "kx:b", "Beta", 5.0)],
        };
        let b = HitGroup {
            source: "ov".into(),
            weight: 1.0,
            hits: vec![hit("ov", "viking://x", "alpha", 0.9), hit("ov", "kx:b", "Beta again", 0.1)],
        };
        let m = merge(vec![a, b], 10);
        let uris: Vec<_> = m.iter().map(|h| h.uri.as_str()).collect();
        assert_eq!(uris, vec!["kx:a", "kx:b"], "same-text and same-uri hits collapse");
        assert!(render(&m, 1000).contains("1. Alpha"));
    }

    #[test]
    fn fusion_interleaves_sources() {
        // BM25 falls off steeply, semantic scores sit close together: neither may crowd out the other
        let local = HitGroup {
            source: "local".into(),
            weight: 1.0,
            hits: (0..6).map(|i| hit("local", &format!("kx:l{i}"), &format!("local {i}"), 12.0 / (i as f32 + 1.0))).collect(),
        };
        let ov = HitGroup {
            source: "ov".into(),
            weight: 0.9,
            hits: (0..6).map(|i| hit("ov", &format!("viking://p/r/v{i}.md"), &format!("remote {i}"), 0.62 - i as f32 * 0.01)).collect(),
        };
        let m = merge(vec![local, ov], 8);
        let local_n = m.iter().filter(|h| h.source == "local").count();
        assert!((3..=5).contains(&local_n), "{:?}", m.iter().map(|h| &h.uri).collect::<Vec<_>>());
        assert_eq!(m[0].uri, "kx:l0");
    }

    #[test]
    fn adapter_copies_of_local_entries_collapse() {
        let local = HitGroup { source: "local".into(), weight: 1.0, hits: vec![hit("local", "kx:use-postgres", "Use Postgres", 3.0)] };
        let ov = HitGroup {
            source: "ov".into(),
            weight: 0.9,
            hits: vec![
                hit("ov", "viking://user/u/peers/p/resources/kontext/decision/use-postgres.md", "viking copy", 0.9),
                hit("ov", "viking://user/u/peers/p/memories/other.md", "other memory", 0.8),
            ],
        };
        let uris: Vec<String> = merge(vec![local, ov], 10).into_iter().map(|h| h.uri).collect();
        assert_eq!(uris, vec!["kx:use-postgres".to_string(), "viking://user/u/peers/p/memories/other.md".to_string()]);
    }
}
