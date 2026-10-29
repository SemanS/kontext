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

/// How much an inactive (superseded or deprecated) hit counts in the fusion: it stays in the
/// results, marked, but below the active knowledge that replaced it. In reciprocal rank fusion a
/// factor of 0.5 sends even a source's best hit below its twentieth, out of every default result
/// list, which is hiding it; 0.8 moves it a few places down.
pub const INACTIVE_WEIGHT: f32 = 0.8;

/// Whether a hit's status says it is no longer the team's rule.
pub fn is_inactive(status: Option<&str>) -> bool {
    let first = status.unwrap_or("").split([' ', '·']).next().unwrap_or("");
    matches!(first, "superseded" | "deprecated" | "rejected")
}

/// Merge ranked lists from several sources with weighted reciprocal rank fusion: every source
/// contributes by rank, so no source's score scale (BM25 here, cosine similarity there) crowds the
/// others out, and a single source keeps its own order. Only proven copies are dropped: the same
/// URI, an adapter copy of a local entry (a `sync`ed entry comes back as `…/<kind>/<id>.md`) and a
/// hit whose whole normalized text equals an earlier one. A hit that only opens like an earlier one is
/// kept and placed under it (`similar_to`), so the difference stays visible.
#[allow(dead_code)] // the plain form, for callers without digests
pub fn merge(groups: Vec<HitGroup>, limit: usize) -> Vec<Hit> {
    merge_with(groups, limit, &HashMap::new())
}

