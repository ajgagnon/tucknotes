use std::path::Path;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::Connection;
use uuid::Uuid;

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
    /// Summary template id used for this meeting (`NULL` = the Default template).
    pub template: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MeetingDocumentRow {
    pub id: String,
    pub meeting_id: String,
    pub kind: String,
    pub title: String,
    pub body: Option<String>,
    pub sort_order: i64,
    pub created_at: i64,
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

/// One full-text-search hit returned by [`search_meetings`].
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SearchHit {
    pub meeting_id: String,
    pub meeting_title: Option<String>,
    pub meeting_created_at: i64,
    /// `"transcript"` or `"summary"`.
    pub kind: String,
    /// FTS5 snippet with `<mark>` highlight markers around matches.
    pub snippet: String,
    /// BM25 score — lower is better.
    pub rank: f64,
}

pub fn now_unix_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

/// Open (or create) the database at `<base_dir>/tucknotes.db` and initialise the schema.
pub fn open_db(base_dir: &Path) -> Result<Connection, AppError> {
    let db_path = base_dir.join("tucknotes.db");
    let conn = Connection::open(&db_path)?;
    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;
    init_schema(&conn)?;
    migrate_database(&conn)?;
    Ok(conn)
}

#[cfg(test)]
fn open_db_memory() -> Result<Connection, AppError> {
    let conn = Connection::open_in_memory()?;
    conn.execute_batch("PRAGMA foreign_keys=ON;")?;
    init_schema(&conn)?;
    migrate_database(&conn)?;
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
            template     TEXT
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
            ON transcript_segments(meeting_id);
        CREATE TABLE IF NOT EXISTS meeting_documents (
            id           TEXT PRIMARY KEY,
            meeting_id   TEXT NOT NULL,
            kind         TEXT NOT NULL CHECK (kind IN ('summary', 'notes')),
            title        TEXT NOT NULL,
            body         TEXT,
            sort_order   INTEGER NOT NULL,
            created_at   INTEGER NOT NULL,
            FOREIGN KEY (meeting_id) REFERENCES meetings(id) ON DELETE CASCADE
        );
        CREATE UNIQUE INDEX IF NOT EXISTS idx_meeting_summary_notes
            ON meeting_documents(meeting_id, kind) WHERE kind IN ('summary', 'notes');
        CREATE INDEX IF NOT EXISTS idx_documents_meeting_sort
            ON meeting_documents(meeting_id, sort_order, created_at);",
    )?;
    Ok(())
}

/// Create the FTS5 virtual table and its sync triggers. Runs **after**
/// `migrate_database`, because the meeting_documents migration recreates that
/// table (dropping any triggers attached to it). Idempotent.
fn init_fts_index(conn: &Connection) -> Result<(), AppError> {
    conn.execute_batch(
        "CREATE VIRTUAL TABLE IF NOT EXISTS meeting_search USING fts5(
            text,
            meeting_id UNINDEXED,
            kind       UNINDEXED,
            source_id  UNINDEXED,
            tokenize='porter unicode61'
        );

        CREATE TRIGGER IF NOT EXISTS segments_ai AFTER INSERT ON transcript_segments BEGIN
            INSERT INTO meeting_search(text, meeting_id, kind, source_id)
            VALUES (new.text, new.meeting_id, 'transcript', CAST(new.id AS TEXT));
        END;
        CREATE TRIGGER IF NOT EXISTS segments_ad AFTER DELETE ON transcript_segments BEGIN
            DELETE FROM meeting_search
            WHERE kind = 'transcript' AND source_id = CAST(old.id AS TEXT);
        END;

        CREATE TRIGGER IF NOT EXISTS docs_summary_ai AFTER INSERT ON meeting_documents
            WHEN new.kind = 'summary' BEGIN
            INSERT INTO meeting_search(text, meeting_id, kind, source_id)
            VALUES (COALESCE(new.body, ''), new.meeting_id, 'summary', new.id);
        END;
        CREATE TRIGGER IF NOT EXISTS docs_summary_au AFTER UPDATE OF body ON meeting_documents
            WHEN new.kind = 'summary' BEGIN
            DELETE FROM meeting_search WHERE kind = 'summary' AND source_id = new.id;
            INSERT INTO meeting_search(text, meeting_id, kind, source_id)
            VALUES (COALESCE(new.body, ''), new.meeting_id, 'summary', new.id);
        END;
        CREATE TRIGGER IF NOT EXISTS docs_summary_ad AFTER DELETE ON meeting_documents
            WHEN old.kind = 'summary' BEGIN
            DELETE FROM meeting_search WHERE kind = 'summary' AND source_id = old.id;
        END;",
    )?;
    Ok(())
}

