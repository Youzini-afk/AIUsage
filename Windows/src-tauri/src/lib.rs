use aiusage_tauri::{
    activate_claude_config, activate_codex_config, activate_opencode_config,
    build_phase_a_snapshot, managed_config_statuses, restore_claude_config, restore_codex_config,
    restore_opencode_config, ClaudeActivationRequest, CodexActivationRequest, DesktopSnapshot,
    ManagedConfigStatus, OpenCodeActivationRequest,
};

#[tauri::command]
fn app_snapshot() -> DesktopSnapshot {
    build_phase_a_snapshot()
}

#[tauri::command]
fn config_statuses() -> Result<Vec<ManagedConfigStatus>, String> {
    managed_config_statuses().map_err(|error| error.to_string())
}

#[tauri::command]
fn apply_claude_config(request: ClaudeActivationRequest) -> Result<ManagedConfigStatus, String> {
    activate_claude_config(request).map_err(|error| error.to_string())
}

#[tauri::command]
fn restore_claude_managed_config(
    config_path: Option<std::path::PathBuf>,
) -> Result<ManagedConfigStatus, String> {
    restore_claude_config(config_path).map_err(|error| error.to_string())
}

#[tauri::command]
fn apply_codex_config(request: CodexActivationRequest) -> Result<ManagedConfigStatus, String> {
    activate_codex_config(request).map_err(|error| error.to_string())
}

#[tauri::command]
fn restore_codex_managed_config(
    config_path: Option<std::path::PathBuf>,
) -> Result<ManagedConfigStatus, String> {
    restore_codex_config(config_path).map_err(|error| error.to_string())
}

#[tauri::command]
fn apply_opencode_config(
    request: OpenCodeActivationRequest,
) -> Result<ManagedConfigStatus, String> {
    activate_opencode_config(request).map_err(|error| error.to_string())
}

#[tauri::command]
fn restore_opencode_managed_config(
    config_path: Option<std::path::PathBuf>,
) -> Result<ManagedConfigStatus, String> {
    restore_opencode_config(config_path).map_err(|error| error.to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            app_snapshot,
            config_statuses,
            apply_claude_config,
            restore_claude_managed_config,
            apply_codex_config,
            restore_codex_managed_config,
            apply_opencode_config,
            restore_opencode_managed_config
        ])
        .run(tauri::generate_context!())
        .expect("failed to run AIUsage Windows desktop shell");
}
