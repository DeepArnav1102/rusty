use anyhow::{Context, Result};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use crate::commit::Commit;
use crate::index::Index;
use crate::objects;
use crate::repository::get_head_commit;
use crate::tree::load_tree_files;

pub fn checkout(repo_path: &Path, target: &str) -> Result<()> {
    let repo_root = repo_path
        .parent()
        .context("Invalid repository path")?
        .canonicalize()?;

    let (commit_hash, is_branch) = resolve_target(repo_path, target)?;

    println!(
        "Checking out {}...",
        &commit_hash[..8.min(commit_hash.len())]
    );

    let commit_path = repo_path.join("objects").join("commits").join(&commit_hash);

    if !commit_path.exists() {
        anyhow::bail!("Commit not found: {}", commit_hash);
    }

    let commit_data = fs::read(&commit_path)?;
    let commit: Commit = serde_json::from_slice(&commit_data).context("Invalid commit object")?;

    let target_files = load_tree_files(repo_path, &commit.tree)?;

    // Load current repository state
    let current_index = Index::load(repo_path)?;
    let current_head_commit = get_head_commit(repo_path)?;
    let current_head_files: BTreeMap<String, String> = match &current_head_commit {
        Some(h) => {
            let c = crate::commit::get_commit(repo_path, h)?;
            load_tree_files(repo_path, &c.tree)?
        }
        None => BTreeMap::new(),
    };

    // ─────────────────────────────────────────────────────────────
    // VALIDATION: Protect user data before modifying working tree
    // ─────────────────────────────────────────────────────────────
    let mut conflicts = Vec::new();

    // 1. Check tracked files for uncommitted local modifications or deletions
    for (path, index_entry) in &current_index.entries {
        let full_path = repo_root.join(path);
        if full_path.exists() && full_path.is_file() {
            let disk_hash = objects::hash_file(&full_path)?;
            // If disk is modified compared to index
            if disk_hash != index_entry.blob_hash {
                // If target commit has a different version or doesn't have it
                if target_files.get(path) != Some(&disk_hash) {
                    conflicts.push(format!(
                        "local changes to '{}' would be overwritten by checkout",
                        path
                    ));
                }
            } else if let Some(head_hash) = current_head_files.get(path) {
                // If staged change compared to current HEAD
                if &index_entry.blob_hash != head_hash
                    && target_files.get(path) != Some(&index_entry.blob_hash)
                {
                    conflicts.push(format!(
                        "staged changes to '{}' would be overwritten by checkout",
                        path
                    ));
                }
            }
        } else if !full_path.exists() {
            // Tracked file deleted locally
            if target_files.contains_key(path) {
                conflicts.push(format!(
                    "local deletion of '{}' would be overwritten by checkout",
                    path
                ));
            }
        }
    }

    // 2. Check untracked files on disk that would be overwritten by target commit
    for (path, _target_hash) in &target_files {
        if !current_index.contains(path) && !current_head_files.contains_key(path) {
            let full_path = repo_root.join(path);
            if full_path.exists() && full_path.is_file() {
                conflicts.push(format!(
                    "untracked file '{}' would be overwritten by checkout",
                    path
                ));
            }
        }
    }

    // 3. Check staged deletions: path in HEAD but absent from current Index means
    //    the user staged a deletion. If the target commit restores that path, reject.
    for (path, _head_hash) in &current_head_files {
        if !current_index.contains(path) && target_files.contains_key(path) {
            conflicts.push(format!(
                "staged deletion of '{}' would be overwritten by checkout",
                path
            ));
        }
    }

    if !conflicts.is_empty() {
        anyhow::bail!(
            "Checkout aborted to prevent data loss:\n  {}",
            conflicts.join("\n  ")
        );
    }

    // ─────────────────────────────────────────────────────────────
    // PREFLIGHT: Verify all target blobs exist before any mutation
    // ─────────────────────────────────────────────────────────────
    for (path, blob_hash) in &target_files {
        let blob_path = repo_path.join("objects").join("blobs").join(blob_hash);
        if !blob_path.exists() {
            anyhow::bail!(
                "Blob object missing for '{}' ({}): aborting checkout",
                path, blob_hash
            );
        }
        // Verify the blob is readable (catches corruption / permission errors)
        fs::read(&blob_path).with_context(|| {
            format!(
                "Blob object unreadable for '{}' ({}): aborting checkout",
                path, blob_hash
            )
        })?;
    }

    // ─────────────────────────────────────────────────────────────
    // EXECUTION: Atomic update
    // ─────────────────────────────────────────────────────────────

    // Step A: Remove tracked files that no longer exist in the target commit
    for path in current_index.entries.keys() {
        if !target_files.contains_key(path) {
            let full_path = repo_root.join(path);
            if full_path.exists() {
                fs::remove_file(&full_path)?;
                println!("  - {}", path);
                clean_empty_parents(&repo_root, &full_path);
            }
        }
    }

    // Step B: Write target files from blobs to working tree
    for (path, blob_hash) in &target_files {
        let full_path = repo_root.join(path);
        let blob_path = repo_path.join("objects").join("blobs").join(blob_hash);

        // Blob was pre-verified above; this bail is a safety net only
        if !blob_path.exists() {
            anyhow::bail!("Blob object not found: {}", blob_hash);
        }

        let data = fs::read(&blob_path)?;
        if let Some(parent) = full_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&full_path, data)?;
        println!("  ✓ {}", path);
    }

    // Step C: Update Index to represent the checked-out commit
    let mut new_index = Index::new();
    for (path, blob_hash) in &target_files {
        new_index.add(path.clone(), blob_hash.clone());
    }
    new_index.save(repo_path)?;

    // Step D: Update HEAD
    let head_path = repo_path.join("HEAD");
    if is_branch {
        fs::write(head_path, format!("ref: refs/heads/{}\n", target))?;
    } else {
        fs::write(head_path, format!("{}\n", commit_hash))?;
    }

    println!("✓ Checked out {}", &commit_hash[..8.min(commit_hash.len())]);

    Ok(())
}

