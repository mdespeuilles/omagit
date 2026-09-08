//! The list of repositories, and the groups they are filed under.
//!
//! Separate from [`Settings`](crate::Settings) and in its own file, because the
//! two change for different reasons: preferences are edited a handful of times
//! in a lifetime, this is written every time a repository is opened, added or
//! dragged. Keeping them apart means a corrupted write to one cannot take the
//! other with it.
//!
//! It holds only what the user put there — paths, names, descriptions, order.
//! Everything else on the Repositories screen (the branch, the ahead/behind,
//! the status counts) is read from the repository itself: a cached branch name
//! is wrong the moment the user checks out another one in a terminal.

use std::path::{Path, PathBuf};

use crate::SettingsError;

/// Where a repository sits: which group, and where in it.
///
/// Index-based rather than by path, because that is what drag and drop moves
/// and what the keyboard walks. Positions shift when the library changes, so a
/// `Location` is only valid until the next edit.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Location {
    pub group: usize,
    pub index: usize,
}

/// Every repository the user has added, in the order they arranged them.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Library {
    #[serde(rename = "group")]
    pub groups: Vec<Group>,
}

/// A user group: a folder in the sidebar, collapsible and reorderable.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Group {
    pub name: String,
    pub collapsed: bool,
    #[serde(rename = "repository")]
    pub repositories: Vec<Entry>,
}

impl Default for Group {
    fn default() -> Self {
        Self {
            name: Library::DEFAULT_GROUP.to_owned(),
            collapsed: false,
            repositories: Vec::new(),
        }
    }
}

/// One repository in the list.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Entry {
    /// Absolute, and canonicalised when it was added. The repository can still
    /// disappear from it — DESIGN §4 says such an entry keeps its row with a
    /// struck-through icon and is *never* silently removed.
    pub path: PathBuf,
    /// What the sidebar shows. Defaults to the directory's name, and the user
    /// may not rename it yet (M9): two clones of the same project in different
    /// folders are told apart by their paths in the card.
    pub name: String,
    /// The user's own note, edited in place on the card. Not read from the
    /// repository: Git has no such field.
    pub description: String,
    /// Unix seconds, or `None` for a repository that has never been opened.
    pub last_opened: Option<i64>,
}

impl Default for Entry {
    fn default() -> Self {
        Self {
            path: PathBuf::new(),
            name: String::new(),
            description: String::new(),
            last_opened: None,
        }
    }
}

impl Entry {
    /// A new entry for `path`, named after its directory.
    pub fn new(path: impl Into<PathBuf>) -> Self {
        let path = path.into();
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            // A repository at the filesystem root has no directory name; the
            // full path is a poor label but a better one than an empty row.
            .unwrap_or_else(|| path.display().to_string());
        Self {
            path,
            name,
            ..Self::default()
        }
    }
}

impl Library {
    pub const FILE_NAME: &'static str = "repositories.toml";

    /// The group a newly added repository lands in when the user has made none.
    pub const DEFAULT_GROUP: &'static str = "Récents";

