use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "bark")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    Add,
    List,
    Show { input: String },
    Export,
    Import { filename: String },
    Tag { input: String, tag: String }
}
