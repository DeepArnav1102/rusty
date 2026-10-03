use anyhow::Result;
use std::collections::{HashSet, VecDeque};
use std::fs;
use std::path::Path;

use crate::commit::Commit;

pub fn show_log(repo_path: &Path) -> Result<()> {
    let head_path = repo_path.join("HEAD");

    if !head_path.exists() {
        println!("No commits yet");
        return Ok(());
    }

    let head = fs::read_to_string(&head_path)?;
    let head = head.trim();

    // ------------------------------------------------------------
    // Get starting commit from HEAD
    // ------------------------------------------------------------

    let start_hash = if let Some(branch) = head.strip_prefix("ref: ") {
        let branch_path = repo_path.join(branch.trim());

        if !branch_path.exists() {
            println!("No commits yet");
            return Ok(());
        }

        let hash = fs::read_to_string(branch_path)?;
        hash.trim().to_string()
    } else {
        // Detached HEAD
        head.to_string()
    };

    if start_hash.is_empty() {
        println!("No commits yet");
        return Ok(());
    }

    // ------------------------------------------------------------
    // Traverse the complete commit DAG
    // ------------------------------------------------------------

    let mut queue = VecDeque::new();
    let mut visited = HashSet::new();

    queue.push_back(start_hash);

    while let Some(current_hash) = queue.pop_front() {
        // Prevent showing the same commit multiple times.
        if !visited.insert(current_hash.clone()) {
            continue;
        }

        let commit_path = repo_path
            .join("objects")
            .join("commits")
            .join(&current_hash);

        if !commit_path.exists() {
            eprintln!("Warning: commit object not found: {}", current_hash);
            continue;
        }

        let data = fs::read(&commit_path)?;

        let commit: Commit = serde_json::from_slice(&data)?;

        // --------------------------------------------------------
        // Display commit
        // --------------------------------------------------------

        println!("commit {}", current_hash);
        println!("       {}", commit.message);

        if commit.parents.len() > 1 {
            println!("       merge parents: {}", commit.parents.join(", "));
        }

        println!();

        // --------------------------------------------------------
        // Add ALL parents to traversal queue
        // --------------------------------------------------------

        for parent_hash in &commit.parents {
            if !visited.contains(parent_hash) {
                queue.push_back(parent_hash.clone());
            }
        }
    }

    Ok(())
}
