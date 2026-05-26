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

        CREATE TABLE IF NOT EXISTS content (
            id TEXT PRIMARY KEY,
            reference_id TEXT NOT NULL UNIQUE,
            kind TEXT NOT NULL,
            location TEXT NOT NULL,
            FOREIGN KEY (reference_id) REFERENCES refs(id)
        );
        "
    )?;

    // Migration: add note_path column if not present
    let has_note_path: bool = {
        let mut stmt = conn.prepare("PRAGMA table_info(refs)")?;
        let cols: Vec<String> = stmt
            .query_map([], |row| row.get::<_, String>(1))?
            .filter_map(|r| r.ok())
            .collect();
        cols.contains(&"note_path".to_string())
    };
    if !has_note_path {
        conn.execute_batch("ALTER TABLE refs ADD COLUMN note_path TEXT")?;
    }

    Ok(conn)
}

pub fn insert_reference(
    conn: &Connection,
    id: &str,
    bibtex: &str,
    entry_type: &str,
    entry_key: &str,
    title: Option<&str>,
) -> Result<()> {
    conn.execute(
        "INSERT INTO refs (id, bibtex, entry_type, entry_key, title)
        VALUES (?1, ?2, ?3, ?4, ?5)",
        (&id, bibtex, &entry_type, &entry_key, &title),
    )?;

    Ok(())
}

pub fn remove_reference(conn: &Connection, id: &str) -> Result<()> {
    // Delete relations first
    conn.execute("DELETE FROM reference_tags where reference_id = ?1", [id])?;
    conn.execute("DELETE FROM content WHERE reference_id = ?1", [id])?;

    // Finally delete the reference
    conn.execute("DELETE FROM refs WHERE id = ?1", [id])?;

    Ok(())
}

pub fn purge(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "
        DELETE FROM reference_tags;
        DELETE FROM tags;
        DELETE FROM content;
        DELETE FROM refs;
        "
    )?;

    Ok(())
}

pub fn list_references(
    conn: &Connection,
    tag: Option<&str>,
) -> Result<Vec<(String, String, String, Option<String>, Option<String>)>> {

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
        WHERE (?1 IS NULL OR r.id IN (
            SELECT rt2.reference_id
            FROM reference_tags rt2
            INNER JOIN tags t2 ON rt2.tag_id = t2.id
            WHERE t2.name = ?1
        ))
        GROUP BY r.id
        ORDER BY r.created_at DESC
        "
    )?;

    let rows = stmt.query_map([tag], |row| {
        let id: String = row.get(0)?;
        let entry_key: String = row.get(1)?;
        let entry_type: String = row.get(2)?;
        let title: Option<String> = row.get(3)?;
        let tags: Option<String> = row.get(4)?;
        Ok((id, entry_key, entry_type, title, tags))
    })?;

    let mut result = Vec::new();
    for r in rows {
        result.push(r?);
    }

    Ok(result)
}

pub fn reference_exists(conn: &Connection, id: &str) -> Result<bool> {
    let mut stmt = conn.prepare(
        "SELECT 1 FROM refs WHERE id = ?1"
    )?;

    let mut rows = stmt.query([id])?;

    Ok(rows.next()?.is_some())
}

pub fn resolve_reference(conn: &Connection, input: &str) -> Result<String> {
    // Exact match on entry_key
    let mut stmt = conn.prepare(
        "SELECT id FROM refs WHERE entry_key = ?1"
    )?;

    let mut rows = stmt.query([input])?;
    if let Some(row) = rows.next()? {
        return row.get(0);
    }

    // Exact match on full UUID
    let mut stmt = conn.prepare(
        "SELECT id FROM refs WHERE id = ?1"
    )?;

    let mut rows = stmt.query([input])?;
    if let Some(row) = rows.next()? {
        return row.get(0);
    }

    // Prefix match (short UUID)
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

pub fn get_reference(conn: &Connection, id: &str) -> Result<String> {
    conn.query_row(
        "SELECT bibtex FROM refs WHERE id = ?1",
        [id],
        |row| row.get::<_, String>(0),
    )
}

pub fn insert_tag(conn: &Connection,
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

pub fn insert_content(
    conn: &Connection,
    reference_id: &str,
    kind: &str,
    location: &str,
) -> Result<()> {
    let id = Uuid::new_v4().to_string();

    conn.execute(
        "INSERT INTO content (id, reference_id, kind, location)
         VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(reference_id) DO UPDATE SET
             kind = excluded.kind,
             location = excluded.location",
         (&id, reference_id, kind, location),
    )?;

    Ok(())
}

pub fn get_content(
    conn: &Connection,
    reference_id: &str,
) -> Result<(String, String)> {
    let mut stmt = conn.prepare(
        "SELECT kind, location FROM content WHERE reference_id = ?1"
    )?;

    let mut rows = stmt.query([reference_id])?;

    if let Some(row) = rows.next()? {
        let kind: String = row.get(0)?;
        let location: String = row.get(1)?;
        Ok((kind, location))
    } else {
        Err(rusqlite::Error::QueryReturnedNoRows)
    }
}

pub fn complete_entry_keys(
    conn: &Connection,
    partial: &str,
) -> Result<Vec<String>> {
    let pattern = format!("{}%", partial);

    let mut stmt = conn.prepare(
        "
        SELECT entry_key
        FROM refs
        WHERE entry_key LIKE ?1
        ORDER BY entry_key
        "
    )?;

    let rows = stmt.query_map([pattern], |row| {
        row.get(0)
    })?;

    let mut result = Vec::new();

    for row in rows {
        result.push(row?);
    }

    Ok(result)
}

pub fn set_note_path(conn: &Connection, id: &str, path: &str) -> Result<()> {
    conn.execute(
        "UPDATE refs SET note_path = ?1 WHERE id = ?2",
        (path, id),
    )?;
    Ok(())
}

pub fn get_note_path(conn: &Connection, id: &str) -> Result<Option<String>> {
    conn.query_row(
        "SELECT note_path FROM refs WHERE id = ?1",
        [id],
        |row| row.get::<_, Option<String>>(0),
    )
}
