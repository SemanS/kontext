//! A bounded, shared history scan for decisions whose governed code may have moved on.
use crate::{glob::path_matches, repo::Repo, store::Entry};
use chrono::NaiveDate;

const HISTORY_LIMIT: &str = "5000";

fn cutoff(entry: &Entry) -> Option<i64> {
    if entry.kind != "decision" || !entry.is_active() || !entry.paths.iter().any(|p| !p.trim().is_empty()) {
        return None;
    }
    let date = entry.date.as_deref()?;
    if date.len() != 10 {
        return None;
    }
    NaiveDate::parse_from_str(date, "%Y-%m-%d").ok()?.succ_opt()?.and_hms_opt(0, 0, 0).map(|d| d.and_utc().timestamp())
}

struct Commit<'a> {
    time: i64,
    paths: Vec<&'a str>,
}

fn parse_history(history: &str) -> Vec<Commit<'_>> {
    let mut commits: Vec<Commit<'_>> = Vec::new();
    // A NUL-prefixed timestamp marks a commit; paths are NUL-terminated and cannot be empty.
    // Git inserts one newline before the first path. Preserve any newline in the path itself.
    let mut header = true;
    let mut first_path = false;
    for field in history.split('\0') {
        if field.is_empty() {
            header = true;
        } else if header {
            if let Ok(time) = field.parse() {
                commits.push(Commit { time, paths: Vec::new() });
                first_path = true;
            }
            header = false;
        } else if let Some(commit) = commits.last_mut() {
            let path = if first_path { field.strip_prefix('\n').unwrap_or(field) } else { field };
            first_path = false;
            if !path.is_empty() {
                commit.paths.push(path);
            }
        }
    }
    commits
}

pub fn warnings(repo: &Repo, entries: &[Entry], threshold: usize) -> Vec<String> {
    if threshold == 0 {
        return Vec::new();
    }
    let decisions: Vec<_> = entries.iter().filter_map(|e| cutoff(e).map(|t| (e, t))).collect();
    if decisions.is_empty() {
        return Vec::new();
    }
    let Some(head) = repo.head() else { return Vec::new() };
    // One history call for every decision, independent of the search index and adapter state.
    // Disabling rename detection reports both ends of a rename as changed paths.
    // Signature display can inject text into the path stream and spawn verification processes.
    let history = match repo.git(&[
        "log",
        "-n",
        HISTORY_LIMIT,
        "--no-merges",
        "--no-show-signature",
        "--root",
        "--no-renames",
        "--name-only",
        "-z",
        "--format=%x00%ct",
        &head,
        "--",
    ]) {
        Ok(history) => history,
        Err(err) => return vec![format!("Knowledge freshness unavailable: {err}")],
    };
    let commits = parse_history(&history);
    decisions
        .into_iter()
        .filter(|(e, since)| {
            commits
                .iter()
                .filter(|c| c.time >= *since && c.paths.iter().any(|p| e.paths.iter().any(|pattern| path_matches(pattern, p))))
                .take(threshold)
                .count()
                >= threshold
        })
        .map(|(e, _)| {
            format!(
                "Decision {} may need a refresh: at least {threshold} commits touched its paths since {}.",
                e.id,
                e.date.as_deref().unwrap_or_default()
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn history_preserves_unusual_paths_and_empty_commits() {
        let commits = parse_history(concat!(
            "\0",
            "1735819200\0\n123\0src/tab\tname.rs\0src/line\nname.rs\0",
            "\0",
            "1735905600\0",
            "\0",
            "1735992000\0\n\nleading.rs\0",
        ));
        assert_eq!(commits.len(), 3);
        assert_eq!(commits[0].time, 1735819200);
        assert_eq!(commits[0].paths, vec!["123", "src/tab\tname.rs", "src/line\nname.rs"]);
        assert!(commits[1].paths.is_empty());
        assert_eq!(commits[2].paths, vec!["\nleading.rs"]);
    }

    #[test]
    fn cutoff_starts_the_next_utc_day_and_requires_valid_metadata() {
        let mut entry =
            Entry { kind: "decision".into(), date: Some("2025-01-01".into()), paths: vec!["src/**".into()], ..Entry::default() };
        assert_eq!(cutoff(&entry), Some(1735776000));
        for date in ["2025-02-30", "2025-1-01", "2025-01-01T12:00:00Z", ""] {
            entry.date = Some(date.into());
            assert_eq!(cutoff(&entry), None, "{date}");
        }
        entry.date = Some("2025-01-01".into());
        entry.paths = vec![" ".into()];
        assert_eq!(cutoff(&entry), None);
    }
}
