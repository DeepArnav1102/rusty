use anyhow::{Context, Result};
use std::collections::HashSet;
use std::fs;
use std::path::Path;

use crate::auth::{self, Credentials};
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

    let mut sent = HashSet::new();

    println!("Pushing objects to {}...", origin);

    send_object(repo_path, &origin, "commit", commit_hash, &mut sent, &creds)?;

    // Update remote branch ref
    remote::update_ref(&origin, branch, commit_hash, &creds)?;

    let clean_branch = branch.strip_prefix("refs/heads/").unwrap_or(branch);

    println!(
        "\n✓ Pushed commit {} to branch '{}'",
        &commit_hash[..8.min(commit_hash.len())],
        clean_branch
    );

    println!("  Remote: {}", origin);
    println!("  View your code on the website repository page.\n");

    Ok(())
}

fn send_object(
    repo_path: &Path,
    origin: &str,
    object_type: &str,
    hash: &str,
    sent: &mut HashSet<String>,
    creds: &Credentials,
) -> Result<()> {
    if sent.contains(hash) {
        return Ok(());
    }

    sent.insert(hash.to_string());

    let object_dir = match object_type {
        "blob" => "blobs",
        "tree" => "trees",
        "commit" => "commits",
        _ => anyhow::bail!("Unknown object type: {}", object_type),
    };

    let object_path = repo_path.join("objects").join(object_dir).join(hash);

    let data =
        fs::read(&object_path).with_context(|| format!("Cannot read {} {}", object_type, hash))?;

    if remote::object_exists(origin, hash, creds)? {
        println!(
            "  ✓ {} {} (already exists on remote)",
            object_type,
            &hash[..8.min(hash.len())]
        );
    } else {
        remote::send_object(origin, object_type, hash, &data, creds)?;

        println!(
            "  + uploaded {} {}",
            object_type,
            &hash[..8.min(hash.len())]
        );
    }

    match object_type {
        "commit" => {
            let commit: Commit = serde_json::from_slice(&data)?;

            // Send the tree referenced by this commit.
            send_object(repo_path, origin, "tree", &commit.tree, sent, creds)?;

            // Send ALL parents.
            // This is required for merge commits.
            for parent in &commit.parents {
                send_object(repo_path, origin, "commit", parent, sent, creds)?;
            }
        }

        "tree" => {
            let tree: Tree = serde_json::from_slice(&data)?;

            for entry in tree.entries {
                send_object(
                    repo_path,
                    origin,
                    &entry.object_type,
                    &entry.object_hash,
                    sent,
                    creds,
                )?;
            }
        }

        "blob" => {}

        _ => {}
    }

    Ok(())
}
