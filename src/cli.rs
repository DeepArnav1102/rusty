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
    HashObject { file: String },
    Add { file: String },
    Status,
    WriteTree,
}
