use anyhow::{Context, Result};
use std::collections::HashSet;
use std::fs;
use std::path::Path;

use crate::auth::Credentials;
use crate::remote;

pub fn fetch(repo_path: &Path) -> Result<()> {
    let creds = crate::auth::require_auth()?;

    let origin = remote::get_remote(repo_path, "origin")?;

    let branch = get_current_branch(repo_path)?;

    println!("Fetching from {}...", origin);

    let commit_hash = remote::get_ref(
        &origin,
        &branch,
        &creds,
    )?;

    if commit_hash.trim().is_empty() {
        anyhow::bail!("Remote branch '{}' has no commits", branch);
    }

    let mut fetched = HashSet::new();

    fetch_object(
        repo_path,
        &origin,
        "commit",
        commit_hash.trim(),
        &mut fetched,
        &creds,
    )?;

    let remote_ref = repo_path
        .join("refs")
        .join("remotes")
        .join("origin");

    fs::create_dir_all(&remote_ref)?;

    fs::write(
        remote_ref.join(&branch),
        commit_hash.trim(),
    )?;

    println!(
        "\nFetched commit {}",
        &commit_hash[..8.min(commit_hash.len())]
    );

    println!(
        "Updated origin/{}",
        branch
    );

    Ok(())
}

fn get_current_branch(repo_path: &Path) -> Result<String> {
    let head_path = repo_path.join("HEAD");

    let head = fs::read_to_string(head_path)?;

    let branch = head
        .strip_prefix("ref: ")
        .context("Invalid HEAD file")?
        .trim();

    let branch = branch
        .strip_prefix("refs/heads/")
        .unwrap_or(branch);

    Ok(branch.to_string())
}

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

    let object_path = repo_path
        .join("objects")
        .join(object_dir)
        .join(hash);

    if object_path.exists() {
        println!(
            "  ✓ {} {} already exists",
            object_type,
            &hash[..8.min(hash.len())]
        );
    } else {
        let object = remote::get_object(
            origin,
            hash,
            creds,
        )?;

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

        fs::write(
            &object_path,
            data.as_bytes(),
        )?;

        println!(
            "  + downloaded {} {}",
            object_type,
            &hash[..8.min(hash.len())]
        );
    }

    match object_type {
        "commit" => {
            let data = fs::read(&object_path)?;

            let commit: crate::commit::Commit =
                serde_json::from_slice(&data)?;

            fetch_object(
                repo_path,
                origin,
                "tree",
                &commit.tree,
                fetched,
                creds,
            )?;

            if let Some(parent) = commit.parent {
                fetch_object(
                    repo_path,
                    origin,
                    "commit",
                    &parent,
                    fetched,
                    creds,
                )?;
            }
        }

        "tree" => {
            let data = fs::read(&object_path)?;

            let tree: crate::tree::Tree =
                serde_json::from_slice(&data)?;

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
