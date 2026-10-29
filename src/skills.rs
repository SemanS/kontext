//! Skills: recipes for knowledge maintenance that an agent reads as `SKILL.md` files. They are
//! compiled into the binary from `skills/` so an installed kontext carries the ones its commands
//! and API fit, and `kontext connect skills --write` puts them where agents find them, project
//! scope, committed and shared with the team: `.claude/skills/<name>/SKILL.md` (Claude Code,
//! Cursor, OpenCode) and `.agents/skills/<name>/SKILL.md` (Codex and the Agent Skills layout).
//!
//! A written copy ends with a `kontext-skill-hash` marker naming the shipped text it was written
//! from. A copy the team edited no longer matches its marker and is kept; `--force` replaces it.
use crate::util;
use anyhow::{Result, bail};
use std::path::{Path, PathBuf};

pub struct Skill {
    pub name: &'static str,
    pub text: &'static str,
}

pub const ALL: &[Skill] = &[
    Skill { name: "kontext-audit", text: include_str!("../skills/kontext-audit/SKILL.md") },
    Skill { name: "kontext-flows", text: include_str!("../skills/kontext-flows/SKILL.md") },
    Skill { name: "kontext-verify", text: include_str!("../skills/kontext-verify/SKILL.md") },
    Skill { name: "kontext-merge", text: include_str!("../skills/kontext-merge/SKILL.md") },
];

/// Where skills go under a repository root, in the order reports name them.
pub const DIRS: &[&str] = &[".claude/skills", ".agents/skills"];

const MARKER: &str = "<!-- kontext-skill-hash: ";

/// Where a skill lives under a repository root (Claude Code's layout).
#[cfg(test)]
pub fn path(root: &Path, name: &str) -> PathBuf {
    root.join(".claude").join("skills").join(name).join("SKILL.md")
}

/// Every place a skill is written under a repository root.
pub fn paths(root: &Path, name: &str) -> Vec<PathBuf> {
    DIRS.iter().map(|d| root.join(d).join(name).join("SKILL.md")).collect()
}

/// The first sentence of a skill's `description` front-matter line.
pub fn first_sentence(text: &str) -> String {
    let desc = description(text);
    let end = desc.find(". ").map_or(desc.len(), |i| i + 1);
    desc[..end].to_string()
}

fn description(text: &str) -> &str {
    text.lines().find_map(|l| l.strip_prefix("description:")).map(str::trim).unwrap_or_default()
}

fn hash(text: &str) -> String {
    format!("{:016x}", util::fnv1a64(text.as_bytes()))
}

/// The file kontext writes: the shipped text and the marker naming it.
pub fn written_text(s: &Skill) -> String {
    format!("{}\n{MARKER}{} -->\n", s.text.trim_end_matches('\n'), hash(s.text))
}

/// The text of a written copy without its marker, and the hash the marker names.
fn split_marker(file: &str) -> (String, Option<String>) {
    let Some(pos) = file.rfind(MARKER) else { return (file.to_string(), None) };
    let tail = &file[pos + MARKER.len()..];
    let Some(h) = tail.split_whitespace().next() else { return (file.to_string(), None) };
    let text = format!("{}\n", file[..pos].trim_end_matches('\n'));
    (text, Some(h.to_string()))
}

/// What the Agent Skills specification requires of a skill: `name` equals its directory and is
/// lowercase letters, digits and hyphens (at most 64), a `description` of at most 1,024
/// characters, and a body under 500 lines.
pub fn validate(s: &Skill) -> Vec<String> {
    let mut problems = Vec::new();
    let Some(rest) = s.text.strip_prefix("---\n") else {
        return vec![format!("{}: no front matter", s.name)];
    };
    let Some((fm, body)) = rest.split_once("\n---\n") else {
        return vec![format!("{}: front matter is not closed", s.name)];
    };
    let name = fm.lines().find_map(|l| l.strip_prefix("name:")).map(str::trim).unwrap_or_default();
    if name != s.name {
        problems.push(format!("{}: name '{name}' differs from its directory", s.name));
    }
    let name_ok = !name.is_empty()
        && name.len() <= 64
        && name.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        && !name.starts_with('-')
        && !name.ends_with('-')
        && !name.contains("--");
    if !name_ok {
        problems.push(format!("{}: name must be 1-64 lowercase letters, digits and single hyphens", s.name));
    }
    let desc = description(s.text);
    if desc.is_empty() || desc.chars().count() > 1024 {
        problems.push(format!("{}: description must be 1-1024 characters (has {})", s.name, desc.chars().count()));
    }
    let lines = body.lines().count();
    if lines >= 500 {
        problems.push(format!("{}: body has {lines} lines (fewer than 500)", s.name));
    }
    problems
}

enum Outcome {
    Current,
    Wrote,
    Kept,
}

fn place(p: &Path, s: &Skill, force: bool) -> Result<Outcome> {
    let want = written_text(s);
    let Ok(current) = std::fs::read_to_string(p) else {
        util::write_atomic(p, &want)?;
        return Ok(Outcome::Wrote);
    };
    if current == want {
        return Ok(Outcome::Current);
    }
    let (text, marker) = split_marker(&current);
    // as shipped by this or an earlier kontext: the team did not touch it
    let untouched = marker.is_some_and(|h| h == hash(&text)) || text == s.text;
    if untouched || force {
        util::write_atomic(p, &want)?;
        return Ok(Outcome::Wrote);
    }
    Ok(Outcome::Kept)
}

