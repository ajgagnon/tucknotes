use std::path::Path;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::Connection;

use crate::errors::AppError;

pub struct DatabaseState {
    pub conn: Mutex<Connection>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MeetingRow {
    pub id: String,
    pub title: Option<String>,
    pub created_at: i64,
    pub ended_at: Option<i64>,
    pub duration_ms: Option<i64>,
    pub summary: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SegmentRow {
    pub id: i64,
    pub meeting_id: String,
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

/// Open (or create) the database at `<base_dir>/grain.db` and initialise the schema.
pub fn open_db(base_dir: &Path) -> Result<Connection, AppError> {
    let db_path = base_dir.join("grain.db");
    let conn = Connection::open(&db_path)?;
    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;
    init_schema(&conn)?;
    Ok(conn)
}

#[cfg(test)]
fn open_db_memory() -> Result<Connection, AppError> {
    let conn = Connection::open_in_memory()?;
    conn.execute_batch("PRAGMA foreign_keys=ON;")?;
    init_schema(&conn)?;
    Ok(conn)
}

fn init_schema(conn: &Connection) -> Result<(), AppError> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS meetings (
            id           TEXT PRIMARY KEY,
            title        TEXT,
            created_at   INTEGER NOT NULL,
            ended_at     INTEGER,
            duration_ms  INTEGER,
            summary      TEXT
        );
        CREATE TABLE IF NOT EXISTS transcript_segments (
            id           INTEGER PRIMARY KEY AUTOINCREMENT,
            meeting_id   TEXT NOT NULL,
            text         TEXT NOT NULL,
            source       TEXT NOT NULL,
            timestamp_ms INTEGER NOT NULL,
            prompt       TEXT,
            created_at   INTEGER NOT NULL,
            FOREIGN KEY (meeting_id) REFERENCES meetings(id) ON DELETE CASCADE
        );
        CREATE INDEX IF NOT EXISTS idx_segments_meeting
            ON transcript_segments(meeting_id);",
    )?;
    Ok(())
}

pub fn create_meeting(
    conn: &Connection,
    id: &str,
    title: &str,
    created_at: i64,
) -> Result<(), AppError> {
    conn.execute(
        "INSERT INTO meetings (id, title, created_at) VALUES (?1, ?2, ?3)",
        rusqlite::params![id, title, created_at],
    )?;
    Ok(())
}

pub fn end_meeting(
    conn: &Connection,
    id: &str,
    ended_at: i64,
    duration_ms: i64,
) -> Result<(), AppError> {
    conn.execute(
        "UPDATE meetings SET ended_at = ?1, duration_ms = ?2 WHERE id = ?3",
        rusqlite::params![ended_at, duration_ms, id],
    )?;
    Ok(())
}

/// Clears `ended_at` so a stopped meeting can receive more recording (resume).
pub fn reopen_meeting(conn: &Connection, id: &str) -> Result<(), AppError> {
    let n = conn.execute(
        "UPDATE meetings SET ended_at = NULL WHERE id = ?1",
        rusqlite::params![id],
    )?;
    if n == 0 {
        return Err(AppError::DatabaseError(format!("Meeting not found: {id}")));
    }
    Ok(())
}

pub fn meeting_exists(conn: &Connection, id: &str) -> Result<bool, AppError> {
    let n: i64 = conn.query_row(
        "SELECT COUNT(*) FROM meetings WHERE id = ?1",
        [id],
        |row| row.get(0),
    )?;
    Ok(n > 0)
}

/// Largest `timestamp_ms` among transcript segments (0 if none).
pub fn max_segment_timestamp_ms(conn: &Connection, meeting_id: &str) -> Result<i64, AppError> {
    conn.query_row(
        "SELECT COALESCE(MAX(timestamp_ms), 0) FROM transcript_segments WHERE meeting_id = ?1",
        [meeting_id],
        |row| row.get(0),
    )
    .map_err(Into::into)
}

/// Stored cumulative recording duration (0 when NULL).
pub fn meeting_recording_duration_ms(conn: &Connection, id: &str) -> Result<i64, AppError> {
    conn.query_row(
        "SELECT COALESCE(duration_ms, 0) FROM meetings WHERE id = ?1",
        [id],
        |row| row.get(0),
    )
    .map_err(Into::into)
}

pub fn update_meeting_summary(
    conn: &Connection,
    id: &str,
    summary: &str,
) -> Result<(), AppError> {
    conn.execute(
        "UPDATE meetings SET summary = ?1 WHERE id = ?2",
        rusqlite::params![summary, id],
    )?;
    Ok(())
}

pub fn update_meeting_title(
    conn: &Connection,
    id: &str,
    title: &str,
) -> Result<(), AppError> {
    conn.execute(
        "UPDATE meetings SET title = ?1 WHERE id = ?2",
        rusqlite::params![title, id],
    )?;
    Ok(())
}

pub fn insert_segment(
    conn: &Connection,
    meeting_id: &str,
    text: &str,
    source: &str,
    timestamp_ms: i64,
    prompt: Option<&str>,
    created_at: i64,
) -> Result<(), AppError> {
    conn.execute(
        "INSERT INTO transcript_segments (meeting_id, text, source, timestamp_ms, prompt, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        rusqlite::params![meeting_id, text, source, timestamp_ms, prompt, created_at],
    )?;
    Ok(())
}

pub fn list_meetings(conn: &Connection) -> Result<Vec<MeetingRow>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT id, title, created_at, ended_at, duration_ms, summary
         FROM meetings ORDER BY created_at DESC",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(MeetingRow {
            id: row.get(0)?,
            title: row.get(1)?,
            created_at: row.get(2)?,
            ended_at: row.get(3)?,
            duration_ms: row.get(4)?,
            summary: row.get(5)?,
        })
    })?;
    let mut meetings = Vec::new();
    for row in rows {
        meetings.push(row?);
    }
    Ok(meetings)
}

