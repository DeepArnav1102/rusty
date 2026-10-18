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
    Clone {
        url: String,
        directory: Option<String>,
    },
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
        branch: Option<String>,

        #[arg(long)]
        abort: bool,
    },
    Branch {
        name: Option<String>,

        #[arg(short, long)]
        remote: bool,

        #[arg(short, long)]
        all: bool,
    },
    Pull,
    Fsck {
        /// Show objects that exist but are never directly referenced
        #[arg(long, default_value_t = true, action = clap::ArgAction::Set)]
        dangling: bool,

        /// Print out objects that exist but are unreachable from any reference
        #[arg(long)]
        unreachable: bool,

        /// Check connectivity of reachable objects without hashing blob payloads
        #[arg(long)]
        connectivity_only: bool,

        /// Enable strict validation (check tree entry names, hash syntax, commit messages)
        #[arg(long)]
        strict: bool,

        /// Write dangling objects into .rusty/lost-found/
        #[arg(long)]
        lost_found: bool,

        /// Output diagnostic report in JSON format for automated tooling
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand, Debug)]
pub enum RemoteCommands {
    Add { name: String, url: String },
}
