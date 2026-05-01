use rusqlite::{Connection, Result};
use uuid::Uuid;

pub fn init_db(path: &str) -> Result<Connection> {
    let conn = Connection::open(path)?;

    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS refs (
            id TEXT PRIMARY KEY,
            bibtex TEXT NOT NULL,
            entry_type TEXT NOT NULL,
            entry_key TEXT NOT NULL UNIQUE,
            title TEXT,
            created_at TEXT DEFAULT CURRENT_TIMESTAMP
        );

        CREATE TABLE IF NOT EXISTS files (
            id TEXT PRIMARY KEY,
            path TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS reference_files (
            reference_id TEXT,
            file_id TEXT,
            PRIMARY KEY (reference_id, file_id)
        );

        CREATE TABLE IF NOT EXISTS tags (
            id TEXT PRIMARY KEY,
            name TEXT UNIQUE NOT NULL
        );

        CREATE TABLE IF NOT EXISTS reference_tags (
            reference_id TEXT,
            tag_id TEXT,
            PRIMARY KEY (reference_id, tag_id),
            FOREIGN KEY (reference_id) REFERENCES refs(id),
            FOREIGN KEY (tag_id) REFERENCES tags(id)
        );
        "
    )?;

    Ok(conn)
}

pub fn add_reference(conn: &Connection, bibtex: &str) -> Result<String> {
    let id = Uuid::new_v4().to_string();

    let (entry_type, entry_key) =
        parse_bibtex_header(bibtex)
            .ok_or_else(|| rusqlite::Error::InvalidQuery)?;

    let title = extract_field_bibtex(bibtex, "title");

    conn.execute(
        "INSERT INTO refs (id, bibtex, entry_type, entry_key, title)
        VALUES (?1, ?2, ?3, ?4, ?5)",
        (&id, bibtex, &entry_type, &entry_key, &title),
    )?;

    Ok(id)
}

pub fn list_references(conn: &Connection) -> Result<Vec<(String, String)>> {
    let mut stmt = conn.prepare(
        "
        SELECT
            r.id,
            r.entry_key,
            r.entry_type,
            r.title,
            GROUP_CONCAT(t.name)
        FROM refs r
        LEFT JOIN reference_tags rt ON r.id = rt.reference_id
        LEFT JOIN tags t ON rt.tag_id = t.id
        GROUP BY r.id
        ORDER BY r.created_at DESC
        "
    )?;

    let rows = stmt.query_map([], |row| {
        let id: String = row.get(0)?;
        let key: String = row.get(1)?;
        let ty: String = row.get(2)?;
        let title: Option<String> = row.get(3)?;
        let tags: Option<String> = row.get(4)?;

        let mut preview = match title {
            Some(t) => format!("{}:  {}", key, t),
            None => format!("@{}{{{},...}}", ty, key)
        };

        if let Some(tag_str) = tags {
            let formatted = tag_str.replace(",", ", ");
            preview.push_str(&format!(" [{}]", formatted));
        }

        Ok((id, preview))
    })?;

    let mut result = Vec::new();
    for r in rows {
        result.push(r?);
    }

    Ok(result)
}

pub fn get_reference(conn: &Connection, id: &str) -> Result<String> {
    conn.query_row(
        "SELECT bibtex FROM refs WHERE id = ?1",
        [id],
        |row| row.get::<_, String>(0),
    )
}

pub fn add_tag_to_reference(conn: &Connection,
                            reference_id: &str,
                            tag_name: &str) -> Result<()> {
    // Get or create tag
    let tag_id = get_or_create_tag(conn, tag_name)?;

    conn.execute(
        "INSERT OR IGNORE INTO reference_tags (reference_id, tag_id)
         VALUES (?1, ?2)",
        (reference_id, tag_id),
    )?;

    Ok(())
}

fn get_or_create_tag(conn: &Connection, name: &str) -> Result<String> {
    let mut stmt = conn.prepare("SELECT id FROM tags WHERE name = ?1")?;
    let mut rows = stmt.query([name])?;

    if let Some(row) = rows.next()? {
        return row.get(0);
    }

    let id = Uuid::new_v4().to_string();

    conn.execute(
        "INSERT INTO tags (id, name) VALUES (?1, ?2)",
        (&id, name),
    )?;

    Ok(id)
}