/// Populate `meeting_search` from existing rows when the index is empty. Idempotent —
/// once the FTS table has any rows, this is a no-op.
fn backfill_meeting_search(conn: &Connection) -> Result<(), AppError> {
    let count: i64 = conn.query_row("SELECT COUNT(*) FROM meeting_search", [], |r| r.get(0))?;
    if count > 0 {
        return Ok(());
    }
    conn.execute_batch(
        "INSERT INTO meeting_search(text, meeting_id, kind, source_id)
            SELECT text, meeting_id, 'transcript', CAST(id AS TEXT) FROM transcript_segments;
         INSERT INTO meeting_search(text, meeting_id, kind, source_id)
            SELECT COALESCE(body, ''), meeting_id, 'summary', id
            FROM meeting_documents WHERE kind = 'summary';",
    )?;
    Ok(())
}

/// Upgrade paths for pre-launch dev DBs (e.g. drop legacy `meetings.summary`).
fn migrate_database(conn: &Connection) -> Result<(), AppError> {
    if meetings_table_has_column(conn, "summary")? {
        conn.execute("ALTER TABLE meetings DROP COLUMN summary", [])?;
    }
    if !meetings_table_has_column(conn, "template")? {
        conn.execute("ALTER TABLE meetings ADD COLUMN template TEXT", [])?;
    }
    if !meetings_table_has_column(conn, "base_title")? {
        // Holds the user's entered title; the AI suffix is appended to it.
        // No backfill: legacy rows keep NULL and are treated as already-final.
        conn.execute("ALTER TABLE meetings ADD COLUMN base_title TEXT", [])?;
    }
    migrate_meeting_documents_to_summary_schema(conn)?;
    backfill_default_documents(conn)?;
    init_fts_index(conn)?;
    backfill_meeting_search(conn)?;
    Ok(())
}

/// Rebuilds `meeting_documents` with `kind IN ('summary', 'notes')` and rewrites data from dev-era CHECKs.
/// Always run (idempotent for DBs that are already on the new schema).
///
/// Rename the table *before* rewriting any rows: older dev DBs had a CHECK like
/// `kind IN ('minutes','notes','custom','enhanced','enhanced_raw')` that rejects `'summary'`,
/// so any UPDATE on the original table would fail. Remapping happens in the INSERT ... SELECT
/// into the freshly created table, which has the new CHECK.
fn migrate_meeting_documents_to_summary_schema(conn: &Connection) -> Result<(), AppError> {
    // After RENAME, SQLite keeps the same index *names* on `meeting_documents_old`, so we must
    // drop them before reusing those names (init_schema already created them on the pre-rename
    // table).
    conn.execute("ALTER TABLE meeting_documents RENAME TO meeting_documents_old", [])?;
    conn.execute_batch(
        "DROP INDEX IF EXISTS idx_meeting_summary_notes;
         DROP INDEX IF EXISTS idx_meeting_minutes_notes;
         DROP INDEX IF EXISTS idx_documents_meeting_sort;",
    )?;
    conn.execute_batch(
        "CREATE TABLE meeting_documents (
            id           TEXT PRIMARY KEY,
            meeting_id   TEXT NOT NULL,
            kind         TEXT NOT NULL CHECK (kind IN ('summary', 'notes')),
            title        TEXT NOT NULL,
            body         TEXT,
            sort_order   INTEGER NOT NULL,
            created_at   INTEGER NOT NULL,
            FOREIGN KEY (meeting_id) REFERENCES meetings(id) ON DELETE CASCADE
        );
        CREATE UNIQUE INDEX idx_meeting_summary_notes
            ON meeting_documents(meeting_id, kind) WHERE kind IN ('summary', 'notes');
        CREATE INDEX idx_documents_meeting_sort
            ON meeting_documents(meeting_id, sort_order, created_at);",
    )?;
    // Drop dev-era kinds we no longer store (`custom`, `enhanced`, `enhanced_raw`, …).
    conn.execute(
        "DELETE FROM meeting_documents_old WHERE kind NOT IN ('minutes', 'notes', 'summary')",
        [],
    )?;
    // If both `summary` and `minutes` exist for a meeting, keep the existing summary row.
    conn.execute(
        "DELETE FROM meeting_documents_old WHERE kind = 'minutes' AND meeting_id IN (
            SELECT meeting_id FROM meeting_documents_old WHERE kind = 'summary'
        )",
        [],
    )?;
    conn.execute_batch(
        "INSERT INTO meeting_documents (id, meeting_id, kind, title, body, sort_order, created_at)
            SELECT id,
                   meeting_id,
                   CASE WHEN kind = 'minutes' THEN 'summary' ELSE kind END,
                   CASE WHEN kind = 'minutes' THEN 'Summary' ELSE title END,
                   body,
                   sort_order,
                   created_at
              FROM meeting_documents_old;
         DROP TABLE meeting_documents_old;",
    )?;
    Ok(())
}

