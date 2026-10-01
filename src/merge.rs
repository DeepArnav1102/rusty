use anyhow::{Context, Result};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashSet, VecDeque};
use std::fs;
use std::path::Path;

use crate::commit::{Commit, get_commit};
use crate::index::Index;
use crate::objects;
use crate::repository::get_head_commit;
use crate::tree::load_tree_files;

pub fn merge(repo_path: &Path, target: &str) -> Result<()> {
    // ------------------------------------------------------------
    // 1. Make sure HEAD is attached
    // ------------------------------------------------------------

    let head_path = repo_path.join("HEAD");
    let head_content = fs::read_to_string(&head_path)?;
    let head_content = head_content.trim();

    let branch_ref = head_content
        .strip_prefix("ref: ")
        .context("Cannot merge while HEAD is detached")?
        .trim();

    let current_branch_path = repo_path.join(branch_ref);

    if !current_branch_path.exists() {
        anyhow::bail!("Current branch has no commits");
    }

    let current_hash = fs::read_to_string(&current_branch_path)?.trim().to_string();

    if current_hash.is_empty() {
        anyhow::bail!("Current branch has no commits");
    }

    // ------------------------------------------------------------
    // 2. Resolve merge target
    // ------------------------------------------------------------

    let target_hash = resolve_target(repo_path, target)?;

    // ------------------------------------------------------------
    // 3. Same commit
    // ------------------------------------------------------------

    if target_hash == current_hash {
        println!("Already up to date.");
        return Ok(());
    }

    println!(
        "Merging {} into {}...",
        target,
        branch_name_from_ref(branch_ref)
    );

    // ------------------------------------------------------------
    // 4. Working tree must be clean
    // ------------------------------------------------------------

    ensure_clean_working_tree(repo_path, &current_hash)?;

    // ------------------------------------------------------------
    // 5. Load commits
    // ------------------------------------------------------------

    let current_commit = get_commit(repo_path, &current_hash)?;
    let target_commit = get_commit(repo_path, &target_hash)?;

    // ------------------------------------------------------------
    // 6. Fast-forward checks
    // ------------------------------------------------------------

    // Target is already contained in current history.
    if is_ancestor(repo_path, &target_hash, &current_hash)? {
        println!("Already up to date.");
        return Ok(());
    }

    // Current is contained in target history.
    if is_ancestor(repo_path, &current_hash, &target_hash)? {
        println!("Fast-forwarding...");

        let target_files = load_tree_files(repo_path, &target_commit.tree)?;

        check_untracked_overwrites(repo_path, &target_files)?;

        apply_tree(repo_path, &target_files)?;

        let mut index = Index::new();

        for (path, blob_hash) in &target_files {
            index.add(path.clone(), blob_hash.clone());
        }

        index.save(repo_path)?;

        fs::write(&current_branch_path, &target_hash)?;

        println!("Fast-forwarded to {}", short_hash(&target_hash));

        return Ok(());
    }

    // ------------------------------------------------------------
    // 7. Find common ancestor
    // ------------------------------------------------------------

    let base_hash = find_common_ancestor(repo_path, &current_hash, &target_hash)?
        .context("No common ancestor found")?;

    let base_commit = get_commit(repo_path, &base_hash)?;

    // ------------------------------------------------------------
    // 8. Load all three trees
    // ------------------------------------------------------------

    let base_files = load_tree_files(repo_path, &base_commit.tree)?;
    let ours_files = load_tree_files(repo_path, &current_commit.tree)?;
    let theirs_files = load_tree_files(repo_path, &target_commit.tree)?;

    // ------------------------------------------------------------
    // 9. Three-way merge
    // ------------------------------------------------------------

    let merged_files = three_way_merge(&base_files, &ours_files, &theirs_files)?;

    // ------------------------------------------------------------
    // 10. Prevent untracked files from being overwritten
    // ------------------------------------------------------------

    check_untracked_overwrites(repo_path, &merged_files)?;

    // ------------------------------------------------------------
    // 11. Apply merged tree
    // ------------------------------------------------------------

    apply_tree(repo_path, &merged_files)?;

    // ------------------------------------------------------------
    // 12. Rebuild index
    // ------------------------------------------------------------

    let mut index = Index::new();

    for (path, blob_hash) in &merged_files {
        index.add(path.clone(), blob_hash.clone());
    }

    index.save(repo_path)?;

    // ------------------------------------------------------------
    // 13. Write merge tree
    // ------------------------------------------------------------

    let merge_tree_hash = crate::tree::write_tree(repo_path)?;

    // ------------------------------------------------------------
    // 14. Create merge commit
    // ------------------------------------------------------------

    let commit = Commit {
        tree: merge_tree_hash,
        message: format!("Merge branch '{}'", target),
        parents: vec![current_hash.clone(), target_hash.clone()],
    };

    let data = serde_json::to_vec(&commit)?;

    let mut hasher = Sha256::new();
    hasher.update(&data);

    let merge_hash = hex::encode(hasher.finalize());

    let merge_path = repo_path.join("objects").join("commits").join(&merge_hash);

    if !merge_path.exists() {
        fs::write(&merge_path, &data)?;
    }

    // ------------------------------------------------------------
    // 15. Update current branch
    // ------------------------------------------------------------

    fs::write(&current_branch_path, &merge_hash)?;

    println!();
    println!("Merge successful.");
    println!("Merge commit: {}", merge_hash);
    println!(
        "Parents: {} {}",
        short_hash(&current_hash),
        short_hash(&target_hash)
    );

    Ok(())
}

