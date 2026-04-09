pub mod models;

use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;

use anyhow::Result;
use rusqlite::{params, Connection};

pub use models::{DownloadRecord, ElcAccountRecord, MegaAccountRecord, MegaSessionRecord};

pub struct Database {
    conn: Mutex<Connection>,
}

unsafe impl Send for Database {}
unsafe impl Sync for Database {}

impl Database {
    pub fn open(db_path: &Path) -> Result<Self> {
        let conn = Connection::open(db_path)?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=OFF;")?;
        let db = Self { conn: Mutex::new(conn) };
        db.setup_tables()?;
        Ok(db)
    }

    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        let db = Self { conn: Mutex::new(conn) };
        db.setup_tables()?;
        Ok(db)
    }

    pub fn setup_tables(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute_batch("
            CREATE TABLE IF NOT EXISTS settings(key VARCHAR(255), value TEXT, PRIMARY KEY('key'));
            CREATE TABLE IF NOT EXISTS downloads(url TEXT, email TEXT, path TEXT, filename TEXT, filekey TEXT, filesize UNSIGNED BIG INT, filepass VARCHAR(64), filenoexpire VARCHAR(64), custom_chunks_dir TEXT, PRIMARY KEY('url'), UNIQUE(path, filename));
            CREATE TABLE IF NOT EXISTS mega_accounts(email TEXT, password TEXT, password_aes TEXT, user_hash TEXT, PRIMARY KEY('email'));
            CREATE TABLE IF NOT EXISTS mega_sessions(email TEXT, ma BLOB, crypt INT, PRIMARY KEY('email'));
            CREATE TABLE IF NOT EXISTS downloads_queue(url TEXT, PRIMARY KEY('url'));
            CREATE TABLE IF NOT EXISTS elc_accounts(host TEXT, user TEXT, apikey TEXT, PRIMARY KEY('host'));
        ")?;
        Ok(())
    }

    pub fn vacuum(&self) -> Result<()> {
        self.conn.lock().unwrap().execute_batch("VACUUM;")?;
        Ok(())
    }

    pub fn get_setting(&self, key: &str) -> Result<Option<String>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare_cached("SELECT value FROM settings WHERE key = ?1")?;
        let mut rows = stmt.query(params![key])?;
        if let Some(row) = rows.next()? { Ok(Some(row.get(0)?)) } else { Ok(None) }
    }

    pub fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("INSERT OR REPLACE INTO settings(key, value) VALUES(?1, ?2)", params![key, value])?;
        Ok(())
    }

    pub fn get_all_settings(&self) -> Result<HashMap<String, String>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare_cached("SELECT key, value FROM settings")?;
        let rows = stmt.query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))?;
        let mut map = HashMap::new();
        for row in rows { let (k, v) = row?; map.insert(k, v); }
        Ok(map)
    }

    pub fn set_all_settings(&self, settings: &HashMap<String, String>) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        for (k, v) in settings {
            conn.execute("INSERT OR REPLACE INTO settings(key, value) VALUES(?1, ?2)", params![k, v])?;
        }
        Ok(())
    }

    pub fn insert_download(&self, d: &DownloadRecord) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("INSERT OR REPLACE INTO downloads(url,email,path,filename,filekey,filesize,filepass,filenoexpire,custom_chunks_dir) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)",
            params![d.url,d.email,d.path,d.filename,d.filekey,d.filesize,d.filepass,d.filenoexpire,d.custom_chunks_dir])?;
        Ok(())
    }

    pub fn delete_download(&self, url: &str) -> Result<()> {
        self.conn.lock().unwrap().execute("DELETE FROM downloads WHERE url = ?1", params![url])?;
        Ok(())
    }

    pub fn select_downloads(&self) -> Result<HashMap<String, DownloadRecord>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare_cached("SELECT url,email,path,filename,filekey,filesize,filepass,filenoexpire,custom_chunks_dir FROM downloads")?;
        let rows = stmt.query_map([], |row| Ok(DownloadRecord {
            url: row.get(0)?, email: row.get(1)?, path: row.get(2)?,
            filename: row.get(3)?, filekey: row.get(4)?, filesize: row.get(5)?,
            filepass: row.get(6)?, filenoexpire: row.get(7)?, custom_chunks_dir: row.get(8)?,
        }))?;
        let mut map = HashMap::new();
        for row in rows { let d = row?; map.insert(d.url.clone(), d); }
        Ok(map)
    }

    pub fn insert_downloads_queue(&self, urls: &[String]) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        for url in urls { conn.execute("INSERT OR IGNORE INTO downloads_queue(url) VALUES(?1)", params![url])?; }
        Ok(())
    }

    pub fn select_downloads_queue(&self) -> Result<Vec<String>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare_cached("SELECT url FROM downloads_queue")?;
        let rows = stmt.query_map([], |row| row.get(0))?;
        rows.collect::<std::result::Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn truncate_downloads_queue(&self) -> Result<()> {
        self.conn.lock().unwrap().execute_batch("DELETE FROM downloads_queue;")?;
        Ok(())
    }

    pub fn delete_from_queue(&self, url: &str) -> Result<()> {
        self.conn.lock().unwrap().execute("DELETE FROM downloads_queue WHERE url = ?1", params![url])?;
        Ok(())
    }

    pub fn insert_mega_account(&self, a: &MegaAccountRecord) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("INSERT OR REPLACE INTO mega_accounts(email,password,password_aes,user_hash) VALUES(?1,?2,?3,?4)",
            params![a.email, a.password, a.password_aes, a.user_hash])?;
        Ok(())
    }

    pub fn select_mega_accounts(&self) -> Result<HashMap<String, MegaAccountRecord>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare_cached("SELECT email,password,password_aes,user_hash FROM mega_accounts")?;
        let rows = stmt.query_map([], |row| Ok(MegaAccountRecord {
            email: row.get(0)?, password: row.get(1)?, password_aes: row.get(2)?, user_hash: row.get(3)?,
        }))?;
        let mut map = HashMap::new();
        for row in rows { let a = row?; map.insert(a.email.clone(), a); }
        Ok(map)
    }

    pub fn delete_mega_account(&self, email: &str) -> Result<()> {
        self.conn.lock().unwrap().execute("DELETE FROM mega_accounts WHERE email = ?1", params![email])?;
        Ok(())
    }

    pub fn truncate_mega_accounts(&self) -> Result<()> {
        self.conn.lock().unwrap().execute_batch("DELETE FROM mega_accounts;")?;
        Ok(())
    }

    pub fn insert_mega_session(&self, email: &str, data: &[u8], encrypted: bool) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("INSERT OR REPLACE INTO mega_sessions(email,ma,crypt) VALUES(?1,?2,?3)",
            params![email, data, encrypted as i32])?;
        Ok(())
    }

    pub fn select_mega_session(&self, email: &str) -> Result<Option<MegaSessionRecord>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare_cached("SELECT email,ma,crypt FROM mega_sessions WHERE email = ?1")?;
        let mut rows = stmt.query(params![email])?;
        if let Some(row) = rows.next()? {
            Ok(Some(MegaSessionRecord { email: row.get(0)?, data: row.get(1)?, encrypted: row.get::<_,i32>(2)? != 0 }))
        } else { Ok(None) }
    }

    pub fn truncate_mega_sessions(&self) -> Result<()> {
        self.conn.lock().unwrap().execute_batch("DELETE FROM mega_sessions;")?;
        Ok(())
    }

    pub fn insert_elc_account(&self, a: &ElcAccountRecord) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("INSERT OR REPLACE INTO elc_accounts(host,user,apikey) VALUES(?1,?2,?3)",
            params![a.host, a.user, a.apikey])?;
        Ok(())
    }

    pub fn select_elc_accounts(&self) -> Result<HashMap<String, ElcAccountRecord>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare_cached("SELECT host,user,apikey FROM elc_accounts")?;
        let rows = stmt.query_map([], |row| Ok(ElcAccountRecord {
            host: row.get(0)?, user: row.get(1)?, apikey: row.get(2)?,
        }))?;
        let mut map = HashMap::new();
        for row in rows { let a = row?; map.insert(a.host.clone(), a); }
        Ok(map)
    }

    pub fn delete_elc_account(&self, host: &str) -> Result<()> {
        self.conn.lock().unwrap().execute("DELETE FROM elc_accounts WHERE host = ?1", params![host])?;
        Ok(())
    }

    pub fn truncate_elc_accounts(&self) -> Result<()> {
        self.conn.lock().unwrap().execute_batch("DELETE FROM elc_accounts;")?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_db() -> Database { Database::open_in_memory().expect("in-memory db") }

    #[test]
    fn test_settings_roundtrip() {
        let db = make_db();
        db.set_setting("foo", "bar").unwrap();
        assert_eq!(db.get_setting("foo").unwrap(), Some("bar".to_string()));
        assert_eq!(db.get_setting("missing").unwrap(), None);
    }

    #[test]
    fn test_download_crud() {
        let db = make_db();
        let rec = DownloadRecord {
            url: "https://mega.nz/file/abc123".to_string(), email: None,
            path: "/downloads".to_string(), filename: "test.zip".to_string(),
            filekey: "KEY".to_string(), filesize: 1024*1024,
            filepass: None, filenoexpire: None, custom_chunks_dir: None,
        };
        db.insert_download(&rec).unwrap();
        assert!(db.select_downloads().unwrap().contains_key("https://mega.nz/file/abc123"));
        db.delete_download("https://mega.nz/file/abc123").unwrap();
        assert!(db.select_downloads().unwrap().is_empty());
    }

    #[test]
    fn test_queue_operations() {
        let db = make_db();
        db.insert_downloads_queue(&["url1".to_string(), "url2".to_string()]).unwrap();
        assert_eq!(db.select_downloads_queue().unwrap().len(), 2);
        db.truncate_downloads_queue().unwrap();
        assert!(db.select_downloads_queue().unwrap().is_empty());
    }

    #[test]
    fn test_session_roundtrip() {
        let db = make_db();
        db.insert_mega_session("user@example.com", b"blob", false).unwrap();
        let s = db.select_mega_session("user@example.com").unwrap().unwrap();
        assert_eq!(s.data, b"blob");
        assert!(!s.encrypted);
    }
}