fn meetings_table_has_column(conn: &Connection, name: &str) -> Result<bool, AppError> {
    let mut stmt = conn.prepare("PRAGMA table_info(meetings)")?;
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        let col_name: String = row.get(1)?;
        if col_name == name {
            return Ok(true);
        }
    }
    Ok(false)
}

fn backfill_default_documents(conn: &Connection) -> Result<(), AppError> {
    let mut stmt = conn.prepare("SELECT id, created_at FROM meetings")?;
    let meetings: Vec<(String, i64)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<Result<_, rusqlite::Error>>()?;

    for (meeting_id, created_at) in meetings {
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM meeting_documents WHERE meeting_id = ?1",
            [&meeting_id],
            |r| r.get(0),
        )?;
        if count == 0 {
            insert_default_meeting_documents(conn, &meeting_id, created_at)?;
        }
    }
    Ok(())
}

fn insert_default_meeting_documents(
    conn: &Connection,
    meeting_id: &str,
    created_at: i64,
) -> Result<(), AppError> {
    let summary_id = Uuid::new_v4().to_string();
    let notes_id = Uuid::new_v4().to_string();
    conn.execute(
        "INSERT INTO meeting_documents (id, meeting_id, kind, title, body, sort_order, created_at)
         VALUES (?1, ?2, 'summary', 'Summary', NULL, 0, ?3)",
        rusqlite::params![summary_id, meeting_id, created_at],
    )?;
    conn.execute(
        "INSERT INTO meeting_documents (id, meeting_id, kind, title, body, sort_order, created_at)
         VALUES (?1, ?2, 'notes', 'Notes', NULL, 1, ?3)",
        rusqlite::params![notes_id, meeting_id, created_at],
    )?;
    Ok(())
}

pub fn create_meeting(
    conn: &Connection,
    id: &str,
    title: Option<&str>,
    created_at: i64,
) -> Result<(), AppError> {
    let tx = conn.unchecked_transaction()?;
    tx.execute(
        "INSERT INTO meetings (id, title, created_at) VALUES (?1, ?2, ?3)",
        rusqlite::params![id, title, created_at],
    )?;
    insert_default_meeting_documents(&tx, id, created_at)?;
    tx.commit()?;
    Ok(())
}

pub fn get_meeting_title(conn: &Connection, id: &str) -> Result<Option<String>, AppError> {
    let title = conn.query_row(
        "SELECT title FROM meetings WHERE id = ?1",
        rusqlite::params![id],
        |row| row.get::<_, Option<String>>(0),
    )?;
    Ok(title)
}

