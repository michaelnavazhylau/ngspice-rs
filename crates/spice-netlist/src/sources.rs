//! Where deck and `.include`/`.lib` text comes from.
//!
//! [`crate::Parser::parse_file_with_sources`] resolves directives through a
//! [`SourceProvider`]. [`FileSystem`] is the operating-system file system that
//! [`crate::Parser::parse_file`] uses; [`MemorySources`] is an in-memory file
//! map for hosts without one (WebAssembly in a browser) and for tests.
//! Resolution semantics (source-relative paths, cycle identity, work limits)
//! do not depend on the provider.

use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::{self, Read};
use std::path::{Component, Path, PathBuf};

/// A source of deck text addressed by path.
pub trait SourceProvider {
    /// The canonical identity of an existing source. Two paths naming the same
    /// source must map to the same identity, because include-cycle detection
    /// keys on it. Fails when `path` does not name a source.
    ///
    /// # Errors
    /// The source does not exist or cannot be resolved.
    fn canonicalize(&self, path: &Path) -> io::Result<PathBuf>;

    /// Reads at most `limit` bytes of a canonical source. Returning more than
    /// `limit` bytes is allowed and is reported by the caller as an exhausted
    /// byte budget, so a provider may return `limit + 1` bytes to signal it.
    ///
    /// # Errors
    /// The source cannot be read.
    fn read(&self, path: &Path, limit: u64) -> io::Result<Vec<u8>>;
}

/// The operating-system file system.
#[derive(Debug, Clone, Copy, Default)]
pub struct FileSystem;

impl SourceProvider for FileSystem {
    fn canonicalize(&self, path: &Path) -> io::Result<PathBuf> {
        fs::canonicalize(path)
    }

    fn read(&self, path: &Path, limit: u64) -> io::Result<Vec<u8>> {
        let mut bytes = Vec::new();
        File::open(path)?
            .take(limit.saturating_add(1))
            .read_to_end(&mut bytes)?;
        Ok(bytes)
    }
}

/// An in-memory set of source files.
///
/// Paths are normalized lexically to absolute paths under `/`: `.` is
/// dropped, `..` removes one component (never above the root) and a relative
/// path is taken relative to `/`. So `models/a.lib`, `/models/a.lib` and
/// `/x/../models/./a.lib` all name the same file. There are no directories,
/// links or permissions; any path that was not inserted does not exist.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MemorySources {
    files: BTreeMap<PathBuf, String>,
}

impl MemorySources {
    /// An empty file set.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds or replaces a file and returns its normalized path.
    pub fn insert(&mut self, path: impl AsRef<Path>, text: impl Into<String>) -> PathBuf {
        let path = normalize(path.as_ref());
        self.files.insert(path.clone(), text.into());
        path
    }

    /// The text stored under `path`, if any.
    #[must_use]
    pub fn get(&self, path: impl AsRef<Path>) -> Option<&str> {
        self.files
            .get(&normalize(path.as_ref()))
            .map(String::as_str)
    }

    /// Number of files.
    #[must_use]
    pub fn len(&self) -> usize {
        self.files.len()
    }

    /// True when there are no files.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }
}

impl SourceProvider for MemorySources {
    fn canonicalize(&self, path: &Path) -> io::Result<PathBuf> {
        let path = normalize(path);
        if self.files.contains_key(&path) {
            Ok(path)
        } else {
            Err(io::Error::new(
                io::ErrorKind::NotFound,
                "no such file in the in-memory sources",
            ))
        }
    }

    fn read(&self, path: &Path, limit: u64) -> io::Result<Vec<u8>> {
        let text = self.files.get(&normalize(path)).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "no such file in the in-memory sources",
            )
        })?;
        let keep = usize::try_from(limit.saturating_add(1)).unwrap_or(usize::MAX);
        Ok(text.as_bytes()[..text.len().min(keep)].to_vec())
    }
}

fn normalize(path: &Path) -> PathBuf {
    let mut parts: Vec<&std::ffi::OsStr> = Vec::new();
    for component in path.components() {
        match component {
            Component::Prefix(_) | Component::RootDir => parts.clear(),
            Component::CurDir => {}
            Component::ParentDir => {
                parts.pop();
            }
            Component::Normal(part) => parts.push(part),
        }
    }
    let mut normalized = PathBuf::from("/");
    normalized.extend(parts);
    normalized
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_paths_normalize_to_one_identity() {
        let mut sources = MemorySources::new();
        let stored = sources.insert("models/a.lib", "* a\n");
        assert_eq!(stored, Path::new("/models/a.lib"));
        for spelling in [
            "/models/a.lib",
            "models/./a.lib",
            "/x/../models/a.lib",
            "../../models/a.lib",
        ] {
            assert_eq!(
                sources.canonicalize(Path::new(spelling)).unwrap(),
                stored,
                "{spelling}"
            );
        }
        assert_eq!(
            sources
                .canonicalize(Path::new("/models"))
                .unwrap_err()
                .kind(),
            io::ErrorKind::NotFound
        );
    }

    #[test]
    fn memory_reads_stop_one_byte_past_the_limit() {
        let mut sources = MemorySources::new();
        let path = sources.insert("a.cir", "0123456789");
        assert_eq!(sources.read(&path, 3).unwrap(), b"0123");
        assert_eq!(sources.read(&path, 100).unwrap(), b"0123456789");
    }
}