/// What `kontext connect skills [--write] [--force]` prints: without `write`, the list and where
/// they go; with it, each file written, left as it is when current, or kept when the team edited it.
pub fn connect(root: &Path, write: bool, force: bool) -> Result<String> {
    let invalid: Vec<String> = ALL.iter().flat_map(validate).collect();
    if !invalid.is_empty() {
        bail!("shipped skills break the Agent Skills spec: {}", invalid.join("; "));
    }
    let mut lines = Vec::new();
    if !write {
        lines.push(format!("Skills for knowledge maintenance ({}), written by `kontext connect skills --write`:", ALL.len()));
        for s in ALL {
            lines.push(format!("  .claude/skills/{}/SKILL.md — {}", s.name, first_sentence(s.text)));
        }
        lines.push(
            "Each is also written to .agents/skills/<name>/SKILL.md (Codex). Project scope: commit them so the team's agents share them (`/<name>` in Claude Code)."
                .into(),
        );
        return Ok(lines.join("\n"));
    }
    let mut wrote = 0;
    let mut kept = Vec::new();
    for s in ALL {
        let mut any = false;
        for (dir, p) in DIRS.iter().zip(paths(root, s.name)) {
            match place(&p, s, force)? {
                Outcome::Wrote => any = true,
                Outcome::Current => {}
                Outcome::Kept => kept.push(format!("{dir}/{}/SKILL.md", s.name)),
            }
        }
        if any {
            wrote += 1;
        }
    }
    let names: Vec<&str> = ALL.iter().map(|s| s.name).collect();
    let mut out = if wrote == 0 {
        format!(
            "skills: .claude/skills/ already has the current {} {} ({}), .agents/skills/ too",
            ALL.len(),
            if ALL.len() == 1 { "skill" } else { "skills" },
            names.join(", ")
        )
    } else {
        format!(
            "skills: wrote {wrote} of {} into .claude/skills/ and .agents/skills/ ({}); commit them for the team",
            ALL.len(),
            names.join(", ")
        )
    };
    for k in kept {
        out.push_str(&format!(
            "\n  kept {k}: the team edited it (it differs from the version kontext wrote); `git log -p -- {k}` shows the edit, `--force` replaces it"
        ));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_skill_has_a_name_and_a_description() {
        for s in ALL {
            assert!(s.text.starts_with("---\nname: "), "{} starts with front matter", s.name);
            assert!(s.text.contains(&format!("name: {}\n", s.name)), "{} names itself", s.name);
            let first = first_sentence(s.text);
            assert!(first.ends_with('.') && first.len() > 30, "{}: {first:?}", s.name);
            assert!(s.text.contains("## 1."), "{} is a numbered recipe", s.name);
        }
    }

    #[test]
    fn every_skill_meets_the_agent_skills_spec() {
        for s in ALL {
            assert_eq!(validate(s), Vec::<String>::new(), "{}", s.name);
        }
        let bad = Skill { name: "kontext-x", text: "---\nname: Kontext_X\ndescription: \n---\n\nbody\n" };
        assert_eq!(validate(&bad).len(), 3, "{:?}", validate(&bad));
    }

    #[test]
    fn the_marker_round_trips() {
        let s = &ALL[0];
        let (text, marker) = split_marker(&written_text(s));
        assert_eq!(text, s.text);
        assert_eq!(marker.as_deref(), Some(hash(s.text).as_str()));
    }

    #[test]
    fn connect_writes_once_and_keeps_team_edits() {
        let root = std::env::temp_dir().join(format!("kontext-skills-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let listed = connect(&root, false, false).unwrap();
        assert!(listed.contains(".claude/skills/kontext-audit/SKILL.md — Audit"), "{listed}");
        assert!(!path(&root, "kontext-audit").exists(), "listing writes nothing");
        let first = connect(&root, true, false).unwrap();
        assert!(first.starts_with(&format!("skills: wrote {} of {} into .claude/skills/", ALL.len(), ALL.len())), "{first}");
        for p in paths(&root, "kontext-flows") {
            assert_eq!(std::fs::read_to_string(p).unwrap(), written_text(&ALL[1]));
        }
        let again = connect(&root, true, false).unwrap();
        assert!(again.starts_with("skills: .claude/skills/ already has the current"), "{again}");
        // a copy the team edited is kept, with a hint; --force replaces it
        let edited = path(&root, "kontext-merge");
        std::fs::write(&edited, "---\nname: kontext-merge\ndescription: ours.\n---\n\nOur own check script.\n").unwrap();
        let third = connect(&root, true, false).unwrap();
        assert!(third.contains("kept .claude/skills/kontext-merge/SKILL.md: the team edited it"), "{third}");
        assert!(std::fs::read_to_string(&edited).unwrap().contains("Our own check script."));
        // a copy written by an earlier kontext (its marker matches its text) is brought up to date
        let older = paths(&root, "kontext-audit")[1].clone();
        let old_text = "---\nname: kontext-audit\ndescription: Old.\n---\n\nOld recipe.\n";
        std::fs::write(&older, format!("{old_text}\n{MARKER}{} -->\n", hash(old_text))).unwrap();
        let fourth = connect(&root, true, false).unwrap();
        assert!(fourth.starts_with(&format!("skills: wrote 1 of {}", ALL.len())), "{fourth}");
        assert_eq!(std::fs::read_to_string(&older).unwrap(), written_text(&ALL[0]));
        let forced = connect(&root, true, true).unwrap();
        assert!(forced.starts_with(&format!("skills: wrote 1 of {}", ALL.len())), "{forced}");
        assert_eq!(std::fs::read_to_string(&edited).unwrap(), written_text(&ALL[3]));
        let _ = std::fs::remove_dir_all(&root);
    }
}
