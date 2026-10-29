//! One sandbox for the integration tests: a throwaway repository, a private kontext config and a
//! private HOME, with git cut off from the developer's global and system config (signing, templates,
//! a global `core.hooksPath`) so a test never writes outside its sandbox and behaves as it does in CI.
//!
//! A test crate pulls it in with `mod common;` and uses what it needs; the rest is unused there.
#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT: AtomicUsize = AtomicUsize::new(0);

/// The date of the commit that records a fixture.
pub const FIXTURE_DATE: &str = "2026-05-01T10:00:00+00:00";

/// The kontext binary under test.
pub fn kontext_bin() -> &'static str {
    env!("CARGO_BIN_EXE_kontext")
}

/// The repository root of kontext itself.
pub fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

#[cfg(windows)]
const NULL_DEVICE: &str = "NUL";
#[cfg(not(windows))]
const NULL_DEVICE: &str = "/dev/null";

/// Cut a command off from the developer's environment: no global or system git config, HOME and
/// XDG_CONFIG_HOME inside the sandbox, no agent homes, and a fixed author and committer. Usable on
/// its own by harnesses that keep their own repository type.
pub fn isolate<'a>(cmd: &'a mut Command, home: &Path) -> &'a mut Command {
    cmd.env("GIT_CONFIG_GLOBAL", NULL_DEVICE)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("HOME", home)
        .env("XDG_CONFIG_HOME", home.join(".config"))
        .env("GIT_AUTHOR_NAME", "Test")
        .env("GIT_AUTHOR_EMAIL", "test@example.invalid")
        .env("GIT_COMMITTER_NAME", "Test")
        .env("GIT_COMMITTER_EMAIL", "test@example.invalid")
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_INDEX_FILE")
        .env_remove("GIT_CONFIG_PARAMETERS")
        .env_remove("CLAUDE_CONFIG_DIR")
        .env_remove("CODEX_HOME")
        .env_remove("SUPERSET_HOME_DIR")
        .env_remove("SUPERSET_WORKSPACE_ID")
        .env_remove("SUPERSET_ORGANIZATION_ID")
}

pub struct Sandbox {
    /// The sandbox directory; everything below is removed on drop.
    pub root: PathBuf,
    /// The repository the commands run in (`root/repo`).
    pub repo: PathBuf,
    /// `KONTEXT_CONFIG_DIR` (`root/config`).
    pub config: PathBuf,
    /// HOME and XDG_CONFIG_HOME of every command (`root/home`).
    pub home: PathBuf,
    /// A free path next to the repository for a second worktree (`root/other-worktree`).
    pub worktree: PathBuf,
    /// Commands run with `KONTEXT_SKIP=1`, so installed hooks stay quiet.
    pub skip_hooks: bool,
}

