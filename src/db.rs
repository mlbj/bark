use rusqlite::{Connection, Result};
use uuid::Uuid;

pub fn init_db(path: &str) -> Result<Connection> {
    let conn = Connection::open(path)?;

    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS refs (
            id TEXT PRIMARY KEY,
            bibtex TEXT NOT NULL,
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

    conn.execute(
        "INSERT INTO refs (id, bibtex) VALUES (?1, ?2)",
        (&id, bibtex),
    )?;

    Ok(id)
}

pub fn list_references(conn: &Connection) -> Result<Vec<(String, String)>> {
    let mut stmt = conn.prepare(
        "SELECT id, substr(bibtex, 1, 60) FROM refs"
    )?;

    let rows = stmt.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
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
