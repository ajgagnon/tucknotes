use std::path::Path;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::Connection;

use crate::errors::AppError;

pub struct DatabaseState {
    pub conn: Mutex<Connection>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SessionRow {
    pub id: String,
    pub title: Option<String>,
    pub created_at: i64,
    pub ended_at: Option<i64>,
    pub duration_ms: Option<i64>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SegmentRow {
    pub id: i64,
    pub session_id: String,
    pub text: String,
    pub source: String,
    pub timestamp_ms: i64,
    pub prompt: Option<String>,
    pub created_at: i64,
}

pub fn now_unix_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

/// Open (or create) the database at `<base_dir>/grain.db` and run migrations.
pub fn open_db(base_dir: &Path) -> Result<Connection, AppError> {
    let db_path = base_dir.join("grain.db");
    let conn = Connection::open(&db_path)?;
    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;
    migrate(&conn)?;
    Ok(conn)
}

#[cfg(test)]
fn open_db_memory() -> Result<Connection, AppError> {
    let conn = Connection::open_in_memory()?;
    conn.execute_batch("PRAGMA foreign_keys=ON;")?;
    migrate(&conn)?;
    Ok(conn)
}

fn migrate(conn: &Connection) -> Result<(), AppError> {
    let version: i32 = conn.pragma_query_value(None, "user_version", |row| row.get(0))?;

    if version < 1 {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS sessions (
                id           TEXT PRIMARY KEY,
                title        TEXT,
                created_at   INTEGER NOT NULL,
                ended_at     INTEGER,
                duration_ms  INTEGER
            );
            CREATE TABLE IF NOT EXISTS transcript_segments (
                id           INTEGER PRIMARY KEY AUTOINCREMENT,
                session_id   TEXT NOT NULL,
                text         TEXT NOT NULL,
                source       TEXT NOT NULL,
                timestamp_ms INTEGER NOT NULL,
                prompt       TEXT,
                created_at   INTEGER NOT NULL,
                FOREIGN KEY (session_id) REFERENCES sessions(id) ON DELETE CASCADE
            );
            CREATE INDEX IF NOT EXISTS idx_segments_session
                ON transcript_segments(session_id);
            PRAGMA user_version = 1;",
        )?;
    }

    Ok(())
}

pub fn create_session(
    conn: &Connection,
    id: &str,
    title: &str,
    created_at: i64,
) -> Result<(), AppError> {
    conn.execute(
        "INSERT INTO sessions (id, title, created_at) VALUES (?1, ?2, ?3)",
        rusqlite::params![id, title, created_at],
    )?;
    Ok(())
}

pub fn end_session(
    conn: &Connection,
    id: &str,
    ended_at: i64,
    duration_ms: i64,
) -> Result<(), AppError> {
    conn.execute(
        "UPDATE sessions SET ended_at = ?1, duration_ms = ?2 WHERE id = ?3",
        rusqlite::params![ended_at, duration_ms, id],
    )?;
    Ok(())
}

pub fn insert_segment(
    conn: &Connection,
    session_id: &str,
    text: &str,
    source: &str,
    timestamp_ms: i64,
    prompt: Option<&str>,
    created_at: i64,
) -> Result<(), AppError> {
    conn.execute(
        "INSERT INTO transcript_segments (session_id, text, source, timestamp_ms, prompt, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        rusqlite::params![session_id, text, source, timestamp_ms, prompt, created_at],
    )?;
    Ok(())
}

pub fn list_sessions(conn: &Connection) -> Result<Vec<SessionRow>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT id, title, created_at, ended_at, duration_ms
         FROM sessions ORDER BY created_at DESC",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(SessionRow {
            id: row.get(0)?,
            title: row.get(1)?,
            created_at: row.get(2)?,
            ended_at: row.get(3)?,
            duration_ms: row.get(4)?,
        })
    })?;
    let mut sessions = Vec::new();
    for row in rows {
        sessions.push(row?);
    }
    Ok(sessions)
}

pub fn get_session_with_segments(
    conn: &Connection,
    session_id: &str,
) -> Result<(SessionRow, Vec<SegmentRow>), AppError> {
    let session = conn.query_row(
        "SELECT id, title, created_at, ended_at, duration_ms FROM sessions WHERE id = ?1",
        rusqlite::params![session_id],
        |row| {
            Ok(SessionRow {
                id: row.get(0)?,
                title: row.get(1)?,
                created_at: row.get(2)?,
                ended_at: row.get(3)?,
                duration_ms: row.get(4)?,
            })
        },
    )?;

    let mut stmt = conn.prepare(
        "SELECT id, session_id, text, source, timestamp_ms, prompt, created_at
         FROM transcript_segments WHERE session_id = ?1 ORDER BY timestamp_ms ASC",
    )?;
    let rows = stmt.query_map(rusqlite::params![session_id], |row| {
        Ok(SegmentRow {
            id: row.get(0)?,
            session_id: row.get(1)?,
            text: row.get(2)?,
            source: row.get(3)?,
            timestamp_ms: row.get(4)?,
            prompt: row.get(5)?,
            created_at: row.get(6)?,
        })
    })?;
    let mut segments = Vec::new();
    for row in rows {
        segments.push(row?);
    }

    Ok((session, segments))
}

pub fn delete_session(conn: &Connection, session_id: &str) -> Result<(), AppError> {
    conn.execute(
        "DELETE FROM sessions WHERE id = ?1",
        rusqlite::params![session_id],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_and_segment_lifecycle() {
        let conn = open_db_memory().unwrap();

        create_session(&conn, "s1", "Test Session", 1708700000000).unwrap();
        insert_segment(&conn, "s1", "Hello world", "system", 0, None, 1708700003000).unwrap();
        insert_segment(
            &conn,
            "s1",
            "How are you",
            "microphone",
            3000,
            Some("Hello world"),
            1708700006000,
        )
        .unwrap();
        end_session(&conn, "s1", 1708700300000, 300000).unwrap();

        let (session, segments) = get_session_with_segments(&conn, "s1").unwrap();
        assert_eq!(session.id, "s1");
        assert_eq!(session.title.as_deref(), Some("Test Session"));
        assert_eq!(session.ended_at, Some(1708700300000));
        assert_eq!(session.duration_ms, Some(300000));
        assert_eq!(segments.len(), 2);
        assert_eq!(segments[0].text, "Hello world");
        assert_eq!(segments[0].source, "system");
        assert_eq!(segments[0].prompt, None);
        assert_eq!(segments[1].text, "How are you");
        assert_eq!(segments[1].prompt.as_deref(), Some("Hello world"));
    }

    #[test]
    fn list_sessions_ordering() {
        let conn = open_db_memory().unwrap();

        create_session(&conn, "s1", "First", 1708700000000).unwrap();
        create_session(&conn, "s2", "Second", 1708700060000).unwrap();

        let sessions = list_sessions(&conn).unwrap();
        assert_eq!(sessions.len(), 2);
        assert_eq!(sessions[0].id, "s2");
        assert_eq!(sessions[1].id, "s1");
    }

    #[test]
    fn delete_cascades_segments() {
        let conn = open_db_memory().unwrap();

        create_session(&conn, "s1", "To Delete", 1708700000000).unwrap();
        insert_segment(&conn, "s1", "text", "system", 0, None, 1708700003000).unwrap();

        delete_session(&conn, "s1").unwrap();

        let sessions = list_sessions(&conn).unwrap();
        assert!(sessions.is_empty());

        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM transcript_segments WHERE session_id = 's1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 0);
    }
}
