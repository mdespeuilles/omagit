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
        }
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
        let handle = Arc::new(Open {
            repo,
            path: path.clone(),
            write_lock: Mutex::new(()),
            history: Mutex::new(None),
        });
        open.insert(path, handle.clone());
        Ok(handle)
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

    pub fn settings(&self) -> Settings {
        self.lock(&self.settings).clone()
    }

    /// A token that nothing cancels yet.
    ///
    /// Cancellation arrives with the progress overlay at M7; until then every
    /// read runs to completion, which is what the GPUI build did too outside a
    /// refresh.
    pub fn cancel(&self) -> Cancel {
        Cancel::new()
    }

    /// A poisoned lock is not worth taking the app down for: nothing here is
    /// held across code that can panic, so recovering the guard is safe.
    fn lock<'a, T>(&self, mutex: &'a Mutex<T>) -> std::sync::MutexGuard<'a, T> {
        mutex
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}
