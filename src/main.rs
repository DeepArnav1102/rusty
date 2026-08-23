mod add;
mod cli;
mod commit;
mod index;
mod log;
mod objects;
mod repository;
mod status;
mod tree;

use anyhow::Result;
use clap::Parser;
use cli::{Cli, Commands};
use std::env;
use std::path::PathBuf;

fn find_repo_path() -> Result<PathBuf> {
    let mut current_dir = env::current_dir()?;

    loop {
        let repo_path = current_dir.join(".rusty");

        if repo_path.is_dir() {
            return Ok(repo_path);
        }

        if !current_dir.pop() {
            break;
        }
    }

    anyhow::bail!("Not a rusty repository");
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Init => {
            repository::init()?;
        }

        Commands::HashObject { file } => {
            let repo_path = find_repo_path()?;

            let hash = objects::hash_objects(&PathBuf::from(file), &repo_path)?;

            println!("Hash = {}", hash);
        }

        Commands::Add { file } => {
            let repo_path = find_repo_path()?;

            add::add_file(&PathBuf::from(file), &repo_path)?;
        }

        Commands::Status => {
            let repo_path = find_repo_path()?;

            status::check_status(&repo_path)?;
        }

        Commands::WriteTree => {
            let repo_path = find_repo_path()?;

            let hash = tree::write_tree(&repo_path)?;

            println!("Root tree: {}", hash);
        }

        Commands::Commit { message } => {
            let repo_path = find_repo_path()?;
            let hash = commit::create_commit(&repo_path, message)?;
            println!("Created commit: {}", hash);
        }
        Commands::Log => {
            let repo_path = find_repo_path()?;
            log::show_log(&repo_path)?;
        }
    }

    Ok(())
}
