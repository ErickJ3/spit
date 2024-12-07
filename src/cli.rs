use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    Scan {
        #[arg(short, long)]
        url: String,

        #[arg(short, long, default_value = "8080")]
        port: u16,

        #[arg(short = 'H', long, default_value = "127.0.0.1")]
        host: String,

        #[arg(short, long)]
        delay: Option<u64>,

        #[arg(short = 'C', long)]
        config: Option<std::path::PathBuf>,
    },
    File {
        #[arg(short, long)]
        path: PathBuf,

        #[arg(short, long, default_value = "8080")]
        port: u16,

        #[arg(short = 'H', long, default_value = "127.0.0.1")]
        host: String,

        #[arg(short, long)]
        delay: Option<u64>,

        #[arg(short = 'C', long)]
        config: Option<std::path::PathBuf>,
    },
}
