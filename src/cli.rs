use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(about, author, version)]
pub struct Cli {
    #[command(subcommand)]
    pub subcommands: Subcommands,
    /// Use custom config file
    #[arg(long, short, value_name = "FILE")]
    pub config: Option<PathBuf>,
}

#[derive(Subcommand, Debug)]
pub enum Subcommands {
    /// Setup the bot
    Setup {
        data_dir: PathBuf,
        device_name: String,
    },
    /// Run the bot
    Run { data_dir: Option<PathBuf> },
    /// Logout the bot
    Logout { data_dir: PathBuf },
}
