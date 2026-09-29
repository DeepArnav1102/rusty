use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(name = "rusty")]
#[command(about = "Git like version control")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    Init,
    HashObject {
        file: String,
    },
    Add {
        file: String,
    },
    Status,
    WriteTree,
    Commit {
        #[arg(short, long)]
        message: String,
    },
    Log,
    Push,
    Login {
        #[arg(short, long)]
        server: Option<String>,
    },
    Logout,
    Whoami,
    Remote {
        #[command(subcommand)]
        command: RemoteCommands,
    },
}

#[derive(Subcommand, Debug)]
pub enum RemoteCommands {
    Add { name: String, url: String },
}
