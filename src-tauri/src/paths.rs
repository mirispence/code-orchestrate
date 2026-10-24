//! The one shared path normaliser and comparison (spec §6.1). Every path the
//! app stores or compares (Workspace paths, former paths, dismissed paths,
//! transcript `cwd`s) goes through [`normalize`] first.

use std::fmt;
use std::path::{Component, Path, Prefix};

use serde::{Serialize, Serializer};

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum PathError {
    #[error("The path is empty")]
    Empty,
    #[error("The path cannot be made absolute: {0}")]
    NotAbsolute(String),
}

/// A path in canonical display form: absolute, `.`/`..` resolved lexically,
/// `\` separators and an uppercase drive letter on Windows, no trailing
/// separator except on roots. Symlinks and junctions are left unresolved.
///
/// Equality (`==`) is exact; use [`NormalizedPath::same_as`] or
/// [`NormalizedPath::comparison_key`] for the platform's path comparison.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct NormalizedPath(String);

/// Normalise a raw path. Relative paths resolve against the current directory.
pub fn normalize(raw: &str) -> Result<NormalizedPath, PathError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(PathError::Empty);
    }
    // `std::path::absolute` is lexical (no symlink resolution) on every platform.
    let absolute = std::path::absolute(trimmed)
        .map_err(|e| PathError::NotAbsolute(format!("{trimmed}: {e}")))?;
    Ok(NormalizedPath(render(&absolute)))
}

impl NormalizedPath {
    /// Wrap a path the store saved after normalising it.
    pub(crate) fn from_stored(path: String) -> Self {
        NormalizedPath(path)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn as_path(&self) -> &Path {
        Path::new(&self.0)
    }

    /// The form two paths are compared by: case-folded on Windows, exact elsewhere.
    pub fn comparison_key(&self) -> String {
        if cfg!(windows) { self.0.to_lowercase() } else { self.0.clone() }
    }

    /// The same location under the platform's comparison rules.
    pub fn same_as(&self, other: &NormalizedPath) -> bool {
        self.comparison_key() == other.comparison_key()
    }

    /// A strict ancestor, compared by whole path segments: `E:\code` is an
    /// ancestor of `E:\code\x` but not of `E:\code-orchestrate`.
    pub fn is_ancestor_of(&self, other: &NormalizedPath) -> bool {
        let mine = self.comparison_key();
        let theirs = other.comparison_key();
        if mine.len() >= theirs.len() || !theirs.starts_with(&mine) {
            return false;
        }
        // Roots already end in a separator; otherwise the next char must be one.
        mine.ends_with(SEPARATOR) || theirs[mine.len()..].starts_with(SEPARATOR)
    }
}

impl fmt::Display for NormalizedPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl Serialize for NormalizedPath {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}

const SEPARATOR: char = std::path::MAIN_SEPARATOR;

/// Rebuild an absolute path from its components, resolving `.` and `..`
/// lexically and never climbing above the root.
fn render(path: &Path) -> String {
    let mut prefix = String::new();
    let mut has_root = false;
    let mut segments: Vec<String> = Vec::new();
    for component in path.components() {
        match component {
            Component::Prefix(p) => prefix = render_prefix(p.kind(), p.as_os_str()),
            Component::RootDir => has_root = true,
            Component::CurDir => {}
            Component::ParentDir => {
                segments.pop();
            }
            Component::Normal(s) => segments.push(s.to_string_lossy().into_owned()),
        }
    }
    let mut out = prefix;
    if has_root {
        out.push(SEPARATOR);
    }
    out.push_str(&segments.join(&SEPARATOR.to_string()));
    out
}

fn render_prefix(kind: Prefix<'_>, raw: &std::ffi::OsStr) -> String {
    match kind {
        Prefix::Disk(letter) => format!("{}:", (letter as char).to_ascii_uppercase()),
        Prefix::VerbatimDisk(letter) => format!(r"\\?\{}:", (letter as char).to_ascii_uppercase()),
        _ => raw.to_string_lossy().replace('/', r"\"),
    }
}
