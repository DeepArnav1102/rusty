use anyhow::{Context, Result};
use crate::index::Index;
use crate::objects;
use std::path::Path;

pub fn add_file(file_path: &Path, repo_path: &Path) -> Result<()> {
    let repo_root = repo_path
        .parent()
        .context("Invalid repository path")?;

    // Resolve the path from the current working directory.
    let full_path = if file_path.is_absolute() {
        file_path.to_path_buf()
    } else {
        std::env::current_dir()?.join(file_path)
    };

    if !full_path.exists() {
        anyhow::bail!("File not found: {}", full_path.display());
    }

    if !full_path.is_file() {
        anyhow::bail!("Not a file: {}", full_path.display());
    }

    let full_path = full_path.canonicalize()?;
    let repo_root = repo_root.canonicalize()?;

    if !full_path.starts_with(&repo_root) {
        anyhow::bail!("File is outside the repository");
    }

    let blob_hash = objects::hash_objects(
        &full_path,
        repo_path,
    )?;

    let mut index = Index::load(repo_path)?;

    let relative_path = full_path
        .strip_prefix(&repo_root)?
        .to_string_lossy()
        .to_string();

    index.add(
        relative_path.clone(),
        blob_hash,
    );

    index.save(repo_path)?;

    println!("Added {}", relative_path);

    Ok(())
}
