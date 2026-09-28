//! Git repository discovery, identity and plumbing helpers.
use anyhow::{Context, Result, bail};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

#[derive(Debug, Clone)]
pub struct Repo {
    /// Worktree root (`git rev-parse --show-toplevel`).
    pub root: PathBuf,
    /// Shared by all worktrees of the clone; kontext's local state lives here.
    pub common_dir: PathBuf,
    pub remote: Option<String>,
    /// Normalized remote, e.g. `github.com/acme/shop`.
    pub id: String,
    /// Filesystem-safe id, e.g. `github.com-acme-shop`.
    pub slug: String,
    /// Repository name from the remote (or the main worktree's directory).
    pub name: String,
    /// Directory name of the main worktree (stable across Superset/agent worktrees).
    pub dir_name: String,
    pub branch: Option<String>,
    pub worktree_key: String,
}

#[derive(Debug, Clone)]
pub struct TrackedFile {
    pub path: String,
    pub submodule: bool,
}

impl Repo {
    pub fn discover(start: &Path) -> Result<Repo> {
        let out = Command::new("git")
            .arg("-C")
            .arg(start)
            .args(["rev-parse", "--path-format=absolute", "--show-toplevel", "--git-dir", "--git-common-dir"])
            .stderr(Stdio::piped())
            .output()
            .context("failed to run git (is it installed?)")?;
        if !out.status.success() {
            bail!("not inside a git repository ({}): {}", start.display(), String::from_utf8_lossy(&out.stderr).trim());
        }
        let text = String::from_utf8_lossy(&out.stdout);
        let mut lines = text.lines();
        let root = PathBuf::from(lines.next().unwrap_or_default());
        let _git_dir = lines.next();
        let common_dir = PathBuf::from(lines.next().unwrap_or_default());
        if root.as_os_str().is_empty() {
            bail!("bare repositories are not supported");
        }
        let mut repo = Repo {
            root,
            common_dir,
            remote: None,
            id: String::new(),
            slug: String::new(),
            name: String::new(),
            dir_name: String::new(),
            branch: None,
            worktree_key: String::new(),
        };
        repo.remote = repo
            .git_opt(&["config", "--get", "remote.origin.url"])
            .or_else(|| {
                let remotes = repo.git_opt(&["remote"])?;
                let first = remotes.lines().next()?.trim().to_string();
                repo.git_opt(&["config", "--get", &format!("remote.{first}.url")])
            })
            .filter(|s| !s.is_empty());
        repo.dir_name = main_worktree_dir(&repo.common_dir, &repo.root);
        match &repo.remote {
            Some(url) => {
                repo.id = normalize_remote(url);
                repo.name = repo.id.rsplit('/').next().unwrap_or(&repo.dir_name).to_string();
            }
            None => {
                repo.id = format!("local/{}-{}", repo.dir_name, &crate::util::short_hash(&repo.common_dir.to_string_lossy())[..8]);
                repo.name = repo.dir_name.clone();
            }
        }
        repo.slug = repo.id.replace('/', "-");
        repo.branch = repo.git_opt(&["symbolic-ref", "--short", "-q", "HEAD"]).filter(|s| !s.is_empty());
        let base = repo.root.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        repo.worktree_key = format!("{}-{}", crate::util::slugify(&base, 32), &crate::util::short_hash(&repo.root.to_string_lossy())[..8]);
        Ok(repo)
    }

    fn cmd(&self) -> Command {
        let mut c = Command::new("git");
        c.arg("-C").arg(&self.root).args(["-c", "core.quotePath=false"]);
        c.env("GIT_OPTIONAL_LOCKS", "0");
        c
    }

    pub fn git(&self, args: &[&str]) -> Result<String> {
        let out = self.cmd().args(args).stdin(Stdio::null()).output().context("failed to run git")?;
        if !out.status.success() {
            bail!("git {} failed: {}", args.join(" "), String::from_utf8_lossy(&out.stderr).trim());
        }
        Ok(String::from_utf8_lossy(&out.stdout).to_string())
    }

