use crate::commit::get_commit;
use crate::ignore::RustyIgnore;
use crate::index::Index;
use crate::objects;
use crate::repository::get_head_commit;
use crate::tree::load_tree_files;
use anyhow::{Context, Result};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use walkdir::WalkDir;

pub fn check_status(repo_path: &Path) -> Result<()> {
    let repo_root = repo_path
        .parent()
        .context("Invalid repo path")?
        .canonicalize()?;

    let ignore = RustyIgnore::load(&repo_root);
    let index = Index::load(repo_path)?;

    // 1. Get HEAD files (if any)
    let head_commit = get_head_commit(repo_path)?;
    let head_files: BTreeMap<String, String> = match &head_commit {
        Some(commit_hash) => {
            let commit = get_commit(repo_path, commit_hash)?;
            load_tree_files(repo_path, &commit.tree)?
        }
        None => BTreeMap::new(),
    };

    // Show detached HEAD notice if applicable
    let head_path = repo_path.join("HEAD");
    if head_path.exists() {
        let head_content = fs::read_to_string(&head_path).unwrap_or_default();
        let head_content = head_content.trim();
        if !head_content.starts_with("ref: ") && !head_content.is_empty() {
            println!(
                "HEAD detached at {}",
                &head_content[..8.min(head_content.len())]
            );
        }
    }

    // 2. Discover working tree files (that exist on disk)
    let mut disk_files: BTreeMap<String, String> = BTreeMap::new();
    let mut untracked_files: Vec<String> = Vec::new();

    for entry in WalkDir::new(&repo_root) {
        let entry = entry?;
        let path = entry.path();

        if !path.is_file() {
            continue;
        }

        let canonical = match path.canonicalize() {
            Ok(c) => c,
            Err(_) => continue,
        };

        let rel_path = match canonical.strip_prefix(&repo_root) {
            Ok(p) => p.to_string_lossy().replace('\\', "/"),
            Err(_) => continue,
        };

        // Skip internal .rusty and .git
        if rel_path.split('/').any(|c| c == ".rusty" || c == ".git") {
            continue;
        }

        let is_tracked = index.contains(&rel_path) || head_files.contains_key(&rel_path);

        if is_tracked {
            let hash = objects::hash_file(&canonical)?;
            disk_files.insert(rel_path, hash);
        } else if !ignore.is_ignored(&rel_path, false) {
            untracked_files.push(rel_path);
        }
    }

    // 3. Staged changes: HEAD vs Index
    let mut staged_new = Vec::new();
    let mut staged_modified = Vec::new();
    let mut staged_deleted = Vec::new();

    for (path, entry) in &index.entries {
        match head_files.get(path) {
            Some(head_hash) => {
                if &entry.blob_hash != head_hash {
                    staged_modified.push(path.clone());
                }
            }
            None => {
                staged_new.push(path.clone());
            }
        }
    }

    for path in head_files.keys() {
        if !index.contains(path) {
            staged_deleted.push(path.clone());
        }
    }

    // 4. Unstaged changes: Index vs Working Tree
    let mut unstaged_modified = Vec::new();
    let mut unstaged_deleted = Vec::new();

    for (path, entry) in &index.entries {
        match disk_files.get(path) {
            Some(disk_hash) => {
                if disk_hash != &entry.blob_hash {
                    unstaged_modified.push(path.clone());
                }
            }
            None => {
                unstaged_deleted.push(path.clone());
            }
        }
    }

    // 5. Output
    let mut has_changes = false;

    if !staged_new.is_empty() || !staged_modified.is_empty() || !staged_deleted.is_empty() {
        has_changes = true;
        println!("Changes to be committed:");
        for path in &staged_new {
            println!("  New file: {} (staged new file)", path);
        }
        for path in &staged_modified {
            println!("  Modified: {} (staged modification)", path);
        }
        for path in &staged_deleted {
            println!("  Deleted: {} (staged deletion)", path);
        }
        println!();
    }

    if !unstaged_modified.is_empty() || !unstaged_deleted.is_empty() {
        has_changes = true;
        println!("Changes not staged for commit:");
        for path in &unstaged_modified {
            println!("  Modified: {} (unstaged modification)", path);
        }
        for path in &unstaged_deleted {
            println!("  Deleted: {} (unstaged deletion)", path);
        }
        println!();
    }

    if !untracked_files.is_empty() {
        has_changes = true;
        println!("Untracked files:");
        for path in &untracked_files {
            println!("  Untracked: {}", path);
        }
        println!();
    }

    if !has_changes {
        println!("Working tree is clean");
    }

    Ok(())
}
