use anyhow::Result;
use std::fs;
use std::path::Path;

use crate::repository::get_head_commit;


pub fn create_branch(
    repo_path: &Path,
    branch_name: &str,
) -> Result<()> {

    let head_commit = get_head_commit(repo_path)?
        .ok_or_else(|| anyhow::anyhow!("Cannot create branch: no commits yet"))?;


    let branch_path = repo_path
        .join("refs")
        .join("heads")
        .join(branch_name);


    if branch_path.exists() {
        anyhow::bail!(
            "Branch '{}' already exists",
            branch_name
        );
    }


    fs::create_dir_all(
        branch_path.parent().unwrap()
    )?;


    fs::write(
        branch_path,
        head_commit,
    )?;


    println!(
        "Created branch '{}'",
        branch_name
    );


    Ok(())
}



pub fn list_branches(
    repo_path: &Path,
) -> Result<()> {

    let heads_path = repo_path
        .join("refs")
        .join("heads");


    if !heads_path.exists() {
        return Ok(());
    }


    let head = fs::read_to_string(
        repo_path.join("HEAD")
    )?;

    let current_branch = head
        .strip_prefix("ref: refs/heads/")
        .unwrap_or("")
        .trim();



    let mut branches = Vec::new();


    for entry in fs::read_dir(heads_path)? {

        let entry = entry?;

        let name = entry
            .file_name()
            .to_string_lossy()
            .to_string();

        branches.push(name);
    }


    branches.sort();


    for branch in branches {

        if branch == current_branch {

            println!(
                "* {}",
                branch
            );

        } else {

            println!(
                "  {}",
                branch
            );

        }
    }


    Ok(())
}