    pub fn git_bytes(&self, args: &[&str]) -> Result<Vec<u8>> {
        let out = self.cmd().args(args).stdin(Stdio::null()).output().context("failed to run git")?;
        if !out.status.success() {
            bail!("git {} failed: {}", args.join(" "), String::from_utf8_lossy(&out.stderr).trim());
        }
        Ok(out.stdout)
    }

    pub fn git_with_stdin(&self, args: &[&str], input: &str) -> Result<String> {
        self.git_with_stdin_bytes(args, input).map(|b| String::from_utf8_lossy(&b).to_string())
    }

    pub fn git_with_stdin_bytes(&self, args: &[&str], input: &str) -> Result<Vec<u8>> {
        use std::io::Write;
        let mut child = self
            .cmd()
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .context("failed to run git")?;
        let mut stdin = child.stdin.take().expect("piped stdin");
        let input = input.to_string();
        let writer = std::thread::spawn(move || {
            let _ = stdin.write_all(input.as_bytes());
        });
        let out = child.wait_with_output()?;
        let _ = writer.join();
        if !out.status.success() {
            bail!("git {} failed: {}", args.join(" "), String::from_utf8_lossy(&out.stderr).trim());
        }
        Ok(out.stdout)
    }

    pub fn git_opt(&self, args: &[&str]) -> Option<String> {
        let out = self.cmd().args(args).stdin(Stdio::null()).stderr(Stdio::null()).output().ok()?;
        out.status.success().then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
    }

    pub fn git_status_ok(&self, args: &[&str]) -> bool {
        self.cmd()
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }

    /// Local, never-committed state shared by all worktrees of this clone.
    pub fn state_dir(&self) -> PathBuf {
        self.common_dir.join("kontext")
    }

    /// State that belongs to this worktree only (search index, init cache).
    pub fn worktree_state_dir(&self) -> PathBuf {
        self.state_dir().join("worktrees").join(&self.worktree_key)
    }

    /// Remember which worktree a state dir belongs to, and drop state of worktrees that are gone
    /// (agent orchestrators such as Superset create and delete many of them).
    /// Whether the worktree with this key still exists (its state dir records its root).
    pub fn worktree_alive(&self, key: &str) -> bool {
        key == self.worktree_key
            || std::fs::read_to_string(self.state_dir().join("worktrees").join(key).join("path"))
                .is_ok_and(|root| std::path::Path::new(root.trim()).exists())
    }

    pub fn prune_worktree_state(&self) -> usize {
        let mine = self.worktree_state_dir();
        let _ = std::fs::create_dir_all(&mine);
        let _ = std::fs::write(mine.join("path"), self.root.to_string_lossy().as_bytes());
        let mut removed = 0;
        if let Ok(rd) = std::fs::read_dir(self.state_dir().join("worktrees")) {
            for d in rd.flatten() {
                let p = d.path();
                if p == mine {
                    continue;
                }
                if let Ok(root) = std::fs::read_to_string(p.join("path"))
                    && !std::path::Path::new(root.trim()).exists()
                    && std::fs::remove_dir_all(&p).is_ok()
                {
                    removed += 1;
                }
            }
        }
        removed
    }

    pub fn head(&self) -> Option<String> {
        self.git_opt(&["rev-parse", "-q", "--verify", "HEAD"]).filter(|s| !s.is_empty())
    }

    pub fn user_name(&self) -> Option<String> {
        self.git_opt(&["config", "user.name"]).filter(|s| !s.is_empty())
    }

    pub fn abs(&self, rel: &str) -> PathBuf {
        self.root.join(rel)
    }

    pub fn rel(&self, path: &Path) -> Option<String> {
        let path = if path.is_absolute() { path.to_path_buf() } else { std::env::current_dir().ok()?.join(path) };
        let canon = path.canonicalize().unwrap_or(path);
        let root = self.root.canonicalize().unwrap_or_else(|_| self.root.clone());
        canon.strip_prefix(&root).ok().map(|p| p.to_string_lossy().replace('\\', "/"))
    }

