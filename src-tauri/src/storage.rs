use crate::model::*;
use rusqlite::{params, Connection, OptionalExtension};
use std::path::Path;

pub struct Store {
    db: Connection,
}
impl Store {
    pub fn open(path: &Path) -> Result<Self, String> {
        let db = Connection::open(path).map_err(|e| e.to_string())?;
        db.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL;
            CREATE TABLE IF NOT EXISTS settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS peers (id TEXT PRIMARY KEY, device TEXT NOT NULL, address TEXT NOT NULL, token TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS messages (peer_id TEXT NOT NULL, id TEXT NOT NULL, direction TEXT NOT NULL, text TEXT NOT NULL, images TEXT NOT NULL, created_at INTEGER NOT NULL, status TEXT NOT NULL, unread INTEGER NOT NULL, digest TEXT NOT NULL, PRIMARY KEY(peer_id,id,direction));
            CREATE INDEX IF NOT EXISTS history ON messages(peer_id,created_at);") .map_err(|e| e.to_string())?;
        let has_error: bool = db.query_row("SELECT count(*) > 0 FROM pragma_table_info('messages') WHERE name='delivery_error'", [], |r| r.get(0)).map_err(|e| e.to_string())?;
        if !has_error {
            db.execute("ALTER TABLE messages ADD COLUMN delivery_error TEXT", [])
                .map_err(|e| e.to_string())?;
        }
        Ok(Self { db })
    }
    pub fn setting(&self, key: &str) -> Result<Option<String>, String> {
        self.db
            .query_row("SELECT value FROM settings WHERE key=?1", [key], |r| {
                r.get(0)
            })
            .optional()
            .map_err(|e| e.to_string())
    }
    pub fn set_setting(&self, key: &str, value: &str) -> Result<(), String> {
        self.db
            .execute(
                "INSERT OR REPLACE INTO settings VALUES (?1,?2)",
                params![key, value],
            )
            .map_err(|e| e.to_string())?;
        Ok(())
    }
    pub fn trust(&self, peer: &TrustedPeer) -> Result<(), String> {
        self.db
            .execute(
                "INSERT OR REPLACE INTO peers VALUES (?1,?2,?3,?4)",
                params![
                    peer.device.id,
                    serde_json::to_string(&peer.device).map_err(|e| e.to_string())?,
                    peer.address,
                    peer.token
                ],
            )
            .map_err(|e| e.to_string())?;
        Ok(())
    }
    pub fn peers(&self) -> Result<Vec<TrustedPeer>, String> {
        let mut stmt = self
            .db
            .prepare("SELECT device,address,token FROM peers")
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                ))
            })
            .map_err(|e| e.to_string())?;
        rows.map(|row| {
            let (device, address, token) = row.map_err(|e| e.to_string())?;
            Ok(TrustedPeer {
                device: serde_json::from_str(&device).map_err(|e| e.to_string())?,
                address,
                token,
            })
        })
        .collect()
    }
    pub fn forget(&self, id: &str) -> Result<(), String> {
        self.db
            .execute("DELETE FROM peers WHERE id=?1", [id])
            .map_err(|e| e.to_string())?;
        Ok(())
    }
    pub fn recover_sending(&self) -> Result<(), String> {
        self.db
            .execute(
                "UPDATE messages SET status='failed' WHERE status='sending'",
                [],
            )
            .map_err(|e| e.to_string())?;
        Ok(())
    }
    pub fn insert(&self, message: &Message, digest: &str) -> Result<(), String> {
        self.db
            .execute(
                "INSERT INTO messages (peer_id,id,direction,text,images,created_at,status,unread,digest,delivery_error) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
                params![
                    message.peer_id,
                    message.id,
                    message.direction,
                    message.text,
                    serde_json::to_string(&message.images).map_err(|e| e.to_string())?,
                    message.created_at,
                    message.status,
                    message.unread,
                    digest,
                    message.delivery_error
                ],
            )
            .map_err(|e| e.to_string())?;
        Ok(())
    }
    pub fn duplicate(&self, peer: &str, id: &str, digest: &str) -> Result<bool, String> {
        let previous: Option<String> = self
            .db
            .query_row(
                "SELECT digest FROM messages WHERE peer_id=?1 AND id=?2 AND direction='incoming'",
                params![peer, id],
                |r| r.get(0),
            )
            .optional()
            .map_err(|e| e.to_string())?;
        match previous {
            Some(old) if old != digest => Err("消息 ID 冲突".into()),
            Some(_) => Ok(true),
            None => Ok(false),
        }
    }
    pub fn status(
        &self,
        peer: &str,
        id: &str,
        status: &str,
        error: Option<&str>,
    ) -> Result<(), String> {
        self.db
            .execute(
                "UPDATE messages SET status=?3,delivery_error=?4 WHERE peer_id=?1 AND id=?2 AND direction='outgoing'",
                params![peer, id, status, error],
            )
            .map_err(|e| e.to_string())?;
        Ok(())
    }
    pub fn messages(&self, peer: &str, limit: usize) -> Result<Vec<Message>, String> {
        let mut stmt=self.db.prepare("SELECT id,peer_id,text,images,created_at,direction,status,unread,delivery_error FROM (SELECT rowid,* FROM messages WHERE peer_id=?1 ORDER BY rowid DESC LIMIT ?2) ORDER BY rowid").map_err(|e|e.to_string())?;
        let rows = stmt
            .query_map(params![peer, limit], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, i64>(4)?,
                    r.get::<_, String>(5)?,
                    r.get::<_, String>(6)?,
                    r.get::<_, bool>(7)?,
                    r.get::<_, Option<String>>(8)?,
                ))
            })
            .map_err(|e| e.to_string())?;
        rows.map(|row| {
            let (id, peer_id, text, images, created_at, direction, status, unread, delivery_error) =
                row.map_err(|e| e.to_string())?;
            Ok(Message {
                id,
                peer_id,
                text,
                images: serde_json::from_str(&images).map_err(|e| e.to_string())?,
                created_at,
                direction,
                status,
                unread,
                delivery_error,
            })
        })
        .collect()
    }
    pub fn outgoing(&self, peer: &str, id: &str) -> Result<Message, String> {
        let (text, images, created_at, status, delivery_error): (String, String, i64, String, Option<String>) = self.db.query_row(
            "SELECT text,images,created_at,status,delivery_error FROM messages WHERE peer_id=?1 AND id=?2 AND direction='outgoing'",
            params![peer, id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
        ).map_err(|e| e.to_string())?;
        Ok(Message {
            id: id.into(),
            peer_id: peer.into(),
            text,
            images: serde_json::from_str(&images).map_err(|e| e.to_string())?,
            created_at,
            direction: "outgoing".into(),
            status,
            unread: false,
            delivery_error,
        })
    }
    pub fn unread(&self, peer: &str) -> usize {
        self.db
            .query_row(
                "SELECT count(*) FROM messages WHERE peer_id=?1 AND unread=1",
                [peer],
                |r| r.get(0),
            )
            .unwrap_or(0)
    }
    pub fn mark_read(&self, peer: &str) -> Result<(), String> {
        self.db
            .execute("UPDATE messages SET unread=0 WHERE peer_id=?1", [peer])
            .map_err(|e| e.to_string())?;
        Ok(())
    }
}
