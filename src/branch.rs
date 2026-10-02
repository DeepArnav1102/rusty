use anyhow::Result;
use std::fs;
use std::path::{Path, PathBuf};

use crate::repository::get_head_commit;

pub fn create_branch(repo_path: &Path, branch_name: &str) -> Result<()> {
    let head_commit = get_head_commit(repo_path)?
        .ok_or_else(|| anyhow::anyhow!("Cannot create branch: no commits yet"))?;

    let branch_path = repo_path.join("refs").join("heads").join(branch_name);

    if branch_path.exists() {
        anyhow::bail!("Branch '{}' already exists", branch_name);
    }

    if let Some(parent) = branch_path.parent() {
        fs::create_dir_all(parent)?;
    }

    fs::write(branch_path, head_commit)?;

    println!("Created branch '{}'", branch_name);

    Ok(())
}

// ============================================================
// BRANCH LISTING
// ============================================================

pub fn list_branches(repo_path: &Path, remote: bool, all: bool) -> Result<()> {
    let head = fs::read_to_string(repo_path.join("HEAD")).unwrap_or_default();

    let current_branch = head.strip_prefix("ref: refs/heads/").unwrap_or("").trim();

    if all {
        list_local_branches(repo_path, current_branch)?;

        list_remote_branches(repo_path)?;

        return Ok(());
    }

    if remote {
        list_remote_branches(repo_path)?;

        return Ok(());
    }

    list_local_branches(repo_path, current_branch)?;

    Ok(())
}

// ============================================================
// LOCAL BRANCHES
// ============================================================

fn list_local_branches(repo_path: &Path, current_branch: &str) -> Result<()> {
    let heads_path = repo_path.join("refs").join("heads");

    if !heads_path.exists() {
        return Ok(());
    }

    let mut branches = Vec::new();

    collect_branch_files(&heads_path, &heads_path, &mut branches)?;

    branches.sort();

    for branch in branches {
        if branch == current_branch {
            println!("* {}", branch);
        } else {
            println!("  {}", branch);
        }
    }

    Ok(())
}

// ============================================================
// REMOTE BRANCHES
// ============================================================

fn list_remote_branches(repo_path: &Path) -> Result<()> {
    let remote_path = repo_path.join("refs").join("remotes").join("origin");

    if !remote_path.exists() {
        return Ok(());
    }

    let mut branches = Vec::new();

    collect_branch_files(&remote_path, &remote_path, &mut branches)?;

    branches.sort();

    for branch in branches {
        println!("  origin/{}", branch);
    }

    Ok(())
}

// ============================================================
// RECURSIVE REF FILE DISCOVERY
// ============================================================

fn collect_branch_files(root: &Path, current: &Path, branches: &mut Vec<String>) -> Result<()> {
    for entry in fs::read_dir(current)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_dir() {
            collect_branch_files(root, &path, branches)?;
        } else if path.is_file() {
            let relative = path
                .strip_prefix(root)?
                .to_string_lossy()
                .replace('\\', "/");

            branches.push(relative);
        }
    }

    Ok(())
}