    /// Tracked files plus untracked-but-not-ignored ones. Submodules are reported, not descended.
    pub fn tracked_files(&self) -> Result<Vec<TrackedFile>> {
        let mut files = Vec::new();
        let staged = self.git_bytes(&["ls-files", "-z", "-s"])?;
        for rec in staged.split(|b| *b == 0) {
            if rec.is_empty() {
                continue;
            }
            let rec = String::from_utf8_lossy(rec);
            let Some((meta, path)) = rec.split_once('\t') else { continue };
            let mut parts = meta.split_whitespace();
            let mode = parts.next().unwrap_or("");
            let _blob = parts.next();
            let stage = parts.next().unwrap_or("0");
            if stage != "0" && files.last().is_some_and(|f: &TrackedFile| f.path == path) {
                continue;
            }
            files.push(TrackedFile { path: path.to_string(), submodule: mode == "160000" });
        }
        let others = self.git_bytes(&["ls-files", "-z", "--others", "--exclude-standard"])?;
        for rec in others.split(|b| *b == 0) {
            if rec.is_empty() {
                continue;
            }
            let path = String::from_utf8_lossy(rec).to_string();
            if path.ends_with('/') {
                continue;
            }
            files.push(TrackedFile { path, submodule: false });
        }
        Ok(files)
    }

    /// Paths staged for commit (added, copied, modified, renamed).
    pub fn staged_paths(&self) -> Result<Vec<(char, String)>> {
        self.staged_paths_since(None)
    }

    /// What the index changes against `base` (default HEAD); an amend compares with HEAD's parent.
    pub fn staged_paths_since(&self, base: Option<&str>) -> Result<Vec<(char, String)>> {
        let mut args = vec!["diff", "--cached", "--name-status", "-z", "--no-renames"];
        args.extend(base);
        let out = self.git_bytes(&args)?;
        let mut res = Vec::new();
        let mut it = out.split(|b| *b == 0).filter(|s| !s.is_empty());
        while let Some(status) = it.next() {
            let status = String::from_utf8_lossy(status).chars().next().unwrap_or('M');
            if let Some(path) = it.next() {
                res.push((status, String::from_utf8_lossy(path).to_string()));
            }
        }
        Ok(res)
    }

    /// Staged (index) contents of `paths`, read with one `ls-files` and one `cat-file --batch`
    /// instead of a `git show :path` per file. Paths that are not in the index are left out.
    pub fn staged_contents(&self, paths: &[String]) -> Result<std::collections::HashMap<String, Vec<u8>>> {
        let mut out = std::collections::HashMap::new();
        if paths.is_empty() {
            return Ok(out);
        }
        let wanted: std::collections::HashSet<&str> = paths.iter().map(String::as_str).collect();
        let literal: Vec<String> = paths.iter().map(|p| format!(":(literal){p}")).collect();
        let mut args = vec!["ls-files", "-s", "-z"];
        // a long list goes through the whole index instead of the command line
        if literal.len() <= 200 {
            args.push("--");
            args.extend(literal.iter().map(String::as_str));
        }
        let listing = self.git_bytes(&args)?;
        let mut blobs: Vec<(String, String)> = Vec::new();
        for rec in listing.split(|b| *b == 0).filter(|r| !r.is_empty()) {
            let rec = String::from_utf8_lossy(rec);
            let Some((meta, path)) = rec.split_once('\t') else { continue };
            let mut meta = meta.split_whitespace();
            let (Some(mode), Some(sha)) = (meta.next(), meta.next()) else { continue };
            // regular files only (no submodules or symlinks), first stage only
            if wanted.contains(path) && mode.starts_with("100") && !blobs.iter().any(|(p, _)| p == path) {
                blobs.push((path.to_string(), sha.to_string()));
            }
        }
        if blobs.is_empty() {
            return Ok(out);
        }
        let input: String = blobs.iter().map(|(_, s)| format!("{s}\n")).collect();
        let data = self.git_with_stdin_bytes(&["cat-file", "--batch"], &input)?;
        let mut rest: &[u8] = &data;
        for (path, _) in blobs {
            let Some(nl) = rest.iter().position(|b| *b == b'\n') else { break };
            let header = String::from_utf8_lossy(&rest[..nl]).to_string();
            rest = &rest[nl + 1..];
            let Some(size) = header.split_whitespace().nth(2).and_then(|s| s.parse::<usize>().ok()) else { continue };
            let size = size.min(rest.len());
            out.insert(path, rest[..size].to_vec());
            rest = rest.get(size + 1..).unwrap_or(&[]);
        }
        Ok(out)
    }

