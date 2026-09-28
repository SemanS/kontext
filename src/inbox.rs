//! Local candidate memories (`<git-common-dir>/kontext/inbox`). Never committed; shared by all
//! worktrees of the clone. Team candidates become shared truth only when promoted into the store
//! and committed — the diff/PR is the review gate. Private ones stay here (and may be mirrored to
//! personal adapters through `capture` events).
use crate::config::Style;
use crate::repo::Repo;
use crate::store::{Entry, FmValue};
use crate::util;
use anyhow::{Result, bail};
use std::path::PathBuf;

pub struct Inbox {
    pub dir: PathBuf,
}

/// Where a candidate was captured, seen from the current worktree. The inbox is shared by all
/// worktrees, so parallel agents see each other's candidates — only their own belong in their commits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Origin {
    /// this worktree (or an entry that does not say)
    Here,
    /// another worktree that still exists: its session owns the candidate
    Elsewhere { worktree: String, branch: Option<String> },
    /// a worktree that is gone: anyone may adopt or drop it
    Orphan,
}

impl Origin {
    pub fn of(repo: &Repo, e: &Entry) -> Origin {
        match e.get_extra("worktree").filter(|w| !w.is_empty()) {
            None => Origin::Here,
            Some(w) if w == repo.worktree_key => Origin::Here,
            Some(w) if repo.worktree_alive(&w) => Origin::Elsewhere { worktree: w, branch: e.get_extra("branch") },
            Some(_) => Origin::Orphan,
        }
    }

    /// Candidates this worktree may put into its commits.
    pub fn is_ours(&self) -> bool {
        !matches!(self, Origin::Elsewhere { .. })
    }

    /// A short note for listings, empty for this worktree.
    pub fn note(&self) -> String {
        match self {
            Origin::Here => String::new(),
            Origin::Elsewhere { worktree, branch } => match branch {
                Some(b) => format!(" (captured in worktree {worktree}, branch {b})"),
                None => format!(" (captured in worktree {worktree})"),
            },
            Origin::Orphan => " (captured in a worktree that no longer exists)".into(),
        }
    }
}

impl Inbox {
    pub fn open(repo: &Repo) -> Inbox {
        Inbox { dir: repo.state_dir().join("inbox") }
    }

    fn promoted_dir(&self) -> PathBuf {
        self.dir.join(".promoted")
    }

    pub fn list(&self) -> Vec<Entry> {
        let mut v = read_dir_entries(&self.dir);
        v.sort_by_key(|e| std::cmp::Reverse(e.get_extra("captured")));
        v
    }

    pub fn promoted(&self) -> Vec<Entry> {
        read_dir_entries(&self.promoted_dir())
    }

    pub fn get(&self, key: &str) -> Option<Entry> {
        let all = self.list();
        let key = key.trim();
        all.iter().find(|e| e.id == key).cloned().or_else(|| {
            let m: Vec<_> = all.iter().filter(|e| e.id.starts_with(key) || e.id.ends_with(key)).collect();
            if m.len() == 1 { Some(m[0].clone()) } else { None }
        })
    }

    pub fn add(&self, repo: &Repo, mut e: Entry) -> Result<Entry> {
        std::fs::create_dir_all(&self.dir)?;
        let date = e.date.clone().unwrap_or_else(util::today);
        let slug = util::slugify(&e.title, 50);
        let base = format!("{date}-{}", if slug.is_empty() { "note" } else { &slug });
        let mut id = base.clone();
        let mut n = 2;
        while self.dir.join(format!("{id}.md")).exists() {
            id = format!("{base}-{n}");
            n += 1;
        }
        e.id = id.clone();
        e.style = Style::Frontmatter;
        e.rel_path = format!("inbox/{id}.md");
        if e.date.is_none() {
            e.date = Some(date);
        }
        e.set_extra("captured", FmValue::Str(util::now_iso()));
        if let Some(b) = &repo.branch {
            e.set_extra("branch", FmValue::Str(b.clone()));
        }
        e.set_extra("worktree", FmValue::Str(repo.worktree_key.clone()));
        util::write_atomic(&self.dir.join(format!("{id}.md")), &e.markdown())?;
        Ok(e)
    }

    pub fn remove(&self, id: &str) -> Result<()> {
        let p = self.dir.join(format!("{id}.md"));
        if !p.exists() {
            bail!("no inbox entry '{id}'");
        }
        std::fs::remove_file(p)?;
        Ok(())
    }

    /// Move a candidate aside after it was written into the store (kept until the store file is committed).
    pub fn mark_promoted(&self, id: &str, target_rel: &str) -> Result<()> {
        let src = self.dir.join(format!("{id}.md"));
        let text = std::fs::read_to_string(&src)?;
        let mut e = Entry::parse(&format!("inbox/{id}.md"), &text, "learning");
        e.set_extra("promoted_to", FmValue::Str(target_rel.to_string()));
        e.set_extra("promoted_at", FmValue::Str(util::now_iso()));
        std::fs::create_dir_all(self.promoted_dir())?;
        util::write_atomic(&self.promoted_dir().join(format!("{id}.md")), &e.markdown())?;
        std::fs::remove_file(src)?;
        Ok(())
    }

    /// Drop promoted candidates whose store file is now part of HEAD.
    pub fn cleanup_committed(&self, repo: &Repo) -> usize {
        let mut n = 0;
        for e in self.promoted() {
            if let Some(target) = e.get_extra("promoted_to")
                && repo.git_status_ok(&["cat-file", "-e", &format!("HEAD:{target}")])
            {
                let _ = std::fs::remove_file(self.promoted_dir().join(format!("{}.md", e.id)));
                n += 1;
            }
        }
        n
    }
}

fn read_dir_entries(dir: &std::path::Path) -> Vec<Entry> {
    let mut out = Vec::new();
    let Ok(rd) = std::fs::read_dir(dir) else { return out };
    for f in rd.flatten() {
        let p = f.path();
        if p.extension().and_then(|e| e.to_str()) != Some("md") {
            continue;
        }
        if let Ok(text) = std::fs::read_to_string(&p) {
            let name = p.file_name().unwrap_or_default().to_string_lossy().to_string();
            out.push(Entry::parse(&format!("inbox/{name}"), &text, "learning"));
        }
    }
    out
}
