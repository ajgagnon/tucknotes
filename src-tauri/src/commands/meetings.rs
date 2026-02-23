use crate::errors::AppError;
use crate::services::database::{self, DatabaseState, MeetingRow, SegmentRow};

#[tauri::command]
pub fn list_meetings(
    state: tauri::State<'_, DatabaseState>,
) -> Result<Vec<MeetingRow>, AppError> {
    let conn = state.conn.lock().map_err(|_| AppError::LockPoisoned)?;
    database::list_meetings(&conn)
}

#[derive(serde::Serialize)]
pub struct MeetingDetail {
    pub meeting: MeetingRow,
    pub segments: Vec<SegmentRow>,
}

#[tauri::command]
pub fn get_meeting(
    state: tauri::State<'_, DatabaseState>,
    meeting_id: String,
) -> Result<MeetingDetail, AppError> {
    let conn = state.conn.lock().map_err(|_| AppError::LockPoisoned)?;
    let (meeting, segments) = database::get_meeting_with_segments(&conn, &meeting_id)?;
    Ok(MeetingDetail { meeting, segments })
}

#[tauri::command]
pub fn delete_meeting(
    state: tauri::State<'_, DatabaseState>,
    meeting_id: String,
) -> Result<(), AppError> {
    let conn = state.conn.lock().map_err(|_| AppError::LockPoisoned)?;
    database::delete_meeting(&conn, &meeting_id)
}
