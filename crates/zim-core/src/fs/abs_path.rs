//! Type-level absolute path enforcement.
//!
//! [`AbsPath`] wraps a `PathBuf` that is guaranteed to be absolute. All
//! public [`Fs`](super::Fs) APIs take `&AbsPath`, so callers can't
//! accidentally pass a relative path into the tree-traversal code.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// A validated absolute path. Constructable via [`AbsPath::new`] (which
/// returns `None` for relative inputs) or [`AbsPath::from_abs`] (which
/// debug-asserts and is meant for paths that are absolute by
/// construction). No runtime panic in release builds.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct AbsPath(PathBuf);

impl AbsPath {
    /// Validate `path` is absolute and wrap it. Returns `None` for a
    /// relative path.
    ///
    /// ```
    /// # use zim_core::fs::AbsPath;
    /// assert!(AbsPath::new("/foo").is_some());
    /// assert!(AbsPath::new("foo").is_none());
    /// ```
    pub fn new(path: impl Into<PathBuf>) -> Option<Self> {
        let p = path.into();
        p.is_absolute().then_some(Self(p))
    }

    /// Wrap a `PathBuf` that is known to be absolute by construction
    /// (e.g. built via `Path::new("/").join(rel)`). Debug-asserts the
    /// invariant; in release the assertion is a no-op.
    pub fn from_abs(path: PathBuf) -> Self {
        debug_assert!(
            path.is_absolute(),
            "AbsPath::from_abs called with relative path: {}",
            path.display()
        );
        Self(path)
    }

    /// The filesystem root, `/`.
    pub fn root() -> Self {
        Self(PathBuf::from("/"))
    }

    /// The path with the leading `/` stripped. `Path::new("/foo")` →
    /// `Path::new("foo")`; the root returns an empty `Path`.
    pub fn relative(&self) -> &Path {
        self.0.strip_prefix("/").unwrap_or(&self.0)
    }

    /// Append a relative segment, returning a new [`AbsPath`].
    pub fn join(&self, segment: impl AsRef<Path>) -> Self {
        Self(self.0.join(segment))
    }

    /// Unwrap the inner `PathBuf`.
    pub fn into_inner(self) -> PathBuf {
        self.0
    }

    /// Split into `(parent, name)`. Returns `None` for the root, which
    /// has no parent and no name.
    ///
    /// Used by every fs op that needs to mutate the tree: look up the
    /// parent dir via [`Fs::get_dir_at_path`](super::Fs::get_dir_at_path),
    /// then insert/remove the named child.
    ///
    /// ```
    /// # use zim_core::fs::AbsPath;
    /// let path = AbsPath::new("/a/b").unwrap();
    /// let (parent, name) = path.split().unwrap();
    /// assert_eq!(parent, AbsPath::new("/a").unwrap());
    /// assert_eq!(name, "b");
    ///
    /// assert!(AbsPath::root().split().is_none());
    /// ```
    pub fn split(&self) -> Option<(AbsPath, String)> {
        let rel = self.relative();
        let parent = rel.parent()?;
        let name = rel.file_name()?.to_string_lossy().to_string();
        Some((AbsPath::from_abs(Path::new("/").join(parent)), name))
    }
}

impl AsRef<Path> for AbsPath {
    fn as_ref(&self) -> &Path {
        &self.0
    }
}

impl std::ops::Deref for AbsPath {
    type Target = Path;
    fn deref(&self) -> &Path {
        &self.0
    }
}

impl std::fmt::Display for AbsPath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.display().fmt(f)
    }
}
