use anyhow::{Context, Result, bail};
use std::path::Path;

use crate::index::Index;

pub fn rm_cached(path: &str, repo_path: &Path) -> Result<()> {
    let repo_root = repo_path.parent().context("Invalid repository path")?;

    let mut index = Index::load(repo_path)?;

    let input_path = Path::new(path);

    let relative_path = if input_path.is_absolute() {
        input_path
            .strip_prefix(repo_root)
            .with_context(|| format!("path is outside repository: {}", path))?
            .to_string_lossy()
            .replace('\\', "/")
    } else {
        let current_dir = std::env::current_dir()?;

        current_dir
            .join(input_path)
            .canonicalize()
            .with_context(|| format!("path not found: {}", path))?
            .strip_prefix(repo_root)?
            .to_string_lossy()
            .replace('\\', "/")
    };

    if !index.contains(&relative_path) {
        bail!("pathspec '{}' is not tracked", path);
    }

    // Remove from index only.
    // The actual working-tree file remains untouched.
    index.remove(&relative_path);

    index.save(repo_path)?;

    println!("rm '{}'", relative_path);

    Ok(())
}
