//! Process-wide context: repository, configuration, lazily built adapter registry and search index.
use crate::adapters::Registry;
use crate::config::{self, Config, LoadedConfig};
use crate::index::LocalIndex;
use crate::repo::Repo;
use crate::store::{Entry, Store};
use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, Mutex, MutexGuard, OnceLock};
use std::time::{Duration, Instant};

/// How many other repositories one process keeps open (each may hold adapter processes).
const OTHERS_MAX: usize = 16;

/// Contexts of other repositories this process was pointed at (another worktree, a submodule, a
/// sibling project), opened once and kept like the server's own: by worktree root, the least
/// recently used dropped first.
struct Others {
    by_root: HashMap<PathBuf, (Arc<App>, u64)>,
    root_of: HashMap<PathBuf, PathBuf>,
    tick: u64,
}

impl Others {
    fn touch(&mut self, root: &Path) -> Option<Arc<App>> {
        self.tick += 1;
        let tick = self.tick;
        self.by_root.get_mut(root).map(|(app, used)| {
            *used = tick;
            app.clone()
        })
    }

    fn evict(&mut self) {
        while self.by_root.len() > OTHERS_MAX {
            let Some(oldest) = self.by_root.iter().min_by_key(|(_, (_, used))| *used).map(|(r, _)| r.clone()) else { break };
            self.by_root.remove(&oldest);
            self.root_of.retain(|_, r| *r != oldest);
        }
    }
}

static OTHERS: LazyLock<Mutex<Others>> = LazyLock::new(|| Mutex::new(Others { by_root: HashMap::new(), root_of: HashMap::new(), tick: 0 }));

/// A lock that survives a panic in another holder: the data under these locks stays consistent
/// (caches and serialization guards), so one panicking request must not break the next ones.
pub fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// Whether a repository now sits between `dir` and the root it was resolved to (a submodule
/// initialized while a server runs): the cached answer belongs to the outer repository.
fn repo_appeared(dir: &Path, root: &Path) -> bool {
    let mut cur = dir;
    while cur != root {
        if cur.join(".git").exists() {
            return true;
        }
        match cur.parent() {
            Some(p) => cur = p,
            None => return true,
        }
    }
    false
}

/// The context of the repository (worktree) that contains `path`, a file or a directory.
pub fn open_shared(path: &Path) -> Result<Arc<App>> {
    let canon = path.canonicalize().with_context(|| format!("no such file or directory: {}", path.display()))?;
    let dir = if canon.is_dir() { canon } else { canon.parent().map(Path::to_path_buf).unwrap_or(canon) };
    {
        let mut g = lock(&OTHERS);
        if let Some(root) = g.root_of.get(&dir).cloned()
            && let Some(app) = g.touch(&root)
            && !app.stale()
            && !repo_appeared(&dir, &app.repo.root)
        {
            return Ok(app);
        }
    }
    // git runs outside the lock: concurrent MCP requests for other repositories do not wait on it
    let fresh = Arc::new(App::open(&dir)?);
    let mut g = lock(&OTHERS);
    let root = fresh.repo.root.clone();
    g.root_of.insert(dir, root.clone());
    g.tick += 1;
    let tick = g.tick;
    let slot = g.by_root.entry(root).or_insert_with(|| (fresh.clone(), tick));
    if slot.0.stale() {
        slot.0 = fresh;
    }
    slot.1 = tick;
    let app = slot.0.clone();
    g.evict();
    Ok(app)
}

pub struct App {
    pub repo: Repo,
    pub loaded: LoadedConfig,
    /// mtimes/sizes of every config layer when loaded — lets a long-running server notice edits
    pub config_stamp: String,
    /// The shared config files this context looked at (where `store.dir` pointed when opened):
    /// a stamp only stats them, it never parses a layer again to find them.
    shared_candidates: Vec<PathBuf>,
    /// The HEAD file's stamp when opened: a checkout makes the context (and its branch) stale.
    head_stamp: String,
    registry: OnceLock<Registry>,
    index: Mutex<Option<LocalIndex>>,
    last_refresh: Mutex<Option<Instant>>,
    /// Serializes writes to the store and inbox within one process (MCP requests run concurrently).
    pub write_lock: Mutex<()>,
}

impl App {
    pub fn open(dir: &Path) -> Result<App> {
        let repo = Repo::discover(dir)?;
        // stat before parsing: an edit landing while the layers are read makes the next stamp differ
        let shared_candidates = config::shared_config_candidates(&repo);
        let config_stamp = stamp_files(&layer_paths(&repo, &shared_candidates));
        let head_stamp = repo.head_stamp();
        let loaded = config::load(&repo)?;
        Ok(App {
            repo,
            loaded,
            config_stamp,
            shared_candidates,
            head_stamp,
            registry: OnceLock::new(),
            index: Mutex::new(None),
            last_refresh: Mutex::new(None),
            write_lock: Mutex::new(()),
        })
    }

    /// Whether a config layer changed since this context was opened. Stats files only.
    pub fn config_changed(&self) -> bool {
        self.config_stamp_now() != self.config_stamp
    }

    fn config_stamp_now(&self) -> String {
        stamp_files(&layer_paths(&self.repo, &self.shared_candidates))
    }

