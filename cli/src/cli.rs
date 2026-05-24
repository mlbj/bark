use clap::{Parser, Subcommand, CommandFactory};
use clap_complete::{
    generate,
    shells::{Bash},
};

use std::io::{self, Read};
use std::fs;
use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

use bark_core::{service, Bark};

use crate::sync;

#[derive(Parser)]
#[command(name = "bark",
          version,
          about = "A minimal, headless reference manager for storing BibTex entries and associated files"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Add a new reference (defaults to @article)
    Add {
        /// BibTeX entry type: article, book, misc, inproceedings, incollection,
        /// phdthesis, mastersthesis, techreport, unpublished
        #[arg(index = 1)]
        entry_type: Option<String>,
    },

    /// Remove a reference 
    Rm {
        /// Entry key, full id or short id
        input: String,
    },

    /// Edit a reference
    Edit {
        /// Entry key, full id or short id
        input: String,
    },

    /// List stored references
    List {
        /// Filter by tag
        #[arg(index = 1)]
        tag: Option<String>,
    },

    /// Show full BibTeX entry
    Show {
        /// Entry key, full id or short id
        input: String },

    /// Export references
    Export {
        /// Entry key, full id or short id
        #[arg(index = 1)]
        input: Option<String>,

        /// Filter by tag
        #[arg(long)]
        tag: Option<String>,
        
        /// Export TOML snapshot
        #[arg(long)]
        toml: bool,
    },

    /// Import references
    Import {
        /// Input file
        filename: String,

        #[arg(long)]
        toml: bool,
    },

    /// Add a tag to a reference
    Tag {
        /// Entry key, full id or short id
        input: String,
        
        /// Tag name
        tag: String
    },
    
    /// Attach content to a reference
    Attach {
        /// Entry key, full id or short id
        input: String,

        /// Content location
        location: String,
    },
    
    /// Open reference content
    Open {
        /// Entry key, full id or short id
        input: String,
    },

    /// Sync bark library using a git repository
    Sync {
        /// Restore or push actions
        #[command(subcommand)]
        action: SyncAction,
    },

    /// Generate command completion file
    Completions {
        /// Target shell
        shell: String,
    },

    #[command(hide = true)]
    Complete {
        kind: String,
        partial: Option<String>,
    },
}

#[derive(Subcommand)]
pub enum SyncAction {
    /// Force pull database from remote sync repository
    Restore,

    /// Push local database to remote sync repository 
    Push,
}

fn bibtex_template(entry_type: &str) -> String {
    match entry_type.to_lowercase().as_str() {
        "article" => "@article{key,\n  author = {},\n  title = {},\n  journal = {},\n  year = {},\n  volume = {},\n  pages = {},\n}\n".to_string(),
        "book" => "@book{key,\n  author = {},\n  title = {},\n  publisher = {},\n  year = {},\n}\n".to_string(),
        "inproceedings" | "conference" => "@inproceedings{key,\n  author = {},\n  title = {},\n  booktitle = {},\n  year = {},\n  pages = {},\n}\n".to_string(),
        "incollection" => "@incollection{key,\n  author = {},\n  title = {},\n  booktitle = {},\n  publisher = {},\n  year = {},\n  pages = {},\n}\n".to_string(),
        "phdthesis" => "@phdthesis{key,\n  author = {},\n  title = {},\n  school = {},\n  year = {},\n}\n".to_string(),
        "mastersthesis" => "@mastersthesis{key,\n  author = {},\n  title = {},\n  school = {},\n  year = {},\n}\n".to_string(),
        "techreport" => "@techreport{key,\n  author = {},\n  title = {},\n  institution = {},\n  year = {},\n  number = {},\n}\n".to_string(),
        "unpublished" => "@unpublished{key,\n  author = {},\n  title = {},\n  note = {},\n  year = {},\n}\n".to_string(),
        "misc" => "@misc{key,\n  author = {},\n  title = {},\n  howpublished = {},\n  year = {},\n  note = {},\n}\n".to_string(),
        other => format!("@{}{{\n  key,\n  author = {{}},\n  title = {{}},\n  year = {{}},\n}}\n", other),
    }
}

