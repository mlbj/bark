mod db;
mod cli;

use clap::Parser;
use cli::{Cli, Commands};
use std::io::{self, Read};
use directories::ProjectDirs;
use std::path::PathBuf;
use std::fs;

fn get_db_path() -> PathBuf {
    let proj_dirs = ProjectDirs::from("", "", "bark")
        .expect("Could not determine data directory");

    let data_dir = proj_dirs.data_dir();

    std::fs::create_dir_all(data_dir).unwrap();

    data_dir.join("library.db")
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    let db_path = get_db_path();
    let conn = db::init_db(db_path.to_str().unwrap())?;

    match cli.command {
        Commands::Add => {
            println!("Paste BibTeX, Ctrl+D to finish:\n");

            let mut input = String::new();
            io::stdin().read_to_string(&mut input)?;

            let id = db::add_reference(&conn, &input)?;
            println!("Saved as {}", id);
        }

        Commands::List => {
            let refs = db::list_references(&conn)?;
            for (id, preview) in refs {
                println!("{} | {}", id, preview);
            }
        }

        Commands::Show { id } => {
            let bib = db::get_reference(&conn, &id)?;
            println!("{}", bib);
        }

        Commands::Export => {
            let filename = "references.bib";
            let mut content = String::new();
            
            // Fill content
            let refs = db::list_references(&conn)?;
            for (id, _preview) in refs {
                let bib = db::get_reference(&conn, &id)?;
                content.push_str(&bib);

                // Break line
                content.push_str("\n\n");
            }

            // Save file
            fs::write(filename, content)?;
        }

        Commands::Tag { id, tag } => {
            db::add_tag_to_reference(&conn, &id, &tag)?;
        }
    }

    Ok(())
}
