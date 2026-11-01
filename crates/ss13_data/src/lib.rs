//! Runtime data files (prototypes, generated `.ron` data, maps, the manifest, atlas pages, sounds,
//! admin config) by path. Natively every call is the `std::fs` one, read lazily as before. A
//! browser build has no filesystem: its entry [`mount`]s the downloaded bundle before the app is
//! built, and from then on every read is served from it by the path relative to the bundle root
//! (`docs/WEB.md`).
//!
//! A process-wide mount rather than a Bevy resource: the loaders are plain functions called from
//! plugin `build`, converters and tests, long before (or without) any `World`.

use std::collections::BTreeSet;
use std::io;
use std::path::{Component, Path, PathBuf};
use std::sync::OnceLock;

type Reader = Box<dyn Fn(&str) -> Option<Vec<u8>> + Send + Sync>;

static MOUNT: OnceLock<Memory> = OnceLock::new();

/// Serves every later read from `read` (bytes by bundle key, `None`: missing) instead of the
/// disk. `keys` lists every file, for directory listings and existence checks. Once per
/// process; `false` if something is already mounted.
pub fn mount(
    keys: impl IntoIterator<Item = String>,
    read: impl Fn(&str) -> Option<Vec<u8>> + Send + Sync + 'static,
) -> bool {
    MOUNT.set(Memory::new(keys, Box::new(read))).is_ok()
}

pub fn read(path: &Path) -> io::Result<Vec<u8>> {
    match MOUNT.get() {
        Some(memory) => memory.read(path),
        None => std::fs::read(path),
    }
}

pub fn read_to_string(path: &Path) -> io::Result<String> {
    match MOUNT.get() {
        Some(memory) => String::from_utf8(memory.read(path)?)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e)),
        None => std::fs::read_to_string(path),
    }
}

/// The entries of `dir` as `dir.join(name)`, in no particular order (as `std::fs::read_dir`).
pub fn read_dir(dir: &Path) -> io::Result<Vec<PathBuf>> {
    match MOUNT.get() {
        Some(memory) => memory.read_dir(dir),
        None => std::fs::read_dir(dir)?
            .map(|entry| entry.map(|e| e.path()))
            .collect(),
    }
}

pub fn is_dir(path: &Path) -> bool {
    match MOUNT.get() {
        Some(memory) => memory.is_dir(path),
        None => path.is_dir(),
    }
}

pub fn is_file(path: &Path) -> bool {
    match MOUNT.get() {
        Some(memory) => memory.keys.contains(&key(path)),
        None => path.is_file(),
    }
}

pub fn exists(path: &Path) -> bool {
    is_file(path) || is_dir(path)
}

struct Memory {
    keys: BTreeSet<String>,
    read: Reader,
}

impl Memory {
    fn new(keys: impl IntoIterator<Item = String>, read: Reader) -> Self {
        Self {
            keys: keys.into_iter().map(|k| key(Path::new(&k))).collect(),
            read,
        }
    }

    fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
        let key = key(path);
        if !self.keys.contains(&key) {
            return Err(not_found(path));
        }
        (self.read)(&key).ok_or_else(|| not_found(path))
    }

    /// The first path component under `dir` of every key below it (repeated per file).
    fn children(&self, dir: &Path) -> impl Iterator<Item = &str> {
        let key = key(dir);
        let prefix = if key.is_empty() { key } else { key + "/" };
        let skip = prefix.len();
        self.keys
            .range(prefix.clone()..)
            .take_while(move |k| k.starts_with(&prefix))
            .filter_map(move |k| k[skip..].split('/').next().filter(|name| !name.is_empty()))
    }

    fn read_dir(&self, dir: &Path) -> io::Result<Vec<PathBuf>> {
        let names: BTreeSet<&str> = self.children(dir).collect();
        if names.is_empty() {
            return Err(not_found(dir));
        }
        Ok(names.into_iter().map(|n| dir.join(n)).collect())
    }

    fn is_dir(&self, path: &Path) -> bool {
        self.children(path).next().is_some()
    }
}

/// `a/./b` and `a\b` (Windows) both name `a/b`; `..` is kept, so it never matches a key.
fn key(path: &Path) -> String {
    let mut out = String::new();
    for c in path.components() {
        let part = match c {
            Component::CurDir | Component::RootDir | Component::Prefix(_) => continue,
            Component::ParentDir => "..",
            Component::Normal(s) => &*s.to_string_lossy(),
        };
        if !out.is_empty() {
            out.push('/');
        }
        out.push_str(part);
    }
    out
}

fn not_found(path: &Path) -> io::Error {
    io::Error::new(
        io::ErrorKind::NotFound,
        format!("{} is not in the data bundle", path.display()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn memory() -> Memory {
        let files = [
            "manifest.postcard",
            "prototypes/a.ron",
            "prototypes/sub/b.ron",
        ];
        Memory::new(
            files.map(str::to_owned),
            Box::new(move |k| files.contains(&k).then(|| k.as_bytes().to_vec())),
        )
    }

    #[test]
    fn reads_by_normalized_relative_path() {
        let m = memory();
        assert_eq!(
            m.read(Path::new("./prototypes/a.ron")).unwrap(),
            b"prototypes/a.ron"
        );
        assert_eq!(
            m.read(&Path::new("").join("manifest.postcard")).unwrap(),
            b"manifest.postcard"
        );
        let missing = m.read(Path::new("prototypes")).unwrap_err();
        assert_eq!(missing.kind(), io::ErrorKind::NotFound);
        assert!(
            m.read(Path::new("prototypes/../manifest.postcard"))
                .is_err()
        );
    }

    #[test]
    fn lists_files_and_directories_one_level_deep() {
        let m = memory();
        assert_eq!(
            m.read_dir(Path::new("prototypes")).unwrap(),
            [PathBuf::from("prototypes/a.ron"), "prototypes/sub".into()]
        );
        assert_eq!(
            m.read_dir(Path::new("")).unwrap(),
            [PathBuf::from("manifest.postcard"), "prototypes".into()]
        );
        assert!(m.is_dir(Path::new("prototypes/sub")));
        assert!(!m.is_dir(Path::new("prototypes/a.ron")));
        assert!(!m.is_dir(Path::new("proto")));
        assert!(m.read_dir(Path::new("nothing")).is_err());
    }
}
