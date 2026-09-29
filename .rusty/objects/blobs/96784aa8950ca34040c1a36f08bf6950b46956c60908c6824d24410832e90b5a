use anyhow::{Context, Result};
use crate::index::Index;
use crate::objects;
use std::path::Path;
use walkdir::WalkDir;

pub fn check_status(repo_path: &Path) -> Result<()> {
    let repo_root = repo_path
        .parent()
        .context("Invalid repo path")?;

    let index = Index::load(repo_path)?;

    let mut found_untracked = false;
    let mut found_modified = false;

    for entry in WalkDir::new(repo_root) {
        let entry = entry?;
        let path = entry.path();

        if path.components().any(|component| {
            component.as_os_str() == ".rusty"
        }) {
            continue;
        }

        if !path.is_file() {
            continue;
        }

        let relative_path = path
            .strip_prefix(repo_root)?
            .to_string_lossy()
            .to_string();

        if let Some(index_entry) = index.entries.get(&relative_path) {
            let curr_hash = objects::hash_file(path)?;

            if curr_hash != index_entry.blob_hash {
                println!("Modified: {}", relative_path);
                found_modified = true;
            }
        } else {
            println!("Untracked: {}", relative_path);
            found_untracked = true;
        }
    }

    if !found_modified && !found_untracked {
        println!("Working tree is clean");
    }

    Ok(())
}
