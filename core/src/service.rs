use rusqlite::{Connection, Result};
use uuid::Uuid;
use std::fs;

use crate::db;
use crate::reference::Reference;
use crate::bibtex::{parse_bibtex_header, extract_field_bibtex, split_bibtex_entries};

use serde::{Serialize, Deserialize};

pub struct ImportResult {
    pub added: usize,
    pub skipped: usize,
}

#[derive(Serialize, Deserialize)]
pub struct ExportV1 {
    pub version: u32,
    pub references: Vec<ExportReference>,
}

#[derive(Serialize, Deserialize)]
pub struct ExportReference {
    pub id: Option<String>,
    pub bibtex: String,

    #[serde(default)]
    pub tags: Vec<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<ExportContent>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

#[derive(Serialize, Deserialize)]
pub struct ExportContent {
    pub kind: String,
    pub location: String,
}

pub fn add_reference(conn: &Connection, bibtex: &str) -> Result<String> {
    let id = Uuid::new_v4().to_string();

    let (entry_type, entry_key) =
        parse_bibtex_header(bibtex)
            .ok_or_else(|| rusqlite::Error::InvalidQuery)?;

    let title = extract_field_bibtex(bibtex, "title");

    db::insert_reference(
        conn,
        &id,
        bibtex,
        &entry_type,
        &entry_key,
        title.as_deref(),
    )?;

    Ok(id)
}

pub fn remove_reference(
    conn: &Connection,
    input: &str,
) -> Result<()> {
    let id = db::resolve_reference(conn, input)?;
    db::remove_reference(conn, &id)
}


pub fn list_references_and_data(
    conn: &Connection,
    tag: Option<&str>,
) -> Result<Vec<Reference>> {
    let raw = db::list_references(conn, tag)?;

    let mut result = Vec::new();

    for (id, entry_key, entry_type, title, tags) in raw {
        let tags_vec = tags
            .map(|t| t.split(',').map(|s| s.to_string()).collect())
            .unwrap_or_else(Vec::new);

        result.push(Reference {
            id,
            entry_key,
            entry_type,
            title,
            tags: tags_vec,
        });
    }

    Ok(result)
}

pub fn resolve_reference(conn: &Connection, input: &str) -> Result<String> {
    db::resolve_reference(conn, input)
}

pub fn get_reference(conn: &Connection, input: &str) -> Result<String> {
    let id = db::resolve_reference(conn, input)?;
    db::get_reference(conn, &id)
}

pub fn add_tag(conn: &Connection, input: &str, tag: &str) -> Result<()> {
    let id = db::resolve_reference(conn, input)?;
    db::insert_tag(conn, &id, tag)
}

fn infer_content_kind(location: &str) -> &str {
    if location.starts_with("http://") || location.starts_with("https://") {
        "url"
    } else if location.contains('@') && location.contains(':') {
        // Simple ssh heuristic for now
        "ssh"
    } else {
        "local"
    }
}

pub fn add_content(
    conn: &Connection,
    input: &str,
    location: &str,
) -> Result<()> {
    let id = db::resolve_reference(conn, input)?;
    let kind = infer_content_kind(location);

    db::insert_content(conn, &id, kind, location)
}

pub fn get_content(
    conn: &Connection,
    input: &str,
) -> Result<(String, String)> {
    let id = db::resolve_reference(conn, input)?;
    db::get_content(conn, &id)
}

pub fn import_bibtex(conn: &Connection, content: &str) -> Result<ImportResult> {
    let entries = split_bibtex_entries(&content);

    let mut added = 0;
    let mut skipped = 0;

    for entry in entries {
        let (_entry_type, _entry_key) = match parse_bibtex_header(&entry) {
            Some(v) => v,
            None => {
                skipped += 1;
                continue;
            }
        };

        match add_reference(conn, &entry) {
            Ok(_) => added += 1,
            Err(_) => skipped += 1,
        }
    }

    Ok(ImportResult { added, skipped })
}

pub fn export_bibtex(conn: &Connection, input: &str) -> Result<String> {
    let id = db::resolve_reference(conn, input)?;
    let bib = db::get_reference(conn, &id)?;
    
    Ok(bib)
}

pub fn export_bibtex_by_tag(
    conn: &Connection,
    tag: Option<&str>
) -> Result<String> {
    let references = list_references_and_data(conn, tag)?;

    let mut content = String::new();
    for r in references {
        let bib = db::get_reference(conn, &r.id)?;
        content.push_str(&bib);
        content.push_str("\n\n");
    }

    Ok(content)
}

pub fn import_toml(conn: &Connection, content: &str) -> Result<()> {
    let data: ExportV1 = toml::from_str(&content)
        .map_err(|_| rusqlite::Error::InvalidQuery)?;

    if data.version != 1 {
        return Err(rusqlite::Error::InvalidQuery);
    }

    for r in data.references {
        let (entry_type, entry_key) =
            parse_bibtex_header(&r.bibtex)
                .ok_or(rusqlite::Error::InvalidQuery)?;

        let title = extract_field_bibtex(&r.bibtex, "title");
        
        // Generate UUID if empty
        let id = match &r.id {
            Some(id) if !id.trim().is_empty() => id.clone(),
            _ => uuid::Uuid::new_v4().to_string(),
        };

        // If reference already exists, remove it first
        if db::reference_exists(conn, &id)? {
            db::remove_reference(conn, &id)?;
        } else if let Ok(existing_id) =
            db::resolve_reference(conn, &entry_key)
        {
            db::remove_reference(conn, &existing_id)?;
        }

        db::insert_reference(
            conn,
            &id,
            &r.bibtex,
            &entry_type,
            &entry_key,
            title.as_deref(),
        )?;

        // tags
        for tag in r.tags {
            if !tag.trim().is_empty() {
                db::insert_tag(conn, &id, &tag)?;
            }
        }

        // content
        if let Some(c) = r.content {
            if !c.kind.trim().is_empty()
                && !c.location.trim().is_empty() {
                db::insert_content(conn, &id, &c.kind, &c.location)?;
            }
        }

        // note
        if let Some(note) = r.note {
            if !note.trim().is_empty() {
                db::set_note_path(conn, &id, &note)?;
            }
        }
    }

    Ok(())
}

pub fn export_toml(conn: &Connection, input: &str) -> Result<String> {
    let id = db::resolve_reference(conn, input)?;

    let bibtex = db::get_reference(conn, &id)?;
    let tags = db::get_tags_for_reference(conn, &id)?;
    let content = match db::get_content(conn, &id) {
        Ok((kind, location)) => Some(ExportContent { kind, location }),
        Err(_) => None,
    };
    let note = db::get_note_path(conn, &id).ok().flatten();

    let export = ExportV1 {
        version: 1,
        references: vec![
            ExportReference {
                id: Some(id),
                bibtex,
                tags,
                content,
                note,
            }
        ],
    };

    Ok(toml::to_string_pretty(&export).unwrap())
}

pub fn export_toml_by_tag(
    conn: &Connection, 
    tag: Option<&str>,
) -> Result<String> {
    let raw_references = db::list_references(conn, tag)?;
    
    let mut references = Vec::new();

    for (id, _entry_key, _entry_type, _title, _tags_str) in raw_references {
        let bibtex = db::get_reference(conn, &id)?;
        let tags = db::get_tags_for_reference(conn, &id)?;
        let content = match db::get_content(conn, &id) {
            Ok((kind, location)) => Some(ExportContent { kind, location }),
            Err(_) => None,
        };
        let note = db::get_note_path(conn, &id).ok().flatten();

        references.push(ExportReference {
            id: Some(id),
            bibtex,
            tags,
            content,
            note,
        });
    }

    let export = ExportV1 {
        version: 1,
        references,
    };
    
    Ok(toml::to_string_pretty(&export).unwrap())
}

pub fn export_ref_toml(conn: &Connection, id: &str) -> Result<String> {
    let bibtex = db::get_reference(conn, id)?;
    let tags = db::get_tags_for_reference(conn, id)?;
    let content = match db::get_content(conn, id) {
        Ok((kind, location)) => Some(ExportContent { kind, location }),
        Err(_) => None,
    };
    let note = db::get_note_path(conn, id).ok().flatten();

    let r = ExportReference {
        id: Some(id.to_string()),
        bibtex,
        tags,
        content,
        note,
    };

    Ok(toml::to_string_pretty(&r).unwrap())
}

pub fn import_ref_toml(conn: &Connection, content: &str) -> Result<()> {
    let r: ExportReference = toml::from_str(content)
        .map_err(|_| rusqlite::Error::InvalidQuery)?;

    let (entry_type, entry_key) =
        parse_bibtex_header(&r.bibtex)
            .ok_or(rusqlite::Error::InvalidQuery)?;

    let title = extract_field_bibtex(&r.bibtex, "title");

    let id = match &r.id {
        Some(id) if !id.trim().is_empty() => id.clone(),
        _ => uuid::Uuid::new_v4().to_string(),
    };

    if db::reference_exists(conn, &id)? {
        db::remove_reference(conn, &id)?;
    } else if let Ok(existing_id) = db::resolve_reference(conn, &entry_key) {
        db::remove_reference(conn, &existing_id)?;
    }

    db::insert_reference(conn, &id, &r.bibtex, &entry_type, &entry_key, title.as_deref())?;

    for tag in r.tags {
        if !tag.trim().is_empty() {
            db::insert_tag(conn, &id, &tag)?;
        }
    }

    if let Some(c) = r.content {
        if !c.kind.trim().is_empty() && !c.location.trim().is_empty() {
            db::insert_content(conn, &id, &c.kind, &c.location)?;
        }
    }

    if let Some(note) = r.note {
        if !note.trim().is_empty() {
            db::set_note_path(conn, &id, &note)?;
        }
    }

    Ok(())
}

pub fn complete_entry_keys(
    conn: &Connection,
    partial: &str,
) -> Result<Vec<String>> {
    db::complete_entry_keys(conn, partial)
}

pub fn set_note(conn: &Connection, id: &str, path: &str) -> Result<()> {
    db::set_note_path(conn, id, path)
}

pub fn get_note(conn: &Connection, input: &str) -> Result<Option<String>> {
    let id = db::resolve_reference(conn, input)?;
    db::get_note_path(conn, &id)
}