    /// Working tree changes (staged or not) — used when nothing is staged yet.
    pub fn changed_paths(&self) -> Result<Vec<(char, String)>> {
        let out = self.git_bytes(&["status", "--porcelain=v1", "-z", "--untracked-files=all", "--no-renames"])?;
        let mut res = Vec::new();
        for rec in out.split(|b| *b == 0) {
            if rec.len() < 4 {
                continue;
            }
            let rec = String::from_utf8_lossy(rec);
            let x = rec.chars().next().unwrap_or(' ');
            let y = rec.chars().nth(1).unwrap_or(' ');
            let st = if x == '?' {
                'A'
            } else if x != ' ' {
                x
            } else {
                y
            };
            res.push((st, rec[3..].to_string()));
        }
        Ok(res)
    }

    pub fn template_ctx(&self) -> Value {
        json!({
            "id": self.id,
            "slug": self.slug,
            "name": self.name,
            "dir": self.dir_name,
            "root": self.root.to_string_lossy(),
            "remote": self.remote.clone().unwrap_or_default(),
            "branch": self.branch.clone().unwrap_or_default(),
            "worktree": self.worktree_key,
        })
    }
}

fn main_worktree_dir(common_dir: &Path, root: &Path) -> String {
    let name = |p: &Path| p.file_name().map(|s| s.to_string_lossy().to_string());
    if name(common_dir).as_deref() == Some(".git")
        && let Some(parent) = common_dir.parent().and_then(name)
    {
        return parent;
    }
    if let Some(n) = name(common_dir) {
        return n.trim_end_matches(".git").to_string();
    }
    name(root).unwrap_or_else(|| "repo".into())
}

/// `git@github.com:Org/Repo.git`, `https://user@github.com/Org/Repo` → `github.com/org/repo`.
pub fn normalize_remote(url: &str) -> String {
    let mut s = url.trim().to_string();
    for scheme in ["https://", "http://", "ssh://", "git://", "git+ssh://", "file://"] {
        if let Some(rest) = s.strip_prefix(scheme) {
            s = rest.to_string();
            break;
        }
    }
    if let Some(at) = s.find('@')
        && !s[..at].contains('/')
    {
        s = s[at + 1..].to_string();
    }
    // scp-like host:path (but not host:port/path)
    if let Some(colon) = s.find(':') {
        let (host, rest) = s.split_at(colon);
        let rest = &rest[1..];
        if !host.contains('/') {
            let rest = rest.trim_start_matches(|c: char| c.is_ascii_digit()).trim_start_matches('/');
            s = format!("{host}/{rest}");
        }
    }
    let s = s.trim_end_matches('/').trim_end_matches(".git").to_lowercase();
    s.split('/').filter(|p| !p.is_empty() && *p != "..").collect::<Vec<_>>().join("/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remotes() {
        assert_eq!(normalize_remote("https://github.com/Acme-Corp/Shop-API"), "github.com/acme-corp/shop-api");
        assert_eq!(normalize_remote("git@github.com:acme/foo.git"), "github.com/acme/foo");
        assert_eq!(normalize_remote("ssh://git@gitlab.com:2222/a/b.git"), "gitlab.com/a/b");
        assert_eq!(normalize_remote("https://user:tok@github.com/Acme/Kontext.git/"), "github.com/acme/kontext");
    }
}
