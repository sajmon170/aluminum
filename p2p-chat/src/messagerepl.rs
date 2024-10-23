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
    Img { path: PathBuf },
    File { path: PathBuf }
}
