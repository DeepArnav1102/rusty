use anyhow::{Context, Result};
use std::collections::HashSet;
use std::fs;
use std::path::Path;

use crate::auth;
use crate::commit::Commit;
use crate::remote;
use crate::tree::Tree;

pub fn push(repo_path: &Path) -> Result<()> {
    let creds = auth::require_auth()?;

    println!("Authenticated as: {}", creds.email);

    let origin = remote::get_remote(repo_path, "origin")?;

    let head_path = repo_path.join("HEAD");
    let head = fs::read_to_string(head_path)?;

    let branch = head
        .strip_prefix("ref: ")
        .context("Invalid HEAD file")?
        .trim();

    let branch_path = repo_path.join(branch);

    if !branch_path.exists() {
        anyhow::bail!("No commits to push. Make a commit with `rusty commit -m \"...\"` first.");
    }

    let commit_hash = fs::read_to_string(branch_path)?;
    let commit_hash = commit_hash.trim();

    if commit_hash.is_empty() {
        anyhow::bail!("No commits to push");
    }

    println!("Scanning and packaging objects for {}...", origin);

    // 1. Gather all local objects reachable from the target commit
    let mut reachable = Vec::new();
    let mut visited = HashSet::new();
    collect_reachable_objects(repo_path, "commit", commit_hash, &mut visited, &mut reachable)?;

    println!("Total local objects in lineage: {}", reachable.len());

    // 2. N+1 Elimination: Batch query remote for missing objects
    let all_hashes: Vec<String> = reachable.iter().map(|(h, _, _)| h.clone()).collect();
    let batch_check = remote::check_objects_batch(&origin, &all_hashes, &creds)?;

    let missing_hashes: HashSet<String> = batch_check.missing.into_iter().collect();

    println!(
        "Remote status: {} already present, {} to upload",
        batch_check.existing.len(),
        missing_hashes.len()
    );

    // 3. Batch upload only missing objects (chunked into batches of 100)
    let missing_objects: Vec<&(String, &'static str, std::path::PathBuf)> = reachable
        .iter()
        .filter(|(h, _, _)| missing_hashes.contains(h))
        .collect();

    let chunk_size = 100;
    for (chunk_idx, chunk) in missing_objects.chunks(chunk_size).enumerate() {
        let mut payloads = Vec::new();
        for (hash, obj_type, path) in chunk {
            let data_bytes = fs::read(path)
                .with_context(|| format!("Cannot read {} {}", obj_type, hash))?;
            let data_str = String::from_utf8_lossy(&data_bytes).to_string();
            payloads.push(remote::BatchObjectPayload {
                object_type: obj_type.to_string(),
                hash: hash.clone(),
                data: data_str,
            });
        }

        remote::send_objects_batch(&origin, &payloads, &creds)?;
        let uploaded_so_far = (chunk_idx + 1) * chunk_size;
        let total_missing = missing_objects.len();
        println!(
            "  + batch uploaded {}/{} objects",
            uploaded_so_far.min(total_missing),
            total_missing
        );
    }

    // 4. Update remote branch ref
    remote::update_ref(&origin, branch, commit_hash, &creds)?;

    let clean_branch = branch.strip_prefix("refs/heads/").unwrap_or(branch);

    println!(
        "\n✓ Successfully pushed commit {} to branch '{}'",
        &commit_hash[..8.min(commit_hash.len())],
        clean_branch
    );

    println!("  Remote: {}", origin);
    println!("  View your code on the website repository page.\n");

    Ok(())
}

fn collect_reachable_objects(
    repo_path: &Path,
    object_type: &'static str,
    hash: &str,
    visited: &mut HashSet<String>,
    collected: &mut Vec<(String, &'static str, std::path::PathBuf)>,
) -> Result<()> {
    if visited.contains(hash) {
        return Ok(());
    }
    visited.insert(hash.to_string());

    let object_dir = match object_type {
        "blob" => "blobs",
        "tree" => "trees",
        "commit" => "commits",
        _ => anyhow::bail!("Unknown object type: {}", object_type),
    };

    let object_path = repo_path.join("objects").join(object_dir).join(hash);
    if !object_path.exists() {
        return Ok(());
    }

    collected.push((hash.to_string(), object_type, object_path.clone()));

    let data = fs::read(&object_path)?;

    match object_type {
        "commit" => {
            let commit: Commit = serde_json::from_slice(&data)?;
            collect_reachable_objects(repo_path, "tree", &commit.tree, visited, collected)?;
            for parent in &commit.parents {
                collect_reachable_objects(repo_path, "commit", parent, visited, collected)?;
            }
        }
        "tree" => {
            let tree: Tree = serde_json::from_slice(&data)?;
            for entry in tree.entries {
                let entry_type = if entry.object_type == "blob" {
                    "blob"
                } else {
                    "tree"
                };
                collect_reachable_objects(
                    repo_path,
                    entry_type,
                    &entry.object_hash,
                    visited,
                    collected,
                )?;
            }
        }
        _ => {}
    }

    Ok(())
}
