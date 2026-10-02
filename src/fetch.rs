use anyhow::{Context, Result};
use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::Path;

use crate::auth::Credentials;
use crate::remote;

pub fn fetch(repo_path: &Path) -> Result<()> {
    let creds = crate::auth::require_auth()?;

    let origin = remote::get_remote(repo_path, "origin")?;

    println!("Fetching from {}...", origin);

    // ------------------------------------------------------------
    // 1. Get ALL remote branch references
    // ------------------------------------------------------------

    let remote_refs = remote::get_all_refs(&origin, &creds)?;

    let remote_ref_dir = repo_path.join("refs").join("remotes").join("origin");

    fs::create_dir_all(&remote_ref_dir)?;

    // ------------------------------------------------------------
    // 2. Empty remote repository
    // ------------------------------------------------------------

    if remote_refs.is_empty() {
        println!("Remote repository has no branches.");

        prune_stale_refs(&remote_ref_dir, &remote_refs)?;

        return Ok(());
    }

    // ------------------------------------------------------------
    // 3. Download objects for every remote branch
    // ------------------------------------------------------------

    let mut fetched = HashSet::new();

    for (branch, commit_hash) in &remote_refs {
        println!("\nFetching origin/{}...", branch);

        fetch_object(
            repo_path,
            &origin,
            "commit",
            commit_hash,
            &mut fetched,
            &creds,
        )?;
    }

    // ------------------------------------------------------------
    // 4. Update remote-tracking refs
    //
    // Only reached if ALL object fetching succeeded.
    // This prevents refs from pointing to incomplete history.
    // ------------------------------------------------------------

    for (branch, commit_hash) in &remote_refs {
        let ref_path = remote_ref_dir.join(branch);

        if let Some(parent) = ref_path.parent() {
            fs::create_dir_all(parent)?;
        }

        fs::write(&ref_path, commit_hash)?;

        println!("Updated origin/{} -> {}", branch, short_hash(commit_hash));
    }

    // ------------------------------------------------------------
    // 5. Remove stale remote-tracking refs
    // ------------------------------------------------------------

    prune_stale_refs(&remote_ref_dir, &remote_refs)?;

    println!("\nFetch complete.");

    Ok(())
}

// ============================================================
// REMOVE LOCAL REMOTE-TRACKING BRANCHES THAT NO LONGER EXIST
// ============================================================

fn prune_stale_refs(remote_ref_dir: &Path, remote_refs: &BTreeMap<String, String>) -> Result<()> {
    if !remote_ref_dir.exists() {
        return Ok(());
    }

    let mut stale = Vec::new();

    collect_ref_files(remote_ref_dir, &mut stale)?;

    for ref_path in stale {
        let relative = ref_path
            .strip_prefix(remote_ref_dir)?
            .to_string_lossy()
            .replace('\\', "/");

        if !remote_refs.contains_key(&relative) {
            fs::remove_file(&ref_path)?;

            println!("Pruned origin/{}", relative);
        }
    }

    remove_empty_dirs(remote_ref_dir)?;

    Ok(())
}

// ============================================================
// FIND ALL REMOTE-TRACKING REF FILES
// ============================================================

fn collect_ref_files(current: &Path, files: &mut Vec<std::path::PathBuf>) -> Result<()> {
    for entry in fs::read_dir(current)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_dir() {
            collect_ref_files(&path, files)?;
        } else if path.is_file() {
            files.push(path);
        }
    }

    Ok(())
}

// ============================================================
// REMOVE EMPTY REMOTE REF DIRECTORIES
// ============================================================

fn remove_empty_dirs(path: &Path) -> Result<()> {
    if !path.is_dir() {
        return Ok(());
    }

    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let child = entry.path();

        if child.is_dir() {
            remove_empty_dirs(&child)?;

            if fs::read_dir(&child)?.next().is_none() {
                fs::remove_dir(&child)?;
            }
        }
    }

    Ok(())
}

// ============================================================
// FETCH ONE OBJECT AND ITS ENTIRE HISTORY
// ============================================================

fn fetch_object(
    repo_path: &Path,
    origin: &str,
    object_type: &str,
    hash: &str,
    fetched: &mut HashSet<String>,
    creds: &Credentials,
) -> Result<()> {
    if fetched.contains(hash) {
        return Ok(());
    }

    fetched.insert(hash.to_string());

    let object_dir = match object_type {
        "blob" => "blobs",
        "tree" => "trees",
        "commit" => "commits",
        _ => anyhow::bail!("Unknown object type: {}", object_type),
    };

    let object_path = repo_path.join("objects").join(object_dir).join(hash);

    // ------------------------------------------------------------
    // Download object if it doesn't already exist
    // ------------------------------------------------------------

    if object_path.exists() {
        println!("  ✓ {} {} already exists", object_type, short_hash(hash));
    } else {
        let object = remote::get_object(origin, hash, creds)?;

        let remote_type = object["type"]
            .as_str()
            .context("Remote object missing type")?;

        let data = object["data"]
            .as_str()
            .context("Remote object missing data")?;

        if remote_type != object_type {
            anyhow::bail!(
                "Object type mismatch for {}: expected {}, got {}",
                hash,
                object_type,
                remote_type
            );
        }

        if let Some(parent) = object_path.parent() {
            fs::create_dir_all(parent)?;
        }

        fs::write(&object_path, data.as_bytes())?;

        println!("  + downloaded {} {}", object_type, short_hash(hash));
    }

    // ------------------------------------------------------------
    // Recursively fetch referenced objects
    // ------------------------------------------------------------

    match object_type {
        "commit" => {
            // IMPORTANT:
            // Use get_commit() instead of directly deserializing
            // Commit. get_commit() understands both:
            //
            // Old:
            //     "parent": "abc..."
            //
            // New:
            //     "parents": ["abc...", "..."]
            //
            // This ensures old commit histories are fetched too.

            let commit = crate::commit::get_commit(repo_path, hash)?;

            // Fetch commit tree
            fetch_object(repo_path, origin, "tree", &commit.tree, fetched, creds)?;

            // Fetch ALL parents
            for parent in &commit.parents {
                fetch_object(repo_path, origin, "commit", parent, fetched, creds)?;
            }
        }

        "tree" => {
            let data = fs::read(&object_path)?;

            let tree: crate::tree::Tree = serde_json::from_slice(&data)?;

            for entry in tree.entries {
                fetch_object(
                    repo_path,
                    origin,
                    &entry.object_type,
                    &entry.object_hash,
                    fetched,
                    creds,
                )?;
            }
        }

        "blob" => {}

        _ => {}
    }

    Ok(())
}

fn short_hash(hash: &str) -> &str {
    &hash[..8.min(hash.len())]
}
