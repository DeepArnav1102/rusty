use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::Path;

use crate::merge::MergeState;
use crate::tree::write_tree;

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct Commit {
    pub tree: String,
    pub message: String,

    // New format: supports 0, 1, or multiple parents.
    #[serde(default)]
    pub parents: Vec<String>,
}

// Used only for reading old commit objects that contain:
// "parent": "abc123..."
#[derive(Deserialize)]
struct LegacyCommit {
    tree: String,
    message: String,

    #[serde(default)]
    parents: Vec<String>,

    #[serde(default)]
    parent: Option<String>,
}

pub fn get_commit(repo_path: &Path, commit_hash: &str) -> Result<Commit> {
    let commit_path = repo_path.join("objects").join("commits").join(commit_hash);

    if !commit_path.exists() {
        anyhow::bail!("Commit not found: {}", commit_hash);
    }

    let data = fs::read(&commit_path)?;

    let legacy: LegacyCommit = serde_json::from_slice(&data)
        .with_context(|| format!("Invalid commit object {}", commit_hash))?;

    let mut parents = legacy.parents;

    // Convert old:
    // "parent": "abc..."
    //
    // into:
    // parents: ["abc..."]
    if parents.is_empty() {
        if let Some(parent) = legacy.parent {
            parents.push(parent);
        }
    }

    Ok(Commit {
        tree: legacy.tree,
        message: legacy.message,
        parents,
    })
}

pub fn create_commit(repo_path: &Path, message: String) -> Result<String> {
    let tree_hash = write_tree(repo_path)?;

    let head_path = repo_path.join("HEAD");
    let head_content = fs::read_to_string(&head_path)?;

    let branch = match head_content.strip_prefix("ref: ") {
        Some(b) => b.trim(),
        None => {
            anyhow::bail!("Cannot commit: HEAD is detached. Switch to a branch to commit.");
        }
    };

    let branch_path = repo_path.join(branch);

    // Check for an active merge.
    let merge_head = crate::merge::read_merge_head(repo_path)?;

    // If MERGE_STATE exists, enforce all conflicts resolved.
    if let Some(state) = MergeState::load(repo_path)? {
        if state.has_unresolved_conflicts() {
            anyhow::bail!(
                "Cannot commit: unresolved merge conflicts remain. \
                 Resolve them with 'rusty add' first."
            );
        }
    }

    // Build parent list.
    let parents: Vec<String> = if let Some(merge_head_hash) = &merge_head {
        // Merge commit: current HEAD + merge head.
        let current = if branch_path.exists() {
            let h = fs::read_to_string(&branch_path)?;
            let h = h.trim();
            if h.is_empty() { None } else { Some(h.to_string()) }
        } else {
            None
        };

        match current {
            Some(cur) => vec![cur, merge_head_hash.clone()],
            None => vec![merge_head_hash.clone()],
        }
    } else {
        // Normal commit: at most one parent.
        if branch_path.exists() {
            let hash = fs::read_to_string(&branch_path)?;
            let hash = hash.trim();
            if hash.is_empty() {
                Vec::new()
            } else {
                vec![hash.to_string()]
            }
        } else {
            Vec::new()
        }
    };

    // Check if there is anything to commit (only for non-merge commits).
    if merge_head.is_none() {
        if let Some(parent_hash) = parents.first() {
            let parent_commit = get_commit(repo_path, parent_hash)?;
            if parent_commit.tree == tree_hash {
                anyhow::bail!("Nothing to commit");
            }
        }
    }

    let commit = Commit {
        tree: tree_hash,
        message,
        parents,
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

    fs::write(&branch_path, &commit_hash)?;

    // Only AFTER successful branch-ref update: remove merge state.
    if merge_head.is_some() {
        let merge_head_path = repo_path.join("MERGE_HEAD");
        if merge_head_path.exists() {
            fs::remove_file(&merge_head_path)?;
        }
        MergeState::remove(repo_path)?;
    }

    Ok(commit_hash)
}
