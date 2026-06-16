use std::path::Path;
use rusqlite::Connection;
use uuid::Uuid;

use bark_core::{ExportV1, ExportReference, import_toml, ImportResult};

pub fn make_toml(
    bibtex: &str,
    pdf_url: Option<&str>,
) -> Result<String, Box<dyn std::error::Error>> {
    let content = pdf_url.map(|url| bark_core::ExportContent {
        kind: "url".to_string(),
        location: url.to_string(),
    });

    let export = ExportV1 {
        version: 1,
        references: vec![ExportReference {
            id: Some(Uuid::new_v4().to_string()),
            bibtex: bibtex.to_string(),
            tags: vec![],
            content,
            note: None,
        }],
    };

    Ok(toml::to_string_pretty(&export)?)
}

pub fn write_raw_toml_to_inbox(
    inbox_dir: &Path,
    toml: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    std::fs::create_dir_all(inbox_dir)?;
    let filename = format!("{}.toml", Uuid::new_v4());
    std::fs::write(inbox_dir.join(filename), toml)?;
    Ok(())
}

pub fn fetch_inbox(
    conn: &Connection,
    inbox_dir: &Path,
) -> Result<ImportResult, Box<dyn std::error::Error>> {
    let mut result = ImportResult { added: 0, skipped: 0 };

    if !inbox_dir.exists() {
        return Ok(result);
    }

    let mut paths: Vec<_> = std::fs::read_dir(inbox_dir)?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("toml"))
        .collect();

    paths.sort();

    for path in paths {
        let content = std::fs::read_to_string(&path)?;
        match import_toml(conn, &content) {
            Ok(_) => result.added += 1,
            Err(_) => result.skipped += 1,
        }
        std::fs::remove_file(&path)?;
    }

    Ok(result)
}
