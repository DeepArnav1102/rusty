mod add;
mod cli;
mod index;
mod objects;
mod repository;
mod status;
mod tree;

use anyhow::Result;
use clap::Parser;
use cli::{Cli, Commands};
use std::env;
use std::path::PathBuf;

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Init => {
            repository::init()?;
        }
        Commands::HashObject { file } => {
            let curr_dir = env::current_dir()?;
            let repo_path = curr_dir.join(".rusty");

            let hash = objects::hash_objects(&PathBuf::from(file), &repo_path)?;
            println!("Hash = {}", hash);
        }
        Commands::Add { file } => {
            let curr_dir = env::current_dir()?;
            let repo_path = curr_dir.join(".rusty");
            add::add_file(&PathBuf::from(file), &repo_path)?;
        }
        Commands::Status => {
            let curr_dir = env::current_dir()?;
            let repo_path = curr_dir.join(".rusty");
            status::check_status(&repo_path)?;
        }
        Commands::WriteTree => {
            let curr_dir = env::current_dir()?;
            let repo_path = curr_dir.join(".rusty");

            let hash = tree::write_tree(&repo_path)?;

            println!("Root tree: {}", hash);
        }
    }
    Ok(())
}
