use anyhow::{Context, Result};
use std::fs;
use std::path::Path;

use crate::fetch::fetch;
use crate::merge::merge;

pub fn pull(repo_path: &Path) -> Result<()> {
    // ------------------------------------------------------------
    // 1. Read HEAD
    // ------------------------------------------------------------

    let head_path = repo_path.join("HEAD");

    let head = fs::read_to_string(&head_path)?.trim().to_string();

    // ------------------------------------------------------------
    // 2. Pull requires an attached HEAD
    // ------------------------------------------------------------

    let branch_ref = head
        .strip_prefix("ref: ")
        .context("Cannot pull while HEAD is detached")?
        .trim();

    let branch = branch_ref
        .strip_prefix("refs/heads/")
        .context("Invalid HEAD reference")?;

    if branch.is_empty() {
        anyhow::bail!("Cannot determine current branch");
    }

    println!("Pulling branch '{}'...", branch);

    // ------------------------------------------------------------
    // 3. Fetch remote changes
    // ------------------------------------------------------------

    fetch(repo_path)?;

    // ------------------------------------------------------------
    // 4. Merge remote-tracking branch
    // ------------------------------------------------------------

    let remote_branch = format!("origin/{}", branch);

    merge(repo_path, &remote_branch)?;

    Ok(())
}
