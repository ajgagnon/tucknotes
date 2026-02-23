use crate::errors::AppError;
use crate::services::database::{self, DatabaseState, SegmentRow, SessionRow};

#[tauri::command]
pub fn list_sessions(
    state: tauri::State<'_, DatabaseState>,
) -> Result<Vec<SessionRow>, AppError> {
    let conn = state.conn.lock().map_err(|_| AppError::LockPoisoned)?;
    database::list_sessions(&conn)
}

#[derive(serde::Serialize)]
pub struct SessionDetail {
    pub session: SessionRow,
    pub segments: Vec<SegmentRow>,
}

#[tauri::command]
pub fn get_session(
    state: tauri::State<'_, DatabaseState>,
    session_id: String,
) -> Result<SessionDetail, AppError> {
    let conn = state.conn.lock().map_err(|_| AppError::LockPoisoned)?;
    let (session, segments) = database::get_session_with_segments(&conn, &session_id)?;
    Ok(SessionDetail { session, segments })
}

#[tauri::command]
pub fn delete_session(
    state: tauri::State<'_, DatabaseState>,
    session_id: String,
) -> Result<(), AppError> {
    let conn = state.conn.lock().map_err(|_| AppError::LockPoisoned)?;
    database::delete_session(&conn, &session_id)
}
