use anyhow::{Context, Result};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use crate::commit::get_commit;
use crate::index::Index;
use crate::tree::load_tree_files;

pub fn checkout(repo_path: &Path, branch: &str) -> Result<()> {
    let branch_path = repo_path.join("refs").join("heads").join(branch);

    if !branch_path.exists() {
        anyhow::bail!("Branch '{}' does not exist", branch);
    }

    let commit_hash = fs::read_to_string(&branch_path)?.trim().to_string();

    if commit_hash.is_empty() {
        anyhow::bail!("Branch '{}' has no commits", branch);
    }

    let commit = get_commit(repo_path, &commit_hash)?;

    let files = load_tree_files(repo_path, &commit.tree)?;

    // Update working tree
    restore_working_tree(repo_path, &files)?;

    // Rebuild index
    let mut index = Index::new();

    for (path, hash) in &files {
        index.add(path.clone(), hash.clone());
    }

    index.save(repo_path)?;

    // Update HEAD
    fs::write(
        repo_path.join("HEAD"),
        format!("ref: refs/heads/{}\n", branch),
    )?;

    println!("Switched to branch '{}'", branch);

    Ok(())
}

fn restore_working_tree(repo_path: &Path, files: &BTreeMap<String, String>) -> Result<()> {
    let repo_root = repo_path
        .parent()
        .context("Invalid repository path")?
        .canonicalize()?;

    for (path, blob_hash) in files {
        let file_path = repo_root.join(path);

        if let Some(parent) = file_path.parent() {
            fs::create_dir_all(parent)?;
        }

        let blob_path = repo_path.join("objects").join("blobs").join(blob_hash);

        if !blob_path.exists() {
            anyhow::bail!("Missing blob object {}", blob_hash);
        }

        let data = fs::read(blob_path)?;

        fs::write(file_path, data)?;
    }

    Ok(())
}
