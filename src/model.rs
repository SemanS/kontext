//! Shared result model: search hits from the local index and from adapters, merged and budgeted.
use crate::util;
use serde::Serialize;
use std::collections::{HashMap, HashSet};

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
    /// The URI of an earlier hit this one opens like: a look-alike shown under it, never dropped.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub similar_to: Option<String>,
}

pub struct HitGroup {
    pub source: String,
    pub weight: f32,
    pub hits: Vec<Hit>,
}

/// The whole text of a hit, normalized: equal only for copies of the same claim.
fn norm_text(s: &str) -> String {
    util::one_line(s).to_lowercase().chars().filter(|c| c.is_alphanumeric()).collect()
}

/// How a hit opens. Two claims with the same opening are not the same claim: this key groups
/// look-alikes so a reader sees both, it never decides that one of them goes.
fn opening(s: &str) -> String {
    norm_text(s).chars().take(90).collect()
}

/// Merge ranked lists from several sources with weighted reciprocal rank fusion: every source
/// contributes by rank, so no source's score scale (BM25 here, cosine similarity there) crowds the
/// others out, and a single source keeps its own order. Only proven copies are dropped: the same
/// URI, an adapter copy of a local entry (a `sync`ed entry comes back as `…/<id>.md`) and a hit
/// whose whole normalized text equals an earlier one. A hit that only opens like an earlier one is
/// kept and placed under it (`similar_to`), so the difference stays visible.
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
    let mut openings: HashMap<String, usize> = HashMap::new();
    let mut grouped: Vec<Vec<Hit>> = Vec::new();
    let mut kept = 0;
    for mut h in all {
        if kept >= limit {
            break;
        }
        if !h.uri.is_empty() && !seen_uri.insert(h.uri.clone()) {
            continue;
        }
        if mirrored_id(&h.uri).is_some_and(|id| local_ids.contains(id)) {
            continue;
        }
        let text = format!("{} {}", h.title, h.snippet);
        let full = norm_text(&text);
        if !full.is_empty() && !seen_text.insert(full) {
            continue;
        }
        let open = opening(&text);
        match openings.get(&open).filter(|_| !open.is_empty()) {
            Some(&g) => {
                h.similar_to = Some(grouped[g][0].uri.clone());
                grouped[g].push(h);
            }
            None => {
                openings.insert(open, grouped.len());
                grouped.push(vec![h]);
            }
        }
        kept += 1;
    }
    grouped.into_iter().flatten().collect()
}

/// The entry id an adapter URI names by its last segment (`viking://…/decision/<id>.md` → `<id>`).
fn mirrored_id(uri: &str) -> Option<&str> {
    if !uri.contains("://") {
        return None;
    }
    let last = uri.trim_end_matches('/').rsplit('/').next()?;
    Some(last.strip_suffix(".md").unwrap_or(last)).filter(|s| !s.is_empty())
}

/// The part of `h`'s text after the words it shares with `head`, from the first word that differs.
fn differing_tail(head: &Hit, h: &Hit) -> String {
    let a = util::one_line(&format!("{} {}", head.title, head.snippet));
    let b = util::one_line(&format!("{} {}", h.title, h.snippet));
    let (aw, bw): (Vec<&str>, Vec<&str>) = (a.split_whitespace().collect(), b.split_whitespace().collect());
    let same = aw.iter().zip(&bw).take_while(|(x, y)| x.eq_ignore_ascii_case(y)).count();
    let tail = bw[same.min(bw.len())..].join(" ");
    if tail.is_empty() {
        b
    } else if same > 0 {
        format!("… {tail}")
    } else {
        tail
    }
}

pub fn render(hits: &[Hit], budget_tokens: usize) -> String {
    if hits.is_empty() {
        return "No matches.".into();
    }
    let mut out = String::new();
    let mut used = 0;
    let mut n = 0;
    for (i, h) in hits.iter().enumerate() {
        let mut meta = Vec::new();
        if let Some(k) = &h.kind {
            meta.push(k.clone());
        }
        // an accepted entry is the norm and costs no tokens; anything else is said
        if let Some(s) = h.status.as_deref().filter(|s| *s != "accepted") {
            meta.push(s.to_string());
        }
        if let Some(d) = &h.date {
            meta.push(d.chars().take(10).collect());
        }
        if h.source != "local" {
            meta.push(h.source.clone());
        }
        let head = h.similar_to.as_ref().and_then(|uri| hits[..i].iter().find(|x| &x.uri == uri));
        let line = match head {
            // a look-alike: only what differs from the hit it sits under
            Some(head) => format!(
                "   ↳ similar, differs: {} [{}] <{}>\n",
                util::truncate_chars(&differing_tail(head, h), 200),
                meta.join(" · "),
                h.uri
            ),
            None => {
                n += 1;
                let snippet = util::truncate_chars(&util::one_line(&h.snippet), 240);
                let title = util::truncate_chars(&util::one_line(&h.title), 140);
                let body = if snippet.is_empty() || snippet == title { String::new() } else { format!(" — {snippet}") };
                format!("{n}. {title}{body} [{}] <{}>\n", meta.join(" · "), h.uri)
            }
        };
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
    fn same_opening_is_grouped_never_dropped() {
        let opening =
            "Webhook retries arrive twice — the provider sends the same webhook again when our answer takes longer than ten seconds";
        let mut a = hit("local", "kx:provider", "Webhook retries", 2.0);
        a.snippet = format!("{opening}, so ignore an event id already processed.");
        let mut b = hit("local", "kx:carrier", "Webhook retries", 1.0);
        b.snippet = format!("{opening}, and the carrier sends tracking events without an id.");
        let mut copy = hit("ov", "viking://notes/copy", "Webhook retries", 0.9);
        copy.snippet = a.snippet.clone();
        let merged = merge(
            vec![
                HitGroup { source: "local".into(), weight: 1.0, hits: vec![a, b] },
                HitGroup { source: "ov".into(), weight: 1.0, hits: vec![copy] },
            ],
            10,
        );
        let uris: Vec<&str> = merged.iter().map(|h| h.uri.as_str()).collect();
        assert_eq!(uris, vec!["kx:provider", "kx:carrier"], "the full-text copy collapses, the look-alike stays");
        assert_eq!(merged[1].similar_to.as_deref(), Some("kx:provider"));
        let text = render(&merged, 1000);
        assert!(text.contains("   ↳ similar, differs: … and the carrier sends tracking events without an id. [] <kx:carrier>"), "{text}");
        assert!(!text.contains("2."), "a look-alike is not numbered: {text}");
    }

    #[test]
    fn render_marks_everything_short_of_accepted() {
        let mut accepted = hit("local", "kx:a", "Accepted rule", 1.0);
        accepted.status = Some("accepted".into());
        let mut old = hit("local", "kx:b", "Old rule", 1.0);
        old.status = Some("deprecated".into());
        let text = render(&[accepted, old], 1000);
        assert!(text.contains("1. Accepted rule [] <kx:a>"), "{text}");
        assert!(text.contains("2. Old rule [deprecated] <kx:b>"), "{text}");
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