impl Sandbox {
    /// An empty repository directory (not yet `git init`ed); hooks run.
    pub fn new(name: &str) -> Sandbox {
        let root = std::env::temp_dir().join(format!("kontext-it-{name}-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
        let _ = std::fs::remove_dir_all(&root);
        let sandbox = Sandbox {
            repo: root.join("repo"),
            config: root.join("config"),
            home: root.join("home"),
            worktree: root.join("other-worktree"),
            root,
            skip_hooks: false,
        };
        for dir in [&sandbox.repo, &sandbox.config, &sandbox.home] {
            std::fs::create_dir_all(dir).unwrap();
        }
        sandbox
    }

    /// A copy of `tests/eval/fixtures/<fixture>`, committed once on [`FIXTURE_DATE`]; hooks skipped.
    pub fn fixture(fixture: &str) -> Sandbox {
        let mut sandbox = Sandbox::new(&format!("fixture-{fixture}"));
        sandbox.skip_hooks = true;
        copy_dir(&manifest_dir().join("tests/eval/fixtures").join(fixture), &sandbox.repo);
        sandbox.ok_git(&["init", "-q", "-b", "main"]);
        sandbox.ok_git(&["add", "-A"]);
        sandbox.git_dated(&["commit", "-qm", "fixture"], FIXTURE_DATE);
        sandbox
    }

    pub fn bin_dir() -> PathBuf {
        PathBuf::from(kontext_bin()).parent().unwrap().to_path_buf()
    }

    /// PATH with the binary under test first, so installed hooks find it.
    pub fn path_env() -> String {
        let sep = if cfg!(windows) { ";" } else { ":" };
        format!("{}{sep}{}", Self::bin_dir().display(), std::env::var("PATH").unwrap_or_default())
    }

    /// `program` in the repository with the isolated environment.
    pub fn cmd(&self, program: &str) -> Command {
        let mut c = Command::new(program);
        c.current_dir(&self.repo).env("PATH", Self::path_env()).env("KONTEXT_CONFIG_DIR", &self.config);
        isolate(&mut c, &self.home);
        if self.skip_hooks {
            c.env("KONTEXT_SKIP", "1");
        } else {
            c.env_remove("KONTEXT_SKIP");
        }
        c
    }

    /// The kontext binary in the repository with the isolated environment.
    pub fn kontext_cmd(&self) -> Command {
        self.cmd(kontext_bin())
    }

    pub fn git(&self, args: &[&str]) -> Output {
        self.cmd("git").args(args).output().unwrap()
    }

    /// git, asserted to succeed; its stdout as is.
    pub fn ok_git(&self, args: &[&str]) -> String {
        let o = self.git(args);
        assert!(o.status.success(), "git {args:?} failed: {}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr));
        String::from_utf8_lossy(&o.stdout).to_string()
    }

    /// git with author and committer date `date` (none when empty), asserted to succeed; stdout trimmed.
    pub fn git_dated(&self, args: &[&str], date: &str) -> String {
        let mut c = self.cmd("git");
        if !date.is_empty() {
            c.env("GIT_AUTHOR_DATE", date).env("GIT_COMMITTER_DATE", date);
        }
        let o = c.args(args).output().unwrap();
        assert!(o.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&o.stderr));
        String::from_utf8_lossy(&o.stdout).trim().to_string()
    }

    /// git in another directory (`git -C dir …`), asserted to succeed.
    pub fn git_in(&self, dir: &Path, args: &[&str]) {
        let o = self.cmd("git").arg("-C").arg(dir).args(args).output().unwrap();
        assert!(o.status.success(), "git {args:?} in {}: {}", dir.display(), String::from_utf8_lossy(&o.stderr));
    }

    /// kontext, whatever its exit status.
    pub fn run(&self, args: &[&str]) -> Output {
        self.kontext_cmd().args(args).output().unwrap()
    }

    /// kontext, asserted to succeed; its stdout.
    pub fn kontext(&self, args: &[&str]) -> String {
        let o = self.run(args);
        assert!(
            o.status.success(),
            "kontext {args:?} failed: {}{}",
            String::from_utf8_lossy(&o.stdout),
            String::from_utf8_lossy(&o.stderr)
        );
        String::from_utf8_lossy(&o.stdout).to_string()
    }

    /// Stage everything and commit; the new HEAD.
    pub fn commit(&self, message: &str) -> String {
        self.ok_git(&["add", "-A"]);
        self.ok_git(&["commit", "-qm", message]);
        self.ok_git(&["rev-parse", "HEAD"]).trim().to_string()
    }

    pub fn write(&self, rel: &str, text: &str) {
        let p = self.repo.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, text).unwrap();
    }

    pub fn read(&self, rel: &str) -> String {
        std::fs::read_to_string(self.repo.join(rel)).unwrap()
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

pub fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap().flatten() {
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), target).unwrap();
        }
    }
}

/// Whether `program` runs (`program --version` succeeds).
pub fn have(program: &str) -> bool {
    Command::new(program).arg("--version").output().is_ok_and(|o| o.status.success())
}
