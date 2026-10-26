//! Map untrusted recovered names into a private output tree without collisions.
use crate::{safe_name, ApiResult, Entry};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
};

pub fn plan(entries: &[Entry]) -> Vec<ApiResult<PathBuf>> {
    let mut directories = BTreeMap::<Vec<String>, PathBuf>::new();
    let mut occupied = BTreeSet::<String>::new();
    entries
        .iter()
        .enumerate()
        .map(|(index, entry)| {
            let mut components = vec![entry
                .partition
                .clone()
                .unwrap_or_else(|| "Unknown partition".into())];
            if let Some(path) = &entry.relative_path {
                let parts: Vec<_> = path.split('/').collect();
                if path.len() > 8192
                    || parts.len() > 64
                    || parts
                        .iter()
                        .any(|p| p.is_empty() || *p == "." || *p == "..")
                {
                    return Err("Invalid recovered directory path".into());
                }
                components.extend(
                    parts[..parts.len().saturating_sub(1)]
                        .iter()
                        .map(|p| (*p).to_owned()),
                );
            } else {
                components.push("Recovered by type".into());
                components.push(entry.kind.to_uppercase());
            }
            let mut identity = Vec::new();
            let mut parent = PathBuf::new();
            for component in components {
                identity.push(component.clone());
                if let Some(existing) = directories.get(&identity) {
                    parent = existing.clone();
                    continue;
                }
                let base = safe_name(&component);
                let mut candidate = parent.join(&base);
                let mut suffix = 2u64;
                while !occupied.insert(candidate.to_string_lossy().to_lowercase()) {
                    candidate = parent.join(format!("{base} ({suffix})"));
                    suffix += 1;
                }
                directories.insert(identity.clone(), candidate.clone());
                parent = candidate;
            }
            // Stable per-selection prefix also distinguishes duplicate recovered file names.
            let base = format!("{:06}_{}", index + 1, safe_name(&entry.name));
            let mut candidate = parent.join(&base);
            let mut suffix = 2u64;
            while !occupied.insert(candidate.to_string_lossy().to_lowercase()) {
                candidate = parent.join(format!("{base} ({suffix})"));
                suffix += 1;
            }
            Ok(candidate)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn entry(path: &str) -> Entry {
        Entry {
            name: "clip.mp4".into(),
            relative_path: Some(path.into()),
            partition: Some("USB".into()),
            ..Default::default()
        }
    }
    #[test]
    fn preserves_tree_and_separates_case_and_sanitization_collisions() {
        let paths = plan(&[
            entry("Photos/clip.mp4"),
            entry("Photos/clip.mp4"),
            entry("photos/clip.mp4"),
            entry("a:b/clip.mp4"),
            entry("a?b/clip.mp4"),
        ]);
        let paths: Vec<_> = paths.into_iter().map(Result::unwrap).collect();
        assert_eq!(paths[0].parent(), paths[1].parent());
        assert_eq!(paths[0], PathBuf::from("USB/Photos/000001_clip.mp4"));
        let folded: BTreeSet<_> = paths
            .iter()
            .map(|p| p.to_string_lossy().to_lowercase())
            .collect();
        assert_eq!(folded.len(), paths.len());
        assert_ne!(paths[0].parent(), paths[2].parent());
        assert_ne!(paths[3].parent(), paths[4].parent());
    }
    #[test]
    fn files_and_directories_share_the_collision_namespace() {
        for entries in [
            vec![
                entry("Photos/clip.mp4"),
                entry("Photos/000001_clip.mp4/other.jpg"),
            ],
            vec![
                entry("Photos/000002_clip.mp4/other.jpg"),
                entry("Photos/clip.mp4"),
            ],
        ] {
            let paths: Vec<_> = plan(&entries).into_iter().map(Result::unwrap).collect();
            assert!(!paths[0].starts_with(&paths[1]));
            assert!(!paths[1].starts_with(&paths[0]));
        }
    }
    #[test]
    fn rejects_escape_paths_and_bounds_depth() {
        for p in ["../clip.mp4", "/clip.mp4", "a//clip.mp4", "./clip.mp4"] {
            assert!(plan(&[entry(p)])[0].is_err());
        }
        assert!(plan(&[entry(&format!("{}clip.mp4", "a/".repeat(65)))])[0].is_err());
    }
}
