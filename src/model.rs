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

/// Normalize scores per source (max = 1), apply weights, interleave, dedupe by URI and near-identical text.
pub fn merge(groups: Vec<HitGroup>, limit: usize) -> Vec<Hit> {
    let mut all = Vec::new();
    for g in groups {
        let max = g.hits.iter().map(|h| h.score).fold(0.0f32, f32::max);
        let n = g.hits.len().max(1) as f32;
        for (i, mut h) in g.hits.into_iter().enumerate() {
            if h.source.is_empty() {
                h.source = g.source.clone();
            }
            let norm = if max > 0.0 { h.score / max } else { 1.0 - (i as f32 / n) * 0.5 };
            h.score = norm * g.weight;
            all.push(h);
        }
    }
    all.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    let mut seen_uri = HashSet::new();
    let mut seen_text = HashSet::new();
    let mut out = Vec::new();
    for h in all {
        if !h.uri.is_empty() && !seen_uri.insert(h.uri.clone()) {
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
}
