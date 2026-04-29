use crate::errors::{lock_or_err, AppError};
use crate::models::RecordingState;
use crate::services::database::{
    self, DatabaseState, MeetingDocumentRow, MeetingRow, SegmentRow,
};

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
    pub documents: Vec<MeetingDocumentRow>,
}

#[tauri::command]
pub fn get_meeting(
    state: tauri::State<'_, DatabaseState>,
    meeting_id: String,
) -> Result<MeetingDetail, AppError> {
    let conn = lock_or_err(&state.conn)?;
    let (meeting, segments, documents) =
        database::get_meeting_with_segments(&conn, &meeting_id)?;
    Ok(MeetingDetail {
        meeting,
        segments,
        documents,
    })
}

#[tauri::command]
pub fn update_meeting_document_body(
    state: tauri::State<'_, DatabaseState>,
    document_id: String,
    body: String,
) -> Result<(), AppError> {
    let conn = lock_or_err(&state.conn)?;
    database::update_meeting_document_body(&conn, &document_id, &body)
}

#[tauri::command]
pub fn delete_meeting(
    db_state: tauri::State<'_, DatabaseState>,
    recording_state: tauri::State<'_, RecordingState>,
    meeting_id: String,
) -> Result<(), AppError> {
    #[cfg(target_os = "macos")]
    {
        let active = lock_or_err(&recording_state.session_id)?;
        if active.as_deref() == Some(meeting_id.as_str()) {
            return Err(AppError::CaptureFailed(
                "Cannot delete a meeting that is currently being recorded".into(),
            ));
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = &recording_state;
    }

    let conn = lock_or_err(&db_state.conn)?;
    database::delete_meeting(&conn, &meeting_id)
}
