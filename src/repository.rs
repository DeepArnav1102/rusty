use anyhow::Result;
use std::fs;
use std::path::{Path, PathBuf};

pub fn init() -> Result<()> {
    let current_dir = std::env::current_dir()?;
    let rusty_dir = current_dir.join(".rusty");

    if rusty_dir.exists() {
        anyhow::bail!("Repository already initialized");
    }

    fs::create_dir(&rusty_dir)?;

    fs::create_dir(rusty_dir.join("objects"))?;
    fs::create_dir(rusty_dir.join("objects").join("blobs"))?;
    fs::create_dir(rusty_dir.join("objects").join("trees"))?;
    fs::create_dir(rusty_dir.join("objects").join("commits"))?;

    fs::create_dir(rusty_dir.join("refs"))?;
    fs::create_dir(rusty_dir.join("refs").join("heads"))?;
    fs::create_dir(rusty_dir.join("refs").join("remotes"))?;

    fs::write(rusty_dir.join("HEAD"), "ref: refs/heads/main\n")?;

    fs::write(rusty_dir.join("index"), "")?;

    fs::write(rusty_dir.join("config"), "{\n    \"remotes\": {}\n}\n")?;

    println!(
        "Initialized empty Rusty repository in {}",
        PathBuf::from(".rusty").display()
    );

    Ok(())
}

pub fn get_head_commit(repo_path: &Path) -> Result<Option<String>> {
    let head_path = repo_path.join("HEAD");
    if !head_path.exists() {
        return Ok(None);
    }
    let content = fs::read_to_string(&head_path)?;
    let content = content.trim();
    if let Some(branch_ref) = content.strip_prefix("ref: ") {
        let branch_ref = branch_ref.trim();
        let branch_path = repo_path.join(branch_ref);
        if branch_path.exists() {
            let hash = fs::read_to_string(&branch_path)?;
            let hash = hash.trim();
            if !hash.is_empty() {
                return Ok(Some(hash.to_string()));
            }
        }
        Ok(None)
    } else if !content.is_empty() {
        // Detached HEAD
        Ok(Some(content.to_string()))
    } else {
        Ok(None)
    }
}