pub fn get_meeting_with_segments(
    conn: &Connection,
    meeting_id: &str,
) -> Result<(MeetingRow, Vec<SegmentRow>), AppError> {
    let meeting = conn.query_row(
        "SELECT id, title, created_at, ended_at, duration_ms, summary FROM meetings WHERE id = ?1",
        rusqlite::params![meeting_id],
        |row| {
            Ok(MeetingRow {
                id: row.get(0)?,
                title: row.get(1)?,
                created_at: row.get(2)?,
                ended_at: row.get(3)?,
                duration_ms: row.get(4)?,
                summary: row.get(5)?,
            })
        },
    )?;

    let mut stmt = conn.prepare(
        "SELECT id, meeting_id, text, source, timestamp_ms, prompt, created_at
         FROM transcript_segments WHERE meeting_id = ?1 ORDER BY timestamp_ms ASC",
    )?;
    let rows = stmt.query_map(rusqlite::params![meeting_id], |row| {
        Ok(SegmentRow {
            id: row.get(0)?,
            meeting_id: row.get(1)?,
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

    Ok((meeting, segments))
}

pub fn delete_meeting(conn: &Connection, meeting_id: &str) -> Result<(), AppError> {
    conn.execute(
        "DELETE FROM meetings WHERE id = ?1",
        rusqlite::params![meeting_id],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn meeting_and_segment_lifecycle() {
        let conn = open_db_memory().unwrap();

        create_meeting(&conn, "s1", "Test Meeting", 1708700000000).unwrap();
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
        end_meeting(&conn, "s1", 1708700300000, 300000).unwrap();

        let (meeting, segments) = get_meeting_with_segments(&conn, "s1").unwrap();
        assert_eq!(meeting.id, "s1");
        assert_eq!(meeting.title.as_deref(), Some("Test Meeting"));
        assert_eq!(meeting.ended_at, Some(1708700300000));
        assert_eq!(meeting.duration_ms, Some(300000));
        assert_eq!(meeting.summary, None);
        assert_eq!(segments.len(), 2);
        assert_eq!(segments[0].text, "Hello world");
        assert_eq!(segments[0].source, "system");
        assert_eq!(segments[0].prompt, None);
        assert_eq!(segments[1].text, "How are you");
        assert_eq!(segments[1].prompt.as_deref(), Some("Hello world"));
    }

    #[test]
    fn list_meetings_ordering() {
        let conn = open_db_memory().unwrap();

        create_meeting(&conn, "s1", "First", 1708700000000).unwrap();
        create_meeting(&conn, "s2", "Second", 1708700060000).unwrap();

        let meetings = list_meetings(&conn).unwrap();
        assert_eq!(meetings.len(), 2);
        assert_eq!(meetings[0].id, "s2");
        assert_eq!(meetings[1].id, "s1");
    }

    #[test]
    fn delete_cascades_segments() {
        let conn = open_db_memory().unwrap();

        create_meeting(&conn, "s1", "To Delete", 1708700000000).unwrap();
        insert_segment(&conn, "s1", "text", "system", 0, None, 1708700003000).unwrap();

        delete_meeting(&conn, "s1").unwrap();

        let meetings = list_meetings(&conn).unwrap();
        assert!(meetings.is_empty());

        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM transcript_segments WHERE meeting_id = 's1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn reopen_meeting_and_duration_queries() {
        let conn = open_db_memory().unwrap();
        create_meeting(&conn, "m", "M", 1).unwrap();
        insert_segment(&conn, "m", "a", "system", 5000, None, 2).unwrap();
        assert_eq!(max_segment_timestamp_ms(&conn, "m").unwrap(), 5000);
        assert_eq!(meeting_recording_duration_ms(&conn, "m").unwrap(), 0);
        end_meeting(&conn, "m", 10, 3000).unwrap();
        assert_eq!(meeting_recording_duration_ms(&conn, "m").unwrap(), 3000);
        reopen_meeting(&conn, "m").unwrap();
        let (meeting, _) = get_meeting_with_segments(&conn, "m").unwrap();
        assert_eq!(meeting.ended_at, None);
        assert!(meeting_exists(&conn, "m").unwrap());
        assert!(!meeting_exists(&conn, "missing").unwrap());
    }

    #[test]
    fn update_summary() {
        let conn = open_db_memory().unwrap();

        create_meeting(&conn, "s1", "Meeting", 1708700000000).unwrap();
        assert_eq!(
            get_meeting_with_segments(&conn, "s1").unwrap().0.summary,
            None
        );

        update_meeting_summary(&conn, "s1", "This was a productive meeting.").unwrap();
        let (meeting, _) = get_meeting_with_segments(&conn, "s1").unwrap();
        assert_eq!(
            meeting.summary.as_deref(),
            Some("This was a productive meeting.")
        );
    }
}