/// The user-entered base title, which the AI suffix is appended to.
pub fn get_base_title(conn: &Connection, id: &str) -> Result<Option<String>, AppError> {
    let title = conn.query_row(
        "SELECT base_title FROM meetings WHERE id = ?1",
        rusqlite::params![id],
        |row| row.get::<_, Option<String>>(0),
    )?;
    Ok(title)
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

pub fn set_summary_body(conn: &Connection, meeting_id: &str, body: &str) -> Result<(), AppError> {
    let n = conn.execute(
        "UPDATE meeting_documents SET body = ?1 WHERE meeting_id = ?2 AND kind = 'summary'",
        rusqlite::params![body, meeting_id],
    )?;
    if n == 0 {
        return Err(AppError::DatabaseError(format!(
            "No summary document for meeting: {meeting_id}"
        )));
    }
    Ok(())
}

pub fn update_meeting_document_body(
    conn: &Connection,
    document_id: &str,
    body: &str,
) -> Result<(), AppError> {
    let n = conn.execute(
        "UPDATE meeting_documents SET body = ?1 WHERE id = ?2",
        rusqlite::params![body, document_id],
    )?;
    if n == 0 {
        return Err(AppError::NotFound(format!(
            "Meeting document not found: {document_id}"
        )));
    }
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

/// Set the title from a user edit: both `base_title` (the value the AI suffix is
/// appended to) and the displayed `title` become the entered value, clearing any
/// previously appended AI suffix until the next summarization.
pub fn set_user_title(conn: &Connection, id: &str, title: &str) -> Result<(), AppError> {
    conn.execute(
        "UPDATE meetings SET base_title = ?1, title = ?1 WHERE id = ?2",
        rusqlite::params![title, id],
    )?;
    Ok(())
}

/// Set the summary template id for a meeting (`None` clears it back to Default).
pub fn set_meeting_template(
    conn: &Connection,
    id: &str,
    template: Option<&str>,
) -> Result<(), AppError> {
    conn.execute(
        "UPDATE meetings SET template = ?1 WHERE id = ?2",
        rusqlite::params![template, id],
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
        "SELECT id, title, created_at, ended_at, duration_ms, template
         FROM meetings ORDER BY created_at DESC",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(MeetingRow {
            id: row.get(0)?,
            title: row.get(1)?,
            created_at: row.get(2)?,
            ended_at: row.get(3)?,
            duration_ms: row.get(4)?,
            template: row.get(5)?,
        })
    })?;
    let mut meetings = Vec::new();
    for row in rows {
        meetings.push(row?);
    }
    Ok(meetings)
}

fn map_document_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<MeetingDocumentRow> {
    Ok(MeetingDocumentRow {
        id: row.get(0)?,
        meeting_id: row.get(1)?,
        kind: row.get(2)?,
        title: row.get(3)?,
        body: row.get(4)?,
        sort_order: row.get(5)?,
        created_at: row.get(6)?,
    })
}

pub fn list_meeting_documents(
    conn: &Connection,
    meeting_id: &str,
) -> Result<Vec<MeetingDocumentRow>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT id, meeting_id, kind, title, body, sort_order, created_at
         FROM meeting_documents WHERE meeting_id = ?1
         ORDER BY sort_order ASC, created_at ASC, id ASC",
    )?;
    let rows = stmt.query_map([meeting_id], map_document_row)?;
    let mut docs = Vec::new();
    for row in rows {
        docs.push(row?);
    }
    Ok(docs)
}

