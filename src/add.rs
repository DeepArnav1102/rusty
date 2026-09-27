use anyhow::{Context, Result};
use crate::index::Index;
use crate::objects;
use std::path::Path;
use walkdir::WalkDir;

pub fn add_file(file_path: &Path, repo_path: &Path) -> Result<()> {
    let repo_root = repo_path
        .parent()
        .context("Invalid repository path")?;

    let full_path = if file_path.is_absolute() {
        file_path.to_path_buf()
    } else {
        std::env::current_dir()?.join(file_path)
    };

    if !full_path.exists() {
        anyhow::bail!("Path not found: {}", full_path.display());
    }

    let full_path = full_path.canonicalize()?;
    let repo_root = repo_root.canonicalize()?;

    if !full_path.starts_with(&repo_root) {
        anyhow::bail!("File is outside the repository");
    }

    let mut index = Index::load(repo_path)?;
    let mut added_count = 0;

    if full_path.is_dir() {
        for entry in WalkDir::new(&full_path) {
            let entry = entry?;
            let path = entry.path();

            // Skip .rusty, .git, and common ignore folders
            if path.components().any(|c| {
                let s = c.as_os_str().to_string_lossy();
                s == ".rusty" || s == ".git" || s == "target" || s == "node_modules"
            }) {
                continue;
            }

            if !path.is_file() {
                continue;
            }

            let canonical = path.canonicalize()?;
            let relative_path = canonical
                .strip_prefix(&repo_root)?
                .to_string_lossy()
                .replace('\\', "/");

            let blob_hash = objects::hash_objects(&canonical, repo_path)?;
            index.add(relative_path.clone(), blob_hash);
            println!("Added {}", relative_path);
            added_count += 1;
        }
    } else {
        let relative_path = full_path
            .strip_prefix(&repo_root)?
            .to_string_lossy()
            .replace('\\', "/");

        let blob_hash = objects::hash_objects(&full_path, repo_path)?;
        index.add(relative_path.clone(), blob_hash);
        println!("Added {}", relative_path);
        added_count += 1;
    }

    index.save(repo_path)?;

    if added_count == 0 {
        println!("Nothing added");
    }

    Ok(())
}
