//! What the backend holds between commands.
//!
//! Tauri's model is request/response: the front end asks, the backend answers.
//! So there is no `Entity<RepoStore>` observing itself into a redraw any more —
//! but SPEC §10's two rules still apply and are held here instead:
//!
//! * **One open repository handle per path.** Two would mean two watchers
//!   answering the same question differently.
//! * **Writes are serialised per repository.** A mutex per repository, held for
//!   the length of a write, which is the same guarantee the queue gave.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use omagit_git::cli::Git;
use omagit_git::journal::Journal;
use omagit_git::{Cancel, GitError, Repository};
use omagit_settings::{Library, Settings};

/// The one repository the window is looking at, and the lock that serialises
/// writes to it.
pub struct Open {
    pub repo: Repository,
    pub path: PathBuf,
    /// Held for the length of a write. SPEC §10: two writing Git commands must
    /// never run at once on one repository.
    pub write_lock: Mutex<()>,
    /// The history walk in progress, if the window is looking at one.
    ///
    /// Here rather than in a command because it is stateful: SPEC §11 wants
    /// history paged and resumed, not re-walked, and the lane assignment is
    /// incremental — restarting it per page would move a long-running branch
    /// to a different column every time the list scrolled.
    pub history: Mutex<Option<crate::log::Session>>,
    /// The filesystem watch on this repository (SPEC §10).
    ///
    /// Held here so it lives exactly as long as the handle does: closing a tab
    /// drops the handle, which drops the watcher, which stops its thread. Never
    /// read — the work happens on the other end of its channel — hence the
    /// underscore.
    _watch: Option<omagit_git::watch::Watcher>,
}

pub struct AppState {
    /// `None` when `git` is missing or too old. Reads still work — they are
    /// `gix` — so the app opens and shows history and diffs, and only writing
    /// is refused, with the reason (SPEC §8).
    git: Option<Arc<Git>>,
    git_error: Option<String>,
    journal: Journal,
    open: Mutex<HashMap<PathBuf, Arc<Open>>>,
    library: Mutex<Library>,
    settings: Mutex<Settings>,
    config_dir: Option<PathBuf>,
    /// How a change reaches the window, once there is one.
    ///
    /// A closure rather than the `AppHandle` itself, for two reasons that turn
    /// out to be the same one: the handle is generic over the runtime and this
    /// state is not, and it is that genericity that lets the bridge be tested
    /// against Tauri's mock runtime (`tests/watching.rs`). Set in `setup`,
    /// because `AppState` is built before the window exists — SPEC §8 wants the
    /// `git` check before anything is drawn.
    window: Mutex<Option<Announce>>,
    /// The token of the network operation in flight, if one is.
    ///
    /// One at a time, and that is a product decision as much as a technical
    /// one: two fetches on one repository race for `.git/FETCH_HEAD`, and a
    /// progress overlay that had to describe two things at once would describe
    /// neither. The second caller is refused with the name of the first.
    running: Mutex<Option<Running>>,
}

/// How a change under a repository is announced to the window.
type Announce = Arc<dyn Fn(crate::dto::Changed) + Send + Sync>;

/// A network operation in flight.
pub struct Running {
    pub what: String,
    pub cancel: Cancel,
}

impl AppState {
    /// Look for `git`, read the library and the settings. Called once, before
    /// the window exists — which is where SPEC §8 wants the `git` check, so an
    /// app that cannot write says so before anyone writes a commit message.
    pub fn detect(config_dir: Option<PathBuf>) -> Self {
        let journal = Journal::new();
        let (git, git_error) = match Git::detect() {
            Ok(git) => {
                let git = git.with_journal(journal.clone());
                tracing::info!(version = %git.version(), "git detected");
                (Some(Arc::new(git)), None)
            }
            Err(error) => {
                tracing::warn!(%error, "git is unusable; writes will be refused");
                (None, Some(error.to_string()))
            }
        };

        let library = config_dir.as_deref().map(Library::load).unwrap_or_default();
        let settings = config_dir
            .as_deref()
            .map(Settings::load)
            .unwrap_or_default();

        Self {
            git,
            git_error,
            journal,
            open: Mutex::new(HashMap::new()),
            library: Mutex::new(library),
            settings: Mutex::new(settings),
            config_dir,
            window: Mutex::new(None),
            running: Mutex::new(None),
        }
    }

    /// Hand the window over, once it exists. Called from `setup`.
    pub fn attach<R: tauri::Runtime>(&self, app: tauri::AppHandle<R>) {
        *self.lock(&self.window) = Some(Arc::new(move |changed| {
            use tauri::Emitter as _;
            // The window may be gone; nothing here depends on it arriving.
            let _ = app.emit("changed", changed);
        }));
    }

    /// The `git` binary, or the reason there is none.
    pub fn git(&self) -> Result<&Arc<Git>, GitError> {
        self.git.as_ref().ok_or(GitError::GitNotFound)
    }

    pub fn git_error(&self) -> Option<&str> {
        self.git_error.as_deref()
    }

    pub fn journal(&self) -> &Journal {
        &self.journal
    }

