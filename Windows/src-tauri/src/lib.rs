use aiusage_tauri::{
    activate_claude_config, activate_codex_config, activate_opencode_config,
    build_phase_a_snapshot, credential_summaries, delete_credential, managed_config_statuses,
    restore_claude_config, restore_codex_config, restore_opencode_config, save_credential,
    start_proxy_runtime, stop_proxy_runtime, ClaudeActivationRequest, CodexActivationRequest,
    CredentialSummary, DesktopSnapshot, ManagedConfigStatus, OpenCodeActivationRequest,
    ProxyHealth, ProxyRuntimeConfig, ProxyTrack, ProxyUsageArchiveSummary, ProxyUsageStats,
    UpsertCredentialRequest,
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
async fn proxy_statuses() -> Result<Vec<ProxyHealth>, String> {
    Ok(aiusage_tauri::proxy_statuses().await)
}

#[tauri::command]
fn proxy_usage_archives() -> Result<Vec<ProxyUsageArchiveSummary>, String> {
    aiusage_tauri::proxy_usage_archive_summaries().map_err(|error| error.to_string())
}

#[tauri::command]
fn proxy_usage_stats() -> Result<ProxyUsageStats, String> {
    aiusage_tauri::proxy_usage_stats().map_err(|error| error.to_string())
}

#[tauri::command]
fn credentials() -> Result<Vec<CredentialSummary>, String> {
    credential_summaries().map_err(|error| error.to_string())
}

#[tauri::command]
fn save_provider_credential(request: UpsertCredentialRequest) -> Result<CredentialSummary, String> {
    save_credential(request).map_err(|error| error.to_string())
}

#[tauri::command]
fn delete_provider_credential(id: String) -> Result<bool, String> {
    delete_credential(id).map_err(|error| error.to_string())
}

#[tauri::command]
async fn start_proxy(config: ProxyRuntimeConfig) -> Result<ProxyHealth, String> {
    start_proxy_runtime(config)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn stop_proxy(track: ProxyTrack) -> Result<ProxyHealth, String> {
    stop_proxy_runtime(track)
        .await
        .map_err(|error| error.to_string())
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
            proxy_statuses,
            proxy_usage_archives,
            proxy_usage_stats,
            credentials,
            save_provider_credential,
            delete_provider_credential,
            start_proxy,
            stop_proxy,
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
