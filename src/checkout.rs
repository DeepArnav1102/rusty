use anyhow::{Context, Result};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use crate::commit::Commit;
use crate::tree::Tree;

pub fn checkout(repo_path: &Path, target: &str) -> Result<()> {
    let repo_root = repo_path
        .parent()
        .context("Invalid repository path")?;

    let commit_hash = resolve_target(repo_path, target)?;

    println!(
        "Checking out {}...",
        &commit_hash[..8.min(commit_hash.len())]
    );

    let commit_path = repo_path
        .join("objects")
        .join("commits")
        .join(&commit_hash);

    if !commit_path.exists() {
        anyhow::bail!("Commit not found: {}", commit_hash);
    }

    let commit_data = fs::read(&commit_path)?;

    let commit: Commit =
        serde_json::from_slice(&commit_data)
            .context("Invalid commit object")?;

    // Collect every file that should exist after checkout.
    let mut expected_files = HashSet::new();

    restore_tree(
        repo_path,
        repo_root,
        &commit.tree,
        Path::new(""),
        &mut expected_files,
    )?;

    // Remove tracked files that no longer exist in the target commit.
    remove_deleted_files(repo_root, &expected_files)?;

    // Update HEAD / branch reference.
    update_head(repo_path, target, &commit_hash)?;

    println!(
        "✓ Checked out {}",
        &commit_hash[..8.min(commit_hash.len())]
    );

    Ok(())
}

fn resolve_target(repo_path: &Path, target: &str) -> Result<String> {
    // First: treat target as a commit hash.
    let commit_path = repo_path
        .join("objects")
        .join("commits")
        .join(target);

    if commit_path.exists() {
        return Ok(target.to_string());
    }

    // Second: local branch.
    let branch_path = repo_path
        .join("refs")
        .join("heads")
        .join(target);

    if branch_path.exists() {
        let hash = fs::read_to_string(branch_path)?;

        let hash = hash.trim();

        if hash.is_empty() {
            anyhow::bail!("Branch '{}' has no commits", target);
        }

        return Ok(hash.to_string());
    }

    // Third: remote branch such as origin/main.
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

        return Ok(hash.to_string());
    }

    anyhow::bail!(
        "Unknown checkout target '{}'. Expected a commit hash, local branch, or remote branch.",
        target
    );
}

fn restore_tree(
    repo_path: &Path,
    repo_root: &Path,
    tree_hash: &str,
    current_path: &Path,
    expected_files: &mut HashSet<PathBuf>,
) -> Result<()> {
    let tree_path = repo_path
        .join("objects")
        .join("trees")
        .join(tree_hash);

    if !tree_path.exists() {
        anyhow::bail!("Tree object not found: {}", tree_hash);
    }

    let data = fs::read(&tree_path)?;

    let tree: Tree =
        serde_json::from_slice(&data)
            .with_context(|| format!("Invalid tree object {}", tree_hash))?;

    for entry in tree.entries {
        let relative_path = current_path.join(&entry.name);
        let full_path = repo_root.join(&relative_path);

        match entry.object_type.as_str() {
            "blob" => {
                restore_blob(
                    repo_path,
                    &full_path,
                    &relative_path,
                    &entry.object_hash,
                    expected_files,
                )?;
            }

            "tree" => {
                fs::create_dir_all(&full_path)?;

                restore_tree(
                    repo_path,
                    repo_root,
                    &entry.object_hash,
                    &relative_path,
                    expected_files,
                )?;
            }

            other => {
                anyhow::bail!(
                    "Unknown tree entry type '{}' for '{}'",
                    other,
                    relative_path.display()
                );
            }
        }
    }

    Ok(())
}

fn restore_blob(
    repo_path: &Path,
    full_path: &Path,
    relative_path: &Path,
    blob_hash: &str,
    expected_files: &mut HashSet<PathBuf>,
) -> Result<()> {
    let blob_path = repo_path
        .join("objects")
        .join("blobs")
        .join(blob_hash);

    if !blob_path.exists() {
        anyhow::bail!(
            "Blob object not found: {}",
            blob_hash
        );
    }

    let data = fs::read(&blob_path)?;

    if let Some(parent) = full_path.parent() {
        fs::create_dir_all(parent)?;
    }

    fs::write(full_path, data)?;

    expected_files.insert(relative_path.to_path_buf());

    println!("  ✓ {}", relative_path.display());

    Ok(())
}

fn remove_deleted_files(
    repo_root: &Path,
    expected_files: &HashSet<PathBuf>,
) -> Result<()> {
    let mut files_to_remove = Vec::new();

    collect_files(
        repo_root,
        repo_root,
        expected_files,
        &mut files_to_remove,
    )?;

    for file in files_to_remove {
        fs::remove_file(&file)?;
        println!(
            "  - {}",
            file.strip_prefix(repo_root)?.display()
        );
    }

    Ok(())
}

fn collect_files(
    repo_root: &Path,
    current_dir: &Path,
    expected_files: &HashSet<PathBuf>,
    files_to_remove: &mut Vec<PathBuf>,
) -> Result<()> {
    for entry in fs::read_dir(current_dir)? {
        let entry = entry?;
        let path = entry.path();

        let relative = path
            .strip_prefix(repo_root)?
            .to_path_buf();

        // Never touch .rusty.
        if relative
            .components()
            .next()
            .map(|c| c.as_os_str() == ".rusty")
            .unwrap_or(false)
        {
            continue;
        }

        if path.is_dir() {
            collect_files(
                repo_root,
                &path,
                expected_files,
                files_to_remove,
            )?;
        } else if path.is_file() {
            if !expected_files.contains(&relative) {
                files_to_remove.push(path);
            }
        }
    }

    Ok(())
}

fn update_head(
    repo_path: &Path,
    target: &str,
    commit_hash: &str,
) -> Result<()> {
    let head_path = repo_path.join("HEAD");
    let head = fs::read_to_string(&head_path)?;

    // If target is a local branch, move that branch.
    let local_branch = repo_path
        .join("refs")
        .join("heads")
        .join(target);

    if local_branch.exists() {
        fs::write(local_branch, commit_hash)?;
        fs::write(
            head_path,
            format!("ref: refs/heads/{}\n", target),
        )?;

        return Ok(());
    }

    // If currently on a branch and checking out a commit directly,
    // use detached HEAD.
    if head.starts_with("ref: ") {
        fs::write(head_path, commit_hash)?;
    }

    Ok(())
}
