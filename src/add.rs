use crate::ignore::RustyIgnore;
use crate::index::Index;
use crate::merge::MergeState;
use crate::objects;
use anyhow::{Context, Result};
use std::path::Path;
use walkdir::WalkDir;

pub fn add_file(file_path: &Path, repo_path: &Path) -> Result<()> {
    let repo_root = repo_path
        .parent()
        .context("Invalid repository path")?
        .canonicalize()?;

    let ignore = RustyIgnore::load(&repo_root);
    let mut index = Index::load(repo_path)?;

    // Load MERGE_STATE if a merge is in progress.
    let mut merge_state = MergeState::load(repo_path)?;

    let mut added_count = 0;

    let cwd = std::env::current_dir()?.canonicalize()?;
    let target_path = if file_path.is_absolute() {
        file_path.to_path_buf()
    } else {
        cwd.join(file_path)
    };

    if target_path.exists() {
        let canonical_target = target_path.canonicalize()?;
        if !canonical_target.starts_with(&repo_root) {
            anyhow::bail!("File is outside the repository");
        }

        if canonical_target.is_dir() {
            let rel_dir = if canonical_target == repo_root {
                String::new()
            } else {
                canonical_target
                    .strip_prefix(&repo_root)?
                    .to_string_lossy()
                    .replace('\\', "/")
            };

            // 1. Walk working tree to stage added / modified files.
            for entry in WalkDir::new(&canonical_target) {
                let entry = entry?;
                let path = entry.path();

                if !path.is_file() {
                    continue;
                }

                let canonical = match path.canonicalize() {
                    Ok(c) => c,
                    Err(_) => continue,
                };

                let rel_path = canonical
                    .strip_prefix(&repo_root)?
                    .to_string_lossy()
                    .replace('\\', "/");

                // Hardcoded internal ignores.
                if rel_path.split('/').any(|c| c == ".rusty" || c == ".git") {
                    continue;
                }

                // If not already tracked, respect .rustyignore.
                if !index.contains(&rel_path) && ignore.is_ignored(&rel_path, false) {
                    continue;
                }

                let blob_hash = objects::hash_objects(&canonical, repo_path)?;
                index.add(rel_path.clone(), blob_hash);

                // Mark as resolved in MERGE_STATE.
                if let Some(ref mut state) = merge_state {
                    state.resolve_path(&rel_path);
                }

                println!("Added {}", rel_path);
                added_count += 1;
            }

            // 2. Stage deletions: remove tracked files in this directory
            //    that no longer exist on disk.
            let mut to_remove = Vec::new();
            for path in index.entries.keys() {
                let in_scope = if rel_dir.is_empty() {
                    true
                } else {
                    path == &rel_dir || path.starts_with(&format!("{}/", rel_dir))
                };

                if in_scope {
                    let disk_path = repo_root.join(path);
                    if !disk_path.exists() {
                        to_remove.push(path.clone());
                    }
                }
            }

            for path in to_remove {
                index.remove(&path);

                // A staged deletion also resolves the conflict for that path.
                if let Some(ref mut state) = merge_state {
                    state.resolve_path(&path);
                }

                println!("Removed {}", path);
                added_count += 1;
            }
        } else {
            // Single file exists on disk.
            let rel_path = canonical_target
                .strip_prefix(&repo_root)?
                .to_string_lossy()
                .replace('\\', "/");

            if rel_path.split('/').any(|c| c == ".rusty" || c == ".git") {
                anyhow::bail!("Cannot add internal repository files");
            }

            if !index.contains(&rel_path) && ignore.is_ignored(&rel_path, false) {
                println!(
                    "The following path is ignored by .rustyignore:\n  {}",
                    rel_path
                );
                return Ok(());
            }

            let blob_hash = objects::hash_objects(&canonical_target, repo_path)?;
            index.add(rel_path.clone(), blob_hash);

            // Mark as resolved in MERGE_STATE.
            if let Some(ref mut state) = merge_state {
                state.resolve_path(&rel_path);
            }

            println!("Added {}", rel_path);
            added_count += 1;
        }
    } else {
        // Target does NOT exist on disk.
        // Check if it corresponds to an existing tracked file in the index.
        let rel_candidate = if file_path.is_absolute() {
            if let Ok(rel) = file_path.strip_prefix(&repo_root) {
                rel.to_string_lossy().replace('\\', "/")
            } else {
                anyhow::bail!("File is outside the repository");
            }
        } else {
            // Try relative from cwd.
            let full = cwd.join(file_path);
            if let Ok(rel) = full.strip_prefix(&repo_root) {
                rel.to_string_lossy().replace('\\', "/")
            } else {
                file_path.to_string_lossy().replace('\\', "/")
            }
        };

        if index.contains(&rel_candidate) {
            index.remove(&rel_candidate);

            // Staging a deletion resolves the conflict for that path.
            if let Some(ref mut state) = merge_state {
                state.resolve_path(&rel_candidate);
            }

            println!("Removed {}", rel_candidate);
            added_count += 1;
        } else {
            anyhow::bail!("Path not found: {}", file_path.display());
        }
    }

    index.save(repo_path)?;

    // Persist updated MERGE_STATE.
    if let Some(ref state) = merge_state {
        state.save(repo_path)?;
    }

    if added_count == 0 {
        println!("Nothing added");
    }

    Ok(())
}