pub fn get_meeting_with_segments(
    conn: &Connection,
    meeting_id: &str,
) -> Result<(MeetingRow, Vec<SegmentRow>, Vec<MeetingDocumentRow>), AppError> {
    let meeting = conn.query_row(
        "SELECT id, title, created_at, ended_at, duration_ms, template FROM meetings WHERE id = ?1",
        rusqlite::params![meeting_id],
        |row| {
            Ok(MeetingRow {
                id: row.get(0)?,
                title: row.get(1)?,
                created_at: row.get(2)?,
                ended_at: row.get(3)?,
                duration_ms: row.get(4)?,
                template: row.get(5)?,
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

    let documents = list_meeting_documents(conn, meeting_id)?;

    Ok((meeting, segments, documents))
}

/// Build a safe FTS5 MATCH expression from a user query. Whitespace-splits the
/// query, escapes embedded double-quotes, and wraps each token in double quotes
/// (FTS5 phrase syntax). Empty queries return `None` so callers can short-circuit.
fn build_fts_query(query: &str) -> Option<String> {
    let parts: Vec<String> = query
        .split_whitespace()
        .filter(|p| !p.is_empty())
        .map(|p| format!("\"{}\"", p.replace('"', "\"\"")))
        .collect();
    if parts.is_empty() {
        None
    } else {
        Some(parts.join(" "))
    }
}

/// Full-text search across transcript segments and summary documents. Returns
/// up to `limit` hits ordered by BM25 (lower = more relevant). Each hit
/// includes a 24-token snippet with `<mark>...</mark>` markers around matches.
pub fn search_meetings(
    conn: &Connection,
    query: &str,
    limit: i64,
) -> Result<Vec<SearchHit>, AppError> {
    let Some(match_expr) = build_fts_query(query) else {
        return Ok(Vec::new());
    };
    let limit = limit.clamp(1, 50);

    let mut stmt = conn.prepare(
        "SELECT ms.meeting_id,
                m.title,
                m.created_at,
                ms.kind,
                snippet(meeting_search, 0, '<mark>', '</mark>', '…', 24) AS snip,
                bm25(meeting_search) AS rank
           FROM meeting_search ms
           JOIN meetings m ON m.id = ms.meeting_id
          WHERE meeting_search MATCH ?1
          ORDER BY rank ASC
          LIMIT ?2",
    )?;
    let rows = stmt.query_map(rusqlite::params![match_expr, limit], |row| {
        Ok(SearchHit {
            meeting_id: row.get(0)?,
            meeting_title: row.get(1)?,
            meeting_created_at: row.get(2)?,
            kind: row.get(3)?,
            snippet: row.get(4)?,
            rank: row.get(5)?,
        })
    })?;
    let mut hits = Vec::new();
    for row in rows {
        hits.push(row?);
    }
    Ok(hits)
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

        create_meeting(&conn, "s1", Some("Test Meeting"), 1708700000000).unwrap();
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

        let (meeting, segments, docs) = get_meeting_with_segments(&conn, "s1").unwrap();
        assert_eq!(meeting.id, "s1");
        assert_eq!(meeting.title.as_deref(), Some("Test Meeting"));
        assert_eq!(meeting.ended_at, Some(1708700300000));
        assert_eq!(meeting.duration_ms, Some(300000));
        assert_eq!(segments.len(), 2);
        assert_eq!(segments[0].text, "Hello world");
        assert_eq!(segments[0].source, "system");
        assert_eq!(segments[0].prompt, None);
        assert_eq!(segments[1].text, "How are you");
        assert_eq!(segments[1].prompt.as_deref(), Some("Hello world"));
        assert_eq!(docs.len(), 2);
        assert_eq!(docs[0].kind, "summary");
        assert_eq!(docs[1].kind, "notes");
    }

    #[test]
    fn list_meetings_ordering() {
        let conn = open_db_memory().unwrap();

        create_meeting(&conn, "s1", Some("First"), 1708700000000).unwrap();
        create_meeting(&conn, "s2", Some("Second"), 1708700060000).unwrap();

        let meetings = list_meetings(&conn).unwrap();
        assert_eq!(meetings.len(), 2);
        assert_eq!(meetings[0].id, "s2");
        assert_eq!(meetings[1].id, "s1");
    }

    #[test]
    fn delete_cascades_segments_and_documents() {
        let conn = open_db_memory().unwrap();

        create_meeting(&conn, "s1", Some("To Delete"), 1708700000000).unwrap();
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

        let doc_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM meeting_documents WHERE meeting_id = 's1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(doc_count, 0);
    }

    #[test]
    fn reopen_meeting_and_duration_queries() {
        let conn = open_db_memory().unwrap();
        create_meeting(&conn, "m", Some("M"), 1).unwrap();
        insert_segment(&conn, "m", "a", "system", 5000, None, 2).unwrap();
        assert_eq!(max_segment_timestamp_ms(&conn, "m").unwrap(), 5000);
        assert_eq!(meeting_recording_duration_ms(&conn, "m").unwrap(), 0);
        end_meeting(&conn, "m", 10, 3000).unwrap();
        assert_eq!(meeting_recording_duration_ms(&conn, "m").unwrap(), 3000);
        reopen_meeting(&conn, "m").unwrap();
        let (meeting, _, _) = get_meeting_with_segments(&conn, "m").unwrap();
        assert_eq!(meeting.ended_at, None);
        assert!(meeting_exists(&conn, "m").unwrap());
        assert!(!meeting_exists(&conn, "missing").unwrap());
    }

    #[test]
    fn summary_body_persists_in_documents() {
        let conn = open_db_memory().unwrap();

        create_meeting(&conn, "s1", Some("Meeting"), 1708700000000).unwrap();
        let (_, _, docs) = get_meeting_with_segments(&conn, "s1").unwrap();
        assert_eq!(docs[0].body, None);

        super::set_summary_body(&conn, "s1", "This was a productive meeting.").unwrap();
        let (_, _, docs) = get_meeting_with_segments(&conn, "s1").unwrap();
        assert_eq!(
            docs[0].body.as_deref(),
            Some("This was a productive meeting.")
        );
    }

    #[test]
    fn update_meeting_document_body_persists() {
        let conn = open_db_memory().unwrap();
        create_meeting(&conn, "s1", Some("M"), 1).unwrap();
        let (_, _, docs) = get_meeting_with_segments(&conn, "s1").unwrap();
        let notes = docs.iter().find(|d| d.kind == "notes").unwrap();
        assert_eq!(notes.body, None);

        super::update_meeting_document_body(&conn, &notes.id, "# Hello\n\nNote body.").unwrap();
        let (_, _, docs) = get_meeting_with_segments(&conn, "s1").unwrap();
        let notes = docs.iter().find(|d| d.kind == "notes").unwrap();
        assert_eq!(notes.body.as_deref(), Some("# Hello\n\nNote body."));
    }

    #[test]
    fn update_meeting_document_body_missing_id_errors() {
        let conn = open_db_memory().unwrap();
        create_meeting(&conn, "s1", Some("M"), 1).unwrap();
        let err = super::update_meeting_document_body(&conn, "nonexistent-id", "x").unwrap_err();
        match err {
            crate::errors::AppError::NotFound(_) => {}
            _ => panic!("expected NotFound, got {err:?}"),
        }
    }

    #[test]
    fn migration_upgrades_dev_era_check_constraint() {
        // Simulate a pre-launch dev DB where `meeting_documents` had a CHECK that forbids
        // `'summary'`. The migration must not run any UPDATE against the old table, since the
        // old CHECK would reject the rewritten kind.
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys=ON;").unwrap();
        conn.execute_batch(
            "CREATE TABLE meetings (
                id           TEXT PRIMARY KEY,
                title        TEXT,
                created_at   INTEGER NOT NULL,
                ended_at     INTEGER,
                duration_ms  INTEGER
            );
            CREATE TABLE meeting_documents (
                id           TEXT PRIMARY KEY,
                meeting_id   TEXT NOT NULL,
                kind         TEXT NOT NULL CHECK (kind IN ('minutes', 'notes', 'custom', 'enhanced', 'enhanced_raw')),
                title        TEXT NOT NULL,
                body         TEXT,
                sort_order   INTEGER NOT NULL,
                created_at   INTEGER NOT NULL,
                FOREIGN KEY (meeting_id) REFERENCES meetings(id) ON DELETE CASCADE
            );
            INSERT INTO meetings (id, title, created_at) VALUES ('m1', 'Old', 1);
            INSERT INTO meeting_documents VALUES ('d1', 'm1', 'minutes', 'Minutes', 'body-a', 0, 1);
            INSERT INTO meeting_documents VALUES ('d2', 'm1', 'notes', 'Notes', NULL, 1, 1);
            INSERT INTO meeting_documents VALUES ('d3', 'm1', 'enhanced', 'X', 'drop-me', 2, 1);",
        )
        .unwrap();

        init_schema(&conn).unwrap();
        migrate_database(&conn).unwrap();

        let docs = list_meeting_documents(&conn, "m1").unwrap();
        let kinds: Vec<&str> = docs.iter().map(|d| d.kind.as_str()).collect();
        assert_eq!(kinds, vec!["summary", "notes"]);
        let summary = docs.iter().find(|d| d.kind == "summary").unwrap();
        assert_eq!(summary.title, "Summary");
        assert_eq!(summary.body.as_deref(), Some("body-a"));
    }

    #[test]
    fn search_finds_transcript_segment() {
        let conn = open_db_memory().unwrap();
        create_meeting(&conn, "m1", Some("Pricing review"), 1708700000000).unwrap();
        insert_segment(&conn, "m1", "We discussed the budget for next quarter.", "system", 0, None, 1).unwrap();
        insert_segment(&conn, "m1", "Hello world unrelated", "system", 1000, None, 2).unwrap();

        let hits = search_meetings(&conn, "budget", 10).unwrap();
        assert_eq!(hits.len(), 1, "expected exactly one hit for 'budget'");
        assert_eq!(hits[0].meeting_id, "m1");
        assert_eq!(hits[0].kind, "transcript");
        assert!(hits[0].snippet.contains("<mark>budget</mark>"));
        assert_eq!(hits[0].meeting_title.as_deref(), Some("Pricing review"));
    }

    #[test]
    fn search_finds_summary_body_and_updates_on_change() {
        let conn = open_db_memory().unwrap();
        create_meeting(&conn, "m1", Some("Sync"), 1).unwrap();
        set_summary_body(&conn, "m1", "Team decided to ship pricing on Friday.").unwrap();

        let hits = search_meetings(&conn, "pricing", 10).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].kind, "summary");
        assert!(hits[0].snippet.contains("<mark>pricing</mark>"));

        // Update the summary body -- the index must reflect the new content.
        set_summary_body(&conn, "m1", "Team decided to defer launch.").unwrap();
        let hits = search_meetings(&conn, "pricing", 10).unwrap();
        assert!(hits.is_empty(), "old summary content must drop out of index");
        let hits = search_meetings(&conn, "defer", 10).unwrap();
        assert_eq!(hits.len(), 1);
    }

    #[test]
    fn search_ignores_notes_documents() {
        let conn = open_db_memory().unwrap();
        create_meeting(&conn, "m1", Some("M"), 1).unwrap();
        let (_, _, docs) = get_meeting_with_segments(&conn, "m1").unwrap();
        let notes = docs.iter().find(|d| d.kind == "notes").unwrap();
        update_meeting_document_body(&conn, &notes.id, "Notes mentioning xylophone.").unwrap();

        let hits = search_meetings(&conn, "xylophone", 10).unwrap();
        assert!(hits.is_empty(), "notes documents must not be indexed");
    }

    #[test]
    fn search_clears_when_meeting_deleted() {
        let conn = open_db_memory().unwrap();
        create_meeting(&conn, "m1", Some("M"), 1).unwrap();
        insert_segment(&conn, "m1", "rare-token-xyzzy here", "system", 0, None, 2).unwrap();
        set_summary_body(&conn, "m1", "rare-token-xyzzy summary").unwrap();
        assert_eq!(search_meetings(&conn, "xyzzy", 10).unwrap().len(), 2);

        delete_meeting(&conn, "m1").unwrap();
        assert!(search_meetings(&conn, "xyzzy", 10).unwrap().is_empty());

        let fts_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM meeting_search", [], |r| r.get(0))
            .unwrap();
        assert_eq!(fts_count, 0);
    }

    #[test]
    fn search_empty_query_returns_empty() {
        let conn = open_db_memory().unwrap();
        create_meeting(&conn, "m1", Some("M"), 1).unwrap();
        insert_segment(&conn, "m1", "anything", "system", 0, None, 2).unwrap();
        assert!(search_meetings(&conn, "", 10).unwrap().is_empty());
        assert!(search_meetings(&conn, "   ", 10).unwrap().is_empty());
    }

    #[test]
    fn search_query_with_special_chars_is_safe() {
        // FTS5 MATCH chokes on bare punctuation like `"` or `(`; the sanitiser
        // must wrap each token in escaped quotes so the query never crashes.
        let conn = open_db_memory().unwrap();
        create_meeting(&conn, "m1", Some("M"), 1).unwrap();
        insert_segment(&conn, "m1", "alpha beta gamma", "system", 0, None, 2).unwrap();
        let _ = search_meetings(&conn, "alpha \"beta\" (gamma)", 10).unwrap();
    }

    #[test]
    fn backfill_populates_search_from_existing_rows() {
        // Simulate a pre-FTS DB: bypass init_schema so the FTS table/triggers don't exist
        // yet, populate source tables directly, then run migrate_database and verify the
        // backfill ran.
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys=ON;").unwrap();
        conn.execute_batch(
            "CREATE TABLE meetings (
                id           TEXT PRIMARY KEY,
                title        TEXT,
                created_at   INTEGER NOT NULL,
                ended_at     INTEGER,
                duration_ms  INTEGER
            );
            CREATE TABLE transcript_segments (
                id           INTEGER PRIMARY KEY AUTOINCREMENT,
                meeting_id   TEXT NOT NULL,
                text         TEXT NOT NULL,
                source       TEXT NOT NULL,
                timestamp_ms INTEGER NOT NULL,
                prompt       TEXT,
                created_at   INTEGER NOT NULL,
                FOREIGN KEY (meeting_id) REFERENCES meetings(id) ON DELETE CASCADE
            );
            CREATE TABLE meeting_documents (
                id           TEXT PRIMARY KEY,
                meeting_id   TEXT NOT NULL,
                kind         TEXT NOT NULL CHECK (kind IN ('summary', 'notes')),
                title        TEXT NOT NULL,
                body         TEXT,
                sort_order   INTEGER NOT NULL,
                created_at   INTEGER NOT NULL,
                FOREIGN KEY (meeting_id) REFERENCES meetings(id) ON DELETE CASCADE
            );
            INSERT INTO meetings VALUES ('m1', 'Old', 1, NULL, NULL);
            INSERT INTO transcript_segments (meeting_id, text, source, timestamp_ms, created_at)
                VALUES ('m1', 'legacy backfill token alpha', 'system', 0, 1);
            INSERT INTO meeting_documents VALUES ('d1', 'm1', 'summary', 'Summary', 'legacy backfill token beta', 0, 1);
            INSERT INTO meeting_documents VALUES ('d2', 'm1', 'notes', 'Notes', 'legacy notes not indexed', 1, 1);",
        )
        .unwrap();

        // init_schema creates the FTS table + triggers; migrate_database runs the backfill.
        init_schema(&conn).unwrap();
        migrate_database(&conn).unwrap();

        let hits = search_meetings(&conn, "alpha", 10).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].kind, "transcript");
        let hits = search_meetings(&conn, "beta", 10).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].kind, "summary");
        // Notes are never indexed.
        let hits = search_meetings(&conn, "indexed", 10).unwrap();
        assert!(hits.is_empty());
    }

    #[test]
    fn meeting_template_persists_and_defaults_null() {
        let conn = open_db_memory().unwrap();
        create_meeting(&conn, "m1", Some("Sync"), 1).unwrap();

        // New meetings have no template (→ Default).
        let (meeting, _, _) = get_meeting_with_segments(&conn, "m1").unwrap();
        assert_eq!(meeting.template, None);

        // Setting a template round-trips through both read paths.
        set_meeting_template(&conn, "m1", Some("standup")).unwrap();
        let (meeting, _, _) = get_meeting_with_segments(&conn, "m1").unwrap();
        assert_eq!(meeting.template.as_deref(), Some("standup"));
        let listed = list_meetings(&conn).unwrap();
        assert_eq!(listed[0].template.as_deref(), Some("standup"));

        // Clearing it returns to NULL.
        set_meeting_template(&conn, "m1", None).unwrap();
        let (meeting, _, _) = get_meeting_with_segments(&conn, "m1").unwrap();
        assert_eq!(meeting.template, None);
    }
}