/// `merge`, where `digests` maps a hit's URI to a digest of its whole text (the index stores one per
/// document). Two hits that both carry a digest are copies only when the digests are equal; a hit
/// without one (an adapter's) is compared by its title and snippet. Of two copies that differ in
/// status, the active one is kept. Superseded and deprecated hits count `INACTIVE_WEIGHT`.
pub fn merge_with(groups: Vec<HitGroup>, limit: usize, digests: &HashMap<String, String>) -> Vec<Hit> {
    // small k: the top of each list matters most
    const K: f32 = 8.0;
    let mut all = Vec::new();
    for g in groups {
        for (i, mut h) in g.hits.into_iter().enumerate() {
            if h.source.is_empty() {
                h.source = g.source.clone();
            }
            let demote = if is_inactive(h.status.as_deref()) { INACTIVE_WEIGHT } else { 1.0 };
            h.score = g.weight * demote / (K + 1.0 + i as f32);
            all.push(h);
        }
    }
    // stable: on equal scores the earlier group (local) stays first
    all.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    let local_kinds: HashMap<String, Option<String>> =
        all.iter().filter_map(|h| h.uri.strip_prefix("kx:").map(|id| (id.to_string(), h.kind.clone()))).collect();
    let mut seen_uri = HashSet::new();
    // where each kept text sits in `grouped`, and whether it carried a digest
    let mut seen_text: HashMap<String, ((usize, usize), bool)> = HashMap::new();
    let mut seen_digest: HashMap<String, (usize, usize)> = HashMap::new();
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
        if mirrored_id(&h.uri).is_some_and(|id| local_kinds.get(id).is_some_and(|kind| mirrors_kind(&h.uri, kind.as_deref()))) {
            continue;
        }
        let text = format!("{} {}", h.title, h.snippet);
        let full = norm_text(&text);
        let digest = digests.get(&h.uri).filter(|d| !d.is_empty()).cloned();
        let copy_of = match &digest {
            Some(d) => seen_digest.get(d).copied().or_else(|| seen_text.get(&full).filter(|(_, had)| !had).map(|(at, _)| *at)),
            None => seen_text.get(&full).map(|(at, _)| *at),
        };
        if let Some((g, i)) = copy_of {
            // copies of one text: the active one stands where the first copy stood
            if is_inactive(grouped[g][i].status.as_deref()) && !is_inactive(h.status.as_deref()) {
                let old_uri = grouped[g][i].uri.clone();
                h.score = grouped[g][i].score;
                h.similar_to = grouped[g][i].similar_to.take();
                let new_uri = h.uri.clone();
                grouped[g][i] = h;
                if i == 0 {
                    for other in grouped[g].iter_mut().skip(1) {
                        if other.similar_to.as_deref() == Some(old_uri.as_str()) {
                            other.similar_to = Some(new_uri.clone());
                        }
                    }
                }
            }
            continue;
        }
        let open = opening(&text);
        let at = match openings.get(&open).filter(|_| !open.is_empty()) {
            Some(&g) => {
                h.similar_to = Some(grouped[g][0].uri.clone());
                grouped[g].push(h);
                (g, grouped[g].len() - 1)
            }
            None => {
                openings.insert(open, grouped.len());
                grouped.push(vec![h]);
                (grouped.len() - 1, 0)
            }
        };
        if !full.is_empty() {
            seen_text.entry(full).or_insert((at, digest.is_some()));
        }
        if let Some(d) = digest {
            seen_digest.entry(d).or_insert(at);
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

/// Whether an adapter URI sits in the directory of the local entry's kind (`…/decision/<id>.md`),
/// as a synced copy does. A file that only shares the id's name (`…/memories/overview.md`) is not
/// a copy; nor is anything when the local hit's kind is unknown.
fn mirrors_kind(uri: &str, kind: Option<&str>) -> bool {
    let Some(kind) = kind.filter(|k| !k.is_empty()) else { return false };
    let mut segs = uri.trim_end_matches('/').rsplit('/');
    segs.next();
    let Some(dir) = segs.next() else { return false };
    dir == kind || dir.strip_suffix('s') == Some(kind)
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
        let mut local_hit = hit("local", "kx:use-postgres", "Use Postgres", 3.0);
        local_hit.kind = Some("decision".into());
        let mut overview = hit("local", "kx:overview", "Shop", 2.0);
        overview.kind = Some("architecture".into());
        let local = HitGroup { source: "local".into(), weight: 1.0, hits: vec![local_hit, overview] };
        let ov = HitGroup {
            source: "ov".into(),
            weight: 0.9,
            hits: vec![
                hit("ov", "viking://user/u/peers/p/resources/kontext/decision/use-postgres.md", "viking copy", 0.9),
                hit("ov", "viking://user/u/peers/p/memories/other.md", "other memory", 0.8),
                // named like a local id, but not in its kind directory: an unrelated memory
                hit("ov", "viking://user/u/peers/p/memories/overview.md", "team overview memory", 0.7),
            ],
        };
        let uris: Vec<String> = merge(vec![local, ov], 10).into_iter().map(|h| h.uri).collect();
        assert_eq!(
            uris,
            vec![
                "kx:use-postgres".to_string(),
                "kx:overview".to_string(),
                "viking://user/u/peers/p/memories/other.md".to_string(),
                "viking://user/u/peers/p/memories/overview.md".to_string(),
            ]
        );
    }

    #[test]
    fn same_title_and_summary_with_different_bodies_both_stay() {
        let mut a = hit("local", "kx:backoff", "Frobnicator retries use jittered backoff", 2.0);
        a.snippet = "Frobnicator retries use jittered backoff".into();
        let mut b = hit("local", "kx:backoff-2", "Frobnicator retries use jittered backoff", 1.0);
        b.snippet = a.snippet.clone();
        let digests: HashMap<String, String> =
            [("kx:backoff".to_string(), "d1".to_string()), ("kx:backoff-2".to_string(), "d2".to_string())].into_iter().collect();
        let m = merge_with(vec![HitGroup { source: "local".into(), weight: 1.0, hits: vec![a, b] }], 10, &digests);
        let uris: Vec<&str> = m.iter().map(|h| h.uri.as_str()).collect();
        assert_eq!(uris, vec!["kx:backoff", "kx:backoff-2"], "different whole texts are not copies");
        assert_eq!(m[1].similar_to.as_deref(), Some("kx:backoff"), "shown under the first, never dropped");
    }

    #[test]
    fn of_two_copies_the_active_one_stays() {
        let mut old = hit("local", "kx:prices-old", "Prices are integer cents", 2.0);
        old.snippet = "Store prices as integer cents.".into();
        old.status = Some("superseded".into());
        let mut new = hit("local", "kx:prices-new", "Prices are integer cents", 1.0);
        new.snippet = old.snippet.clone();
        new.status = Some("accepted".into());
        let digests: HashMap<String, String> =
            [("kx:prices-old".to_string(), "same".to_string()), ("kx:prices-new".to_string(), "same".to_string())].into_iter().collect();
        // the superseded copy ranks first in its source and is demoted, but even at equal rank the copy kept is the active one
        let m = merge_with(vec![HitGroup { source: "local".into(), weight: 1.0, hits: vec![old.clone(), new.clone()] }], 10, &digests);
        assert_eq!(m.iter().map(|h| h.uri.as_str()).collect::<Vec<_>>(), vec!["kx:prices-new"]);
        let m = merge_with(
            vec![
                HitGroup { source: "local".into(), weight: 1.0, hits: vec![old] },
                HitGroup { source: "sub/".into(), weight: 0.1, hits: vec![new] },
            ],
            10,
            &digests,
        );
        assert_eq!(
            m.iter().map(|h| h.uri.as_str()).collect::<Vec<_>>(),
            vec!["kx:prices-new"],
            "the active copy replaces the inactive one"
        );
    }

    #[test]
    fn inactive_hits_are_demoted_never_hidden() {
        let mut old = hit("local", "kx:old", "Old rule about caching", 9.0);
        old.status = Some("superseded".into());
        let mut dep = hit("local", "kx:dep", "Deprecated rule about caching pages", 8.0);
        dep.status = Some("deprecated".into());
        let mut new = hit("local", "kx:new", "New rule about cache lifetimes", 1.0);
        new.status = Some("accepted".into());
        let m = merge(vec![HitGroup { source: "local".into(), weight: 1.0, hits: vec![old, dep, new] }], 10);
        let uris: Vec<&str> = m.iter().map(|h| h.uri.as_str()).collect();
        assert_eq!(uris, vec!["kx:new", "kx:old", "kx:dep"]);
        assert!(is_inactive(Some("superseded · review due")));
        assert!(!is_inactive(Some("accepted")));
        assert!(!is_inactive(None));
    }

    #[test]
    fn mirror_needs_the_kind_directory() {
        assert!(mirrors_kind("viking://r/kontext/decision/use-postgres.md", Some("decision")));
        assert!(mirrors_kind("viking://r/kontext/decisions/use-postgres.md", Some("decision")));
        assert!(!mirrors_kind("viking://r/memories/use-postgres.md", Some("decision")));
        assert!(!mirrors_kind("viking://r/decision/use-postgres.md", None));
    }
}
