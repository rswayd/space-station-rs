//! Licensing of converted assets: BandaStation's assets are CC BY-SA 3.0 unless an
//! `attributions.txt` / `attribution.txt` / `license.txt` next to them says otherwise, so every
//! converter that copies or derives assets keeps those files, at the source paths, next to its
//! output (`ASSETS-LICENSE.md`).

use std::collections::BTreeSet;
use std::io;
use std::path::{Path, PathBuf};

/// File names (any case) that carry attributions or license terms in the BandaStation tree.
pub const ATTRIBUTION_FILES: [&str; 3] = ["attributions.txt", "attribution.txt", "license.txt"];

pub fn is_attribution_file(name: &str) -> bool {
    ATTRIBUTION_FILES
        .iter()
        .any(|f| f.eq_ignore_ascii_case(name))
}

/// The source checkout's commit, for the `SOURCES` lists.
pub fn source_commit(source: &Path) -> String {
    std::process::Command::new("git")
        .arg("-C")
        .arg(source)
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
        .unwrap_or_else(|| "unknown".to_owned())
}

/// The directories whose attribution files apply to a set of copied or derived files: each
/// file's directory and all its parents up to the source root.
#[derive(Debug, Default)]
pub struct Attributions {
    /// `(source dir, destination dir)`, both relative.
    dirs: BTreeSet<(PathBuf, PathBuf)>,
}

impl Attributions {
    /// `found`: the file's path in the source tree; `rel`: where its attributions go, relative
    /// to the output root (`/`-separated, the same depth as `found`; it differs only in case
    /// when the source was resolved case-insensitively like BYOND does).
    pub fn add(&mut self, found: &Path, rel: &str) {
        let src_parts: Vec<_> = found
            .parent()
            .into_iter()
            .flat_map(Path::components)
            .collect();
        let dst_parts: Vec<&str> = rel.split('/').collect();
        for depth in 0..=src_parts.len().min(dst_parts.len().saturating_sub(1)) {
            let src_dir: PathBuf = src_parts[..depth].iter().collect();
            let dst_dir: PathBuf = dst_parts[..depth].iter().collect();
            self.dirs.insert((src_dir, dst_dir));
        }
    }

    /// Copies the attribution files of every collected directory from `source` to `out`, at
    /// the destination dirs; returns how many were copied.
    pub fn copy(&self, source: &Path, out: &Path) -> io::Result<usize> {
        let mut copied = 0;
        for (src_dir, dst_dir) in &self.dirs {
            let Ok(entries) = std::fs::read_dir(source.join(src_dir)) else {
                continue;
            };
            for entry in entries {
                let entry = entry?;
                let name = entry.file_name();
                if !entry.file_type()?.is_file() || !is_attribution_file(&name.to_string_lossy()) {
                    continue;
                }
                std::fs::create_dir_all(out.join(dst_dir))?;
                std::fs::copy(entry.path(), out.join(dst_dir).join(&name))?;
                copied += 1;
            }
        }
        Ok(copied)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parents_attributions_are_copied_in_any_case() {
        let dir = std::env::temp_dir().join(format!("ss13-license-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let (source, out) = (dir.join("src"), dir.join("out"));
        std::fs::create_dir_all(source.join("icons/obj")).unwrap();
        std::fs::write(source.join("icons/attributions.txt"), "root").unwrap();
        std::fs::write(source.join("icons/obj/LICENSE.txt"), "obj").unwrap();
        std::fs::write(source.join("icons/obj/readme.txt"), "not a license").unwrap();
        let mut attributions = Attributions::default();
        attributions.add(Path::new("icons/obj/Doors.dmi"), "icons/obj/doors.dmi");
        assert_eq!(attributions.copy(&source, &out).unwrap(), 2);
        assert!(out.join("icons/attributions.txt").is_file());
        assert!(out.join("icons/obj/LICENSE.txt").is_file());
        assert!(!out.join("icons/obj/readme.txt").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
