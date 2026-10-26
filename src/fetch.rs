use anyhow::Result;
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
    // 3. Download objects for every remote branch using batching
    // ------------------------------------------------------------

    let mut fetched = HashSet::new();

    for (branch, commit_hash) in &remote_refs {
        println!("\nFetching origin/{}...", branch);

        fetch_branch_objects_batch(
            repo_path,
            &origin,
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
// BATCH OBJECT FETCH ENGINE (N+1 Elimination)
// ============================================================

fn fetch_branch_objects_batch(
    repo_path: &Path,
    origin: &str,
    tip_commit: &str,
    fetched: &mut HashSet<String>,
    creds: &Credentials,
) -> Result<()> {
    let mut pending_commits = vec![tip_commit.to_string()];
    let mut pending_trees = Vec::new();
    let mut pending_blobs = Vec::new();

    // Loop until all levels of commits and trees are resolved
    while !pending_commits.is_empty() || !pending_trees.is_empty() || !pending_blobs.is_empty() {
        // 1. Process Commits
        if !pending_commits.is_empty() {
            let to_fetch: Vec<String> = pending_commits
                .drain(..)
                .filter(|h| !fetched.contains(h) && !object_exists_locally(repo_path, "commits", h))
                .collect();

            if !to_fetch.is_empty() {
                let fetched_objs = remote::get_objects_batch(origin, &to_fetch, creds)?;
                for obj in fetched_objs {
                    let data = obj.bytes()?;
                    save_object_locally(repo_path, "commits", &obj.hash, &data)?;
                    fetched.insert(obj.hash.clone());
                    println!("  + downloaded commit {}", short_hash(&obj.hash));
                }
            }

            // Inspect all newly available commits to discover trees and parents
            let commits_to_inspect: Vec<String> = fetched
                .iter()
                .filter(|h| object_exists_locally(repo_path, "commits", h))
                .cloned()
                .collect();

            for c_hash in commits_to_inspect {
                if let Ok(commit) = crate::commit::get_commit(repo_path, &c_hash) {
                    if !fetched.contains(&commit.tree)
                        && !object_exists_locally(repo_path, "trees", &commit.tree)
                    {
                        pending_trees.push(commit.tree);
                    }
                    for parent in commit.parents {
                        if !fetched.contains(&parent)
                            && !object_exists_locally(repo_path, "commits", &parent)
                        {
                            pending_commits.push(parent);
                        }
                    }
                }
            }
        }

        // 2. Process Trees
        if !pending_trees.is_empty() {
            let to_fetch: Vec<String> = pending_trees
                .drain(..)
                .filter(|h| !fetched.contains(h) && !object_exists_locally(repo_path, "trees", h))
                .collect();

            if !to_fetch.is_empty() {
                let fetched_objs = remote::get_objects_batch(origin, &to_fetch, creds)?;
                for obj in fetched_objs {
                    let data = obj.bytes()?;
                    save_object_locally(repo_path, "trees", &obj.hash, &data)?;
                    fetched.insert(obj.hash.clone());
                    println!("  + downloaded tree {}", short_hash(&obj.hash));

                    // Inspect tree for child trees and blobs
                    if let Ok(tree) = serde_json::from_slice::<crate::tree::Tree>(&data) {
                        for entry in tree.entries {
                            if entry.object_type == "tree" {
                                if !fetched.contains(&entry.object_hash)
                                    && !object_exists_locally(repo_path, "trees", &entry.object_hash)
                                {
                                    pending_trees.push(entry.object_hash);
                                }
                            } else if entry.object_type == "blob" {
                                if !fetched.contains(&entry.object_hash)
                                    && !object_exists_locally(repo_path, "blobs", &entry.object_hash)
                                {
                                    pending_blobs.push(entry.object_hash);
                                }
                            }
                        }
                    }
                }
            }
        }

        // 3. Process Blobs in Batches
        if !pending_blobs.is_empty() {
            let to_fetch: Vec<String> = pending_blobs
                .drain(..)
                .filter(|h| !fetched.contains(h) && !object_exists_locally(repo_path, "blobs", h))
                .collect();

            // Chunk blob fetches into batches of 100
            for chunk in to_fetch.chunks(100) {
                let fetched_objs = remote::get_objects_batch(origin, chunk, creds)?;
                for obj in fetched_objs {
                    let data = obj.bytes()?;
                    save_object_locally(repo_path, "blobs", &obj.hash, &data)?;
                    fetched.insert(obj.hash.clone());
                }
                println!("  + batch downloaded {} blobs", chunk.len());
            }
        }
    }

    Ok(())
}

fn object_exists_locally(repo_path: &Path, dir: &str, hash: &str) -> bool {
    repo_path.join("objects").join(dir).join(hash).exists()
}

fn save_object_locally(repo_path: &Path, dir: &str, hash: &str, data: &[u8]) -> Result<()> {
    let dest = repo_path.join("objects").join(dir).join(hash);
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(dest, data)?;
    Ok(())
}

fn short_hash(hash: &str) -> &str {
    &hash[..8.min(hash.len())]
}
