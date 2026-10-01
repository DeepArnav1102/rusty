use anyhow::Result;
use std::fs;
use std::path::Path;

use crate::commit::Commit;

pub fn show_log(repo_path: &Path) -> Result<()> {
    let head_path = repo_path.join("HEAD");
    if !head_path.exists() {
        println!("No commits yet");
        return Ok(());
    }
    let head = fs::read_to_string(head_path)?;
    let head = head.trim();

    let start_hash = if let Some(branch) = head.strip_prefix("ref: ") {
        let branch_path = repo_path.join(branch.trim());
        if !branch_path.exists() {
            println!("No commits yet");
            return Ok(());
        }
        let hash = fs::read_to_string(branch_path)?;
        hash.trim().to_string()
    } else {
        head.to_string()
    };

    if start_hash.is_empty() {
        println!("No commits yet");
        return Ok(());
    }

    let mut current_hash = start_hash;

    loop {
        let commit_path = repo_path
            .join("objects")
            .join("commits")
            .join(&current_hash);
        let data = fs::read(&commit_path)?;

        let commit: Commit = serde_json::from_slice(&data)?;

        println!("commit {}", current_hash);
        println!("       {}", commit.message);
        println!();

        match commit.parents.first() {
            Some(parent_hash) => {
                current_hash = parent_hash.clone();
            }
            None => {
                break;
            }
        }
    }
    Ok(())
}