fn resolve_target(repo_path: &Path, target: &str) -> Result<(String, bool)> {
    // 1. Local branch
    let branch_path = repo_path.join("refs").join("heads").join(target);
    if branch_path.exists() {
        let hash = fs::read_to_string(branch_path)?;
        let hash = hash.trim();
        if hash.is_empty() {
            anyhow::bail!("Branch '{}' has no commits", target);
        }
        return Ok((hash.to_string(), true));
    }

    // 2. Commit hash directly
    let commit_path = repo_path.join("objects").join("commits").join(target);
    if commit_path.exists() {
        return Ok((target.to_string(), false));
    }

    // Prefix match for commit hash (e.g. 7-8 char hash)
    let commits_dir = repo_path.join("objects").join("commits");
    if commits_dir.exists() && target.len() >= 4 {
        let mut matches = Vec::new();
        for entry in fs::read_dir(&commits_dir)? {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with(target) {
                matches.push(name);
            }
        }
        if matches.len() == 1 {
            return Ok((matches.into_iter().next().unwrap(), false));
        } else if matches.len() > 1 {
            anyhow::bail!("Ambiguous commit prefix '{}'", target);
        }
    }

    // 3. Remote branch
    let remote_branch_path = repo_path
        .join("refs")
        .join("remotes")
        .join(target.replace('/', std::path::MAIN_SEPARATOR_STR));
    if remote_branch_path.exists() {
        let hash = fs::read_to_string(remote_branch_path)?;
        let hash = hash.trim();
        if hash.is_empty() {
            anyhow::bail!("Remote branch '{}' has no commits", target);
        }
        return Ok((hash.to_string(), false));
    }

    anyhow::bail!(
        "Unknown checkout target '{}'. Expected a commit hash, local branch, or remote branch.",
        target
    );
}

fn clean_empty_parents(repo_root: &Path, file_path: &Path) {
    let mut current = file_path.parent();
    while let Some(dir) = current {
        if dir == repo_root {
            break;
        }
        if let Ok(mut entries) = fs::read_dir(dir) {
            if entries.next().is_none() {
                let _ = fs::remove_dir(dir);
            } else {
                break;
            }
        } else {
            break;
        }
        current = dir.parent();
    }
}
