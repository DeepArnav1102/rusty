use anyhow::Result;
use std::fs;
use std::path::Path;

use crate::commit::Commit;

pub fn show_log(repo_path: &Path) -> Result<()> {
    let head_path = repo_path.join("HEAD");
    let head = fs::read_to_string(head_path)?;

    let branch = head.strip_prefix("ref: ").unwrap().trim();

    let branch_path = repo_path.join(branch);

    if !branch_path.exists() {
        println!("No commits yet");
        return Ok(());
    }

    let hash = fs::read_to_string(branch_path)?;
    let mut current_hash = hash.trim().to_string();

    if current_hash.is_empty() {
        println!("No commits yet");
        return Ok(());
    }

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

        match commit.parent {
            Some(parent_hash) => {
                current_hash = parent_hash;
            }
            None => {
                break;
            }
        }
    }
    Ok(())
}