// ============================================================================
// TARGET RESOLUTION
// ============================================================================

fn resolve_target(repo_path: &Path, target: &str) -> Result<String> {
    // Direct commit hash
    let commit_path = repo_path.join("objects").join("commits").join(target);

    if commit_path.exists() {
        return Ok(target.to_string());
    }

    // Remote-tracking branch: origin/main
    if let Some(rest) = target.strip_prefix("origin/") {
        let remote_ref = repo_path
            .join("refs")
            .join("remotes")
            .join("origin")
            .join(rest);

        if remote_ref.exists() {
            let hash = fs::read_to_string(remote_ref)?.trim().to_string();

            if !hash.is_empty() {
                return Ok(hash);
            }
        }

        anyhow::bail!("Remote branch not found: {}", target);
    }

    // Local branch
    let branch_ref = repo_path.join("refs").join("heads").join(target);

    if branch_ref.exists() {
        let hash = fs::read_to_string(branch_ref)?.trim().to_string();

        if !hash.is_empty() {
            return Ok(hash);
        }

        anyhow::bail!("Branch '{}' has no commits", target);
    }

    // Full ref
    let full_ref = repo_path.join(target);

    if full_ref.exists() {
        let hash = fs::read_to_string(full_ref)?.trim().to_string();

        if !hash.is_empty() {
            return Ok(hash);
        }
    }

    anyhow::bail!("Merge target '{}' not found", target);
}

// ============================================================================
// COMMON ANCESTOR
// ============================================================================

fn find_common_ancestor(repo_path: &Path, ours: &str, theirs: &str) -> Result<Option<String>> {
    let ours_ancestors = collect_ancestors(repo_path, ours)?;
    let theirs_ancestors = collect_ancestors(repo_path, theirs)?;

    for hash in ours_ancestors {
        if theirs_ancestors.contains(&hash) {
            return Ok(Some(hash));
        }
    }

    Ok(None)
}

fn collect_ancestors(repo_path: &Path, start: &str) -> Result<Vec<String>> {
    let mut result = Vec::new();
    let mut queue = VecDeque::new();
    let mut visited = HashSet::new();

    queue.push_back(start.to_string());

    while let Some(hash) = queue.pop_front() {
        if !visited.insert(hash.clone()) {
            continue;
        }

        result.push(hash.clone());

        let commit = get_commit(repo_path, &hash)?;

        // Follow every parent.
        //
        // This is important for merge commits because they have
        // multiple parents.
        for parent in commit.parents {
            queue.push_back(parent);
        }
    }

    Ok(result)
}

// ============================================================================
// ANCESTOR CHECK
// ============================================================================

fn is_ancestor(repo_path: &Path, ancestor: &str, descendant: &str) -> Result<bool> {
    if ancestor == descendant {
        return Ok(true);
    }

    let ancestors = collect_ancestors(repo_path, descendant)?;

    Ok(ancestors.iter().any(|hash| hash == ancestor))
}

// ============================================================================
// THREE-WAY MERGE
// ============================================================================

fn three_way_merge(
    base: &BTreeMap<String, String>,
    ours: &BTreeMap<String, String>,
    theirs: &BTreeMap<String, String>,
) -> Result<BTreeMap<String, String>> {
    let mut all_paths = HashSet::new();

    all_paths.extend(base.keys().cloned());
    all_paths.extend(ours.keys().cloned());
    all_paths.extend(theirs.keys().cloned());

    let mut merged = BTreeMap::new();
    let mut conflicts = Vec::new();

    for path in all_paths {
        let base_hash = base.get(&path);
        let ours_hash = ours.get(&path);
        let theirs_hash = theirs.get(&path);

        // --------------------------------------------------------
        // Both sides are identical
        // --------------------------------------------------------

        if ours_hash == theirs_hash {
            if let Some(hash) = ours_hash {
                merged.insert(path, hash.clone());
            }

            continue;
        }

        // --------------------------------------------------------
        // Ours did not change -> take theirs
        // --------------------------------------------------------

        if ours_hash == base_hash {
            if let Some(hash) = theirs_hash {
                merged.insert(path, hash.clone());
            }

            continue;
        }

        // --------------------------------------------------------
        // Theirs did not change -> keep ours
        // --------------------------------------------------------

        if theirs_hash == base_hash {
            if let Some(hash) = ours_hash {
                merged.insert(path, hash.clone());
            }

            continue;
        }

        // --------------------------------------------------------
        // Both changed differently -> conflict
        // --------------------------------------------------------

        conflicts.push(path);
    }

    if !conflicts.is_empty() {
        conflicts.sort();

        println!();
        println!("Merge conflict(s):");

        for path in &conflicts {
            println!("  {}", path);
        }

        println!();
        println!("Automatic merge failed.");
        println!("Resolve the conflicts manually.");

        anyhow::bail!("Merge conflict");
    }

    Ok(merged)
}

