use anyhow::{Context, Result};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use crate::commit::get_commit;
use crate::index::Index;
use crate::objects;
use crate::tree::load_tree_files;

pub fn checkout(repo_path: &Path, branch: &str) -> Result<()> {
    let local_branch_path = repo_path.join("refs").join("heads").join(branch);

    let remote_branch_path = repo_path
        .join("refs")
        .join("remotes")
        .join("origin")
        .join(branch);

    let branch_path;

    // ------------------------------------------------------------
    // 1. Local branch exists
    // ------------------------------------------------------------

    if local_branch_path.exists() {
        branch_path = local_branch_path;
    }
    // ------------------------------------------------------------
    // 2. Local branch doesn't exist, but remote branch exists
    // ------------------------------------------------------------
    else if remote_branch_path.exists() {
        let commit_hash = fs::read_to_string(&remote_branch_path)?.trim().to_string();

        if commit_hash.is_empty() {
            anyhow::bail!("Remote branch 'origin/{}' has no commits", branch);
        }

        if let Some(parent) = local_branch_path.parent() {
            fs::create_dir_all(parent)?;
        }

        fs::write(&local_branch_path, &commit_hash)?;

        println!("Created local branch '{}' from 'origin/{}'", branch, branch);

        branch_path = local_branch_path;
    }
    // ------------------------------------------------------------
    // 3. Neither local nor remote branch exists
    // ------------------------------------------------------------
    else {
        anyhow::bail!("Branch '{}' does not exist locally or on origin", branch);
    }

    let commit_hash = fs::read_to_string(&branch_path)?.trim().to_string();

    if commit_hash.is_empty() {
        anyhow::bail!("Branch '{}' has no commits", branch);
    }

    let commit = get_commit(repo_path, &commit_hash)?;

    let files = load_tree_files(repo_path, &commit.tree)?;

    // ------------------------------------------------------------
    // 4. Make sure checkout will not destroy work
    // ------------------------------------------------------------

    ensure_clean_working_tree(repo_path, &files)?;

    // ------------------------------------------------------------
    // 5. Restore target working tree
    // ------------------------------------------------------------

    restore_working_tree(repo_path, &files)?;

    // ------------------------------------------------------------
    // 6. Rebuild index
    // ------------------------------------------------------------

    let mut index = Index::new();

    for (path, hash) in &files {
        index.add(path.clone(), hash.clone());
    }

    index.save(repo_path)?;

    // ------------------------------------------------------------
    // 7. Update HEAD
    // ------------------------------------------------------------

    fs::write(
        repo_path.join("HEAD"),
        format!("ref: refs/heads/{}\n", branch),
    )?;

    println!("Switched to branch '{}'", branch);

    Ok(())
}

// ============================================================
// CHECK FOR UNCOMMITTED / UNSAFE CHANGES
// ============================================================

fn ensure_clean_working_tree(
    repo_path: &Path,
    target_files: &BTreeMap<String, String>,
) -> Result<()> {
    let repo_root = repo_path
        .parent()
        .context("Invalid repository path")?
        .canonicalize()?;

    let index = Index::load(repo_path)?;

    // ------------------------------------------------------------
    // Check tracked files for modifications
    // ------------------------------------------------------------

    for (path, entry) in &index.entries {
        let file_path = repo_root.join(path);

        if !file_path.exists() {
            anyhow::bail!(
                "Cannot checkout: local changes would be lost (deleted '{}')",
                path
            );
        }

        if file_path.is_file() {
            let current_hash = objects::hash_file(&file_path)?;

            if current_hash != entry.blob_hash {
                anyhow::bail!(
                    "Cannot checkout: local changes would be overwritten in '{}'",
                    path
                );
            }
        }
    }

    // ------------------------------------------------------------
    // Check untracked files that target branch would overwrite
    // ------------------------------------------------------------

    for target_path in target_files.keys() {
        let file_path = repo_root.join(target_path);

        if file_path.exists() && !index.contains(target_path) {
            anyhow::bail!(
                "Cannot checkout: untracked file '{}' would be overwritten",
                target_path
            );
        }
    }

    Ok(())
}

// ============================================================
// RESTORE WORKING TREE
// ============================================================

fn restore_working_tree(repo_path: &Path, files: &BTreeMap<String, String>) -> Result<()> {
    let repo_root = repo_path
        .parent()
        .context("Invalid repository path")?
        .canonicalize()?;

    // Load the current index so we know which files
    // are currently tracked.
    let current_index = Index::load(repo_path)?;

    // ------------------------------------------------------------
    // 1. Remove tracked files that do not exist
    //    in the target commit
    // ------------------------------------------------------------

    for path in current_index.entries.keys() {
        if !files.contains_key(path) {
            let file_path = repo_root.join(path);

            if file_path.exists() {
                fs::remove_file(&file_path)?;
                println!("  - {}", path);
            }
        }
    }

    // ------------------------------------------------------------
    // 2. Restore files from target commit
    // ------------------------------------------------------------

    for (path, blob_hash) in files {
        let file_path = repo_root.join(path);

        if let Some(parent) = file_path.parent() {
            fs::create_dir_all(parent)?;
        }

        let blob_path = repo_path.join("objects").join("blobs").join(blob_hash);

        if !blob_path.exists() {
            anyhow::bail!("Missing blob object {}", blob_hash);
        }

        let data = fs::read(&blob_path)?;

        fs::write(&file_path, data)?;
    }

    Ok(())
}
