use aiusage_tauri::{build_phase_a_snapshot, DesktopSnapshot};

#[tauri::command]
fn app_snapshot() -> DesktopSnapshot {
    build_phase_a_snapshot()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![app_snapshot])
        .run(tauri::generate_context!())
        .expect("failed to run AIUsage Windows desktop shell");
}