    /// Everything a reopen would pick up, as one string: the config layers and HEAD.
    pub fn stamp_now(&self) -> String {
        format!("{}|{}", self.config_stamp_now(), self.repo.head_stamp())
    }

    /// Whether a long-running process should open this repository again: a config layer changed,
    /// HEAD moved (another branch), or an adapter's `when` condition now evaluates differently.
    pub fn stale(&self) -> bool {
        self.config_changed()
            || self.repo.head_stamp() != self.head_stamp
            || self.registry.get().is_some_and(|r| r.when_changed(&self.repo, &self.loaded.cfg))
    }

    pub fn cfg(&self) -> &Config {
        &self.loaded.cfg
    }

    pub fn store(&self) -> Store<'_> {
        Store::new(&self.repo, &self.loaded.cfg)
    }

    /// Whether the repository has a knowledge store. Without one kontext only reads (docs and
    /// history); agents do not start a store in a repository nobody set kontext up for.
    pub fn is_set_up(&self) -> bool {
        self.repo.abs(self.loaded.cfg.store.dir.trim_end_matches('/')).is_dir()
    }

    pub fn registry(&self) -> &Registry {
        self.registry.get_or_init(|| Registry::build(&self.repo, &self.loaded.cfg))
    }

    pub fn registry_if_built(&self) -> Option<&Registry> {
        self.registry.get()
    }

    pub fn entries(&self) -> (Vec<Entry>, Vec<String>) {
        self.store().load_all()
    }

    pub fn project_name(&self) -> String {
        self.cfg().project_name(&self.repo)
    }

    /// Run `f` against a fresh index. Refreshes are throttled to one per second per process.
    pub fn with_index<T>(&self, f: impl FnOnce(&LocalIndex) -> Result<T>) -> Result<T> {
        let mut guard = lock(&self.index);
        if guard.is_none() {
            self.repo.prune_worktree_state();
            *guard = Some(LocalIndex::open(&self.repo)?);
        }
        let idx = guard.as_ref().expect("index opened above");
        // another process (a CLI capture, a second server) may have committed since this handle
        // last looked: reload the reader on every request, not only on a refresh
        if let Err(e) = idx.reload_if_changed() {
            eprintln!("kontext: index reload failed: {e:#}");
        }
        let mut last = lock(&self.last_refresh);
        if last.is_none_or(|t| t.elapsed() > Duration::from_secs(1)) {
            let (entries, _) = self.entries();
            if let Err(e) = idx.refresh(&self.repo, self.cfg(), &entries) {
                eprintln!("kontext: index refresh failed: {e:#}");
            }
            *last = Some(Instant::now());
        }
        f(idx)
    }

    pub fn warnings(&self) -> Vec<String> {
        let mut w = self.loaded.warnings.clone();
        // a redaction pattern that does not compile: distill, threads and autopilot refuse to run
        w.extend(crate::secrets::pattern_errors(&self.loaded));
        if let Some(reg) = self.registry_if_built() {
            w.extend(reg.errors.iter().cloned());
        }
        w
    }
}

fn layer_paths(repo: &Repo, shared: &[PathBuf]) -> Vec<PathBuf> {
    let mut paths = vec![config::user_config_path(), config::user_repo_config_path(repo), config::local_config_path(repo)];
    paths.extend(shared.iter().cloned());
    paths
}

fn stamp_files(paths: &[PathBuf]) -> String {
    paths
        .iter()
        .map(|p| match std::fs::metadata(p) {
            Ok(m) => format!("{}:{:?}:{};", p.display(), m.modified().ok(), m.len()),
            Err(_) => format!("{}:-;", p.display()),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    fn scratch(name: &str) -> PathBuf {
        let unique = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let root = std::env::temp_dir().join(format!("kontext-app-{name}-{}-{unique}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        assert!(Command::new("git").args(["init", "-q", "-b", "main"]).arg(&root).status().unwrap().success());
        root
    }

    #[test]
    fn a_checkout_makes_the_context_stale() {
        let root = scratch("head");
        let app = App::open(&root).unwrap();
        assert!(!app.stale());
        app.repo.git(&["-c", "user.name=T", "-c", "user.email=t@example.invalid", "commit", "--allow-empty", "-qm", "seed"]).unwrap();
        app.repo.git(&["checkout", "-qb", "feature/another-branch"]).unwrap();
        assert!(app.stale(), "the branch a capture records must be the current one");
        assert_eq!(App::open(&root).unwrap().repo.branch.as_deref(), Some("feature/another-branch"));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_config_edit_makes_the_context_stale_without_parsing() {
        let root = scratch("cfg");
        let app = App::open(&root).unwrap();
        let before = app.stamp_now();
        std::fs::create_dir_all(root.join(".ai")).unwrap();
        std::fs::write(root.join(".ai/kontext.toml"), "[project]\nname = \"x\"\n").unwrap();
        assert!(app.config_changed());
        assert_ne!(app.stamp_now(), before);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_poisoned_lock_is_recovered() {
        let m = std::sync::Arc::new(Mutex::new(1));
        let m2 = m.clone();
        let _ = std::thread::spawn(move || {
            let _g = m2.lock().unwrap();
            panic!("boom");
        })
        .join();
        assert!(m.is_poisoned());
        assert_eq!(*lock(&m), 1);
    }
}
