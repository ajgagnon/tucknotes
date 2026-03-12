use crate::errors::AppError;
use crate::models::{Model, ModelInfo, WhisperModel};
use crate::services::model_manager;

#[tauri::command]
pub fn list_available_models() -> Vec<ModelInfo<WhisperModel>> {
    model_manager::list_whisper_models()
}

#[tauri::command]
pub fn get_model_status(app: tauri::AppHandle, model_id: String) -> Result<bool, AppError> {
    let model =
        WhisperModel::from_id(&model_id).ok_or_else(|| AppError::InvalidModel(model_id))?;
    model_manager::is_downloaded(&app, &model)
}

#[tauri::command]
pub async fn download_model(app: tauri::AppHandle, model_id: String) -> Result<(), AppError> {
    let model =
        WhisperModel::from_id(&model_id).ok_or_else(|| AppError::InvalidModel(model_id))?;
    model_manager::download(&app, &model).await
}

#[tauri::command]
pub fn get_selected_model(app: tauri::AppHandle) -> Result<Option<String>, AppError> {
    let settings = model_manager::load_settings(&app)?;
    Ok(settings.selected_model.map(|m| m.id().to_owned()))
}

#[tauri::command]
pub fn set_selected_model(app: tauri::AppHandle, model_id: String) -> Result<(), AppError> {
    let model =
        WhisperModel::from_id(&model_id).ok_or_else(|| AppError::InvalidModel(model_id))?;
    let mut settings = model_manager::load_settings(&app)?;
    settings.selected_model = Some(model);
    model_manager::save_settings(&app, &settings)
}