    /// Open a repository, or hand back the handle already held for it.
    pub fn open(&self, path: &Path) -> Result<Arc<Open>, GitError> {
        let path = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_owned());
        let mut open = self.lock(&self.open);
        if let Some(held) = open.get(&path) {
            return Ok(held.clone());
        }
        let repo = Repository::open(&path)?;
        let watch = self.watch(&repo, &path);
        let handle = Arc::new(Open {
            repo,
            path: path.clone(),
            write_lock: Mutex::new(()),
            history: Mutex::new(None),
            _watch: watch,
        });
        open.insert(path, handle.clone());
        Ok(handle)
    }

    /// Let go of a repository the window has closed.
    ///
    /// The handle is a `gix` repository and a lock, not a cache: nothing is
    /// lost by dropping it and it is rebuilt on the next open. Without this, a
    /// session that visited twenty repositories held twenty of them for the
    /// life of the window — and the tab strip made visiting twenty ordinary.
    ///
    /// A write in flight keeps its own `Arc`, so closing during one is safe:
    /// the handle outlives the map entry until that write returns.
    pub fn close(&self, path: &Path) {
        let path = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_owned());
        self.lock(&self.open).remove(&path);
    }

    /// Watch a repository, and tell the window what changed in it (SPEC §10).
    ///
    /// `omagit_git::watch` has done the hard half since M2 — debounced at
    /// 150 ms, `.gitignore` respected, `index.lock` ignored, and each change set
    /// labelled by what it invalidates — and nothing had ever called it. A
    /// client that only refreshes when clicked is wrong every time you touch a
    /// terminal, which is what this was.
    ///
    /// A failure is logged and left: the app still works, it just stops
    /// noticing. The most likely cause is the platform's watch limit, which is
    /// something the person can act on once told.
    fn watch(&self, repo: &Repository, path: &Path) -> Option<omagit_git::watch::Watcher> {
        let announce = self.lock(&self.window).clone()?;
        let (watcher, changes) = match omagit_git::watch::Watcher::start(repo) {
            Ok(started) => started,
            Err(error) => {
                tracing::warn!(%error, path = ?path, "no watch: this repository will not refresh itself");
                return None;
            }
        };

        let path = path.display().to_string();
        let started = std::thread::Builder::new()
            .name("omagit-watch-bridge".into())
            .spawn(move || {
                // Ends when the channel closes, which is when the watcher is
                // dropped — so the thread dies with the repository handle.
                while let Ok(changes) = changes.recv_blocking() {
                    announce(crate::dto::Changed {
                        path: path.clone(),
                        status: changes.invalidates_status(),
                        refs: changes.invalidates_refs(),
                    });
                }
            });
        if let Err(error) = started {
            tracing::warn!(%error, "no watch bridge: this repository will not refresh itself");
            return None;
        }
        Some(watcher)
    }

    pub fn with_library<T>(&self, act: impl FnOnce(&mut Library) -> T) -> T {
        let mut library = self.lock(&self.library);
        let outcome = act(&mut library);
        if let Some(dir) = &self.config_dir
            && let Err(error) = library.save(dir)
        {
            tracing::warn!(%error, "the repository list could not be saved");
        }
        outcome
    }

    /// Change the settings and write them out.
    ///
    /// The same shape as [`Self::with_library`], and for the same reason: a
    /// caller that had to remember to save would eventually not.
    pub fn with_settings<T>(&self, act: impl FnOnce(&mut Settings) -> T) -> T {
        let mut settings = self.lock(&self.settings);
        let outcome = act(&mut settings);
        if let Some(dir) = &self.config_dir
            && let Err(error) = settings.save(dir)
        {
            tracing::warn!(%error, "the settings could not be saved");
        }
        outcome
    }

    /// Any repository the window has open, for the questions that need one but
    /// do not care which — reading `core.editor`, say.
    ///
    /// `None` before the first is opened, which is a real state: the
    /// Preferences screen exists on the Repositories screen too.
    pub fn any_open(&self) -> Option<Repository> {
        self.lock(&self.open)
            .values()
            .next()
            .map(|held| held.repo.clone())
    }

    pub fn settings(&self) -> Settings {
        self.lock(&self.settings).clone()
    }

    /// A token for a read. Nothing cancels these: they finish in milliseconds,
    /// and a cancelled status would just be asked for again.
    pub fn cancel(&self) -> Cancel {
        Cancel::new()
    }

    /// Claim the one network slot, or say who has it.
    ///
    /// The guard releases it on drop, including on a panic, which is the whole
    /// reason it is a guard: an operation that failed to clear the slot would
    /// leave the app refusing every fetch until it restarted.
    pub fn start_network(&self, what: &str) -> std::result::Result<NetworkSlot<'_>, String> {
        let mut running = self.lock(&self.running);
        if let Some(current) = running.as_ref() {
            return Err(format!("omagit:refuse.networkBusy|{}", current.what));
        }
        let cancel = Cancel::new();
        *running = Some(Running {
            what: what.to_owned(),
            cancel: cancel.clone(),
        });
        Ok(NetworkSlot {
            state: self,
            cancel,
        })
    }

    /// Stop whatever is running on the network. Nothing to stop is not an
    /// error: the button is pressed at the moment an operation ends often
    /// enough that treating it as one would be noise.
    pub fn cancel_network(&self) -> Option<String> {
        let running = self.lock(&self.running);
        running.as_ref().map(|current| {
            current.cancel.cancel();
            current.what.clone()
        })
    }

    /// What is running on the network, if anything.
    pub fn network(&self) -> Option<String> {
        self.lock(&self.running)
            .as_ref()
            .map(|current| current.what.clone())
    }

    /// A poisoned lock is not worth taking the app down for: nothing here is
    /// held across code that can panic, so recovering the guard is safe.
    fn lock<'a, T>(&self, mutex: &'a Mutex<T>) -> std::sync::MutexGuard<'a, T> {
        mutex
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

/// Holds the network slot for as long as the operation runs.
///
/// A guard rather than a pair of calls, so the slot is released on a panic as
/// well as on a return: an operation that failed to clear it would leave the
/// app refusing every fetch until it restarted.
pub struct NetworkSlot<'a> {
    state: &'a AppState,
    pub cancel: Cancel,
}

impl Drop for NetworkSlot<'_> {
    fn drop(&mut self) {
        *self.state.lock(&self.state.running) = None;
    }
}