impl Cli {
    pub fn run(self, bark: &Bark) -> Result<(), Box<dyn std::error::Error>> {
        let conn = bark.conn();

        match self.command {
            Commands::Add { entry_type } => {
                let editor = env::var("BARK_TEXT_EDITOR")
                    .unwrap_or_else(|_| "vim".to_string());

                let tmp_path = env::temp_dir().join("bark_add.toml");

                let bibtex_body = bibtex_template(entry_type.as_deref().unwrap_or("article"));
                let template = format!(
                    "version = 1\n\n[[references]]\nid = \"\"\nbibtex = \"\"\"\n{}\"\"\"\ntags = []\n\n[references.content]\nkind = \"\"\nlocation = \"\"\n",
                    bibtex_body
                );

                fs::write(&tmp_path, &template)?;

                Command::new(&editor)
                    .arg(&tmp_path)
                    .status()?;

                let input = fs::read_to_string(&tmp_path)?;

                if input.trim().is_empty() {
                    println!("Aborted (empty entry)");
                    return Ok(());
                }
                service::import_toml(conn, &input)?;
                println!("Reference imported");

                fs::remove_file(&tmp_path).ok();
            }

            Commands::Rm { input } => {
                service::remove_reference(conn, &input)?;
            }

            Commands::Edit { input } => {
                let editor = env::var("BARK_TEXT_EDITOR")
                    .unwrap_or_else(|_| "vim".to_string());

                let tmp_path = env::temp_dir().join("bark_add.toml");

                let content = service::export_toml(conn, &input)?;

                fs::write(&tmp_path, content);

                Command::new(&editor)
                    .arg(&tmp_path)
                    .status()?;

                let input = fs::read_to_string(&tmp_path)?;

                if input.trim().is_empty() {
                    println!("Aborted (empty entry)");
                    return Ok(());
                }
                service::import_toml(conn, &input)?;
                println!("Reference saved");

                fs::remove_file(&tmp_path).ok();

            }

            Commands::List { tag } => {
                let references = service::list_references_and_data(conn, tag.as_deref())?;

                let max_type = references
                    .iter()
                    .map(|r| r.entry_type.len())
                    .max()
                    .unwrap_or(0);

                let max_key = references
                    .iter()
                    .map(|r| r.entry_key.len())
                    .max()
                    .unwrap_or(0);

                let max_tags = references
                    .iter()
                    .map(|r| r.tags.join(", ").len())
                    .max()
                    .unwrap_or(0) + 2;

                let max_title = references
                    .iter()
                    .map(|r| {
                        r.title
                            .as_deref()
                            .unwrap_or("<no title>")
                            .len()
                    })
                    .max()
                    .unwrap_or(0);
                    

                for r in references {
                    // Currently not used 
                    //let short_id = &r.id[..8];

                    let title =
                        r.title.unwrap_or_else(|| "<no title>".to_string());
                    
                    let tags = if r.tags.is_empty() {
                        String::new()
                    } else {
                        format!("[{}]", r.tags.join(", "))
                    };

                    println!(
                        "{:type_width$}  {:key_width$}  {:title_width$}  {:tags_width$}  ",
                        r.entry_type,
                        r.entry_key,
                        title,
                        tags,
                        type_width = max_type,
                        key_width = max_key,
                        title_width = max_title,
                        tags_width = max_tags,
                    );
                }
            }

            Commands::Show { input } => {
                let bib = service::get_reference(conn, &input)?;
                println!("{}", bib);

                match service::get_content(conn, &input) {
                    Ok((kind, location)) => {
                        println!("\n---");
                        println!("content: {} ({})", location, kind);
                    }
                    Err(_) => {
                        // No content (stay silent)
                    }
                }
            }

            Commands::Export { input, tag, toml } => {
                if toml {
                    let content = if let Some(input) = input {
                        service::export_toml(conn, &input)?
                    } else {
                        service::export_toml_by_tag(conn, tag.as_deref())?
                    };

                    fs::write("bark.toml", content)?;
                } else {
                    let content = if let Some(input) = input {
                        service::export_bibtex(conn, &input)?
                    } else {
                        service::export_bibtex_by_tag(conn, tag.as_deref())?
                    };

                    fs::write("references.bib", content)?;
                }
            }

            Commands::Import { filename, toml } => {
                let content = std::fs::read_to_string(&filename)?;
                let path = std::path::Path::new(&filename);
                let extension = path.extension().and_then(|s| s.to_str());

                match (toml, extension) {
                    (true, _) | (false, Some("toml")) => {
                        service::import_toml(conn, &content)?;
                        println!("Imported TOML reference(s)");
                    }
                    (false, Some("bib")) => {

                        let result = service::import_bibtex(conn, &content)?;
                        println!("Imported: {} | Skipped: {}", result.added, result.skipped);
                    }
                    _ => {
                        return Err(format!(
                            "Could not infer format from '{}'. Use --toml or provide a .bib/.toml file",
                            filename
                        ).into());
                    }
                }
            }

            Commands::Tag { input, tag } => {
                service::add_tag(conn, &input, &tag)?;
            }

            Commands::Attach{ input, location } => {
                service::add_content(conn, &input, &location)?;
            }

            Commands::Open { input } => {
                let (kind, location) = service::get_content(conn, &input)?;

                match kind.as_str() {
                    "url" => {
                        let tmp_path: PathBuf = env::temp_dir().join("bark_tmp.pdf");
                        std::process::Command::new("curl")
                            .args(["-L", "-o"])
                            .arg(&tmp_path)
                            .arg(&location)
                            .status()?;
                        std::process::Command::new("xdg-open")
                            .arg(&tmp_path)
                            .spawn()?;
                    }
                    "local" => {
                        std::process::Command::new("xdg-open")
                            .arg(&location)
                            .spawn()?;
                    }
                    "ssh" => {
                        // location: user@host:/path/to/file
                        let mut parts = location.splitn(2, ":");
                        let host = parts.next().ok_or("Invalid ssh location")?;
                        let path = parts.next().ok_or("Invalid ssh location")?;

                        // Copy file and open locally
                        let tmp_path: PathBuf = env::temp_dir().join("bark_tmp.pdf");
                        std::process::Command::new("scp")
                            .arg(format!("{}:{}", host, path))
                            .arg(&tmp_path)
                            .status()?;
                        std::process::Command::new("xdg-open")
                            .arg(&tmp_path)
                            .spawn()?;
                    }
                    _ => {
                        eprintln!("Unknown content kind: {}", kind)
                    }
                }
            }

            Commands::Sync { action } => {
                match action {
                    SyncAction::Restore => sync::restore(bark)?,
                    SyncAction::Push => sync::push(bark)?,
                }
            }

            Commands::Completions { shell } => {
                let mut cmd = Cli::command();
                let mut buf = Vec::new();

                match shell.as_str() {
                    "bash" => {
                        generate(Bash, &mut cmd, "bark", &mut buf);

                        let script = String::from_utf8(buf).unwrap();

                        print!("{}", script);

                        print!(r#"

_bark_custom()
{{
    local cur="${{COMP_WORDS[COMP_CWORD]}}"
    local prev="${{COMP_WORDS[COMP_CWORD-1]}}"

    # complete subcommands
    if [[ $COMP_CWORD -eq 1 ]]; then
        COMPREPLY=($(compgen -W \
            "add attach edit export import list open rm show completions complete" \
            -- "$cur"))
        return
    fi

    # complete reference keys
    case "$prev" in
        open|show|rm|edit|attach|export)
            COMPREPLY=(
                $(compgen -W "$(bark complete refs "$cur")" -- "$cur")
            )
            return
            ;;
    esac
}}

complete -F _bark_custom bark

"#);
                    }

                    _ => eprintln!("unsupported shell"),
                }
            }

            Commands::Complete { kind, partial } => {
                let partial = partial.unwrap_or_default();

                match kind.as_str() {
                    "refs" => {
                        for key in service::complete_entry_keys(conn, &partial)? {
                            println!("{}", key);
                        }
                    }
                    _ => {}
                }
            }
        }

        Ok(())
    }
}
