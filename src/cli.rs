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
    Show { id: String },
    Export,
    Tag { id: String, tag: String }
}
