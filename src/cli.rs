use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(about, author, version)]
pub struct Cli {
    #[command(subcommand)]
    pub subcommands: Subcommands,
}

#[derive(Subcommand, Debug)]
pub enum Subcommands {
    /// Setup the bot
    Setup {
        data_dir: PathBuf,
        device_name: String,
    },
    /// Run the bot
    Run { data_dir: PathBuf },
    /// Logout the bot
    Logout { data_dir: PathBuf },
}
