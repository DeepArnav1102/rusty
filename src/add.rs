use anyhow::{Context, Result};
use crate::index::Index;
use crate::objects;
use std::path::Path;

pub fn add_file(file_path: &Path, repo_path: &Path) -> Result<()> {
    if !file_path.exists() {
        anyhow::bail!("File not found: {}", file_path.display());
    }

    if !file_path.is_file() {
        anyhow::bail!("Not a file: {}", file_path.display());
    }

    let repo_root = repo_path
        .parent()
        .context("Invalid repository path")?;

    let full_path = repo_root.join(file_path);

    let blob_hash = objects::hash_objects(
        &full_path,
        repo_path,
    )?;

    let mut index = Index::load(repo_path)?;

    let relative_path = full_path
        .strip_prefix(repo_root)?
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