    /// Read the library.
    ///
    /// A file that cannot be parsed is **moved aside**, not ignored. Falling
    /// back to an empty library the way preferences do would mean the next
    /// write silently replaces a list the user spent time arranging — the
    /// failure mode SPEC §15 calls data loss, arriving through the back door.
    pub fn load(dir: &Path) -> Self {
        let file = dir.join(Self::FILE_NAME);
        let text = match std::fs::read_to_string(&file) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Self::default(),
            Err(error) => {
                tracing::warn!(path = %file.display(), %error, "repository list unreadable");
                return Self::default();
            }
        };
        match toml::from_str(&text) {
            Ok(library) => library,
            Err(error) => {
                let kept = file.with_extension("toml.damaged");
                let moved = std::fs::rename(&file, &kept);
                tracing::error!(
                    path = %file.display(),
                    kept = %kept.display(),
                    saved = moved.is_ok(),
                    %error,
                    "repository list malformed; the file was set aside rather than overwritten"
                );
                Self::default()
            }
        }
    }

    pub fn save(&self, dir: &Path) -> Result<PathBuf, SettingsError> {
        std::fs::create_dir_all(dir).map_err(|source| SettingsError::Io {
            path: dir.to_owned(),
            source,
        })?;
        let file = dir.join(Self::FILE_NAME);
        let text = toml::to_string_pretty(self)?;
        std::fs::write(&file, text).map_err(|source| SettingsError::Io {
            path: file.clone(),
            source,
        })?;
        Ok(file)
    }

    pub fn is_empty(&self) -> bool {
        self.groups
            .iter()
            .all(|group| group.repositories.is_empty())
    }

    pub fn len(&self) -> usize {
        self.groups
            .iter()
            .map(|group| group.repositories.len())
            .sum()
    }

    pub fn get(&self, at: Location) -> Option<&Entry> {
        self.groups.get(at.group)?.repositories.get(at.index)
    }

    pub fn get_mut(&mut self, at: Location) -> Option<&mut Entry> {
        self.groups
            .get_mut(at.group)?
            .repositories
            .get_mut(at.index)
    }

    /// Every entry in display order, with where it is.
    pub fn iter(&self) -> impl Iterator<Item = (Location, &Entry)> {
        self.groups.iter().enumerate().flat_map(|(group, g)| {
            g.repositories
                .iter()
                .enumerate()
                .map(move |(index, entry)| (Location { group, index }, entry))
        })
    }

    pub fn find(&self, path: &Path) -> Option<Location> {
        self.iter()
            .find(|(_, entry)| entry.path == path)
            .map(|(at, _)| at)
    }

    /// File a repository under `group`, or under the default group.
    ///
    /// Adding one that is already listed is not an error and not a duplicate:
    /// it returns where the existing one is, which is what lets "add" and
    /// "reveal in the list" be the same gesture.
    pub fn add(&mut self, entry: Entry, group: Option<usize>) -> Location {
        if let Some(existing) = self.find(&entry.path) {
            return existing;
        }
        let group = match group.filter(|index| *index < self.groups.len()) {
            Some(index) => index,
            None => {
                if self.groups.is_empty() {
                    self.groups.push(Group::default());
                }
                0
            }
        };
        self.groups[group].repositories.push(entry);
        Location {
            group,
            index: self.groups[group].repositories.len() - 1,
        }
    }

    /// Take a repository out of the list. The repository on disk is untouched —
    /// this is the only "remove" on this screen, and it destroys nothing.
    pub fn remove(&mut self, at: Location) -> Option<Entry> {
        let group = self.groups.get_mut(at.group)?;
        (at.index < group.repositories.len()).then(|| group.repositories.remove(at.index))
    }

    /// Move a repository to `to`, which may be in another group.
    ///
    /// `to.index` is read *after* the entry has been lifted out, which is what
    /// makes dropping a row one place further down inside its own group behave
    /// the way the insertion line showed it would.
    pub fn move_entry(&mut self, from: Location, to: Location) -> Option<Location> {
        if to.group >= self.groups.len() {
            return None;
        }
        let entry = self.remove(from)?;
        let group = &mut self.groups[to.group];
        let index = to.index.min(group.repositories.len());
        group.repositories.insert(index, entry);
        Some(Location {
            group: to.group,
            index,
        })
    }

    /// Add a group, and return its index. Names are not unique — two groups
    /// called "Work" are the user's business, not an error to refuse.
    pub fn add_group(&mut self, name: impl Into<String>) -> usize {
        self.groups.push(Group {
            name: name.into(),
            ..Group::default()
        });
        self.groups.len() - 1
    }

    /// Remove a group and everything filed under it.
    ///
    /// Destructive in the sense of SPEC §3 rule 7 — the entries go with it — so
    /// the caller confirms first. Nothing on disk is touched.
    pub fn remove_group(&mut self, index: usize) -> Option<Group> {
        (index < self.groups.len()).then(|| self.groups.remove(index))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn library() -> Library {
        let mut library = Library::default();
        library.add(Entry::new("/src/alpha"), None);
        library.add(Entry::new("/src/beta"), None);
        let work = library.add_group("Travail");
        library.add(Entry::new("/work/gamma"), Some(work));
        library
    }

    #[test]
    fn an_entry_is_named_after_its_directory() {
        assert_eq!(Entry::new("/home/u/src/omagit").name, "omagit");
        assert_eq!(Entry::new("/home/u/src/omagit").description, "");
        assert_eq!(Entry::new("/home/u/src/omagit").last_opened, None);
    }

    #[test]
    fn the_first_repository_creates_the_default_group() {
        let mut library = Library::default();
        assert!(library.is_empty());
        let at = library.add(Entry::new("/src/alpha"), None);
        assert_eq!(at, Location { group: 0, index: 0 });
        assert_eq!(library.groups[0].name, Library::DEFAULT_GROUP);
        assert_eq!(library.len(), 1);
    }

    #[test]
    fn adding_a_repository_twice_finds_it_instead_of_duplicating_it() {
        let mut library = library();
        let again = library.add(Entry::new("/src/beta"), None);
        assert_eq!(again, Location { group: 0, index: 1 });
        assert_eq!(library.len(), 3, "still three");
    }

    #[test]
    fn walks_every_entry_in_display_order() {
        let library = library();
        let names: Vec<&str> = library
            .iter()
            .map(|(_, entry)| entry.name.as_str())
            .collect();
        assert_eq!(names, vec!["alpha", "beta", "gamma"]);
    }

    #[test]
    fn moves_a_repository_between_groups() {
        let mut library = library();
        let alpha = library.find(Path::new("/src/alpha")).expect("listed");
        let to = library
            .move_entry(alpha, Location { group: 1, index: 0 })
            .expect("the target group exists");

        assert_eq!(to, Location { group: 1, index: 0 });
        assert_eq!(library.groups[0].repositories.len(), 1);
        assert_eq!(library.groups[1].repositories[0].name, "alpha");
        assert_eq!(library.len(), 3, "moving is not adding");
    }

    #[test]
    fn moves_a_repository_within_its_group() {
        let mut library = library();
        // alpha, beta → beta, alpha
        library
            .move_entry(
                Location { group: 0, index: 0 },
                Location { group: 0, index: 1 },
            )
            .expect("a move inside one group");
        let names: Vec<&str> = library.groups[0]
            .repositories
            .iter()
            .map(|entry| entry.name.as_str())
            .collect();
        assert_eq!(names, vec!["beta", "alpha"]);
    }

    #[test]
    fn a_drop_past_the_end_lands_at_the_end() {
        let mut library = library();
        let at = library
            .move_entry(
                Location { group: 0, index: 0 },
                Location {
                    group: 1,
                    index: 99,
                },
            )
            .expect("clamped, not refused");
        assert_eq!(at, Location { group: 1, index: 1 });
    }

    #[test]
    fn removing_takes_only_the_entry() {
        let mut library = library();
        let removed = library
            .remove(Location { group: 0, index: 0 })
            .expect("it was there");
        assert_eq!(removed.name, "alpha");
        assert_eq!(library.len(), 2);
        assert!(library.find(Path::new("/src/alpha")).is_none());
        assert!(library.remove(Location { group: 9, index: 0 }).is_none());
    }

    #[test]
    fn round_trips_through_disk() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut library = library();
        library
            .get_mut(Location { group: 0, index: 0 })
            .unwrap()
            .description = "a description with = and \" in it".into();
        library
            .get_mut(Location { group: 0, index: 0 })
            .unwrap()
            .last_opened = Some(1_788_873_573);
        library.groups[1].collapsed = true;

        library.save(dir.path()).expect("save");
        assert_eq!(Library::load(dir.path()), library);
    }

    #[test]
    fn an_absent_file_is_an_empty_library() {
        let dir = tempfile::tempdir().expect("tempdir");
        assert_eq!(Library::load(dir.path()), Library::default());
    }

    #[test]
    fn a_malformed_file_is_set_aside_rather_than_overwritten() {
        // Falling back to "empty" the way preferences do would mean the next
        // save replaces a list the user spent time arranging.
        let dir = tempfile::tempdir().expect("tempdir");
        let file = dir.path().join(Library::FILE_NAME);
        std::fs::write(&file, "[[group]]\nname = \"Travail\"\nthis is not toml {{{")
            .expect("write");

        assert_eq!(Library::load(dir.path()), Library::default());
        assert!(
            !file.exists(),
            "the damaged file is not left to be overwritten"
        );
        let kept = file.with_extension("toml.damaged");
        assert!(kept.exists(), "it is kept next to it");
        assert!(
            std::fs::read_to_string(kept)
                .expect("readable")
                .contains("Travail"),
            "with the user's data intact"
        );
    }
}
