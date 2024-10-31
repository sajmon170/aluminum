use std::path::PathBuf;

pub use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(multicall = true)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command
}

#[derive(Debug, Subcommand)]
pub enum Command {
    Share { path: PathBuf },
    Accept
}
