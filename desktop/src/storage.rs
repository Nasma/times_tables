use tt_core::spaced_rep::SpacedRepetition;
use directories::ProjectDirs;
use std::fs;
use std::path::PathBuf;

const APP_NAME: &str = "times_tables";
const ORG_NAME: &str = "practice";
const SAVE_FILE: &str = "progress.json";

fn get_data_dir() -> Option<PathBuf> {
    ProjectDirs::from("com", ORG_NAME, APP_NAME).map(|dirs| dirs.data_dir().to_path_buf())
}

pub fn save(data: &SpacedRepetition) -> Result<(), String> {
    let data_dir = get_data_dir().ok_or("Could not determine data directory")?;

    fs::create_dir_all(&data_dir)
        .map_err(|e| format!("Failed to create data directory: {}", e))?;

    let file_path = data_dir.join(SAVE_FILE);
    let json =
        serde_json::to_string_pretty(data).map_err(|e| format!("Failed to serialize: {}", e))?;

    fs::write(&file_path, json).map_err(|e| format!("Failed to write file: {}", e))?;

    Ok(())
}

pub fn load() -> Result<SpacedRepetition, String> {
    let data_dir = get_data_dir().ok_or("Could not determine data directory")?;
    let file_path = data_dir.join(SAVE_FILE);

    if !file_path.exists() {
        return Err("No save file found".to_string());
    }

    let content =
        fs::read_to_string(&file_path).map_err(|e| format!("Failed to read file: {}", e))?;

    serde_json::from_str(&content).map_err(|e| format!("Failed to deserialize: {}", e))
}

pub fn load_or_new() -> SpacedRepetition {
    load().unwrap_or_else(|_| SpacedRepetition::new())
}
