use crate::errors::{lock_or_err, AppError};
use crate::services::database::{self, DatabaseState, MeetingRow, SegmentRow};

#[tauri::command]
pub fn list_meetings(
    state: tauri::State<'_, DatabaseState>,
) -> Result<Vec<MeetingRow>, AppError> {
    let conn = lock_or_err(&state.conn)?;
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
    let conn = lock_or_err(&state.conn)?;
    let (meeting, segments) = database::get_meeting_with_segments(&conn, &meeting_id)?;
    Ok(MeetingDetail { meeting, segments })
}

#[tauri::command]
pub fn delete_meeting(
    state: tauri::State<'_, DatabaseState>,
    meeting_id: String,
) -> Result<(), AppError> {
    let conn = lock_or_err(&state.conn)?;
    database::delete_meeting(&conn, &meeting_id)
}
