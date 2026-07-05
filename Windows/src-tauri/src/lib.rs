use std::sync::atomic::{AtomicBool, Ordering};

use aiusage_tauri::{
    activate_claude_config, activate_codex_config, activate_opencode_config,
    app_settings as load_app_settings, build_phase_a_snapshot, credential_summaries,
    delete_credential, export_diagnostics as export_windows_diagnostics, managed_config_statuses,
    restore_claude_config, restore_codex_config, restore_opencode_config,
    save_app_settings as persist_app_settings, save_credential, start_proxy_runtime,
    stop_proxy_runtime, AppSettingsDocument, AppSettingsSnapshot, CallAnalyticsInventorySnapshot,
    CallAnalyticsSnapshot, ClaudeActivationRequest, CodexActivationRequest, CredentialSummary,
    DesktopSnapshot, DiagnosticsExportSnapshot, ManagedConfigStatus, OpenCodeActivationRequest,
    ProxyHealth, ProxyRuntimeConfig, ProxyTrack, ProxyUsageArchiveSummary, ProxyUsageStats,
    UpsertCredentialRequest,
};
use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    App, AppHandle, Emitter, Manager, Runtime, Window, WindowEvent,
};

static QUIT_REQUESTED: AtomicBool = AtomicBool::new(false);

const MAIN_WINDOW_LABEL: &str = "main";
const OPEN_SECTION_EVENT: &str = "aiusage-open-section";
const SETTINGS_SECTION_ID: &str = "settings";
const TRAY_ID: &str = "aiusage-main-tray";
const TRAY_MENU_SHOW_ID: &str = "show-main-window";
const TRAY_MENU_SETTINGS_ID: &str = "open-settings";
const TRAY_MENU_QUIT_ID: &str = "quit";

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
fn call_analytics_inventory() -> Result<CallAnalyticsInventorySnapshot, String> {
    aiusage_tauri::call_analytics_inventory().map_err(|error| error.to_string())
}

#[tauri::command]
fn call_analytics_snapshot() -> Result<CallAnalyticsSnapshot, String> {
    aiusage_tauri::call_analytics_snapshot().map_err(|error| error.to_string())
}

#[tauri::command]
fn app_settings() -> Result<AppSettingsSnapshot, String> {
    load_app_settings().map_err(|error| error.to_string())
}

#[tauri::command]
fn save_app_settings(settings: AppSettingsDocument) -> Result<AppSettingsSnapshot, String> {
    persist_app_settings(settings).map_err(|error| error.to_string())
}

#[tauri::command]
fn export_diagnostics() -> Result<DiagnosticsExportSnapshot, String> {
    export_windows_diagnostics().map_err(|error| error.to_string())
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

fn configure_tray<R: Runtime>(app: &mut App<R>) -> tauri::Result<()> {
    let show_window =
        MenuItem::with_id(app, TRAY_MENU_SHOW_ID, "Show AIUsage", true, None::<&str>)?;
    let open_settings = MenuItem::with_id(
        app,
        TRAY_MENU_SETTINGS_ID,
        "Open Settings",
        true,
        None::<&str>,
    )?;
    let separator = PredefinedMenuItem::separator(app)?;
    let quit = MenuItem::with_id(app, TRAY_MENU_QUIT_ID, "Quit AIUsage", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show_window, &open_settings, &separator, &quit])?;

    let mut tray = TrayIconBuilder::with_id(TRAY_ID)
        .menu(&menu)
        .tooltip("AIUsage")
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            TRAY_MENU_SHOW_ID => show_main_window(app),
            TRAY_MENU_SETTINGS_ID => {
                show_main_window(app);
                emit_open_section(app, SETTINGS_SECTION_ID);
            }
            TRAY_MENU_QUIT_ID => {
                QUIT_REQUESTED.store(true, Ordering::SeqCst);
                app.exit(0);
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| match event {
            TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            }
            | TrayIconEvent::DoubleClick {
                button: MouseButton::Left,
                ..
            } => show_main_window(tray.app_handle()),
            _ => {}
        });

    if let Some(icon) = app.default_window_icon().cloned() {
        tray = tray.icon(icon);
    }

    tray.build(app)?;
    Ok(())
}

fn show_main_window<R: Runtime>(app: &AppHandle<R>) {
    if let Some(window) = app.get_webview_window(MAIN_WINDOW_LABEL) {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

fn emit_open_section<R: Runtime>(app: &AppHandle<R>, section_id: &str) {
    if let Some(window) = app.get_webview_window(MAIN_WINDOW_LABEL) {
        let _ = window.emit(OPEN_SECTION_EVENT, section_id);
    }
}

fn handle_window_event<R: Runtime>(window: &Window<R>, event: &WindowEvent) {
    if window.label() != MAIN_WINDOW_LABEL {
        return;
    }

    if let WindowEvent::CloseRequested { api, .. } = event {
        if QUIT_REQUESTED.load(Ordering::SeqCst) {
            return;
        }

        if should_minimize_to_tray() {
            api.prevent_close();
            let _ = window.hide();
        }
    }
}

fn should_minimize_to_tray() -> bool {
    load_app_settings()
        .map(|snapshot| {
            snapshot.settings.minimize_to_tray_on_close
                || snapshot.settings.keep_running_in_background
        })
        .unwrap_or(true)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            configure_tray(app)?;
            Ok(())
        })
        .on_window_event(handle_window_event)
        .invoke_handler(tauri::generate_handler![
            app_snapshot,
            config_statuses,
            proxy_statuses,
            proxy_usage_archives,
            proxy_usage_stats,
            call_analytics_inventory,
            call_analytics_snapshot,
            app_settings,
            save_app_settings,
            export_diagnostics,
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
