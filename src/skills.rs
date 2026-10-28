//! Skills: recipes for knowledge maintenance that an agent reads as `SKILL.md` files. They are
//! compiled into the binary from `skills/` so an installed kontext carries the ones its commands
//! and API fit, and `kontext connect skills --write` puts them where Claude Code (and clients
//! that read the same layout) find them: `.claude/skills/<name>/SKILL.md`, project scope,
//! committed and shared with the team.
use crate::util;
use anyhow::Result;
use std::path::Path;

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

/// Where a skill lives under a repository root.
pub fn path(root: &Path, name: &str) -> std::path::PathBuf {
    root.join(".claude").join("skills").join(name).join("SKILL.md")
}

/// The first sentence of a skill's `description` front-matter line.
pub fn first_sentence(text: &str) -> String {
    let desc = text.lines().find_map(|l| l.strip_prefix("description:")).map(str::trim).unwrap_or_default();
    let end = desc.find(". ").map_or(desc.len(), |i| i + 1);
    desc[..end].to_string()
}

/// What `kontext connect skills [--write]` prints: without `write`, the list and where they go;
/// with it, each file written (or left as it is when already current).
pub fn connect(root: &Path, write: bool) -> Result<String> {
    let mut lines = Vec::new();
    if !write {
        lines.push(format!("Skills for knowledge maintenance ({}), written by `kontext connect skills --write`:", ALL.len()));
        for s in ALL {
            lines.push(format!("  .claude/skills/{}/SKILL.md — {}", s.name, first_sentence(s.text)));
        }
        lines.push("Project scope: commit them so the team's agents share them (`/<name>` in Claude Code).".into());
        return Ok(lines.join("\n"));
    }
    let mut wrote = 0;
    for s in ALL {
        let p = path(root, s.name);
        let current = std::fs::read_to_string(&p).unwrap_or_default();
        if current == s.text {
            continue;
        }
        util::write_atomic(&p, s.text)?;
        wrote += 1;
    }
    let names: Vec<&str> = ALL.iter().map(|s| s.name).collect();
    Ok(if wrote == 0 {
        format!(
            "skills: .claude/skills/ already has the current {} {} ({})",
            ALL.len(),
            if ALL.len() == 1 { "skill" } else { "skills" },
            names.join(", ")
        )
    } else {
        format!("skills: wrote {wrote} of {} into .claude/skills/ ({}); commit them for the team", ALL.len(), names.join(", "))
    })
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
    fn connect_writes_once_and_reports_current_after() {
        let root = std::env::temp_dir().join(format!("kontext-skills-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let listed = connect(&root, false).unwrap();
        assert!(listed.contains(".claude/skills/kontext-audit/SKILL.md — Audit"), "{listed}");
        assert!(!path(&root, "kontext-audit").exists(), "listing writes nothing");
        let first = connect(&root, true).unwrap();
        assert!(first.starts_with(&format!("skills: wrote {} of {}", ALL.len(), ALL.len())), "{first}");
        assert_eq!(std::fs::read_to_string(path(&root, "kontext-flows")).unwrap(), ALL[1].text);
        let again = connect(&root, true).unwrap();
        assert!(again.starts_with("skills: .claude/skills/ already has the current"), "{again}");
        // an edited copy is brought back to the shipped text
        std::fs::write(path(&root, "kontext-merge"), "edited\n").unwrap();
        let third = connect(&root, true).unwrap();
        assert!(third.starts_with(&format!("skills: wrote 1 of {}", ALL.len())), "{third}");
        let _ = std::fs::remove_dir_all(&root);
    }
}