// ============================================================================
// CLEAN WORKING TREE CHECK
// ============================================================================

fn ensure_clean_working_tree(repo_path: &Path, current_hash: &str) -> Result<()> {
    let index = Index::load(repo_path)?;

    let current_commit = get_commit(repo_path, current_hash)?;

    let head_files = load_tree_files(repo_path, &current_commit.tree)?;

    // ------------------------------------------------------------
    // Check staged changes
    // ------------------------------------------------------------

    if index.entries.len() != head_files.len() {
        anyhow::bail!("Cannot merge: you have staged changes. Commit them first.");
    }

    for (path, entry) in &index.entries {
        if head_files.get(path) != Some(&entry.blob_hash) {
            anyhow::bail!("Cannot merge: you have staged changes. Commit them first.");
        }
    }

    // ------------------------------------------------------------
    // Check working tree changes
    // ------------------------------------------------------------

    let repo_root = repo_path
        .parent()
        .context("Invalid repository path")?
        .canonicalize()?;

    for (path, expected_hash) in &head_files {
        let full_path = repo_root.join(path);

        if !full_path.exists() {
            anyhow::bail!(
                "Cannot merge: '{}' was deleted locally. \
                 Commit or restore it first.",
                path
            );
        }

        if !full_path.is_file() {
            anyhow::bail!("Cannot merge: '{}' is not a regular file.", path);
        }

        let actual_hash = objects::hash_file(&full_path)?;

        if &actual_hash != expected_hash {
            anyhow::bail!(
                "Cannot merge: '{}' has uncommitted changes. \
                 Commit or restore it first.",
                path
            );
        }
    }

    Ok(())
}

// ============================================================================
// UNTRACKED OVERWRITE CHECK
// ============================================================================

fn check_untracked_overwrites(
    repo_path: &Path,
    target_files: &BTreeMap<String, String>,
) -> Result<()> {
    let repo_root = repo_path
        .parent()
        .context("Invalid repository path")?
        .canonicalize()?;

    let index = Index::load(repo_path)?;

    let head_files = match get_head_commit(repo_path)? {
        Some(current_hash) => {
            let commit = get_commit(repo_path, &current_hash)?;
            load_tree_files(repo_path, &commit.tree)?
        }
        None => BTreeMap::new(),
    };

    for path in target_files.keys() {
        // Already tracked by HEAD.
        if head_files.contains_key(path) {
            continue;
        }

        // Already tracked by index.
        if index.contains(path) {
            continue;
        }

        let full_path = repo_root.join(path);

        if full_path.exists() {
            anyhow::bail!(
                "Cannot merge: untracked file '{}' would be overwritten.",
                path
            );
        }
    }

    Ok(())
}

// ============================================================================
// APPLY TREE
// ============================================================================

fn apply_tree(repo_path: &Path, target_files: &BTreeMap<String, String>) -> Result<()> {
    let repo_root = repo_path
        .parent()
        .context("Invalid repository path")?
        .canonicalize()?;

    let current_files = match get_head_commit(repo_path)? {
        Some(current_hash) => {
            let commit = get_commit(repo_path, &current_hash)?;
            load_tree_files(repo_path, &commit.tree)?
        }
        None => BTreeMap::new(),
    };

    // ------------------------------------------------------------
    // Remove files that no longer exist in target
    // ------------------------------------------------------------

    for path in current_files.keys() {
        if !target_files.contains_key(path) {
            let full_path = repo_root.join(path);

            if full_path.exists() {
                fs::remove_file(&full_path)?;
            }
        }
    }

    // ------------------------------------------------------------
    // Write target files
    // ------------------------------------------------------------

    for (path, blob_hash) in target_files {
        let full_path = repo_root.join(path);

        if let Some(parent) = full_path.parent() {
            fs::create_dir_all(parent)?;
        }

        let blob_path = repo_path.join("objects").join("blobs").join(blob_hash);

        if !blob_path.exists() {
            anyhow::bail!("Blob object not found: {}", blob_hash);
        }

        let data = fs::read(&blob_path)?;

        fs::write(&full_path, data)?;
    }

    Ok(())
}

// ============================================================================
// HELPERS
// ============================================================================

fn branch_name_from_ref(branch_ref: &str) -> &str {
    branch_ref.strip_prefix("refs/heads/").unwrap_or(branch_ref)
}

fn short_hash(hash: &str) -> &str {
    &hash[..8.min(hash.len())]
}
