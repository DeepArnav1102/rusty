use anyhow::{Context, Result};
use std::env;
use std::fs;
use std::path::PathBuf;

use crate::checkout;
use crate::fetch;
use crate::remote;
use crate::repository;

pub fn clone(url: &str, directory: Option<String>) -> Result<()> {
    // 1. Determine target directory name
    let target_dir_name = match directory {
        Some(dir) => dir,
        None => {
            // Extract the last part of the URL, e.g., "repo" from "http://.../repo"
            let parsed_url = url.trim_end_matches('/');
            let last_part = parsed_url.split('/').last().unwrap_or("rusty-repo");
            last_part.to_string()
        }
    };

    let target_path = PathBuf::from(&target_dir_name);

    if target_path.exists() {
        anyhow::bail!("destination path '{}' already exists", target_dir_name);
    }

    // 2. Create the target directory and switch to it
    fs::create_dir_all(&target_path)
        .with_context(|| format!("Failed to create directory '{}'", target_dir_name))?;
    
    env::set_current_dir(&target_path)
        .with_context(|| format!("Failed to change directory to '{}'", target_dir_name))?;

    println!("Cloning into '{}'...", target_dir_name);

    // 3. Initialize an empty rusty repository
    repository::init()?;
    let repo_path = env::current_dir()?.join(".rusty");

    // 4. Add the 'origin' remote
    remote::add_remote(&repo_path, "origin", url)?;

    // 5. Fetch all data from origin
    fetch::fetch(&repo_path)?;

    // 6. Checkout the default branch
    let remote_refs_dir = repo_path.join("refs").join("remotes").join("origin");
    
    let default_branch = if remote_refs_dir.join("main").exists() {
        "main".to_string()
    } else if remote_refs_dir.join("master").exists() {
        "master".to_string()
    } else {
        // Fallback to the first available remote branch
        let mut first_branch = String::new();
        if remote_refs_dir.exists() {
            if let Ok(entries) = fs::read_dir(&remote_refs_dir) {
                for entry in entries.flatten() {
                    if let Ok(ft) = entry.file_type() {
                        if ft.is_file() {
                            first_branch = entry.file_name().to_string_lossy().into_owned();
                            break;
                        }
                    }
                }
            }
        }
        
        if !first_branch.is_empty() {
            first_branch
        } else {
            "main".to_string() // Ultimate fallback
        }
    };

    if remote_refs_dir.join(&default_branch).exists() {
        checkout::checkout(&repo_path, &default_branch)?;
    } else {
        println!("Warning: No branches found on remote to checkout.");
    }

    Ok(())
}
