pub mod service;
pub mod reference;

pub mod db;
pub mod bibtex;

// Re-export API
pub use service::{
    add_reference,
    list_references_and_data,
    import_bibtex,
    export_bibtex,
    export_bibtex_by_tag,
    import_toml,
    export_toml,
    export_toml_by_tag,
    export_ref_toml,
    import_ref_toml,
    resolve_reference,
    get_reference,
    set_note,
    get_note,
    ImportResult,
};

pub use reference::Reference;

use rusqlite::Connection;

pub struct Bark {
    conn: Connection,
}

impl Bark {
    pub fn new(path: &str) -> rusqlite::Result<Self> {
        let conn = db::init_db(path)?;
        Ok(Self { conn })
    }

    pub fn conn(&self) -> &Connection {
        &self.conn
    }
}
