//! Repository-relative paths, kept as bytes.
//!
//! Git paths are byte strings, `/`-separated, relative to the work tree, and
//! nothing guarantees they are UTF-8 — a file committed on Linux under a Latin-1
//! name is a valid Git path that no `String` can hold. Every path that comes out
//! of the object database or the index therefore travels as [`RepoPath`], and
//! only becomes an OS path at the one place that needs to touch the disk
//! ([`RepoPath::to_absolute`]).
//!
//! SPEC §9 asks for this to be centralised rather than done ad hoc, for a macOS
//! reason: APFS is case-insensitive by default while Git is case-sensitive, and
//! macOS decomposes Unicode (NFD) where Git stores what it was given (NFC).
//!
//! Who handles what, verified rather than assumed (see
//! `docs/notes/gitoxide-capabilities.md`):
//!
//! * **Unicode normalisation** is `gix`'s, through `core.precomposeUnicode`:
//!   its directory walk precomposes what it reads from the filesystem, so the
//!   paths arriving here are already in the form the index holds. This module
//!   must not re-normalise them — doing it twice is how a name stops matching
//!   its index entry.
//! * **Case** is the filesystem's. Nothing here folds case: `Readme.md` and
//!   `README.md` are two distinct Git paths, and on a case-insensitive volume it
//!   is Git's `core.ignorecase` that reconciles them, not us.
//! * **Escaping the work tree** is ours, and is the one thing this module
//!   actively refuses.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use gix::bstr::{BStr, BString, ByteSlice};

/// A path inside a repository: bytes, `/`-separated, relative to the work tree.
#[derive(Clone, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RepoPath(BString);

impl RepoPath {
    pub fn from_bytes(bytes: impl Into<BString>) -> Self {
        Self(bytes.into())
    }

    pub fn as_bytes(&self) -> &[u8] {
        self.0.as_bytes()
    }

    pub fn as_bstr(&self) -> &BStr {
        self.0.as_bstr()
    }

    /// The last `/`-separated component — what a file list shows in its first
    /// column.
    pub fn file_name(&self) -> &BStr {
        match self.as_bytes().rfind_byte(b'/') {
            Some(slash) => self.as_bytes()[slash + 1..].as_bstr(),
            None => self.as_bstr(),
        }
    }

    /// The directory part, without the trailing slash, or `None` at the root.
    pub fn parent(&self) -> Option<&BStr> {
        self.as_bytes()
            .rfind_byte(b'/')
            .map(|slash| self.as_bytes()[..slash].as_bstr())
    }

    /// Resolve against a work tree, refusing anything that would leave it.
    ///
    /// An absolute path, a `..` component or a drive-style prefix in an index
    /// entry means the index is corrupt or hostile; either way a client that
    /// joins it blindly writes outside the repository it was told to open. The
    /// check is cheap and runs on every conversion rather than at call sites,
    /// because a call site that forgets is exactly the bug this prevents.
    pub fn to_absolute(&self, work_dir: &Path) -> Option<PathBuf> {
        if self.0.is_empty() {
            return None;
        }
        let mut out = work_dir.to_path_buf();
        for component in self.as_bytes().split_str("/") {
            if component.is_empty() || component == b".." || component == b"." {
                return None;
            }
            out.push(bytes_to_os_str(component)?);
        }
        // A component could still be absolute on a platform where `push`
        // replaces rather than appends; refuse if the result escaped.
        out.starts_with(work_dir).then_some(out)
    }

    /// For messages and logs. Lossy on purpose: a name that is not UTF-8 must
    /// still be *nameable* in an error, and a replacement character is a better
    /// answer than no message at all.
    pub fn display_lossy(&self) -> std::borrow::Cow<'_, str> {
        self.0.to_str_lossy()
    }
}

impl std::fmt::Display for RepoPath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(&self.display_lossy(), f)
    }
}

/// `Debug` shows the same text as `Display`, quoted. The derived form would
/// print a byte array, which is unreadable in a test failure.
impl std::fmt::Debug for RepoPath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.display_lossy())
    }
}

impl From<&BStr> for RepoPath {
    fn from(value: &BStr) -> Self {
        Self(value.to_owned())
    }
}

impl From<&str> for RepoPath {
    fn from(value: &str) -> Self {
        Self(value.into())
    }
}

/// Bytes to an OS string, without going through `str`.
///
/// On Unix every byte sequence is a valid file name, so this never fails. The
/// function exists as the single place that knows that, so the rest of the crate
/// is free of `cfg`s — and so the day a non-Unix target is considered, exactly
/// one function has to answer for it (SPEC §2: Windows is out of scope, and
/// nothing here is shaped for it).
fn bytes_to_os_str(bytes: &[u8]) -> Option<&OsStr> {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        Some(OsStr::from_bytes(bytes))
    }
    #[cfg(not(unix))]
    {
        std::str::from_utf8(bytes).ok().map(OsStr::new)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn holds_paths_that_are_not_utf8() {
        // A Latin-1 "é" — a perfectly legal Git path, and not valid UTF-8.
        let path = RepoPath::from_bytes(b"caf\xe9/menu.txt".to_vec());
        assert_eq!(path.as_bytes(), b"caf\xe9/menu.txt");
        assert_eq!(path.file_name(), "menu.txt");
        assert_eq!(
            path.parent().map(ToString::to_string),
            Some("caf\u{fffd}".into())
        );
        assert!(
            path.display_lossy().contains('\u{fffd}'),
            "the undecodable byte is replaced, not dropped or panicked on"
        );
    }

    #[test]
    fn splits_on_slash_only() {
        // A backslash is an ordinary character in a Git path, not a separator.
        let path = RepoPath::from("dir\\not-a-dir/file.rs");
        assert_eq!(path.file_name(), "file.rs");
        assert_eq!(path.parent(), Some("dir\\not-a-dir".into()));

        let root = RepoPath::from("README.md");
        assert_eq!(root.file_name(), "README.md");
        assert_eq!(root.parent(), None);
    }

    #[test]
    fn resolves_under_the_work_tree() {
        let work_dir = Path::new("/tmp/repo");
        let file = RepoPath::from("src/main.rs");
        assert_eq!(
            file.to_absolute(work_dir),
            Some(PathBuf::from("/tmp/repo/src/main.rs"))
        );
    }

    #[test]
    fn refuses_to_leave_the_work_tree() {
        let work_dir = Path::new("/tmp/repo");
        for hostile in [
            "../outside",
            "src/../../outside",
            "/etc/passwd",
            "src//main.rs",
            "./main.rs",
            "",
        ] {
            assert_eq!(
                RepoPath::from(hostile).to_absolute(work_dir),
                None,
                "{hostile:?} must not resolve to a path on disk"
            );
        }
    }

    #[test]
    fn case_is_left_alone() {
        // Two distinct Git paths, whatever the filesystem thinks. A client that
        // folded them here would make the case-only rename of SPEC §9 invisible.
        assert_ne!(RepoPath::from("Readme.md"), RepoPath::from("README.md"));
    }
}
