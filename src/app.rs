//! Process-wide context: repository, configuration, lazily built adapter registry and search index.
use crate::adapters::Registry;
use crate::config::{self, Config, LoadedConfig};
use crate::index::LocalIndex;
use crate::repo::Repo;
use crate::store::{Entry, Store};
use anyhow::Result;
use std::path::Path;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

pub struct App {
    pub repo: Repo,
    pub loaded: LoadedConfig,
    /// mtimes/sizes of every config layer when loaded — lets a long-running server notice edits
    pub config_stamp: String,
    registry: OnceLock<Registry>,
    index: Mutex<Option<LocalIndex>>,
    last_refresh: Mutex<Option<Instant>>,
    /// Serializes writes to the store and inbox within one process (MCP requests run concurrently).
    pub write_lock: Mutex<()>,
}

impl App {
    pub fn open(dir: &Path) -> Result<App> {
        let repo = Repo::discover(dir)?;
        let loaded = config::load(&repo)?;
        let config_stamp = config_stamp(&repo);
        Ok(App {
            repo,
            loaded,
            config_stamp,
            registry: OnceLock::new(),
            index: Mutex::new(None),
            last_refresh: Mutex::new(None),
            write_lock: Mutex::new(()),
        })
    }

    pub fn config_changed(&self) -> bool {
        config_stamp(&self.repo) != self.config_stamp
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
        let mut guard = self.index.lock().unwrap();
        if guard.is_none() {
            self.repo.prune_worktree_state();
            *guard = Some(LocalIndex::open(&self.repo)?);
        }
        let idx = guard.as_ref().unwrap();
        let mut last = self.last_refresh.lock().unwrap();
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
        if let Some(reg) = self.registry_if_built() {
            w.extend(reg.errors.iter().cloned());
        }
        w
    }
}

pub fn config_stamp(repo: &Repo) -> String {
    let mut paths = vec![config::user_config_path(), config::user_repo_config_path(repo), config::local_config_path(repo)];
    paths.extend(config::shared_config_candidates(repo));
    paths
        .iter()
        .map(|p| match std::fs::metadata(p) {
            Ok(m) => format!("{}:{:?}:{};", p.display(), m.modified().ok(), m.len()),
            Err(_) => format!("{}:-;", p.display()),
        })
        .collect()
}
