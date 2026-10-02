use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(name = "rusty")]
#[command(about = "Git like version control")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Launch interactive Terminal UI dashboard
    Tui,
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
    Fetch,
    Checkout {
        branch: String,
    },
    Rm {
        #[arg(long)]
        cached: bool,

        path: String,
    },
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
    Merge {
        branch: String,
    },
    Branch {
        name: Option<String>,

        #[arg(short, long)]
        remote: bool,

        #[arg(short, long)]
        all: bool,
    },
    Pull,
}

#[derive(Subcommand, Debug)]
pub enum RemoteCommands {
    Add { name: String, url: String },
}
