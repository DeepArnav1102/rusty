use anyhow::Result;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::Path;

use crate::tree::write_tree;

#[derive(Deserialize, Serialize, Debug)]
pub struct Commit {
    pub tree: String,
    pub message: String,
    pub parent: Option<String>,
}

pub fn create_commit(repo_path: &Path, message: String) -> Result<String> {
    let tree_hash = write_tree(repo_path)?;

    let head_path = repo_path.join("HEAD");
    let head_content = fs::read_to_string(&head_path)?;

    let branch = head_content.strip_prefix("ref: ").unwrap().trim();

    let branch_path = repo_path.join(branch);

    let parent = if branch_path.exists() {
        let hash = fs::read_to_string(&branch_path)?;
        let hash = hash.trim();

        if hash.is_empty() {
            None
        } else {
            Some(hash.to_string())
        }
    } else {
        None
    };

    if let Some(parent_hash) = &parent {
        let parent_path = repo_path.join("objects").join("commits").join(parent_hash);

        let parent_data = fs::read(&parent_path)?;

        let parent_commit: Commit = serde_json::from_slice(&parent_data)?;

        if parent_commit.tree == tree_hash {
            anyhow::bail!("Nothing to commit");
        }
    }

    let commit = Commit {
        tree: tree_hash,
        message,
        parent,
    };

    let data = serde_json::to_vec(&commit)?;

    let mut hasher = Sha256::new();
    hasher.update(&data);

    let commit_hash = hex::encode(hasher.finalize());

    let commit_path = repo_path.join("objects").join("commits").join(&commit_hash);

    if !commit_path.exists() {
        fs::write(&commit_path, &data)?;
    }

    if let Some(parent) = branch_path.parent() {
        fs::create_dir_all(parent)?;
    }

    fs::write(branch_path, &commit_hash)?;

    Ok(commit_hash)
}