pub fn get_tags_for_reference(conn: &Connection, reference_id: &str) -> Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "
        SELECT t.name
        FROM tags t
        INNER JOIN reference_tags rt ON t.id = rt.tag_id
        WHERE rt.reference_id = ?1
        "
    )?;

    let rows = stmt.query_map([reference_id], |row| {
        row.get(0)
    })?;

    let mut tags = Vec::new();
    for tag in rows {
        tags.push(tag?);
    }

    Ok(tags)
}


fn extract_field_bibtex(bibtex: &str, field: &str) -> Option<String> {
    for line in bibtex.lines() {
        let line = line.trim();

        if line.to_lowercase().starts_with(&format!("{} =", field)) {
            let value = line.split('=').nth(1)?.trim();

            // Remove comma/braces crudely
            return Some(
                value.trim_matches(|c| c == '{' || c == '}' || c == ',')
                     .trim()
                     .to_string()
                );
        }
    }
    None
}

fn parse_bibtex_header(bibtex: &str) -> Option<(String, String)> {
    let first_line = bibtex.lines().next()?.trim();

    // Expect something like: @book{key,
    if !first_line.starts_with('@') {
        return None;
    }

    let after_at = &first_line[1..];
    let mut parts = after_at.splitn(2, '{');

    let entry_type = parts.next()?.trim().to_string();
    let rest = parts.next()?;

    let entry_key = rest.split(',').next()?.trim().to_string();

    Some((entry_type, entry_key))
}

pub fn resolve_reference(conn: &Connection, input: &str) -> Result<String> {
    // 1. Exact match on entry_key
    let mut stmt = conn.prepare(
        "SELECT id FROM refs WHERE entry_key = ?1"
    )?;

    let mut rows = stmt.query([input])?;
    if let Some(row) = rows.next()? {
        return row.get(0);
    }

    // 2. Exact match on full UUID
    let mut stmt = conn.prepare(
        "SELECT id FROM refs WHERE id = ?1"
    )?;

    let mut rows = stmt.query([input])?;
    if let Some(row) = rows.next()? {
        return row.get(0);
    }

    // 3. Prefix match (short UUID)
    let mut stmt = conn.prepare(
        "SELECT id FROM refs WHERE id LIKE ?1"
    )?;

    let pattern = format!("{}%", input);
    let mut rows = stmt.query([pattern])?;

    let mut matches = Vec::new();
    while let Some(row) = rows.next()? {
        matches.push(row.get::<_, String>(0)?);
    }

    match matches.len() {
        0 => Err(rusqlite::Error::QueryReturnedNoRows),
        1 => Ok(matches[0].clone()),
        _ => Err(rusqlite::Error::InvalidQuery), // ambiguous
    }
}

pub fn import_bibtex(conn: &Connection , path: &str) -> Result<()> {
    let content = std::fs::read_to_string(path)
        .map_err(|_| rusqlite::Error::InvalidQuery)?;

    let entries = split_bibtex_entries(&content);

    let mut added = 0;
    let mut skipped = 0;

    for entry in entries {
        // Validate header before inserting
        let (etry_type, entry_key) = match parse_bibtex_header(&entry) {
            Some(v) => v,
            None => {
                eprintln!("Skipping invalid entry");
                skipped += 1;
                continue
            }
        };

        match add_reference(conn, &entry) {
            Ok(_) => {
                println!("[Ok] {}", entry_key);
                added += 1;
            }
            Err(e) => {
                eprintln!("[ERROR] {}: {}", entry_key, e);
                skipped += 1;
            }
        }
    }
    
    println!("\nImported: {} | Skipped: {}", added, skipped);
    Ok(())
}

fn split_bibtex_entries(input: &str) -> Vec<String> {
    let mut entries = Vec::new();
    let mut current = String::new();
    let mut brace_level = 0;
    let mut in_entry = false;

    for c in input.chars() {
        if c == '@' && !in_entry {
            in_entry = true;
            current.clear();
        }

        if in_entry {
            current.push(c);

            if c == '{' {
                brace_level += 1;
            } else if c == '}' {
                brace_level -= 1;

                if brace_level == 0 {
                    entries.push(current.trim().to_string());
                    in_entry = false;
                }
            }
        }
    }

    entries
}
