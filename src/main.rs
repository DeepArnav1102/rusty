mod add;
mod auth;
mod checkout;
mod cli;
mod commit;
mod fetch;
mod ignore;
mod index;
mod log;
mod objects;
mod push;
mod remote;
mod repository;
mod status;
mod tree;
mod tui;

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

    let cmd = match cli.command {
        Some(cmd) => cmd,
        None => Commands::Tui,
    };

    match cmd {
        Commands::Tui => {
            tui::run_tui()?;
        }

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

        Commands::Push => {
            let repo_path = find_repo_path()?;

            push::push(&repo_path)?;
        }

        Commands::Fetch => {
            let repo_path = find_repo_path()?;

            fetch::fetch(&repo_path)?;
        }

        Commands::Login { server } => {
            auth::login(server.as_deref())?;
        }

        Commands::Logout => {
            auth::logout()?;
        }

        Commands::Whoami => match auth::load_credentials()? {
            Some(creds) => {
                println!("Logged in as: {}", creds.email);

                if let Some(username) = creds.username {
                    println!("Username:     {}", username);
                }

                println!("Server:       {}", creds.server);
            }

            None => {
                println!("Not logged in. Run `rusty login` to authenticate.");
            }
        },

        Commands::Remote { command } => {
            let repo_path = find_repo_path()?;

            match command {
                cli::RemoteCommands::Add { name, url } => {
                    remote::add_remote(&repo_path, &name, &url)?;
                }
            }
        }
        Commands::Checkout { target } => {
            let repo_path = find_repo_path()?;
            checkout::checkout(&repo_path, &target)?;
        }
    }

    Ok(())
}
